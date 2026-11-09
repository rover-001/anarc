use gpui_kit::{FontWeight, div, prelude::*, px, rgb};

pub fn section_label(label: &str) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(0x6f737d))
        .child(label.to_owned())
}

pub fn new_tab_button() -> impl IntoElement {
    div()
        .id("new-tab")
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .rounded_lg()
        .text_sm()
        .text_color(rgb(0xb9bbc2))
        .hover(|style| style.bg(rgb(0x282b31)))
        .child(div().size(px(16.0)).text_lg().child("+"))
        .child("New tab")
}
