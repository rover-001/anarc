//! Split-group rows, regular tab list, and closing-tab animation overlay.

use super::{SidebarTab, open_sidebar_context_menu};
use super::super::*;

impl Anarc {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_split_group_section(
        &self,
        mut sidebar_content: gpui_kit::Stateful<gpui_kit::Div>,
        theme: visual::SpaceTheme,
        active_tab_id: Option<&str>,
        tabs: &[SidebarTab],
        pair: &SplitPair,
        in_space_transition: bool,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let members = pair
            .panes()
            .into_iter()
            .filter_map(|id| tabs.iter().find(|(tab_id, _, _)| tab_id == id))
            .collect::<Vec<_>>();
        if members.is_empty() {
            return sidebar_content;
        }
        let first = members[0];
        let second = members[1];
        let group_name =
            tab_list::group_labels(&members.iter().map(|tab| tab.1.clone()).collect::<Vec<_>>());
        let group_key = format!("split-{}-{}", pair.first, pair.second);
        let expanded = self.split_group_expanded;
        crate::engine::log_favicon_row_rendered(&first.0, first.2.is_some());
        crate::engine::log_favicon_row_rendered(&second.0, second.2.is_some());
        let group_id = pair.first.clone();
        let target_rotation = if expanded { std::f32::consts::PI } else { 0.0 };
        let chevron = Icon::new(IconName::ChevronDown).size(px(14.0 * UI_SCALE));
        let chevron = if ui::motion::reduced_motion() {
            chevron
                .rotate(gpui_kit::percentage(ui::motion::clamp_percentage(
                    target_rotation / (2.0 * std::f32::consts::PI),
                )))
                .into_any_element()
        } else {
            chevron
                .with_spring(
                    "split-group-chevron-rotation",
                    SpringAnimation::new(ui::motion::toggle().config()).to(target_rotation),
                    |icon, angle| {
                        icon.rotate(gpui_kit::percentage(ui::motion::clamp_percentage(
                            angle / (2.0 * std::f32::consts::PI),
                        )))
                    },
                )
                .into_any_element()
        };
        let group_active =
            !expanded && pair.panes().contains(&active_tab_id.unwrap_or_default());
        let group_row = div()
            .id("split-tab-group")
            .w_full()
            .h(px(visual::TAB_ROW_HEIGHT))
            .cursor_pointer()
            .active(|style| style.opacity(0.92))
            .group(group_key.clone())
            .child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(visual::SIDEBAR_ROW_GAP))
                    .px(px(visual::SIDEBAR_ROW_PADDING_X))
                    .rounded_lg()
                    .when(group_active, |row| {
                        row.bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
                    })
                    .when(!group_active, |row| {
                        row.hover(|row| {
                            row.bg(crate::ui::visual::translucent_surface(
                                theme.surface_hover,
                                0x40,
                            ))
                        })
                    })
                    .child(tab_list::split_favicon(first.2.clone(), second.2.clone()))
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(group_name),
                    )
                    .child(
                        div()
                            .id("split-group-toggle")
                            .size(px(16.0 * UI_SCALE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(theme.muted))
                            .opacity(if expanded { 1.0 } else { 0.0 })
                            .group_hover(group_key, |style| style.opacity(1.0))
                            .child(chevron)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.split_group_expanded = !this.split_group_expanded;
                                this.split_expanded_at =
                                    this.split_group_expanded.then(Instant::now);
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate_split_group();
                this.focus_browser(window, cx);
                cx.notify();
            }))
            .on_mouse_down(
                gpui_kit::MouseButton::Right,
                cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let (x, y, width, height) = tab_context_menu_position(event, window);
                    this.tab_context_menu = Some(TabContextMenu {
                        tab_id: group_id.clone(),
                        showing_split_targets: false,
                        x,
                        y,
                        width,
                        height,
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        sidebar_content = sidebar_content.child(group_row);
        if expanded {
            let animate_members = !in_space_transition
                && !ui::motion::reduced_motion()
                && self.split_expanded_at.is_some_and(|at| {
                    at.elapsed() < ui::motion::row_enter().duration() * 2
                });
            for (pane_index, member) in members.into_iter().enumerate() {
                sidebar_content = sidebar_content.child(render_split_member_row(
                    pane_index,
                    member,
                    active_tab_id == Some(member.0.as_str()),
                    animate_members,
                    theme,
                    cx,
                ));
            }
        }
        sidebar_content
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_regular_tabs_section(
        &self,
        theme: visual::SpaceTheme,
        active_space_id: &str,
        active_tab_id: Option<&str>,
        tabs: &[SidebarTab],
        stored_split_pair: Option<&SplitPair>,
        in_space_transition: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active_space = self.active_space();
        let mut regular_tabs = div().id("regular-tabs").w_full().flex().flex_col();
        let mut has_regular_tabs = false;
        let inline_closing = self.closing_tab.as_ref().filter(|(space, tab, _, started, _)| {
            space == active_space_id
                && !tab.is_pinned()
                && tab.folder.is_none()
                && started.elapsed() < ui::motion::row_exit().duration()
        });
        let mut inserted_closing = false;

        for (tab_index, (id, title, favicon)) in tabs.iter().enumerate() {
            if let Some((_, closing_tab, closing_index, _, serial)) = inline_closing
                && !inserted_closing
                && tab_index >= *closing_index
            {
                inserted_closing = true;
                regular_tabs = regular_tabs.child(render_inline_closing_tab_row(
                    closing_tab,
                    *serial,
                    theme,
                ));
            }
            let is_split =
                stored_split_pair.is_some_and(|pair| pair.panes().contains(&id.as_str()));
            let is_sectioned = active_space
                .tabs
                .iter()
                .find(|tab| tab.id == *id)
                .is_some_and(|tab| tab.is_pinned() || tab.folder.is_some());
            if is_split || is_sectioned {
                continue;
            }
            has_regular_tabs = true;
            regular_tabs = regular_tabs.child(self.render_regular_tab_row(
                id,
                title,
                favicon,
                active_space_id,
                active_tab_id == Some(id.as_str()),
                in_space_transition,
                theme,
                cx,
            ));
        }
        if let Some((_, closing_tab, _, _, serial)) = inline_closing
            && !inserted_closing
        {
            regular_tabs = regular_tabs.child(render_inline_closing_tab_row(
                closing_tab,
                *serial,
                theme,
            ));
        }

        // Dropping a tab anywhere in the regular list removes it from the
        // favorites, pinned, or folder sections.
        div()
            .id("regular-tabs-drop-zone")
            .w_full()
            .flex()
            .flex_col()
            .when(has_regular_tabs, |zone| {
                zone.min_h(px(visual::TAB_ROW_HEIGHT))
            })
            .on_mouse_down(
                gpui_kit::MouseButton::Right,
                cx.listener(open_sidebar_context_menu),
            )
            .drag_over::<TabDrag>(move |style, _, _, _| {
                style.bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
            })
            .on_drop::<TabDrag>(cx.listener(|this, drag: &TabDrag, _window, cx| {
                this.clear_tab_section(&drag.tab_id);
                this.move_tab_to_end(&drag.tab_id);
                cx.notify();
            }))
            .child(regular_tabs)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_regular_tab_row(
        &self,
        id: &str,
        title: &str,
        favicon: &Option<std::sync::Arc<gpui_kit::RenderImage>>,
        active_space_id: &str,
        selected: bool,
        in_space_transition: bool,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        crate::engine::log_favicon_row_rendered(id, favicon.is_some());
        let tab_id = id.to_owned();
        let close_id = tab_id.clone();
        let before_target = tab_id.clone();
        let after_target = tab_id.clone();
        let is_blank_tab = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .is_some_and(|tab| tab.url.is_empty() || tab.url == "about:blank");
        let drag_value = TabDrag {
            tab_id: tab_id.clone(),
            favicon: favicon.clone(),
            source_kept: false,
        };
        let drag_app_view = cx.weak_entity();
        let drag_over_view = cx.weak_entity();
        let animate_entry = !in_space_transition
            && !ui::motion::reduced_motion()
            && self.added_tab.as_ref().is_some_and(|(space, tab, at)| {
                space == active_space_id
                    && tab == id
                    && at.elapsed() < ui::motion::row_enter().duration() * 2
            });
        let row_group = format!("tab-row-{id}");
        let context_tab = tab_id.clone();
        let row = div()
            .id(format!("tab-{id}"))
            .relative()
            .w_full()
            .h(px(visual::TAB_ROW_HEIGHT))
            .cursor_pointer()
            .active(|style| style.opacity(0.92))
            .group(row_group.clone())
            .child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(visual::SIDEBAR_ROW_GAP))
                    .px(px(visual::SIDEBAR_ROW_PADDING_X))
                    .rounded_lg()
                    .when(selected, |row| {
                        row.bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
                    })
                    .when(!selected, |row| {
                        row.hover(|row| {
                            row.bg(crate::ui::visual::translucent_surface(
                                theme.surface_hover,
                                0x40,
                            ))
                        })
                    })
                    .child(tab_list::favicon(favicon.clone()))
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(rgb(if selected { theme.text } else { theme.muted }))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title.to_owned()),
                    )
                    .child(
                        div()
                            .id(format!("close-tab-{id}"))
                            .size(px(16.0 * UI_SCALE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(0.0)
                            .group_hover(row_group, |s| s.opacity(1.0))
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
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate_tab_in_active_space(&tab_id);
                this.focus_browser(window, cx);
                cx.notify();
            }))
            .on_mouse_down(
                gpui_kit::MouseButton::Right,
                cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let (x, y, width, height) = tab_context_menu_position(event, window);
                    this.tab_context_menu = Some(TabContextMenu {
                        tab_id: context_tab.clone(),
                        showing_split_targets: false,
                        x,
                        y,
                        width,
                        height,
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .when(!is_blank_tab, |row| {
                row.on_drag(
                    drag_value,
                    move |drag: &TabDrag, pointer_offset, _window, cx: &mut gpui_kit::App| {
                        drag_app_view
                            .update(cx, |this, cx| {
                                this.kept_drag_source_id = None;
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
            })
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
                style.bg(crate::ui::visual::translucent_surface(
                    theme.surface_hover,
                    0x40,
                ))
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
            }));

        if animate_entry {
            row.with_spring(
                format!("tab-row-entry-{active_space_id}-{id}"),
                ui::motion::row_enter().enter(),
                move |row, progress| {
                    let progress = ui::motion::clamp_percentage(progress);
                    row.h(px(visual::TAB_ROW_HEIGHT * progress))
                        .pl(px(8.0 * UI_SCALE * (1.0 - progress)))
                        .opacity(progress)
                },
            )
            .into_any_element()
        } else {
            row.into_any_element()
        }
    }

    pub(super) fn render_closing_tab_overlay(
        &self,
        theme: visual::SpaceTheme,
        active_space_id: &str,
        stored_split_pair: Option<&SplitPair>,
        sidebar_width_f32: f32,
    ) -> Option<impl IntoElement> {
        let (closing_space, closing_tab, closing_index, started, serial) =
            self.closing_tab.as_ref()?;
        if (!closing_tab.is_pinned() && closing_tab.folder.is_none())
            || closing_space != active_space_id
            || started.elapsed() >= ui::motion::row_exit().duration()
        {
            return None;
        }
        let row_index = closing_tab_row_index(
            self.active_space(),
            stored_split_pair,
            self.split_group_expanded,
            closing_tab,
            *closing_index,
        );
        let row_rect = visual::sidebar_row_rect(sidebar_width_f32, row_index);
        Some(
            div()
                .id(format!("closing-tab-{}-{serial}", closing_tab.id))
                .absolute()
                .left(px(row_rect.left))
                .top(px(row_rect.top))
                .w(px(row_rect.width))
                .h(px(row_rect.height))
                .overflow_hidden()
                .rounded_lg()
                .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
                .child(
                    div()
                        .w_full()
                        .h_full()
                        .flex()
                        .items_center()
                        .gap(px(visual::SIDEBAR_ROW_GAP))
                        .px(px(visual::SIDEBAR_ROW_PADDING_X))
                        .text_sm()
                        .text_color(rgb(theme.muted))
                        .child(tab_list::favicon(None))
                        .child(closing_tab.title.clone()),
                )
                .with_spring(
                    format!("tab-row-close-{}-{serial}", closing_tab.id),
                    ui::motion::row_exit().between(1.0, 0.0),
                    move |row, remaining| {
                        let remaining = ui::motion::clamp_percentage(remaining);
                        row.left(px(row_rect.left - 12.0 * UI_SCALE * (1.0 - remaining)))
                            .h(px(visual::TAB_ROW_HEIGHT * remaining))
                            .opacity(remaining)
                    },
                ),
        )
    }
}

fn render_inline_closing_tab_row(
    closing_tab: &Tab,
    serial: u64,
    theme: visual::SpaceTheme,
) -> gpui_kit::AnyElement {
    div()
        .id(format!("closing-tab-inline-{}-{serial}", closing_tab.id))
        .w_full()
        .h(px(visual::TAB_ROW_HEIGHT))
        .overflow_hidden()
        .rounded_lg()
        .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
        .child(
            div()
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .gap(px(visual::SIDEBAR_ROW_GAP))
                .px(px(visual::SIDEBAR_ROW_PADDING_X))
                .text_sm()
                .text_color(rgb(theme.muted))
                .child(tab_list::favicon(None))
                .child(closing_tab.title.clone()),
        )
        .with_spring(
            format!("tab-row-close-inline-{}-{serial}", closing_tab.id),
            ui::motion::row_exit().between(1.0, 0.0),
            move |row, remaining| {
                let remaining = ui::motion::clamp_percentage(remaining);
                row.h(px(visual::TAB_ROW_HEIGHT * remaining))
                    .pl(px(12.0 * UI_SCALE * (1.0 - remaining)))
                    .opacity(remaining)
            },
        )
        .into_any_element()
}

fn render_split_member_row(
    pane_index: usize,
    member: &SidebarTab,
    member_selected: bool,
    animate_members: bool,
    theme: visual::SpaceTheme,
    cx: &mut Context<Anarc>,
) -> gpui_kit::AnyElement {
    let member_id = member.0.clone();
    let member_animation_id = member_id.clone();
    let close_id = member_id.clone();
    let stagger = std::time::Duration::from_millis(30 * pane_index as u64);
    let base = ui::motion::row_enter();
    let base_duration = base.duration();
    let total_duration = base_duration + stagger;
    let base_seconds = base_duration.as_secs_f32().max(f32::EPSILON);
    let delay_seconds = stagger.as_secs_f32();
    let total_seconds = total_duration.as_secs_f32();
    let base_easing = base.easing();
    crate::engine::log_favicon_row_rendered(&member.0, member.2.is_some());

    let row = div()
        .id(format!("split-member-{member_id}"))
        .w_full()
        .h(px(visual::TAB_ROW_HEIGHT))
        .cursor_pointer()
        .active(|style| style.opacity(0.92))
        .group(format!("tab-row-{member_id}"))
        .child(
            div()
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .gap(px(visual::SIDEBAR_ROW_GAP))
                .pl(px(visual::SIDEBAR_ROW_PADDING_X
                    + visual::SIDEBAR_ROW_ICON_SIZE
                    + visual::SIDEBAR_ROW_GAP))
                .pr(px(visual::SIDEBAR_ROW_PADDING_X))
                .rounded_lg()
                .when(member_selected, |row| {
                    row.bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
                })
                .when(!member_selected, |row| {
                    row.hover(|row| {
                        row.bg(crate::ui::visual::translucent_surface(
                            theme.surface_hover,
                            0x40,
                        ))
                    })
                })
                .child(tab_list::favicon(member.2.clone()))
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(member.1.clone()),
                )
                .child(
                    div()
                        .id(format!("close-member-{member_id}"))
                        .size(px(16.0 * UI_SCALE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(0.0)
                        .group_hover(format!("tab-row-{member_id}"), |s| s.opacity(1.0))
                        .text_color(rgb(theme.muted))
                        .child(Icon::new(IconName::X).size(px(14.0 * UI_SCALE)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_section_tab(&close_id);
                            cx.stop_propagation();
                            cx.notify();
                        })),
                ),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.set_focused_pane(pane_index);
            this.focus_browser(window, cx);
            cx.stop_propagation();
            cx.notify();
        }))
        .on_mouse_down(
            gpui_kit::MouseButton::Right,
            cx.listener(
                move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let (x, y, width, height) = tab_context_menu_position(event, window);
                    this.tab_context_menu = Some(TabContextMenu {
                        tab_id: member_id.clone(),
                        showing_split_targets: false,
                        x,
                        y,
                        width,
                        height,
                    });
                    cx.stop_propagation();
                    cx.notify();
                },
            ),
        );

    if !animate_members {
        return row.into_any_element();
    }
    row.with_animation(
        format!("split-member-entry-{member_animation_id}"),
        Animation::new(ui::motion::duration(total_duration)).with_easing(move |progress| {
            base_easing(((progress * total_seconds - delay_seconds) / base_seconds).clamp(0.0, 1.0))
        }),
        move |row, progress| {
            let progress = ui::motion::clamp_percentage(progress);
            row.h(px(visual::TAB_ROW_HEIGHT * progress))
                .opacity(progress)
        },
    )
    .into_any_element()
}
