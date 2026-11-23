use gpui_kit::{
    AnimationExt as _, Context, Render, SpringAnimation, Window, assets::IconName,
    component::Icon, div, prelude::*, px, relative, rgb,
};

use crate::app::Lumen;
use crate::ui::visual::UI_SCALE;
use crate::ui::{tab_list, visual};

/// Payload carried by a sidebar tab while it is dragged.
#[derive(Clone)]
pub(crate) struct TabDrag {
    pub(crate) tab_id: String,
    pub(crate) favicon: Option<std::sync::Arc<gpui_kit::RenderImage>>,
    pub(crate) source_kept: bool,
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
        source_kept: false,
    };
    let app_view = cx.weak_entity();
    let drag_over_view = cx.weak_entity();
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
                    row.bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
                })
                .when(!(selected && show_selection_background), |row| {
                    row.hover(|row| row.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                })
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
        .drag_over::<TabDrag>(move |style, _, _, app| {
            drag_over_view
                .update(app, |this, cx| {
                    if this.kept_drag_hovered_tab.take().is_some()
                        || this.kept_drag_hover_after
                        || this.kept_section_drag_hovered
                    {
                        this.kept_drag_hover_after = false;
                        this.kept_section_drag_hovered = false;
                        cx.notify();
                    }
                })
                .ok();
            style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
        })
        .on_mouse_move(cx.listener(|this, _, _, cx| {
            if cx.has_active_drag()
                && (this.kept_drag_hovered_tab.take().is_some()
                    || this.kept_drag_hover_after
                    || this.kept_section_drag_hovered)
            {
                this.kept_drag_hover_after = false;
                this.kept_section_drag_hovered = false;
                cx.notify();
            }
        }))
        .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
            this.drop_tab_in_target_section(&drag.tab_id, &before_target, false);
            cx.notify();
        }))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.activate_tab_in_active_space(&click_id);
            this.focus_browser(window, cx);
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
            move |drag: &TabDrag, pointer_offset, _window, cx: &mut gpui_kit::App| {
                app_view
                    .update(cx, |this, cx| {
                        this.kept_drag_source_id = drag.source_kept.then(|| drag.tab_id.clone());
                        this.kept_drag_hovered_tab = None;
                        this.kept_drag_hover_after = false;
                        this.kept_section_drag_hovered = false;
                        cx.notify();
                    })
                    .ok();
                let favicon = drag.favicon.clone();
                cx.new(|_| TabDragGhost {
                    favicon,
                    pointer_offset,
                })
            },
        )
}

/// A kept tab (favorite/pinned are the same section). The favicon is the only
/// content — no title, no close icon. A lone tab fills the whole row; two or
/// three share it as equal-width boxes, and further tabs wrap to new rows.
/// Dropping another tab on a box joins it to this section.
pub(crate) fn kept_tab_box(
    tab_id: String,
    title: String,
    favicon: Option<std::sync::Arc<gpui_kit::RenderImage>>,
    selected: bool,
    expand: bool,
    theme: visual::SpaceTheme,
    cx: &mut Context<Lumen>,
) -> impl IntoElement {
    let click_id = tab_id.clone();
    let context_id = tab_id.clone();
    let before_target = tab_id.clone();
    let after_target = tab_id.clone();
    let before_hover_target = before_target.clone();
    let after_hover_target = after_target.clone();
    let before_drop_target = before_target.clone();
    let after_drop_target = after_target.clone();
    let before_hover_view = cx.weak_entity();
    let after_hover_view = cx.weak_entity();
    let drag_value = TabDrag {
        tab_id: tab_id.clone(),
        favicon: favicon.clone(),
        source_kept: true,
    };
    let app_view = cx.weak_entity();
    // Zen-style tiles: soft translucent fill, generous rounding, and an
    // accent-tinted active state that reads against every space theme.
    let tile = visual::tint_rgb(theme.base, theme.surface, 0.40);
    let tile_hover = visual::tint_rgb(theme.base, theme.surface_hover, 0.58);
    let tile_active = visual::tint_rgb(theme.base, theme.accent, 0.28);
    let drop_hint = visual::tint_rgb(theme.base, theme.accent, 0.55);
    div()
        .id(format!("kept-tab-box-{tab_id}"))
        .relative()
        .h(px(visual::TAB_ROW_HEIGHT))
        .when(expand, |el| el.w_full())
        .when(!expand, |el| el.flex_1())
        .cursor_pointer()
        .active(|style| style.opacity(0.92))
        .rounded_xl()
        .overflow_hidden()
        .flex()
        .items_center()
        .when(expand, |el| el.px(px(visual::SIDEBAR_ROW_PADDING_X)))
        .when(!expand, |el| {
            el.justify_center().px(px(2.0 * UI_SCALE))
        })
        .bg(if selected {
            visual::translucent_surface(tile_active, 0x40)
        } else {
            visual::translucent_surface(tile, 0x30)
        })
        .hover(|el| el.bg(visual::translucent_surface(tile_hover, 0x50)))
        .child(tab_list::favorite_favicon(favicon.clone()))
        .when(expand, |el| {
            el.child(
                div()
                    .flex_1()
                    .pl(px(4.0 * UI_SCALE))
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(title),
            )
        })
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .bottom_0()
                .right(px(16.0 * UI_SCALE))
                .rounded_lg()
                .drag_over::<TabDrag>(move |style, _, _, app| {
                    before_hover_view
                        .update(app, |this, cx| {
                            if this.kept_drag_hovered_tab.as_deref()
                                != Some(before_hover_target.as_str())
                                || this.kept_drag_hover_after
                            {
                                this.kept_drag_hovered_tab = Some(before_hover_target.clone());
                                this.kept_drag_hover_after = false;
                                this.kept_section_drag_hovered = true;
                                cx.notify();
                            }
                        })
                        .ok();
                    style
                })
                .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                    this.kept_drag_hovered_tab = None;
                    this.kept_drag_hover_after = false;
                    this.kept_drag_source_id = None;
                    this.kept_section_drag_hovered = false;
                    this.drop_tab_in_target_section(&drag.tab_id, &before_drop_target, false);
                    cx.notify();
                })),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(16.0 * UI_SCALE))
                .rounded_lg()
                .drag_over::<TabDrag>(move |style, _, _, app| {
                    after_hover_view
                        .update(app, |this, cx| {
                            if this.kept_drag_hovered_tab.as_deref()
                                != Some(after_hover_target.as_str())
                                || !this.kept_drag_hover_after
                            {
                                this.kept_drag_hovered_tab = Some(after_hover_target.clone());
                                this.kept_drag_hover_after = true;
                                this.kept_section_drag_hovered = true;
                                cx.notify();
                            }
                        })
                        .ok();
                    style.bg(visual::translucent_surface(drop_hint, 0x30))
                })
                .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                    this.kept_drag_hovered_tab = None;
                    this.kept_drag_hover_after = false;
                    this.kept_drag_source_id = None;
                    this.kept_section_drag_hovered = false;
                    this.drop_tab_in_target_section(&drag.tab_id, &after_drop_target, true);
                    cx.notify();
                })),
        )
        .drag_over::<TabDrag>(move |style, _, _, _| style.bg(rgb(tile_hover)))
        .on_mouse_exit(cx.listener(move |this, _, _, cx| {
            if this.kept_drag_hovered_tab.as_deref() == Some(before_target.as_str())
                || this.kept_drag_hovered_tab.as_deref() == Some(after_target.as_str())
            {
                this.kept_drag_hovered_tab = None;
                this.kept_drag_hover_after = false;
                cx.notify();
            }
        }))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.activate_tab_in_active_space(&click_id);
            this.focus_browser(window, cx);
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
            move |drag: &TabDrag, pointer_offset, _window, cx: &mut gpui_kit::App| {
                app_view
                    .update(cx, |this, cx| {
                        this.kept_drag_source_id = drag.source_kept.then(|| drag.tab_id.clone());
                        this.kept_drag_hovered_tab = None;
                        this.kept_drag_hover_after = false;
                        this.kept_section_drag_hovered = false;
                        cx.notify();
                    })
                    .ok();
                let favicon = drag.favicon.clone();
                cx.new(|_| TabDragGhost {
                    favicon,
                    pointer_offset,
                })
            },
        )
}

/// Empty slot shown only while a tab is being dragged. It shrinks the existing
/// boxes live and previews where the dragged tab will land in this row.
pub(crate) fn kept_drop_slot(
    slot_id: String,
    group: Option<String>,
    anchor: String,
    after: bool,
    slot_fraction: f32,
    active: bool,
    theme: visual::SpaceTheme,
    cx: &mut Context<Lumen>,
) -> impl IntoElement {
    let hover_view = cx.weak_entity();
    let hover_anchor = anchor.clone();
    let exit_anchor = anchor.clone();
    div()
        .id(format!("kept-drop-slot-{slot_id}"))
        .flex_none()
        .h(px(visual::TAB_ROW_HEIGHT))
        .rounded_xl()
        .border_1()
        .border_dashed()
        .border_color(rgb(theme.border_hover))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(theme.muted))
        .drag_over::<TabDrag>(move |style, _, _, app| {
            hover_view
                .update(app, |this, cx| {
                    if this.kept_drag_hovered_tab.as_deref() != Some(hover_anchor.as_str())
                        || this.kept_drag_hover_after != after
                    {
                        this.kept_drag_hovered_tab = Some(hover_anchor.clone());
                        this.kept_drag_hover_after = after;
                        this.kept_section_drag_hovered = true;
                        cx.notify();
                    }
                })
                .ok();
            style.border_color(rgb(theme.accent))
        })
        .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
            this.drop_kept_tab_in_slot(&drag.tab_id, &anchor, group.clone(), after);
            cx.notify();
        }))
        .on_mouse_exit(cx.listener(move |this, _, _, cx| {
            if this.kept_drag_hovered_tab.as_deref() == Some(exit_anchor.as_str())
                && this.kept_drag_hover_after == after
            {
                this.kept_drag_hovered_tab = None;
                this.kept_drag_hover_after = false;
                cx.notify();
            }
        }))
        .with_spring(
            format!("kept-drop-slot-size-{slot_id}"),
            SpringAnimation::new(crate::ui::motion::tab_reorder().config())
                .to(if active { 1.0 } else { 0.0 })
                .with_epsilon(0.01),
            move |slot, progress| {
                let progress = crate::ui::motion::clamp_percentage(progress);
                slot.w(relative(slot_fraction * progress)).opacity(progress)
            },
        )
}
