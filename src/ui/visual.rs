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

pub const DEFAULT_PAGE_ZOOM_FACTOR: f64 = 0.9;

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

pub fn interpolate_theme(from: SpaceTheme, to: SpaceTheme, progress: f32) -> SpaceTheme {
    let progress = progress.clamp(0.0, 1.0);
    let color = |a, b| {
        let channel = |shift| {
            let a = ((a >> shift) & 0xff_u32) as f32;
            let b = ((b >> shift) & 0xff_u32) as f32;
            (a + (b - a) * progress).round() as u32
        };
        channel(16) << 16 | channel(8) << 8 | channel(0)
    };
    SpaceTheme {
        base: color(from.base, to.base),
        surface: color(from.surface, to.surface),
        surface_hover: color(from.surface_hover, to.surface_hover),
        border: color(from.border, to.border),
        border_hover: color(from.border_hover, to.border_hover),
        accent: color(from.accent, to.accent),
        text: color(from.text, to.text),
        muted: color(from.muted, to.muted),
    }
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
    fn default_zoom_is_ninety_percent() {
        assert!((1.2_f64.powf(default_zoom_level()) - 0.9).abs() < 1e-12);
    }

    #[test]
    fn zoom_reset_returns_to_default_instead_of_zero_level() {
        assert_eq!(zoom_after_delta(Some(2.0), 0.0), default_zoom_level());
        assert_eq!(zoom_after_delta(None, 1.0), default_zoom_level() + 1.0);
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

    #[test]
    fn sidebar_rows_share_icon_and_spacing_metrics() {
        assert_eq!(SIDEBAR_ROW_PADDING_X, 12.0);
        assert_eq!(SIDEBAR_ROW_ICON_SIZE, 16.0 * UI_SCALE);
        assert_eq!(SIDEBAR_ROW_GAP, 8.0);
    }

    #[test]
    fn space_theme_interpolation_reaches_both_endpoints_and_midpoint() {
        let from = space_theme(0x9aa6ff);
        let to = space_theme(0x76d7b2);
        assert_eq!(interpolate_theme(from, to, 0.0), from);
        assert_eq!(interpolate_theme(from, to, 1.0), to);
        let middle = interpolate_theme(from, to, 0.5);
        assert_eq!(middle.accent, 0x8890d8);
        assert_ne!(middle.base, from.base);
        assert_ne!(middle.base, to.base);
    }
}
