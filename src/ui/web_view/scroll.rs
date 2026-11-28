use std::time::{Duration, Instant};

use gpui_kit::{Context, Modifiers, Window};

use super::WebViewport;

const MIN_INPUT_GAP_SECS: f32 = 0.010;
const MAX_INPUT_GAP_SECS: f32 = 0.022;
const DEFAULT_INPUT_INTERVAL_SECS: f32 = 0.012;
const MAX_FLING_STEP: Duration = Duration::from_millis(48);
const DECAY_SECONDS: f32 = 0.28;
/// Stop at one whole pixel per frame. At 0.5 px/frame the integer wheel delta
/// alternates 1px/0px and every skipped frame is a visible hold in the fling.
const MIN_FLING_PIXELS_PER_FRAME: f32 = 1.0;

#[derive(Default)]
pub(super) struct ScrollMotion {
    accum_x: f32,
    accum_y: f32,
    velocity: (f32, f32),
    input_interval_secs: f32,
    last_event: Option<Instant>,
    last_frame: Option<Instant>,
    last_position: (i32, i32),
    modifiers: Modifiers,
    generation: u64,
    running: bool,
}

impl ScrollMotion {
    pub(super) fn accumulate(&mut self, delta: (f32, f32)) -> (i32, i32) {
        (
            step_scroll_accum(&mut self.accum_x, delta.0),
            step_scroll_accum(&mut self.accum_y, delta.1),
        )
    }

    // Send pointer scroll deltas immediately so CEF receives input with zero
    // frame-queue latency, while keeping the display-paced driver only for
    // post-gesture inertia coasting.
    pub(super) fn record_pointer_scroll(
        &mut self,
        delta: (f32, f32),
        position: (i32, i32),
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<WebViewport>,
    ) -> (i32, i32) {
        let now = Instant::now();
        let interval = self.last_event.map(|previous| now.duration_since(previous));
        let raw_seconds = interval
            .unwrap_or(Duration::from_secs_f32(DEFAULT_INPUT_INTERVAL_SECS))
            .as_secs_f32();
        let current_gap = self.input_gap();
        let fresh = !self.running || interval.is_none_or(|gap| gap > current_gap);
        if fresh {
            self.input_interval_secs = DEFAULT_INPUT_INTERVAL_SECS;
        } else {
            let clamped_interval = raw_seconds.clamp(0.004, 0.030);
            self.input_interval_secs =
                self.input_interval_secs + (clamped_interval - self.input_interval_secs) * 0.35;
        }
        let seconds = raw_seconds.clamp(0.006, 0.05);
        self.velocity.0 = input_velocity(self.velocity.0, delta.0, seconds, fresh);
        self.velocity.1 = input_velocity(self.velocity.1, delta.1, seconds, fresh);
        self.last_event = Some(now);
        self.last_position = position;
        self.modifiers = modifiers;

        if !self.running {
            self.running = true;
            self.last_frame = Some(now);
            self.generation = self.generation.wrapping_add(1);
            let generation = self.generation;
            cx.on_next_frame(window, move |view, window, cx| {
                view.scroll_momentum_tick(generation, window, cx);
            });
        }
        self.accumulate(delta)
    }

    fn input_gap(&self) -> Duration {
        let base = if self.input_interval_secs > 0.0 {
            self.input_interval_secs
        } else {
            DEFAULT_INPUT_INTERVAL_SECS
        };
        Duration::from_secs_f32((base * 1.5).clamp(MIN_INPUT_GAP_SECS, MAX_INPUT_GAP_SECS))
    }

    pub(super) fn interrupt(&mut self) {
        let accumulated = (self.accum_x, self.accum_y);
        self.cancel();
        (self.accum_x, self.accum_y) = accumulated;
    }

    pub(super) fn cancel(&mut self) {
        let generation = self.generation.wrapping_add(1);
        *self = Self {
            generation,
            ..Self::default()
        };
    }
}

impl WebViewport {
    fn scroll_momentum_tick(
        &mut self,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let motion = &mut self.scroll_motion;
        if generation != motion.generation || !motion.running {
            return;
        }
        let now = Instant::now();
        let previous_frame = motion.last_frame.replace(now).unwrap_or(now);
        let last_event = motion.last_event.unwrap_or(now);
        let display_interval = window
            .refresh_interval(cx)
            .unwrap_or(Duration::from_millis(16));
        let (delta, velocity) = integrate_fling(
            motion.velocity,
            previous_frame,
            last_event,
            now,
            display_interval,
        );
        motion.velocity = velocity;
        let position = motion.last_position;
        let modifiers = motion.modifiers;
        let steps = motion.accumulate(delta);
        // CEF accepts integer wheel deltas. Stop at a whole pixel per frame so
        // the tail never alternates between a 1px step and no wheel event.
        let stop_speed = (MIN_FLING_PIXELS_PER_FRAME / display_interval.as_secs_f32()).max(2.0);
        let keep_running = motion.velocity.0.abs().max(motion.velocity.1.abs()) >= stop_speed;
        motion.running = keep_running;

        if steps != (0, 0) {
            let parent = self.parent.clone();
            let tab_id = self.tab_id.clone();
            let _ = parent.update(cx, |lumen, _| {
                lumen.engine.send_mouse_wheel(
                    &tab_id, position.0, position.1, steps.0, steps.1, &modifiers, true,
                );
            });
        }
        if keep_running {
            cx.on_next_frame(window, move |view, window, cx| {
                view.scroll_momentum_tick(generation, window, cx);
            });
        }
    }
}

/// Integrate only the slice of this frame after the latest input event: direct
/// input was already delivered, so counting it again would double the fling.
/// Starting at `last_event` (instead of waiting for an adaptive gap) makes
/// inertia begin on the first frame after lift-off. The cap prevents a delayed
/// frame from producing a large catch-up jump.
fn integrate_fling(
    velocity: (f32, f32),
    previous_frame: Instant,
    last_event: Instant,
    now: Instant,
    display_interval: Duration,
) -> ((f32, f32), (f32, f32)) {
    let start = previous_frame.max(last_event);
    let elapsed = now.saturating_duration_since(start);
    // Once input has stopped, advance by a fixed display interval instead of
    // the measured callback gap. Irregular GPUI frames otherwise turn into
    // uneven wheel deltas, which reads as a jerky coast. An event arriving
    // mid-frame still contributes only the time after it; a badly delayed
    // frame still catches up, bounded by MAX_FLING_STEP.
    let step = if last_event > previous_frame || elapsed > display_interval * 2 {
        elapsed.min(MAX_FLING_STEP)
    } else {
        display_interval
    };
    let seconds = step.as_secs_f32();
    let (dx, vx) = decay_step(velocity.0, seconds);
    let (dy, vy) = decay_step(velocity.1, seconds);
    ((dx, dy), (vx, vy))
}

fn input_velocity(previous: f32, delta: f32, seconds: f32, fresh: bool) -> f32 {
    let measured = delta / seconds;
    if fresh || measured.signum() != previous.signum() {
        measured
    } else {
        // Time-based filtering behaves equally on 60, 120 and 144 Hz input.
        let weight = 1.0 - (-seconds / 0.035).exp();
        previous + (measured - previous) * weight
    }
}

// Exact integral of exponential deceleration, symmetric in both directions.
fn decay_step(velocity: f32, seconds: f32) -> (f32, f32) {
    let decay = (-seconds / DECAY_SECONDS).exp();
    (velocity * DECAY_SECONDS * (1.0 - decay), velocity * decay)
}

pub(super) fn step_scroll_accum(accum: &mut f32, delta: f32) -> i32 {
    *accum += delta;
    let step = accum.round() as i32;
    *accum -= step as f32;
    step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn momentum_is_symmetric_and_independent_of_frame_rate() {
        let (down, _) = decay_step(800.0, 0.032);
        let (up, _) = decay_step(-800.0, 0.032);
        assert!((down + up).abs() < 0.0001);
        let (first, velocity) = decay_step(800.0, 0.016);
        let (second, _) = decay_step(velocity, 0.016);
        assert!((first + second - down).abs() < 0.0001);
    }

    #[test]
    fn reversing_input_replaces_previous_direction_immediately() {
        assert!(input_velocity(800.0, -2.0, 0.016, false) < 0.0);
        assert!(input_velocity(-800.0, 2.0, 0.016, false) > 0.0);
    }

    #[test]
    fn fling_begins_on_the_first_frame_after_lift_off_without_double_counting() {
        let now = Instant::now();
        let last_event = now - Duration::from_millis(4);
        let previous_frame = now - Duration::from_millis(16);
        let interval = Duration::from_millis(7);
        let (delta, _) = integrate_fling((800.0, 0.0), previous_frame, last_event, now, interval);
        assert!((delta.0 - decay_step(800.0, 0.004).0).abs() < 0.001);
        assert!(delta.0 > 0.0);

        // An event arriving midway through a frame still gets its full direct
        // delta; only the time after it contributes to inertia.
        let mid_event = now - Duration::from_millis(2);
        let (delta, _) = integrate_fling((800.0, 0.0), previous_frame, mid_event, now, interval);
        assert!((delta.0 - decay_step(800.0, 0.002).0).abs() < 0.001);

        // After lift-off a steady display interval is used rather than the
        // measured callback gap, so pacing jitter doesn't become delta jitter.
        let settled_event = now - Duration::from_millis(20);
        let settled_frame = now - Duration::from_millis(6);
        let (delta, _) = integrate_fling((800.0, 0.0), settled_frame, settled_event, now, interval);
        assert!((delta.0 - decay_step(800.0, 0.007).0).abs() < 0.001);
    }

    #[test]
    fn fling_tail_never_skips_a_whole_frame() {
        let interval = Duration::from_millis(7);
        let seconds = interval.as_secs_f32();
        let stop_speed = (MIN_FLING_PIXELS_PER_FRAME / seconds).max(2.0);
        let mut velocity = 500.0_f32;
        let mut accum = 0.0_f32;
        let mut frames = 0;
        while velocity >= stop_speed {
            let (delta, next) = decay_step(velocity, seconds);
            velocity = next;
            assert_ne!(
                step_scroll_accum(&mut accum, delta),
                0,
                "zero-delta frame at velocity {velocity}"
            );
            frames += 1;
            assert!(frames < 1000, "fling failed to settle");
        }
        assert!(frames > 10, "fling ended too early");
    }

    #[test]
    fn adaptive_input_gap_tracks_input_cadence() {
        let mut motion = ScrollMotion {
            input_interval_secs: 0.007,
            ..ScrollMotion::default()
        };
        assert!(motion.input_gap() <= Duration::from_millis(12));
        motion.input_interval_secs = 0.016;
        assert_eq!(motion.input_gap(), Duration::from_secs_f32(0.022));
    }
}
