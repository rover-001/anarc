/// Global density adjustment for all Lumen UI dimensions.
pub const UI_SCALE: f32 = 0.85;

/// Shared chrome and viewport geometry, in GPUI logical pixels.
pub const TOPBAR_HEIGHT: f32 = 52.0 * UI_SCALE;
pub const ADDRESS_BAR_HEIGHT: f32 = 36.0 * UI_SCALE;
pub const TAB_ROW_HEIGHT: f32 = 36.0 * UI_SCALE;

pub const DEFAULT_PAGE_ZOOM_FACTOR: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpaceTheme {
    pub base: u32,
    pub surface: u32,
    pub surface_hover: u32,
    pub border: u32,
    pub border_hover: u32,
    pub accent: u32,
    pub text: u32,
    pub muted: u32,
}

/// Derives every chrome color from the active space's accent.
pub fn space_theme(accent: u32) -> SpaceTheme {
    let surface = tint_rgb(0x191a1f, accent, 0.18);
    let border = tint_rgb(0x282b33, accent, 0.30);
    SpaceTheme {
        base: tint_rgb(0x111216, accent, 0.24),
        surface,
        surface_hover: tint_rgb(surface, 0xffffff, 0.12),
        border,
        border_hover: tint_rgb(border, 0xffffff, 0.16),
        accent,
        text: 0xe6e7eb,
        muted: 0x9fa3ae,
    }
}

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

    fn luminance(color: u32) -> f64 {
        let channel = |shift: u32| {
            let value = f64::from((color >> shift) & 0xff) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    fn contrast(foreground: u32, background: u32) -> f64 {
        let (lighter, darker) = {
            let foreground = luminance(foreground);
            let background = luminance(background);
            (foreground.max(background), foreground.min(background))
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn offered_space_colors_have_deterministic_readable_themes() {
        let colors = [
            0x9aa6ff, 0x76d7b2, 0xf6ad55, 0xf687b3, 0x63b3ed, 0xb794f4, 0xfbb6ce, 0x68d391,
        ];
        for color in colors {
            let theme = space_theme(color);
            assert_eq!(theme, space_theme(color));
            assert!(
                contrast(theme.text, theme.base) >= 4.5,
                "base contrast for {color:#08x}"
            );
            assert!(
                contrast(theme.text, theme.surface) >= 4.5,
                "surface contrast for {color:#08x}"
            );
        }
    }

    #[test]
    fn viewport_origin_matches_scaled_topbar_height() {
        assert_eq!(TOPBAR_HEIGHT, 52.0 * UI_SCALE);
    }
}
