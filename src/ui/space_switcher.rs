use crate::ui::visual::{SpaceTheme, UI_SCALE};
use gpui_kit::{assets::IconName, component::Icon, div, prelude::*, px, rgb};

pub fn render_space_icon(name: &str) -> Icon {
    let icon_name = match name {
        "globe" => IconName::Globe,
        "compass" => IconName::Compass,
        "code" => IconName::Code,
        "terminal" => IconName::Terminal,
        "folder" => IconName::Folder,
        "book" => IconName::BookOpen,
        "briefcase" => IconName::Briefcase,
        "layers" => IconName::Layers,
        "box" => IconName::Box,
        "sparkles" => IconName::Sparkles,
        "star" => IconName::Star,
        "heart" => IconName::Heart,
        "bookmark" => IconName::Bookmark,
        "music" => IconName::Music,
        "gamepad" => IconName::Gamepad2,
        "cpu" => IconName::Cpu,
        "zap" => IconName::Zap,
        "hash" => IconName::Hash,
        "shield" => IconName::Shield,
        "palette" => IconName::Palette,
        "rocket" => IconName::Rocket,
        "bell" => IconName::Bell,
        _ => IconName::Compass,
    };
    Icon::new(icon_name)
}

pub fn space_icon_button(icon: &str, is_active: bool, theme: SpaceTheme) -> impl IntoElement {
    let bg_color = if is_active { theme.surface } else { theme.base };
    let border_color = if is_active {
        theme.border_hover
    } else {
        theme.border
    };
    let text_color = if is_active { theme.text } else { theme.muted };
    div()
        .id("space-switcher-icon")
        .size(px(28.0 * UI_SCALE))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(bg_color))
        .border_1()
        .border_color(rgb(border_color))
        .text_color(rgb(text_color))
        .active(|style| style.opacity(0.92))
        .hover(move |style| {
            style
                .bg(rgb(theme.surface_hover))
                .text_color(rgb(theme.text))
        })
        .cursor_pointer()
        .child(render_space_icon(icon))
}

pub fn add_space_button(theme: SpaceTheme) -> impl IntoElement {
    div()
        .id("add-space-button")
        .size(px(28.0 * UI_SCALE))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(theme.surface))
        .text_color(rgb(theme.muted))
        .active(|style| style.opacity(0.92))
        .hover(move |style| {
            style
                .bg(rgb(theme.surface_hover))
                .text_color(rgb(theme.text))
        })
        .cursor_pointer()
        .child(Icon::new(IconName::Plus))
}

pub fn settings_button(theme: SpaceTheme) -> impl IntoElement {
    div()
        .id("settings-button")
        .size(px(28.0 * UI_SCALE))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(theme.muted))
        .active(|style| style.opacity(0.92))
        .hover(move |style| {
            style
                .bg(rgb(theme.surface_hover))
                .text_color(rgb(theme.text))
        })
        .cursor_pointer()
        .child(Icon::new(IconName::Settings))
}

pub fn downloads_button(theme: SpaceTheme) -> impl IntoElement {
    div()
        .id("downloads-button")
        .size(px(28.0 * UI_SCALE))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(theme.muted))
        .active(|style| style.opacity(0.92))
        .hover(move |style| {
            style
                .bg(rgb(theme.surface_hover))
                .text_color(rgb(theme.text))
        })
        .cursor_pointer()
        .child(Icon::new(IconName::Download))
}
