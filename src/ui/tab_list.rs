use gpui_kit::{div, prelude::*, px, rgb};

pub fn tab(title: &str, selected: bool) -> impl IntoElement {
    let background = if selected { 0x30343c } else { 0x202227 };
    let foreground = if selected { 0xf0f1f4 } else { 0xa6a9b1 };
    div()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .rounded_lg()
        .bg(rgb(background))
        .hover(|style| style.bg(rgb(0x2a2d34)))
        .child(div().size(px(7.0)).rounded_full().bg(rgb(if selected {
            0x9aa6ff
        } else {
            0x565a64
        })))
        .child(
            div()
                .text_sm()
                .text_color(rgb(foreground))
                .overflow_hidden()
                .child(title.to_owned()),
        )
}
