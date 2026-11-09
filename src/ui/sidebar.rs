use crate::ui::visual::{self, SpaceTheme};
use gpui_kit::{FontWeight, assets::IconName, component::Icon, div, prelude::*, px, rgb};

#[allow(dead_code)]
pub fn section_label(label: &str) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(0x6f737d))
        .child(label.to_owned())
}

pub fn new_tab_button(theme: SpaceTheme) -> impl IntoElement {
    div()
        .id("new-tab")
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .gap(px(visual::SIDEBAR_ROW_GAP))
        .px(px(visual::SIDEBAR_ROW_PADDING_X))
        .rounded_lg()
        .text_sm()
        .text_color(rgb(theme.text))
        .active(|style| style.opacity(0.92))
        .hover(move |style| style.bg(rgb(theme.surface_hover)))
        .child(
            div()
                .size(px(visual::SIDEBAR_ROW_ICON_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(IconName::Plus)),
        )
        .child(div().flex_1().text_sm().child("New tab"))
}
