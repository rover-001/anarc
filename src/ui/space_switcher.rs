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

pub fn space_icon_button(icon: &str, is_active: bool, _accent: u32) -> impl IntoElement {
    let bg_color = if is_active { 0x333742 } else { 0x1f2127 };
    let border_color = if is_active { 0x4a4f5d } else { 0x282b33 };
    let text_color = if is_active { 0xf0f1f4 } else { 0x8b8f9a };
    div()
        .size(px(28.0))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(bg_color))
        .border_1()
        .border_color(rgb(border_color))
        .text_color(rgb(text_color))
        .hover(|style| style.bg(rgb(0x3a3e4a)).text_color(rgb(0xffffff)))
        .cursor_pointer()
        .child(render_space_icon(icon))
}

pub fn add_space_button() -> impl IntoElement {
    div()
        .size(px(28.0))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x1c1e23))
        .text_color(rgb(0x7e828d))
        .hover(|style| style.bg(rgb(0x282b33)).text_color(rgb(0xe6e7eb)))
        .cursor_pointer()
        .child(Icon::new(IconName::Plus))
}

pub fn settings_button() -> impl IntoElement {
    div()
        .size(px(28.0))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(0x7e828d))
        .hover(|style| style.bg(rgb(0x282b33)).text_color(rgb(0xe6e7eb)))
        .cursor_pointer()
        .child(Icon::new(IconName::Settings))
}

pub fn downloads_button() -> impl IntoElement {
    div()
        .size(px(28.0))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(0x7e828d))
        .hover(|style| style.bg(rgb(0x282b33)).text_color(rgb(0xe6e7eb)))
        .cursor_pointer()
        .child(Icon::new(IconName::Download))
}
