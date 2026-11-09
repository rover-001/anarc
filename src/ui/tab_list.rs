use gpui_kit::{div, prelude::*, px, rgb};
use std::sync::Arc;

pub fn tab(
    title: &str,
    selected: bool,
    favicon: Option<Arc<gpui_kit::RenderImage>>,
) -> impl IntoElement {
    let background = if selected { 0x30343c } else { 0x202227 };
    let foreground = if selected { 0xf0f1f4 } else { 0xa6a9b1 };
    let icon = if let Some(favicon) = favicon {
        div()
            .size(px(16.0))
            .rounded_md()
            .overflow_hidden()
            .child(gpui_kit::img(favicon).size_full())
            .into_any_element()
    } else {
        div()
            .w(px(14.0))
            .flex()
            .items_center()
            .justify_center()
            .child(div().size(px(7.0)).rounded_full().bg(rgb(if selected {
                0x9aa6ff
            } else {
                0x565a64
            })))
            .into_any_element()
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .rounded_lg()
        .bg(rgb(background))
        .hover(|style| style.bg(rgb(0x2a2d34)))
        .child(icon)
        .child(
            div()
                .flex_1()
                .text_sm()
                .text_color(rgb(foreground))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title.to_owned()),
        )
}
