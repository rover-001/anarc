use gpui_kit::{
    Context, Render, Window, assets::IconName, component::Icon, div, prelude::*, px, rgb,
};

use crate::app::Lumen;
use crate::ui::visual::UI_SCALE;
use crate::ui::{tab_list, visual};

/// Payload carried by a sidebar tab while it is dragged.
#[derive(Clone)]
pub(crate) struct TabDrag {
    pub(crate) tab_id: String,
    pub(crate) favicon: Option<std::sync::Arc<gpui_kit::RenderImage>>,
}

/// Ghost rendered under the pointer while a tab is dragged. Titles are
/// intentionally omitted so long page names never widen the drag image.
pub(crate) struct TabDragGhost {
    pub(crate) favicon: Option<std::sync::Arc<gpui_kit::RenderImage>>,
    pub(crate) pointer_offset: gpui_kit::Point<gpui_kit::Pixels>,
}

impl Render for TabDragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let half_size = px(15.0 * UI_SCALE);
        div()
            .pl(self.pointer_offset.x - half_size)
            .pt(self.pointer_offset.y - half_size)
            .child(
                div()
                    .size(px(30.0 * UI_SCALE))
                    .rounded_lg()
                    .bg(rgb(0x1f2128))
                    .border_1()
                    .border_color(rgb(0x3a3d47))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(tab_list::favicon(self.favicon.clone())),
            )
    }
}

pub(crate) fn tab_context_menu_position(
    event: &gpui_kit::MouseDownEvent,
    window: &Window,
) -> (f32, f32, f32, f32) {
    let bounds = window.viewport_size();
    let width = f32::from(bounds.width);
    let height = f32::from(bounds.height);
    let margin = 8.0 * UI_SCALE;
    let menu_width = (240.0 * UI_SCALE).min((width - margin * 2.0).max(1.0));

    let x = (f32::from(event.position.x) + margin)
        .clamp(margin, (width - menu_width - margin).max(margin));
    let y = f32::from(event.position.y).clamp(margin, (height - 96.0 * UI_SCALE).max(margin));
    let menu_height = (height - y - margin).max(1.0);
    (x, y, menu_width, menu_height)
}

/// Compact sidebar row used by both the pinned section and expanded folders.
#[allow(clippy::too_many_arguments)]
pub(crate) fn section_tab_row(
    tab_id: String,
    title: String,
    favicon: Option<std::sync::Arc<gpui_kit::RenderImage>>,
    selected: bool,
    show_selection_background: bool,
    theme: visual::SpaceTheme,
    indent: f32,
    cx: &mut Context<Lumen>,
) -> impl IntoElement {
    let click_id = tab_id.clone();
    let close_id = tab_id.clone();
    let context_id = tab_id.clone();
    let before_target = tab_id.clone();
    let after_target = tab_id.clone();
    let row_group = format!("section-row-{tab_id}");
    let close_group = row_group.clone();
    let drag_value = TabDrag {
        tab_id: tab_id.clone(),
        favicon: favicon.clone(),
    };
    div()
        .id(format!("section-tab-{tab_id}"))
        .relative()
        .w_full()
        .h(px(visual::TAB_ROW_HEIGHT))
        .cursor_pointer()
        .active(|style| style.opacity(0.92))
        .group(row_group)
        .child(
            div()
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .gap(px(visual::SIDEBAR_ROW_GAP))
                .pl(px(visual::SIDEBAR_ROW_PADDING_X + indent))
                .pr(px(visual::SIDEBAR_ROW_PADDING_X))
                .rounded_lg()
                .when(selected && show_selection_background, |row| {
                    row.bg(rgb(theme.surface))
                })
                .hover(|row| row.bg(rgb(theme.surface_hover)))
                .child(tab_list::favicon(favicon))
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(rgb(if selected { theme.text } else { theme.muted }))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title),
                )
                .child(
                    div()
                        .id(format!("close-section-tab-{tab_id}"))
                        .size(px(16.0 * UI_SCALE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(0.0)
                        .group_hover(close_group, |style| style.opacity(1.0))
                        .text_color(rgb(theme.muted))
                        .child(Icon::new(IconName::X).size(px(14.0 * UI_SCALE)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_section_tab(&close_id);
                            cx.stop_propagation();
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .bottom_0()
                .w_full()
                .h(px(5.0 * UI_SCALE))
                .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                    this.drop_tab_in_target_section(&drag.tab_id, &after_target, true);
                    cx.notify();
                })),
        )
        .drag_over::<TabDrag>(move |style, _, _, _| style.bg(rgb(theme.surface_hover)))
        .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
            this.drop_tab_in_target_section(&drag.tab_id, &before_target, false);
            cx.notify();
        }))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.activate_tab_in_active_space(&click_id);
            cx.notify();
        }))
        .on_mouse_down(
            gpui_kit::MouseButton::Right,
            cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                let (x, y, width, height) = tab_context_menu_position(event, window);
                this.open_tab_context_menu(context_id.clone(), x, y, width, height);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_drag(
            drag_value,
            |drag: &TabDrag, pointer_offset, _window, cx: &mut gpui_kit::App| {
                let favicon = drag.favicon.clone();
                cx.new(|_| TabDragGhost {
                    favicon,
                    pointer_offset,
                })
            },
        )
}
