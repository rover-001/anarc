use crate::ui::visual::{SpaceTheme, UI_SCALE};
use gpui_kit::{assets::IconName, component::Icon, div, prelude::*, px, rgb};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceSwitchState {
    pub from_index: usize,
    pub to_index: usize,
    /// Outgoing content travels left for a switch to the right, and vice versa.
    pub outgoing_direction: f32,
    pub incoming_opacity_start: f32,
    pub incoming_offset_start: f32,
    pub outgoing_opacity_start: f32,
    pub outgoing_offset_start: f32,
}

impl SpaceSwitchState {
    pub fn new(from_index: usize, to_index: usize) -> Self {
        Self {
            from_index,
            to_index,
            outgoing_direction: if to_index >= from_index { -1.0 } else { 1.0 },
            incoming_opacity_start: 0.0,
            incoming_offset_start: -if to_index >= from_index { -24.0 } else { 24.0 },
            outgoing_opacity_start: 1.0,
            outgoing_offset_start: 0.0,
        }
    }

    pub fn retarget(self, next_index: usize, progress: f32, keep_incoming: bool) -> Self {
        let progress = progress.clamp(0.0, 1.0);
        let from_index = if keep_incoming {
            self.to_index
        } else {
            self.from_index
        };
        let outgoing_opacity_start = if keep_incoming {
            progress
        } else {
            1.0 - progress
        };
        let outgoing_offset_start = if keep_incoming {
            -self.outgoing_direction * 24.0 * (1.0 - progress)
        } else {
            self.outgoing_direction * 24.0 * progress
        };
        let outgoing_direction = if next_index >= from_index { -1.0 } else { 1.0 };
        Self {
            from_index,
            to_index: next_index,
            outgoing_direction,
            incoming_opacity_start: 0.0,
            incoming_offset_start: -outgoing_direction * 24.0,
            outgoing_opacity_start,
            outgoing_offset_start,
        }
    }
}

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

pub fn space_dock_icon_button(icon: &str, is_active: bool, theme: SpaceTheme) -> impl IntoElement {
    div()
        .id("space-switcher-dock-icon")
        .size(px(28.0 * UI_SCALE))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(if is_active { theme.text } else { theme.muted }))
        .active(|style| style.opacity(0.92))
        .hover(move |style| style.text_color(rgb(theme.text)))
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

pub fn plugins_button(theme: SpaceTheme) -> impl IntoElement {
    div()
        .id("plugins-button")
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
        .child(Icon::new(IconName::Puzzle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_switch_direction_follows_dock_order() {
        assert_eq!(SpaceSwitchState::new(0, 1).outgoing_direction, -1.0);
        assert_eq!(SpaceSwitchState::new(2, 0).outgoing_direction, 1.0);
    }

    #[test]
    fn retarget_keeps_the_incoming_layer_position_and_opacity() {
        let state = SpaceSwitchState::new(0, 1).retarget(2, 0.6, true);
        assert_eq!(state.from_index, 1);
        assert_eq!(state.to_index, 2);
        assert_eq!(state.outgoing_direction, -1.0);
        assert_eq!(state.incoming_opacity_start, 0.0);
        assert_eq!(state.incoming_offset_start, 24.0);
        assert!((state.outgoing_opacity_start - 0.6).abs() < f32::EPSILON);
        assert!((state.outgoing_offset_start - 9.6).abs() < 1e-4);
    }

    #[test]
    fn early_retarget_keeps_the_visible_outgoing_space() {
        let state = SpaceSwitchState::new(0, 1).retarget(2, 0.2, false);
        assert_eq!(state.from_index, 0);
        assert_eq!(state.to_index, 2);
        assert_eq!(state.outgoing_opacity_start, 0.8);
        assert!((state.outgoing_offset_start + 4.8).abs() < 1e-4);
    }
}
