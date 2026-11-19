use std::time::{Duration, Instant};

use gpui_kit::{Context, Modifiers, Window};

use super::WebViewport;

const INPUT_GAP: Duration = Duration::from_millis(24);
const DECAY_SECONDS: f32 = 0.24;
const STOP_SPEED: f32 = 2.0;

#[derive(Default)]
pub(super) struct ScrollMotion {
    accum_x: f32,
    accum_y: f32,
    pending: (f32, f32),
    velocity: (f32, f32),
    last_event: Option<Instant>,
    last_frame: Option<Instant>,
    last_position: (i32, i32),
    modifiers: Modifiers,
    generation: u64,
    running: bool,
}

impl ScrollMotion {
    pub(super) fn continuous_recent(&self) -> bool {
        self.running
            && self
                .last_event
                .is_some_and(|time| time.elapsed() < Duration::from_millis(100))
    }

    pub(super) fn accumulate(&mut self, delta: (f32, f32)) -> (i32, i32) {
        (
            step_scroll_accum(&mut self.accum_x, delta.0),
            step_scroll_accum(&mut self.accum_y, delta.1),
        )
    }

    // Send the start of a gesture immediately. Coalesce subsequent input with
    // the display-paced driver to avoid flooding CEF between painted frames.
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
        let seconds = interval
            .unwrap_or(Duration::from_millis(16))
            .as_secs_f32()
            .clamp(0.004, 0.05);
        let fresh = !self.running || interval.is_none_or(|gap| gap > INPUT_GAP);
        self.velocity.0 = input_velocity(self.velocity.0, delta.0, seconds, fresh);
        self.velocity.1 = input_velocity(self.velocity.1, delta.1, seconds, fresh);
        self.pending.0 += delta.0;
        self.pending.1 += delta.1;
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
        if fresh {
            let pending = std::mem::take(&mut self.pending);
            self.accumulate(pending)
        } else {
            (0, 0)
        }
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
        let mut delta = std::mem::take(&mut motion.pending);
        let input_pending = delta != (0.0, 0.0);
        // Integrate only the portion of this frame after direct input ceased.
        // A long scheduling stall ends inertia instead of issuing a large jump.
        if !input_pending && now.duration_since(last_event) > INPUT_GAP {
            let start = previous_frame.max(last_event + INPUT_GAP);
            let elapsed = now.saturating_duration_since(start);
            if elapsed > Duration::from_millis(80) {
                motion.velocity = (0.0, 0.0);
            } else {
                let seconds = elapsed.as_secs_f32();
                let (dx, vx) = decay_step(motion.velocity.0, seconds);
                let (dy, vy) = decay_step(motion.velocity.1, seconds);
                delta = (dx, dy);
                motion.velocity = (vx, vy);
            }
        }
        let position = motion.last_position;
        let modifiers = motion.modifiers;
        let steps = motion.accumulate(delta);
        let keep_running = input_pending
            || now.duration_since(last_event) <= INPUT_GAP
            || motion.velocity.0.abs().max(motion.velocity.1.abs()) >= STOP_SPEED;
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
}
