use super::*;

impl Lumen {
    pub(super) fn render_sidebar(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let active_space_id = self.active_space.clone();
        let active_space = self.active_space().clone();
        let active_tab_id = active_space.active.clone();
        let tabs: Vec<(
            String,
            String,
            Option<std::sync::Arc<gpui_kit::RenderImage>>,
        )> = active_space
            .tabs
            .iter()
            .map(|tab| {
                let display_title = if (tab.pinned || tab.favorite)
                    && let Some(pinned_title) = &tab.pinned_title
                {
                    pinned_title.clone()
                } else {
                    tab.title.clone()
                };
                let display_url = if (tab.pinned || tab.favorite)
                    && let Some(pinned_url) = &tab.pinned_url
                {
                    pinned_url.as_str()
                } else {
                    tab.url.as_str()
                };
                (
                    tab.id.clone(),
                    display_title,
                    self.engine.get_favicon_image(&tab.id, Some(display_url)),
                )
            })
            .collect();
        let stored_split_pair = crate::plugins::split_view_enabled()
            .then(|| self.stored_split_pair())
            .flatten();
        let sidebar_width_f32 = self.sidebar_width_transition.map_or_else(
            || self.effective_sidebar_width(),
            |(from, to, started, _)| {
                let progress = ui::motion::sidebar().progress(started.elapsed());
                from + (to - from) * progress
            },
        );
        let space_switch_transition = self
            .space_switch_transition
            .clone()
            .filter(|(_, _, started, _)| started.elapsed() < ui::motion::space_switch().duration());

        let sidebar_width = px(sidebar_width_f32);
        let mut sidebar_content = div()
            .id("sidebar")
            .relative()
            .flex()
            .flex_col()
            .w(sidebar_width)
            .h_full()
            .px_3()
            .py_3()
            .gap_2()
            .bg(rgb(theme.base))
            .child(visual::space_backdrop(
                self.space_modal
                    .as_ref()
                    .map_or(&active_space, |modal| &modal.theme_draft),
            ))
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.show_suggestions {
                        this.show_suggestions = false;
                        this.selected_suggestion = None;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Right,
                cx.listener(|this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    if this.is_sidebar_collapsed() {
                        return;
                    }
                    let (x, y, width, height) = tab_context_menu_position(event, window);
                    this.sidebar_context_menu = Some(SidebarContextMenu {
                        x,
                        y,
                        width,
                        height,
                    });
                    this.space_context_menu = None;
                    this.theme_editor = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        if !self.is_sidebar_collapsed() || self.sidebar_hovered {
            sidebar_content = sidebar_content.child(
                div()
                    .id("active-space-name")
                    .w_full()
                    .h(px(visual::active_topbar_height() - 24.0 * UI_SCALE))
                    .flex()
                    .items_center()
                    .px_1()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(active_space.name.clone())
                    .on_mouse_down(
                        gpui_kit::MouseButton::Right,
                        cx.listener(|this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            let (x, y, width, height) = tab_context_menu_position(event, window);
                            this.sidebar_context_menu = Some(SidebarContextMenu {
                                x,
                                y,
                                width,
                                height,
                            });
                            this.space_context_menu = None;
                            this.theme_editor = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );

            let mut sidebar_row_index = 0usize;
            let mut active_row_index = None;
            if let Some(pair) = &stored_split_pair {
                if active_tab_id.as_deref() == Some(pair.first.as_str()) {
                    active_row_index =
                        Some(sidebar_row_index + usize::from(self.split_group_expanded));
                } else if active_tab_id.as_deref() == Some(pair.second.as_str()) {
                    active_row_index =
                        Some(sidebar_row_index + usize::from(self.split_group_expanded) * 2);
                }
                sidebar_row_index += 1 + usize::from(self.split_group_expanded) * 2;
            }
            for (id, _, _) in &tabs {
                if stored_split_pair
                    .as_ref()
                    .is_some_and(|pair| id == &pair.first || id == &pair.second)
                {
                    continue;
                }
                if active_tab_id.as_deref() == Some(id.as_str()) {
                    active_row_index = Some(sidebar_row_index);
                }
                sidebar_row_index += 1;
            }
            let _ = active_row_index;

            let is_split_member = |id: &str| {
                stored_split_pair
                    .as_ref()
                    .is_some_and(|pair| pair.first == id || pair.second == id)
            };
            let tab_flags = |id: &str| -> Option<(bool, bool, Option<String>)> {
                active_space
                    .tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .map(|tab| (tab.favorite, tab.pinned, tab.folder.clone()))
            };

            // Arc-style favorites: the top of the sidebar doubles as the
            // favorite drop target. Dragging a tab here favorites it.
            let favorite_tabs = tabs
                .iter()
                .filter(|(id, _, _)| {
                    !is_split_member(id) && tab_flags(id).is_some_and(|(favorite, _, _)| favorite)
                })
                .cloned()
                .collect::<Vec<_>>();
            let mut favorites_grid = div()
                .id("favorites-grid")
                .w_full()
                .flex()
                .flex_wrap()
                .gap_1();
            if favorite_tabs.is_empty() {
                favorites_grid = favorites_grid.child(
                    div()
                        .id("favorites-empty-hint")
                        .w_full()
                        .h(px(28.0 * UI_SCALE))
                        .rounded_md()
                        .border_1()
                        .border_dashed()
                        .border_color(rgb(theme.border))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child("Drag a tab here to star"),
                );
            }
            for (id, _title, favicon) in &favorite_tabs {
                let tile_id = id.clone();
                let context_id = id.clone();
                let before_target = id.clone();
                let after_target = id.clone();
                let selected = active_tab_id.as_ref() == Some(id);
                let drag_value = TabDrag {
                    tab_id: id.clone(),
                    favicon: favicon.clone(),
                };
                favorites_grid = favorites_grid.child(
                    div()
                        .id(format!("favorite-{id}"))
                        .size(px(FAVORITE_TILE_SIZE))
                        .relative()
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .when(selected, |tile| tile.bg(rgb(theme.surface)))
                        .hover(move |tile| tile.bg(rgb(theme.surface_hover)))
                        .child(tab_list::favorite_favicon(favicon.clone()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate_tab_in_active_space(&tile_id);
                            cx.notify();
                        }))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Right,
                            cx.listener(
                                move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                                    let (x, y, width, height) =
                                        tab_context_menu_position(event, window);
                                    this.tab_context_menu = Some(TabContextMenu {
                                        tab_id: context_id.clone(),
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
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .bottom_0()
                                .w_full()
                                .h(px(5.0 * UI_SCALE))
                                .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                                    this.drop_tab_in_target_section(
                                        &drag.tab_id,
                                        &after_target,
                                        true,
                                    );
                                    cx.notify();
                                })),
                        )
                        .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                            this.drop_tab_in_target_section(&drag.tab_id, &before_target, false);
                            cx.notify();
                        })),
                );
            }
            let favorites_section = div()
                .id("favorites-drop-zone")
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .rounded_lg()
                .drag_over::<TabDrag>(move |style, _, _, _| style.bg(rgb(theme.surface)))
                .on_drop::<TabDrag>(cx.listener(|this, drag: &TabDrag, _window, cx| {
                    this.favorite_tab(&drag.tab_id, true);
                    cx.notify();
                }))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_1()
                        .child(
                            div()
                                .text_color(rgb(theme.muted))
                                .child(Icon::new(IconName::StarFill).size(px(12.0 * UI_SCALE))),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.muted))
                                .child("Favorites"),
                        ),
                )
                .child(favorites_grid);
            sidebar_content = sidebar_content.child(favorites_section);

            // Pinned tabs live directly below favorites. Dropping a tab on
            // this section pins it.
            let pinned_tabs = tabs
                .iter()
                .filter(|(id, _, _)| {
                    !is_split_member(id)
                        && tab_flags(id).is_some_and(|(favorite, pinned, _)| !favorite && pinned)
                })
                .cloned()
                .collect::<Vec<_>>();
            let mut pinned_rows = div().id("pinned-rows").w_full().flex().flex_col().gap_1();
            if pinned_tabs.is_empty() {
                pinned_rows = pinned_rows.child(
                    div()
                        .id("pinned-empty-hint")
                        .w_full()
                        .h(px(28.0 * UI_SCALE))
                        .rounded_md()
                        .border_1()
                        .border_dashed()
                        .border_color(rgb(theme.border))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child("Drag a tab here to pin"),
                );
            }
            for (id, title, favicon) in &pinned_tabs {
                pinned_rows = pinned_rows.child(section_tab_row(
                    id.clone(),
                    title.clone(),
                    favicon.clone(),
                    active_tab_id.as_ref() == Some(id),
                    true,
                    theme,
                    0.0,
                    cx,
                ));
            }
            let pinned_section = div()
                .id("pinned-drop-zone")
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .rounded_lg()
                .drag_over::<TabDrag>(move |style, _, _, _| style.bg(rgb(theme.surface)))
                .on_drop::<TabDrag>(cx.listener(|this, drag: &TabDrag, _window, cx| {
                    this.pin_tab(&drag.tab_id, true);
                    cx.notify();
                }))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_1()
                        .child(
                            div()
                                .text_color(rgb(theme.muted))
                                .child(Icon::new(IconName::Pin).size(px(12.0 * UI_SCALE))),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.muted))
                                .child("Pinned"),
                        ),
                )
                .child(pinned_rows);
            sidebar_content = sidebar_content.child(pinned_section);

            // Folders group tabs below pinned. Drop a tab on a folder row to
            // file it, or use the plus button / tab context menu.
            let folders = active_space.folders.clone();
            let mut folders_section = div()
                .id("folders-section")
                .w_full()
                .flex()
                .flex_col()
                .gap_1();
            folders_section = folders_section.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(Icon::new(IconName::Folder).size(px(12.0 * UI_SCALE))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.muted))
                            .child("Folders"),
                    )
                    .child(
                        div()
                            .id("new-folder-button")
                            .size(px(18.0 * UI_SCALE))
                            .rounded_md()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_color(rgb(theme.muted))
                            .hover(move |style| {
                                style
                                    .bg(rgb(theme.surface_hover))
                                    .text_color(rgb(theme.text))
                            })
                            .child(Icon::new(IconName::FolderPlus).size(px(13.0 * UI_SCALE)))
                            .on_click(cx.listener(
                                |this, event: &gpui_kit::ClickEvent, window, cx| {
                                    this.open_create_folder_modal(
                                        None,
                                        event.position(),
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                },
                            )),
                    ),
            );
            for folder in &folders {
                let folder_id = folder.id.clone();
                let toggle_id = folder.id.clone();
                let context_id = folder.id.clone();
                let drop_folder_id = folder.id.clone();
                let folder_name = folder.name.clone();
                let expanded = folder.expanded;
                let members = tabs
                    .iter()
                    .filter(|(id, _, _)| {
                        !is_split_member(id)
                            && tab_flags(id).is_some_and(|(favorite, pinned, tab_folder)| {
                                !favorite
                                    && !pinned
                                    && tab_folder.as_deref() == Some(folder.id.as_str())
                            })
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let mut folder_block = div()
                    .id(format!("folder-block-{folder_id}"))
                    .w_full()
                    .flex()
                    .flex_col();
                folder_block = folder_block.child(
                    div()
                        .id(format!("folder-{folder_id}"))
                        .w_full()
                        .h(px(visual::TAB_ROW_HEIGHT))
                        .cursor_pointer()
                        .active(|style| style.opacity(0.92))
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .flex()
                                .items_center()
                                .gap(px(visual::SIDEBAR_ROW_GAP))
                                .px(px(visual::SIDEBAR_ROW_PADDING_X))
                                .rounded_lg()
                                .hover(|row| row.bg(rgb(theme.surface_hover)))
                                .child(
                                    div().text_color(rgb(theme.muted)).child(
                                        Icon::new(if expanded {
                                            IconName::FolderOpen
                                        } else {
                                            IconName::Folder
                                        })
                                        .size(px(14.0 * UI_SCALE)),
                                    ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .text_color(rgb(theme.text))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(folder_name),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(folder) = this
                                .active_space_mut()
                                .folders
                                .iter_mut()
                                .find(|folder| folder.id == toggle_id)
                            {
                                folder.expanded = !folder.expanded;
                            }
                            this.save_state();
                            cx.notify();
                        }))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Right,
                            cx.listener(
                                move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                                    let (x, y, width, height) =
                                        tab_context_menu_position(event, window);
                                    this.folder_context_menu = Some(FolderContextMenu {
                                        folder_id: context_id.clone(),
                                        x,
                                        y,
                                        width,
                                        height,
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                            ),
                        )
                        .drag_over::<TabDrag>(move |style, _, _, _| style.bg(rgb(theme.surface)))
                        .on_drop::<TabDrag>(cx.listener(
                            move |this, drag: &TabDrag, _window, cx| {
                                this.move_tab_to_folder(&drag.tab_id, &drop_folder_id);
                                cx.notify();
                            },
                        )),
                );
                if expanded && !members.is_empty() {
                    // Git-tree style connector: a vertical line drops from the
                    // folder row and a short tick branches into each tab.
                    let tree_indent = visual::SIDEBAR_ROW_PADDING_X + 7.0 * UI_SCALE;
                    let mut members_tree = div()
                        .id(format!("folder-tree-{folder_id}"))
                        .relative()
                        .w_full()
                        .flex()
                        .flex_col();
                    for (id, title, favicon) in &members {
                        members_tree = members_tree.child(
                            div()
                                .relative()
                                .w_full()
                                .child(section_tab_row(
                                    id.clone(),
                                    title.clone(),
                                    favicon.clone(),
                                    active_tab_id.as_ref() == Some(id),
                                    false,
                                    theme,
                                    tree_indent,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(tree_indent - UI_SCALE))
                                        .top(px(visual::TAB_ROW_HEIGHT / 2.0))
                                        .w(px(visual::SIDEBAR_ROW_PADDING_X + UI_SCALE))
                                        .h(px(1.0))
                                        .bg(rgb(theme.border)),
                                ),
                        );
                    }
                    members_tree = members_tree.child(
                        div()
                            .absolute()
                            .left(px(tree_indent))
                            .top_0()
                            .w(px(1.0 * UI_SCALE))
                            .h(px(visual::TAB_ROW_HEIGHT * members.len() as f32
                                - visual::TAB_ROW_HEIGHT / 2.0))
                            .bg(rgb(theme.border)),
                    );
                    folder_block = folder_block.child(members_tree);
                }
                folders_section = folders_section.child(folder_block);
            }
            sidebar_content = sidebar_content.child(folders_section);

            if let Some(pair) = &stored_split_pair {
                let first = tabs.iter().find(|(id, _, _)| id == &pair.first);
                let second = tabs.iter().find(|(id, _, _)| id == &pair.second);
                if let (Some(first), Some(second)) = (first, second) {
                    let group_name = tab_list::group_label(&first.1, &second.1);
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
                                SpringAnimation::new(ui::motion::toggle().config())
                                    .to(target_rotation),
                                |icon, angle| {
                                    icon.rotate(gpui_kit::percentage(ui::motion::clamp_percentage(
                                        angle / (2.0 * std::f32::consts::PI),
                                    )))
                                },
                            )
                            .into_any_element()
                    };
                    let group_active = !expanded
                        && (active_tab_id.as_deref() == Some(pair.first.as_str())
                            || active_tab_id.as_deref() == Some(pair.second.as_str()));
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
                                .when(group_active, |row| row.bg(rgb(theme.surface)))
                                .when(!group_active, |row| {
                                    row.hover(|row| row.bg(rgb(theme.surface_hover)))
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
                                        .group_hover(group_key.clone(), |style| style.opacity(1.0))
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
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate_split_group();
                            cx.notify();
                        }))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Right,
                            cx.listener(
                                move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                                    let (x, y, width, height) =
                                        tab_context_menu_position(event, window);
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
                                },
                            ),
                        );
                    if expanded {
                        let animate_members = space_switch_transition.is_none()
                            && !ui::motion::reduced_motion()
                            && self.split_expanded_at.is_some_and(|at| {
                                at.elapsed() < ui::motion::row_enter().duration() * 2
                            });
                        let mut member_rows = Vec::new();
                        for (pane_index, member) in [first, second].into_iter().enumerate() {
                            let member_id = member.0.clone();
                            let member_selected =
                                active_tab_id.as_deref() == Some(member_id.as_str());
                            let member_animation_id = member_id.clone();
                            let close_id = member_id.clone();
                            let pane = pane_index;
                            let stagger = std::time::Duration::from_millis(30 * pane_index as u64);
                            let base = ui::motion::row_enter();
                            let base_duration = base.duration();
                            let total_duration = base_duration + stagger;
                            let base_seconds = base_duration.as_secs_f32().max(f32::EPSILON);
                            let delay_seconds = stagger.as_secs_f32();
                            let total_seconds = total_duration.as_secs_f32();
                            let base_easing = base.easing();
                            crate::engine::log_favicon_row_rendered(&member.0, member.2.is_some());
                            let member_row = div()
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
                                        .when(member_selected, |row| row.bg(rgb(theme.surface)))
                                        .when(!member_selected, |row| {
                                            row.hover(|row| row.bg(rgb(theme.surface_hover)))
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
                                                .group_hover(format!("tab-row-{member_id}"), |s| {
                                                    s.opacity(1.0)
                                                })
                                                .text_color(rgb(theme.muted))
                                                .child(
                                                    Icon::new(IconName::X)
                                                        .size(px(14.0 * UI_SCALE)),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.close_section_tab(&close_id);
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_focused_pane(pane);
                                    cx.stop_propagation();
                                    cx.notify();
                                }))
                                .on_mouse_down(
                                    gpui_kit::MouseButton::Right,
                                    cx.listener(
                                        move |this,
                                              event: &gpui_kit::MouseDownEvent,
                                              window,
                                              cx| {
                                            let (x, y, width, height) =
                                                tab_context_menu_position(event, window);
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
                                )
                                .map(|row| {
                                    if !animate_members {
                                        return row.into_any_element();
                                    }
                                    row.with_animation(
                                        format!("split-member-entry-{member_animation_id}"),
                                        Animation::new(ui::motion::duration(total_duration))
                                            .with_easing(move |progress| {
                                                base_easing(
                                                    ((progress * total_seconds - delay_seconds)
                                                        / base_seconds)
                                                        .clamp(0.0, 1.0),
                                                )
                                            }),
                                        move |row, progress| {
                                            let progress = ui::motion::clamp_percentage(progress);
                                            row.h(px(visual::TAB_ROW_HEIGHT * progress))
                                                .opacity(progress)
                                        },
                                    )
                                    .into_any_element()
                                });
                            member_rows.push(member_row);
                        }
                        sidebar_content = sidebar_content.child(group_row);
                        for member_row in member_rows {
                            sidebar_content = sidebar_content.child(member_row);
                        }
                    } else {
                        sidebar_content = sidebar_content.child(group_row);
                    }
                }
            }

            // Regular tabs are the drag-out target: dropping any favorite,
            // pinned, or foldered tab here returns it to this section.
            let mut regular_tabs = div().id("regular-tabs").w_full().flex().flex_col();

            for (id, title, favicon) in &tabs {
                if is_split_member(id)
                    || tab_flags(id).is_some_and(|(favorite, pinned, folder)| {
                        favorite || pinned || folder.is_some()
                    })
                {
                    continue;
                }
                crate::engine::log_favicon_row_rendered(id, favicon.is_some());
                let tab_id = id.clone();
                let close_id = tab_id.clone();
                let before_target = id.clone();
                let after_target = id.clone();
                let drag_value = TabDrag {
                    tab_id: id.clone(),
                    favicon: favicon.clone(),
                };
                let current_space_id = active_space_id.clone();
                let selected = active_tab_id.as_ref() == Some(id);
                let animate_entry = space_switch_transition.is_none()
                    && !ui::motion::reduced_motion()
                    && self.added_tab.as_ref().is_some_and(|(space, tab, at)| {
                        space == &active_space_id
                            && tab == id
                            && at.elapsed() < ui::motion::row_enter().duration() * 2
                    });
                let row_group = format!("tab-row-{id}");
                let context_tab = id.clone();
                regular_tabs = regular_tabs.child(
                    div()
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
                                .when(selected, |row| row.bg(rgb(theme.surface)))
                                .when(!selected, |row| {
                                    row.hover(|row| row.bg(rgb(theme.surface_hover)))
                                })
                                .child(tab_list::favicon(favicon.clone()))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .text_color(rgb(if selected {
                                            theme.text
                                        } else {
                                            theme.muted
                                        }))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(title.clone()),
                                )
                                .child(
                                    div()
                                        .id(format!("close-tab-{id}"))
                                        .size(px(16.0 * UI_SCALE))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .opacity(0.0)
                                        .group_hover(row_group.clone(), |s| s.opacity(1.0))
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
                                    this.drop_tab_in_target_section(
                                        &drag.tab_id,
                                        &after_target,
                                        true,
                                    );
                                    cx.notify();
                                })),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(space) = this
                                .spaces
                                .iter_mut()
                                .find(|space| space.id == current_space_id)
                            {
                                space.active = Some(tab_id.clone());
                            }
                            this.emit_plugin_text("tab.activated", tab_id.clone());
                            cx.notify();
                        }))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Right,
                            cx.listener(
                                move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                                    let (x, y, width, height) =
                                        tab_context_menu_position(event, window);
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
                                },
                            ),
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
                        .drag_over::<TabDrag>(move |style, _, _, _| {
                            style.bg(rgb(theme.surface_hover))
                        })
                        .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                            this.drop_tab_in_target_section(&drag.tab_id, &before_target, false);
                            cx.notify();
                        }))
                        .map(|row| {
                            if animate_entry {
                                row.with_spring(
                                    format!("tab-row-entry-{active_space_id}-{id}"),
                                    ui::motion::row_enter().enter(),
                                    move |row, progress| {
                                        let progress = ui::motion::clamp_percentage(progress);
                                        row.h(px(visual::TAB_ROW_HEIGHT * progress))
                                            .opacity(progress)
                                    },
                                )
                                .into_any_element()
                            } else {
                                row.into_any_element()
                            }
                        }),
                );
            }

            let closing_tab_element =
                if let Some((closing_space, closing_tab, closing_index, started, serial)) =
                    &self.closing_tab
                    && closing_space == &active_space_id
                    && started.elapsed() < ui::motion::row_exit().duration()
                {
                    let row_index = closing_tab_row_index(
                        &active_space,
                        stored_split_pair.as_ref(),
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
                            .bg(rgb(theme.surface))
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
                                    row.h(px(visual::TAB_ROW_HEIGHT * remaining))
                                        .opacity(remaining)
                                },
                            ),
                    )
                } else {
                    None
                };

            // Dropping a tab anywhere in the regular list removes it from the
            // favorites, pinned, or folder sections.
            let has_regular_tabs = tabs.iter().any(|(id, _, _)| {
                !is_split_member(id)
                    && !tab_flags(id).is_some_and(|(favorite, pinned, folder)| {
                        favorite || pinned || folder.is_some()
                    })
            });
            sidebar_content = sidebar_content.child(
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
                        cx.listener(|this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            let (x, y, width, height) = tab_context_menu_position(event, window);
                            this.sidebar_context_menu = Some(SidebarContextMenu {
                                x,
                                y,
                                width,
                                height,
                            });
                            this.space_context_menu = None;
                            this.theme_editor = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .drag_over::<TabDrag>(move |style, _, _, _| style.bg(rgb(theme.surface)))
                    .on_drop::<TabDrag>(cx.listener(|this, drag: &TabDrag, _window, cx| {
                        this.clear_tab_section(&drag.tab_id);
                        this.move_tab_to_end(&drag.tab_id);
                        cx.notify();
                    }))
                    .child(regular_tabs),
            );

            if let Some(el) = closing_tab_element {
                sidebar_content = sidebar_content.child(el);
            }

            sidebar_content = sidebar_content.child(
                div()
                    .id("new-tab-action")
                    .w_full()
                    .h(px(visual::TAB_ROW_HEIGHT))
                    .cursor_pointer()
                    .active(|style| style.opacity(0.92))
                    .child(ui::sidebar::new_tab_button(theme))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_tab();
                        this.address.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                            input.focus(window, cx);
                        });
                        cx.notify();
                    }))
                    .drag_over::<TabDrag>(move |style, _, _, _| style.bg(rgb(theme.surface_hover)))
                    .on_drop::<TabDrag>(cx.listener(|this, drag: &TabDrag, _, cx| {
                        this.clear_tab_section(&drag.tab_id);
                        this.move_tab_to_end(&drag.tab_id);
                        cx.notify();
                    })),
            );

            // Empty sidebar canvas opens the same quick actions as a space menu.
            sidebar_content = sidebar_content.child(
                div()
                    .id("sidebar-empty-space")
                    .flex_1()
                    .min_h(px(8.0 * UI_SCALE))
                    .on_mouse_down(
                        gpui_kit::MouseButton::Right,
                        cx.listener(|this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            let (x, y, width, height) = tab_context_menu_position(event, window);
                            this.sidebar_context_menu = Some(SidebarContextMenu {
                                x,
                                y,
                                width,
                                height,
                            });
                            this.space_context_menu = None;
                            this.theme_editor = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );

            // Bottom bar with Settings, Space Switcher (icons like Arc), and Downloads
            let active_space_index = self
                .spaces
                .iter()
                .position(|space| space.id == active_space_id)
                .unwrap_or(0);
            let mut spaces_dock = div()
                .id("spaces-dock")
                .relative()
                .flex()
                .items_center()
                .gap_1();
            let indicator_left = active_space_index as f32 * (28.0 * UI_SCALE + 4.0);
            let indicator = div()
                .id("space-dock-active-indicator")
                .absolute()
                .top_0()
                .left_0()
                .size(px(28.0 * UI_SCALE))
                .rounded_lg()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border_hover));
            let indicator = if ui::motion::reduced_motion() {
                indicator.left(px(indicator_left)).into_any_element()
            } else {
                indicator
                    .with_spring(
                        "space-dock-active-indicator-position",
                        SpringAnimation::new(ui::motion::toggle().config()).to(px(indicator_left)),
                        |indicator, left| indicator.left(left),
                    )
                    .into_any_element()
            };
            spaces_dock = spaces_dock.child(indicator);

            for space in &self.spaces {
                let id = space.id.clone();
                let is_active = space.id == active_space_id;
                let icon_btn = space_switcher::space_dock_icon_button(
                    &space.icon,
                    is_active,
                    visual::theme_for_space(space),
                );
                let id_right = id.clone();
                spaces_dock = spaces_dock.child(
                    div()
                        .id(format!("space-icon-{id}"))
                        .cursor_pointer()
                        .active(|style| style.opacity(0.92))
                        .child(icon_btn)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.space_context_menu = None;
                            this.activate_space(&id);
                            cx.notify();
                        }))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Right,
                            cx.listener(move |this, _, _, cx| {
                                this.space_context_menu = Some(SpaceContextMenu {
                                    space_id: id_right.clone(),
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                );
            }

            let bottom_bar = div()
                .id("sidebar-bottom-bar")
                .w_full()
                .pt_2()
                .border_t_1()
                .border_color(rgb(theme.border))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .id("settings-btn")
                        .child(space_switcher::settings_button(theme))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.app_page = Some(AppPage::Settings);
                            cx.notify();
                        })),
                )
                .child(spaces_dock)
                .child(
                    div()
                        .id("add-space-btn")
                        .cursor_pointer()
                        .active(|style| style.opacity(0.92))
                        .child(space_switcher::add_space_button(theme))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_create_space_modal(window, cx);
                            cx.notify();
                        })),
                );

            sidebar_content = sidebar_content.child(bottom_bar);
        } else {
            let active_space_icon = self.active_space().icon.clone();
            if self.preferences.sidebar_mode == crate::settings::SidebarMode::Icons {
                for (tab_id, _, favicon) in &tabs {
                    let id = tab_id.clone();
                    let selected = active_tab_id.as_ref() == Some(tab_id);
                    sidebar_content = sidebar_content.child(
                        div()
                            .id(format!("collapsed-tab-{tab_id}"))
                            .w_full()
                            .h(px(visual::TAB_ROW_HEIGHT))
                            .rounded_lg()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .when(selected, |row| row.bg(rgb(theme.surface)))
                            .hover(|row| row.bg(rgb(theme.surface_hover)))
                            .child(tab_list::favorite_favicon(favicon.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.activate_tab_in_active_space(&id);
                                cx.notify();
                            })),
                    );
                }
            }
            sidebar_content = sidebar_content.child(div().flex_1()).child(
                div()
                    .id("collapsed-space-icon")
                    .flex()
                    .justify_center()
                    .w_full()
                    .child(space_switcher::space_icon_button(
                        &active_space_icon,
                        true,
                        theme,
                    )),
            );
        }

        let fully_hidden = self.is_sidebar_collapsed()
            && self.preferences.sidebar_mode == crate::settings::SidebarMode::Hidden
            && !self.sidebar_hovered;
        if fully_hidden {
            sidebar_content = sidebar_content
                .px_0()
                .py_0()
                .when(self.sidebar_width_transition.is_none(), |sidebar| {
                    sidebar.opacity(0.0)
                });
        }

        if let Some((_, state, _, serial)) = &space_switch_transition {
            let start_alpha = state.incoming_opacity_start;
            let start_offset = state.incoming_offset_start;
            sidebar_content
                .with_animation(
                    format!("space-switch-sidebar-in-{serial}"),
                    ui::motion::space_switch().animation(),
                    move |sidebar, progress| {
                        sidebar.left(px(start_offset * (1.0 - progress))).opacity(
                            ui::motion::clamp_percentage(
                                start_alpha + (1.0 - start_alpha) * progress,
                            ),
                        )
                    },
                )
                .into_any_element()
        } else if let Some((from, to, _, serial)) = self.sidebar_width_transition {
            sidebar_content
                .with_animation(
                    format!("sidebar-width-transition-{serial}"),
                    ui::motion::sidebar().animation(),
                    move |sidebar, progress| {
                        let progress = progress.clamp(0.0, 1.0);
                        let opacity = if to == 0.0 && from > 0.0 {
                            1.0 - progress
                        } else if from == 0.0 && to > 0.0 {
                            progress
                        } else {
                            1.0
                        };
                        sidebar
                            .w(px(from + (to - from) * progress))
                            .opacity(ui::motion::clamp_percentage(opacity))
                    },
                )
                .into_any_element()
        } else {
            sidebar_content.into_any_element()
        }
    }
}
