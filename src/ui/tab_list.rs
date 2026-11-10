use crate::ui::visual::{self, UI_SCALE};
use gpui_kit::{assets::IconName, component::Icon, div, prelude::*, px, rgb};
use std::sync::Arc;

pub fn group_label(first: &str, second: &str) -> String {
    format!("{first} | {second}")
}

pub fn favicon(favicon: Option<Arc<gpui_kit::RenderImage>>) -> gpui_kit::AnyElement {
    if let Some(favicon) = favicon {
        div()
            .size(px(visual::SIDEBAR_ROW_ICON_SIZE))
            .rounded(px(3.0 * UI_SCALE))
            .overflow_hidden()
            .child(gpui_kit::img(favicon).size_full())
            .into_any_element()
    } else {
        div()
            .size(px(visual::SIDEBAR_ROW_ICON_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0x7e828d))
            .child(Icon::new(IconName::Globe).size(px(14.0 * UI_SCALE)))
            .into_any_element()
    }
}

pub fn favorite_favicon(favicon: Option<Arc<gpui_kit::RenderImage>>) -> gpui_kit::AnyElement {
    if let Some(favicon) = favicon {
        div()
            .size(px(20.0 * UI_SCALE))
            .rounded(px(4.0 * UI_SCALE))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .child(gpui_kit::img(favicon).size_full())
            .into_any_element()
    } else {
        div()
            .size(px(20.0 * UI_SCALE))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0x7e828d))
            .child(Icon::new(IconName::Globe).size(px(16.0 * UI_SCALE)))
            .into_any_element()
    }
}

/// Shows both panes inside the same fixed-width icon slot used by every sidebar row.
pub fn split_favicon(
    first: Option<Arc<gpui_kit::RenderImage>>,
    second: Option<Arc<gpui_kit::RenderImage>>,
) -> gpui_kit::AnyElement {
    let small_favicon = |favicon: Option<Arc<gpui_kit::RenderImage>>| {
        if let Some(favicon) = favicon {
            div()
                .size(px(7.0 * UI_SCALE))
                .rounded(px(2.0 * UI_SCALE))
                .overflow_hidden()
                .child(gpui_kit::img(favicon).size_full())
                .into_any_element()
        } else {
            div()
                .size(px(7.0 * UI_SCALE))
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0x7e828d))
                .child(Icon::new(IconName::Globe).size(px(7.0 * UI_SCALE)))
                .into_any_element()
        }
    };

    div()
        .size(px(visual::SIDEBAR_ROW_ICON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(1.0 * UI_SCALE))
        .child(small_favicon(first))
        .child(small_favicon(second))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_group_label_joins_titles_in_pane_order() {
        assert_eq!(group_label("First", "Second"), "First | Second");
    }
}
