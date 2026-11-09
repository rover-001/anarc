use crate::ui::visual::{SpaceTheme, UI_SCALE};
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
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .rounded_lg()
        .text_sm()
        .text_color(rgb(0xb9bbc2))
        .hover(move |style| style.bg(rgb(theme.surface_hover)))
        .child(
            div()
                .w(px(14.0 * UI_SCALE))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(IconName::Plus)),
        )
        .child(div().flex_1().text_sm().child("New tab"))
}
