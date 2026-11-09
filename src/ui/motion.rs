use std::time::Duration;

pub const MOTION_FAST: Duration = Duration::from_millis(120);
pub const MOTION_BASE: Duration = Duration::from_millis(180);
pub const MOTION_SLOW: Duration = Duration::from_millis(260);
pub const MOTION_CONTENT: Duration = Duration::from_millis(150);
pub const MOTION_THEME: Duration = Duration::from_millis(220);
pub const MOTION_POPOVER: Duration = Duration::from_millis(140);

pub fn duration(requested: Duration) -> Duration {
    if reduced_motion() {
        Duration::ZERO
    } else {
        requested
    }
}

pub fn reduced_motion() -> bool {
    std::env::var_os("LUMEN_REDUCED_MOTION").is_some_and(|value| value == "1")
}

/// Keep values passed to GPUI percentage and opacity APIs inside their valid range.
pub fn clamp_percentage(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// A compact damped spring curve for passive entrances and exits.
/// Interactive gestures should use GPUI's stateful `with_spring` API instead.
pub fn spring_out(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    let damping = 7.8_f32;
    let frequency = 11.0_f32;
    let value = 1.0
        - (-damping * t).exp()
            * ((frequency * t).cos() + (damping / frequency) * (frequency * t).sin());
    if t == 1.0 { 1.0 } else { value }
}

pub fn ease_out_quint(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_curve_starts_and_finishes_at_targets() {
        assert_eq!(spring_out(0.0), 0.0);
        assert_eq!(spring_out(1.0), 1.0);
        assert!(spring_out(0.4) > 0.0);
    }

    #[test]
    fn spring_curve_has_a_small_damped_overshoot() {
        let peak = (0..=1000)
            .map(|step| spring_out(step as f32 / 1000.0))
            .fold(0.0_f32, f32::max);
        assert!(peak > 1.0 && peak < 1.15);
    }

    #[test]
    fn percentage_easing_outputs_are_clamped_and_finite() {
        for step in 0..=1000 {
            let eased = spring_out(step as f32 / 1000.0);
            let percentage = clamp_percentage(eased);
            assert!((0.0..=1.0).contains(&percentage));
        }

        assert_eq!(clamp_percentage(f32::NAN), 0.0);
        assert_eq!(clamp_percentage(f32::INFINITY), 1.0);
        assert_eq!(clamp_percentage(f32::NEG_INFINITY), 0.0);
    }
}
