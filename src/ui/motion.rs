use std::time::Duration;

use gpui_kit::{Animation, SpringAnimation, SpringConfig, SpringState, sampled_easing};

/// Settling tolerance used when deriving a duration from a spring.
const SPRING_EPSILON: f32 = 0.001;

pub fn duration(requested: Duration) -> Duration {
    if reduced_motion() {
        Duration::ZERO
    } else {
        requested
    }
}

pub fn reduced_motion() -> bool {
    std::env::var_os("ANARC_REDUCED_MOTION").is_some_and(|value| value == "1")
}

/// Keep values passed to GPUI percentage and opacity APIs inside their valid range.
pub fn clamp_percentage(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// A spring together with the time it needs to settle.
///
/// Apple describes springs as `(response, dampingFraction)`; this type converts
/// that pair into a physical oscillator once and reuses it for every frame,
/// duration and easing in a given interaction, so an animated element and the
/// timer or guard that governs its lifetime can never disagree.
#[derive(Clone, Copy, Debug)]
pub struct SpringSpec {
    config: SpringConfig,
    settle: Duration,
}

/// Converts Apple's `(response, dampingFraction)` into a physical spring with
/// unit mass: `k = (2π / response)^2`, `c = 4π ζ / response`.
pub fn spring(response: f32, damping_fraction: f32) -> SpringSpec {
    let response = if response.is_finite() && response > 0.0 {
        response
    } else {
        0.3
    };
    let zeta = if damping_fraction.is_finite() && damping_fraction > 0.0 {
        damping_fraction
    } else {
        1.0
    };
    let natural_frequency = std::f32::consts::TAU / response;
    let config = SpringConfig::new(
        natural_frequency * natural_frequency,
        2.0 * zeta * natural_frequency,
        1.0,
    );
    SpringSpec {
        config,
        settle: config.settle_time(SpringState::default(), 1.0, SPRING_EPSILON),
    }
}

impl SpringSpec {
    /// The Spring configuration this spec drives, for `with_spring` sites that
    /// animate a non-`f32` value such as a rotation or a pixel offset.
    pub fn config(&self) -> SpringConfig {
        self.config
    }

    /// Time after which the spring from 0 to 1 stays settled.
    pub fn settle_time(&self) -> Duration {
        self.settle
    }

    /// Settle time, collapsed to zero when the user asked for reduced motion.
    pub fn duration(&self) -> Duration {
        duration(self.settle_time())
    }

    /// Position of a 0 -> 1 spring after `elapsed`. Overshoots for `zeta < 1`,
    /// and is pinned to the target once settled so callers never hang.
    pub fn progress(&self, elapsed: Duration) -> f32 {
        if reduced_motion() || elapsed >= self.settle {
            return 1.0;
        }
        self.config
            .step(SpringState::default(), 1.0, elapsed.as_secs_f32())
            .position
    }

    /// Easing curve with exact endpoints, sampled over [`SpringSpec::settle_time`].
    pub fn easing(&self) -> impl Fn(f32) -> f32 + 'static {
        sampled_easing(self.config, SPRING_EPSILON).1
    }

    /// A duration-based animation whose shape is this spring. Use it for
    /// interpolations driven by an external `Instant` (theme colours, browser
    /// frames) where a stateful spring has nowhere to store its velocity.
    pub fn animation(&self) -> Animation {
        Animation::new(self.duration()).with_easing(self.easing())
    }

    /// Spring animation for an entrance: mounts at 0 and settles at the target.
    ///
    /// Under reduced motion the element mounts already settled, so the entrance
    /// becomes a direct appearance instead of a large travel.
    pub fn enter(&self) -> SpringAnimation<f32> {
        let initial = if reduced_motion() { 1.0 } else { 0.0 };
        self.to(1.0).from(initial).with_epsilon(SPRING_EPSILON)
    }

    /// Spring animation for something leaving: mounts at `value`, settles at `target`.
    pub fn between(&self, value: f32, target: f32) -> SpringAnimation<f32> {
        self.to(target).from(value).with_epsilon(SPRING_EPSILON)
    }

    /// Spring animation retargeted to `target`, preserving position and velocity
    /// of an element that is already mounted.
    pub fn to(&self, target: f32) -> SpringAnimation<f32> {
        SpringAnimation::new(self.config)
            .to(target)
            .with_epsilon(SPRING_EPSILON)
    }
}

// Interaction presets, as `(response, dampingFraction)`.
//
// Small controls stay tight, large surfaces heavier, and only genuinely
// elastic interactions (a released tap, a popover) are allowed to bounce.
// Passive or large-scale motion is critically damped so nothing overshoots a
// layout edge.
fn spec(response: f32, damping_fraction: f32) -> SpringSpec {
    spring(response, damping_fraction)
}

/// Small pop back after a tap or a released control.
pub fn pop() -> SpringSpec {
    spec(0.26, 0.75)
}

/// Menus, popovers and suggestion lists.
pub fn popover() -> SpringSpec {
    spec(0.28, 0.85)
}

/// Toggle and segmented-control indicators: rotation and dock position.
pub fn toggle() -> SpringSpec {
    spec(0.26, 0.80)
}

/// Sheets, dialogs and the peek overlay.
pub fn sheet() -> SpringSpec {
    spec(0.38, 0.85)
}

/// Tab content swaps and other medium surfaces.
pub fn content() -> SpringSpec {
    spec(0.33, 0.90)
}

/// Full page-level transitions such as switching spaces.
pub fn space_switch() -> SpringSpec {
    spec(0.40, 0.90)
}

/// Large surfaces that resize: the sidebar and its browser panes.
pub fn sidebar() -> SpringSpec {
    spec(0.40, 0.90)
}

/// A row appearing in a list.
pub fn row_enter() -> SpringSpec {
    spec(0.30, 0.85)
}

/// A row leaving a list: short and critically damped.
pub fn row_exit() -> SpringSpec {
    spec(0.22, 1.0)
}

/// Theme colour interpolation: passive, so critically damped.
pub fn theme() -> SpringSpec {
    spec(0.28, 1.0)
}

/// Visual-viewport zoom settling after a pinch ends.
pub fn pinch_settle() -> SpringSpec {
    spec(0.18, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn apple_parameters_map_onto_physical_springs() {
        let spec = spring(0.4, 0.8);
        let (natural_frequency, damping_ratio) = spec.config().canonical();
        assert_close(natural_frequency, std::f32::consts::TAU / 0.4, 1e-4);
        assert_close(damping_ratio, 0.8, 1e-5);
    }

    #[test]
    fn non_finite_parameters_fall_back_to_a_settling_spring() {
        for spec in [spring(f32::NAN, 1.0), spring(-1.0, f32::NAN)] {
            assert!(spec.settle_time() > Duration::ZERO);
            assert_ne!(spec.settle_time(), Duration::MAX);
        }
    }

    #[test]
    fn spring_progress_reaches_the_targets() {
        let spec = popover();
        assert_eq!(spec.progress(Duration::ZERO), 0.0);
        assert_eq!(spec.progress(spec.settle_time()), 1.0);
        assert_eq!(spec.progress(Duration::from_secs(60)), 1.0);
        assert!(spec.progress(spec.settle_time() / 2) > 0.0);
    }

    #[test]
    fn overdamped_presets_never_overshoot_and_bouncy_ones_do() {
        let peak = |spec: SpringSpec| {
            (1..=400)
                .map(|step| spec.progress(spec.settle_time().mul_f32(step as f32 / 400.0)))
                .fold(0.0_f32, f32::max)
        };
        assert!(peak(theme()) <= 1.0001);
        assert!(peak(row_exit()) <= 1.0001);
        assert!(peak(pinch_settle()) <= 1.0001);
        assert!(peak(pop()) > 1.0);
        assert!(peak(pop()) < 1.1);
    }

    #[test]
    fn easing_has_exact_endpoints_and_matches_the_spring_shape() {
        let spec = content();
        let easing = spec.easing();
        assert_eq!(easing(0.0), 0.0);
        assert_eq!(easing(1.0), 1.0);
        assert!(easing(0.5) > 0.5);
    }

    #[test]
    fn presets_settle_quickly_enough_to_use_as_durations() {
        for spec in [
            pop(),
            popover(),
            toggle(),
            sheet(),
            content(),
            space_switch(),
            sidebar(),
            row_enter(),
            row_exit(),
            theme(),
            pinch_settle(),
        ] {
            assert!(
                spec.settle_time() <= Duration::from_millis(900),
                "{:?}",
                spec.settle_time()
            );
        }
    }

    #[test]
    fn percentage_easing_outputs_are_clamped_and_finite() {
        for spec in [
            pop(),
            popover(),
            sheet(),
            content(),
            space_switch(),
            sidebar(),
            row_enter(),
            theme(),
        ] {
            let easing = spec.easing();
            for step in 0..=1000 {
                let percentage = clamp_percentage(easing(step as f32 / 1000.0));
                assert!((0.0..=1.0).contains(&percentage));
            }
        }

        assert_eq!(clamp_percentage(f32::NAN), 0.0);
        assert_eq!(clamp_percentage(f32::INFINITY), 1.0);
        assert_eq!(clamp_percentage(f32::NEG_INFINITY), 0.0);
    }
}
