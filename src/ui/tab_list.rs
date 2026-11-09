use crate::ui::visual::UI_SCALE;
use gpui_kit::{assets::IconName, component::Icon, div, prelude::*, px, rgb};
use std::sync::Arc;

pub fn group_label(first: &str, second: &str) -> String {
    format!("{first} | {second}")
}

pub fn favicon(favicon: Option<Arc<gpui_kit::RenderImage>>) -> gpui_kit::AnyElement {
    if let Some(favicon) = favicon {
        div()
            .size(px(16.0 * UI_SCALE))
            .rounded(px(3.0 * UI_SCALE))
            .overflow_hidden()
            .child(gpui_kit::img(favicon).size_full())
            .into_any_element()
    } else {
        div()
            .w(px(14.0 * UI_SCALE))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0x7e828d))
            .child(Icon::new(IconName::Globe).size(px(14.0 * UI_SCALE)))
            .into_any_element()
    }
}

pub fn trailing_icon(icon: IconName, color: u32) -> impl IntoElement {
    div()
        .w(px(20.0 * UI_SCALE))
        .h(px(20.0 * UI_SCALE))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(color))
        .child(Icon::new(icon).size(px(14.0 * UI_SCALE)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_group_label_joins_titles_in_pane_order() {
        assert_eq!(group_label("First", "Second"), "First | Second");
    }
}
