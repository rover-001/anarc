use gpui_kit::{FontWeight, div, prelude::*, px, rgb};

pub fn row(name: &str, accent: u32) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_3()
        .rounded_lg()
        .hover(|style| style.bg(rgb(0x282b31)))
        .child(div().size(px(10.0)).rounded_full().bg(rgb(accent)))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0xe6e7eb))
                .child(name.to_owned()),
        )
        .child(
            div()
                .ml_auto()
                .text_xs()
                .text_color(rgb(0x777b85))
                .child("⌄"),
        )
}
