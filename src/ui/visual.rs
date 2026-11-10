/// Global density adjustment for all Lumen UI dimensions.
pub const UI_SCALE: f32 = 0.85;

/// Shared chrome and viewport geometry, in GPUI logical pixels.
pub const TOPBAR_HEIGHT: f32 = 52.0 * UI_SCALE;
pub const ADDRESS_BAR_HEIGHT: f32 = 36.0 * UI_SCALE;
pub const NAV_BUTTON_SIZE: f32 = 28.0 * UI_SCALE;
pub const TAB_ROW_HEIGHT: f32 = 36.0 * UI_SCALE;
/// Shared horizontal geometry for tab, split, and new-tab rows.
pub const SIDEBAR_ROW_PADDING_X: f32 = 12.0;
pub const SIDEBAR_ROW_ICON_SIZE: f32 = 16.0 * UI_SCALE;
pub const SIDEBAR_ROW_GAP: f32 = 8.0;
pub const SIDEBAR_ROW_VERTICAL_GAP: f32 = 8.0 * UI_SCALE;
pub const SIDEBAR_LIST_TOP: f32 = 52.0 * UI_SCALE;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SidebarRowRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

pub fn sidebar_row_rect(sidebar_width: f32, row_index: usize) -> SidebarRowRect {
    SidebarRowRect {
        left: SIDEBAR_ROW_PADDING_X,
        top: SIDEBAR_LIST_TOP + row_index as f32 * (TAB_ROW_HEIGHT + SIDEBAR_ROW_VERTICAL_GAP),
        width: (sidebar_width - 2.0 * SIDEBAR_ROW_PADDING_X).max(0.0),
        height: TAB_ROW_HEIGHT,
    }
}

pub const DEFAULT_PAGE_ZOOM_FACTOR: f64 = 0.8;

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
    let hover_tint = tint_rgb(0xffffff, accent, 0.35);
    SpaceTheme {
        base: tint_rgb(0x111216, accent, 0.24),
        surface,
        surface_hover: tint_rgb(surface, hover_tint, 0.12),
        border,
        border_hover: tint_rgb(border, 0xffffff, 0.16),
        accent,
        text: 0xe6e7eb,
        muted: 0x9fa3ae,
    }
}

/// Mixes the selected space colors into the accent used by the browser chrome.
/// The picker keeps each stop separately so users can add and remove colors.
pub fn space_theme_colors(base: u32, colors: &[u32]) -> SpaceTheme {
    if colors.is_empty() {
        return space_theme(base);
    }
    let mut total = [0_u32; 3];
    for color in colors {
        total[0] += (color >> 16) & 0xff;
        total[1] += (color >> 8) & 0xff;
        total[2] += color & 0xff;
    }
    let count = colors.len() as u32;
    let accent = (total[0] / count) << 16 | (total[1] / count) << 8 | (total[2] / count);
    space_theme(accent)
}

pub fn zoom_level_for_factor(factor: f64) -> f64 {
    factor.ln() / 1.2_f64.ln()
}

#[cfg(test)]
pub fn default_zoom_level() -> f64 {
    zoom_level_for_factor(DEFAULT_PAGE_ZOOM_FACTOR)
}

#[cfg(test)]
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
    fn default_zoom_is_eighty_percent() {
        assert!((1.2_f64.powf(default_zoom_level()) - 0.8).abs() < 1e-12);
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
    fn navigation_button_metrics_scale_with_ui() {
        assert_eq!(NAV_BUTTON_SIZE, 28.0 * UI_SCALE);
    }

    #[test]
    fn sidebar_rows_share_icon_and_spacing_metrics() {
        assert_eq!(SIDEBAR_ROW_PADDING_X, 12.0);
        assert_eq!(SIDEBAR_ROW_ICON_SIZE, 16.0 * UI_SCALE);
        assert_eq!(SIDEBAR_ROW_GAP, 8.0);
    }

    #[test]
    fn active_highlight_rect_matches_the_padded_row_rect() {
        let sidebar_width = 237.0;
        let row_index = 3;
        let highlight = sidebar_row_rect(sidebar_width, row_index);
        let row = SidebarRowRect {
            left: SIDEBAR_ROW_PADDING_X,
            top: SIDEBAR_LIST_TOP + row_index as f32 * (TAB_ROW_HEIGHT + SIDEBAR_ROW_VERTICAL_GAP),
            width: sidebar_width - 2.0 * SIDEBAR_ROW_PADDING_X,
            height: TAB_ROW_HEIGHT,
        };
        assert_eq!(highlight, row);
    }

    #[test]
    fn space_theme_interpolation_reaches_both_endpoints_and_midpoint() {
        let from = space_theme(0x9aa6ff);
        let to = space_theme(0x76d7b2);
        assert_eq!(interpolate_theme(from, to, 0.0), from);
        assert_eq!(interpolate_theme(from, to, 1.0), to);
        let middle = interpolate_theme(from, to, 0.5);
        assert_eq!(middle.accent, 0x88bfd9);
        assert_ne!(middle.base, from.base);
        assert_ne!(middle.base, to.base);
    }

    #[test]
    fn pack_theme_tokens_override_only_values_the_pack_declares() {
        let original = space_theme(0x9aa6ff);
        let pack = crate::plugin::ThemePack {
            mode: Some("dark".into()),
            tokens: std::collections::BTreeMap::from([
                ("base".into(), toml::Value::String("#101820".into())),
                ("accent".into(), toml::Value::Integer(0x336699)),
            ]),
            motion: Default::default(),
        };

        let result = apply_pack_theme_tokens(original, &[pack]);
        assert_eq!(result.base, 0x101820);
        assert_eq!(result.accent, 0x336699);
        assert_eq!(result.surface, original.surface);
        assert_eq!(result.text, original.text);
    }

    #[test]
    fn pack_theme_tokens_override_border_hover() {
        let original = space_theme(0x9aa6ff);
        let pack = crate::plugin::ThemePack {
            mode: None,
            tokens: std::collections::BTreeMap::from([(
                "border_hover".into(),
                toml::Value::String("#123456".into()),
            )]),
            motion: Default::default(),
        };

        let result = apply_pack_theme_tokens(original, &[pack]);
        assert_eq!(result.border_hover, 0x123456);
        assert_eq!(result.base, original.base);
    }

    #[test]
    fn pack_theme_colors_accept_hex_strings_and_rgb_integers() {
        assert_eq!(
            parse_theme_color(&toml::Value::from("#a1b2c3")),
            Some(0xa1b2c3)
        );
        assert_eq!(
            parse_theme_color(&toml::Value::from(0x123456)),
            Some(0x123456)
        );
        assert_eq!(parse_theme_color(&toml::Value::from("bad-color")), None);
        assert_eq!(parse_theme_color(&toml::Value::from(0x1_000000)), None);
    }
}

/// Derive surfaces and text from the space's appearance and tint controls.
pub fn theme_for_space(space: &crate::model::Space) -> SpaceTheme {
    let pack_themes = crate::plugin::active_theme_overrides();
    let mut theme = space_theme_colors(space.color, &space.theme_colors);
    let pack_mode = pack_themes
        .iter()
        .filter_map(|pack| pack.mode.as_deref())
        .last();
    let light = match pack_mode {
        Some("dark") => false,
        Some("light") => true,
        _ => space.theme_appearance == 1 || (space.theme_appearance == 0 && space.theme_auto_light),
    };
    let base = if light { 0xf1f2f5 } else { 0x111216 };
    theme.base = tint_rgb(base, theme.accent, space.theme_intensity);
    theme.surface = tint_rgb(
        if light { 0xffffff } else { 0x191a1f },
        theme.accent,
        space.theme_intensity * 0.75,
    );
    let hover_tint = if light {
        tint_rgb(0x000000, theme.accent, 0.25)
    } else {
        tint_rgb(0xffffff, theme.accent, 0.35)
    };
    theme.surface_hover = tint_rgb(theme.surface, hover_tint, 0.10);
    theme.border = tint_rgb(theme.base, if light { 0x000000 } else { 0xffffff }, 0.16);
    theme.border_hover = tint_rgb(theme.border, theme.accent, 0.30);
    theme.text = if light { 0x20232b } else { 0xe6e7eb };
    theme.muted = if light { 0x575b65 } else { 0x9fa3ae };
    apply_pack_theme_tokens(theme, &pack_themes)
}

fn apply_pack_theme_tokens(
    mut theme: SpaceTheme,
    packs: &[crate::plugin::ThemePack],
) -> SpaceTheme {
    for pack in packs {
        if let Some(value) = pack.tokens.get("base").and_then(parse_theme_color) {
            theme.base = value;
        }
        if let Some(value) = pack.tokens.get("surface").and_then(parse_theme_color) {
            theme.surface = value;
        }
        if let Some(value) = pack.tokens.get("surface_hover").and_then(parse_theme_color) {
            theme.surface_hover = value;
        }
        if let Some(value) = pack.tokens.get("border").and_then(parse_theme_color) {
            theme.border = value;
        }
        if let Some(value) = pack.tokens.get("border_hover").and_then(parse_theme_color) {
            theme.border_hover = value;
        }
        if let Some(value) = pack.tokens.get("accent").and_then(parse_theme_color) {
            theme.accent = value;
        }
        if let Some(value) = pack.tokens.get("text").and_then(parse_theme_color) {
            theme.text = value;
        }
        if let Some(value) = pack.tokens.get("muted").and_then(parse_theme_color) {
            theme.muted = value;
        }
    }
    theme
}

pub fn active_layout_preset() -> Option<crate::plugin::LayoutPack> {
    crate::plugin::active_layout_preset()
}

pub fn anarc_logo() -> Option<std::sync::Arc<gpui_kit::RenderImage>> {
    static LOGO: std::sync::OnceLock<Option<std::sync::Arc<gpui_kit::RenderImage>>> =
        std::sync::OnceLock::new();
    LOGO.get_or_init(|| {
        let bytes = include_bytes!("../../assets/anarc_logo.png");
        let decoded = image::load_from_memory(bytes).ok()?;
        let mut rgba = decoded.to_rgba8();
        for pixel in rgba.pixels_mut() {
            pixel.0.swap(0, 2);
        }
        let frame = image::Frame::new(rgba);
        Some(std::sync::Arc::new(gpui_kit::RenderImage::new(
            smallvec::smallvec![frame],
        )))
    })
    .clone()
}

pub fn active_topbar_height() -> f32 {
    match active_layout_preset()
        .as_ref()
        .and_then(|preset| preset.topbar.as_deref())
    {
        Some("hidden") => 0.0,
        Some("compact") => 40.0 * UI_SCALE,
        _ => TOPBAR_HEIGHT,
    }
}

pub fn active_card_radius() -> f32 {
    let pack_themes = crate::plugin::active_theme_overrides();
    for pack in pack_themes.iter().rev() {
        if let Some(val) = pack.tokens.get("card_radius") {
            if let Some(n) = val
                .as_float()
                .or_else(|| val.as_integer().map(|i| i as f64))
            {
                return (n as f32) * UI_SCALE;
            }
        }
    }
    16.0 * UI_SCALE
}

fn parse_theme_color(value: &toml::Value) -> Option<u32> {
    if let Some(integer) = value.as_integer() {
        return u32::try_from(integer)
            .ok()
            .filter(|color| *color <= 0x00ff_ffff);
    }
    let value = value.as_str()?.trim();
    let value = value
        .strip_prefix('#')
        .or_else(|| value.strip_prefix("0x"))
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    (value.len() == 6)
        .then(|| u32::from_str_radix(value, 16).ok())
        .flatten()
}

// Reuse the sparse grain texture when only its opacity or the theme colors change.
// Four bounded entries cover the window, sidebar, picker, and a transition surface.
thread_local! {
    static GRAIN_TEXTURES: std::cell::RefCell<Vec<((u32, u32), std::sync::Arc<gpui_kit::RenderImage>)>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

fn grain_texture(
    width: u32,
    height: u32,
    window: &mut gpui_kit::Window,
) -> std::sync::Arc<gpui_kit::RenderImage> {
    GRAIN_TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        let key = (width, height);
        if let Some(index) = cache.iter().position(|(size, _)| *size == key) {
            let entry = cache.remove(index);
            let image = entry.1.clone();
            cache.push(entry);
            return image;
        }
        let mut pixels = image::RgbaImage::new(width, height);
        for index in 0..1536_u32 {
            let hash = index.wrapping_mul(2654435761);
            let x = ((hash & 65535) as f32 / 65535.0 * width as f32).round() as u32;
            let y =
                ((hash.rotate_left(13) & 65535) as f32 / 65535.0 * height as f32).round() as u32;
            if x < width && y < height {
                pixels.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        let image = std::sync::Arc::new(gpui_kit::RenderImage::new(smallvec::smallvec![
            image::Frame::new(pixels)
        ]));
        if cache.len() == 4 {
            let (_, retired) = cache.remove(0);
            let _ = window.drop_image(retired);
        }
        cache.push((key, image.clone()));
        image
    })
}

pub fn space_backdrop(space: &crate::model::Space) -> impl gpui_kit::IntoElement {
    space_backdrop_rounded(space, 0.0)
}

pub fn space_backdrop_rounded(
    space: &crate::model::Space,
    radius: f32,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{
        Bounds, Corners, ParentElement, Styled, canvas, div, fill, linear_color_stop,
        linear_gradient, point, prelude::FluentBuilder, px, rgb, size,
    };
    let colors = if space.theme_colors.is_empty() {
        vec![space.color]
    } else {
        space.theme_colors.clone()
    };
    let intensity = space.theme_intensity;
    let grain = space.theme_grain;
    let base =
        if space.theme_appearance == 1 || (space.theme_appearance == 0 && space.theme_auto_light) {
            0xf1f2f5
        } else {
            0x111216
        };
    let gradient = canvas(
        |_, _, _| (),
        move |area, _, window, _| {
            let segments = colors.len().saturating_sub(1).max(1);
            let height = f32::from(area.size.height) / segments as f32;
            for index in 0..segments {
                let from = tint_rgb(base, colors[index], intensity);
                let to = tint_rgb(base, colors[(index + 1).min(colors.len() - 1)], intensity);
                let mut quad = fill(
                    Bounds::new(
                        area.origin + point(px(0.0), px(index as f32 * height)),
                        size(area.size.width, px(height)),
                    ),
                    linear_gradient(
                        180.0,
                        linear_color_stop(rgb(from), 0.0),
                        linear_color_stop(rgb(to), 1.0),
                    ),
                );
                quad.corner_radii = Corners {
                    top_left: px(if index == 0 { radius } else { 0.0 }),
                    top_right: px(if index == 0 { radius } else { 0.0 }),
                    bottom_left: px(if index + 1 == segments { radius } else { 0.0 }),
                    bottom_right: px(if index + 1 == segments { radius } else { 0.0 }),
                };
                window.paint_quad(quad);
            }
        },
    )
    .absolute()
    .inset_0();
    let texture = canvas(
        |_, _, _| (),
        move |area, _, window, _| {
            // Cap each allocation at 16 MiB even on unusually large windows.
            let width = (f32::from(area.size.width).ceil() as u32).clamp(1, 2048);
            let height = (f32::from(area.size.height).ceil() as u32).clamp(1, 2048);
            let image = grain_texture(width, height, window);
            let _ = window.paint_image(area, area, px(radius).into(), image, 0, false);
        },
    )
    .absolute()
    .inset_0();
    div()
        .absolute()
        .inset_0()
        .child(gradient)
        .when(grain > 0.01, |d| {
            d.child(
                div()
                    .absolute()
                    .inset_0()
                    .opacity((grain * 0.48).clamp(0.0, 0.48))
                    .child(texture),
            )
        })
}
