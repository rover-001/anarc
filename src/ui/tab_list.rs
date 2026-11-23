use crate::ui::visual::{self, UI_SCALE};
use gpui_kit::{assets::IconName, component::Icon, div, prelude::*, px, rgb};
use std::sync::Arc;

pub fn group_label(first: &str, second: &str) -> String {
    format!("{first} | {second}")
}

/// Label for a split group of any size. Titles are shown in pane order and
/// longer groups are abbreviated so the sidebar row stays one line.
pub fn group_labels(titles: &[String]) -> String {
    match titles {
        [] => "Split view".to_owned(),
        [only] => only.clone(),
        [first, second] => group_label(first, second),
        [first, second, rest @ ..] => format!("{first} | {second} | +{}", rest.len()),
    }
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

/// Favorite tiles show a star placeholder when a tab has no favicon, matching
/// the pin glyph used by the pinned section.
pub fn favorite_favicon(image: Option<Arc<gpui_kit::RenderImage>>) -> gpui_kit::AnyElement {
    if image.is_some() {
        favicon(image)
    } else {
        div()
            .size(px(20.0 * UI_SCALE))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0x7e828d))
            .child(Icon::new(IconName::StarFill).size(px(16.0 * UI_SCALE)))
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
