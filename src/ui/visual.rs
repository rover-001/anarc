/// Global density adjustment for all Lumen UI dimensions.
pub const UI_SCALE: f32 = 0.85;

/// Shared chrome and viewport geometry, in GPUI logical pixels.
pub const TOPBAR_HEIGHT: f32 = 52.0 * UI_SCALE;
pub const ADDRESS_BAR_HEIGHT: f32 = 36.0 * UI_SCALE;
pub const TAB_ROW_HEIGHT: f32 = 36.0 * UI_SCALE;
/// Shared horizontal geometry for tab, split, and new-tab rows.
pub const SIDEBAR_ROW_PADDING_X: f32 = 12.0;
pub const SIDEBAR_ROW_ICON_SIZE: f32 = 16.0 * UI_SCALE;
pub const SIDEBAR_ROW_GAP: f32 = 8.0;

pub const DEFAULT_PAGE_ZOOM_FACTOR: f64 = 1.0;

pub fn zoom_level_for_factor(factor: f64) -> f64 {
    factor.ln() / 1.2_f64.ln()
}

pub fn default_zoom_level() -> f64 {
    zoom_level_for_factor(DEFAULT_PAGE_ZOOM_FACTOR)
}

pub fn zoom_after_delta(saved_level: Option<f64>, delta: f64) -> f64 {
    if delta == 0.0 {
        default_zoom_level()
    } else {
        (saved_level.unwrap_or_else(default_zoom_level) + delta).clamp(-5.0, 5.0)
    }
}

/// Composite an RGB accent over an RGB background at the given opacity.
pub fn tint_rgb(background: u32, accent: u32, opacity: f32) -> u32 {
    let opacity = opacity.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let bg = ((background >> shift) & 0xff_u32) as f32;
        let fg = ((accent >> shift) & 0xff_u32) as f32;
        (bg + (fg - bg) * opacity).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_zoom_is_one_hundred_percent() {
        assert!((1.2_f64.powf(default_zoom_level()) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn zoom_reset_returns_to_default_instead_of_zero_level() {
        assert_eq!(zoom_after_delta(Some(2.0), 0.0), default_zoom_level());
        assert_eq!(zoom_after_delta(None, 1.0), 1.0);
    }

    #[test]
    fn accent_tint_is_derived_from_each_space_color() {
        assert_eq!(tint_rgb(0x191a1f, 0xffffff, 0.18), 0x424347);
        assert_ne!(
            tint_rgb(0x191a1f, 0x9aa6ff, 0.18),
            tint_rgb(0x191a1f, 0x76d7b2, 0.18)
        );
        assert_eq!(tint_rgb(0x191a1f, 0x9aa6ff, 0.0), 0x191a1f);
    }

    #[test]
    fn viewport_origin_matches_scaled_topbar_height() {
        assert_eq!(TOPBAR_HEIGHT, 52.0 * UI_SCALE);
    }

    #[test]
    fn sidebar_rows_share_icon_and_spacing_metrics() {
        assert_eq!(SIDEBAR_ROW_PADDING_X, 12.0);
        assert_eq!(SIDEBAR_ROW_ICON_SIZE, 16.0 * UI_SCALE);
        assert_eq!(SIDEBAR_ROW_GAP, 8.0);
    }
}
