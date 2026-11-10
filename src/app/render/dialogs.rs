use super::*;

impl Lumen {
    pub(super) fn render_dialogs(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> Vec<gpui_kit::AnyElement> {
        let mut overlays = Vec::new();
        if let Some(modal) = &self.space_modal {
            overlays.push(
                div()
                    .id("space-popover-dismiss-layer")
                    .occlude()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        gpui_kit::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.space_modal = None;
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
            );
            let is_edit = matches!(modal.mode, SpaceModalMode::Edit { .. });
            let title = if is_edit { "Edit Space" } else { "New Space" };
            let action_label = if is_edit { "Save" } else { "Create" };
            let current_selected_icon = modal.selected_icon.clone();
            let modal_theme = visual::theme_for_space(&modal.theme_draft);

            let popover = div()
                .id("space-popover-card")
                .occlude()
                .absolute()
                .bottom(px(52.0 * UI_SCALE))
                .left(px(8.0 * UI_SCALE))
                .w(px(224.0 * UI_SCALE))
                .bg(rgb(modal_theme.surface))
                .border_1()
                .border_color(rgb(modal_theme.border))
                .rounded_xl()
                .p_3()
                .flex()
                .flex_col()
                .gap_3()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(modal_theme.text))
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(modal_theme.muted))
                                .child("Name"),
                        )
                        .child(
                            div()
                                .id("space-name-input-container")
                                .w_full()
                                .px_0()
                                .py_1()
                                .rounded_lg()
                                .border_1()
                                .border_color(rgb(modal_theme.border))
                                .bg(rgb(modal_theme.base))
                                .cursor_text()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(modal) = &this.space_modal {
                                        modal.name_input.update(cx, |input, cx| {
                                            input.focus(window, cx);
                                        });
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    Input::new(&modal.name_input)
                                        .id("space-name-input")
                                        .w_full()
                                        .appearance(false),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(modal_theme.muted))
                                .child("Icon"),
                        )
                        .child({
                            let mut icons_grid = div()
                                .id("svg-picker-grid")
                                .h(px(100.0 * UI_SCALE))
                                .overflow_y_scroll()
                                .p_1()
                                .rounded_md()
                                .bg(rgb(modal_theme.surface))
                                .border_1()
                                .border_color(rgb(modal_theme.border))
                                .flex()
                                .flex_wrap()
                                .gap_1();
                            for (i, &icon) in SPACE_ICONS.iter().enumerate() {
                                let is_selected = icon == current_selected_icon;
                                let icon_str = icon.to_string();
                                icons_grid = icons_grid.child(
                                    div()
                                        .id(format!("space-icon-choice-{i}"))
                                        .size(px(26.0 * UI_SCALE))
                                        .rounded_lg()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(rgb(if is_selected {
                                            modal_theme.surface_hover
                                        } else {
                                            modal_theme.surface
                                        }))
                                        .border_1()
                                        .border_color(rgb(if is_selected {
                                            modal_theme.accent
                                        } else {
                                            modal_theme.border
                                        }))
                                        .text_color(rgb(if is_selected {
                                            0xffffff
                                        } else {
                                            0x8b8f9a
                                        }))
                                        .hover(|s| {
                                            s.bg(rgb(modal_theme.surface_hover))
                                                .text_color(rgb(modal_theme.text))
                                        })
                                        .cursor_pointer()
                                        .child(space_switcher::render_space_icon(icon))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(modal) = &mut this.space_modal {
                                                modal.selected_icon = icon_str.clone();
                                            }
                                            cx.notify();
                                        })),
                                );
                            }
                            icons_grid
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(modal_theme.muted))
                                .child("Color"),
                        )
                        .child({
                            div()
                                .id("space-edit-theme")
                                .px_3()
                                .py_2()
                                .rounded_lg()
                                .bg(rgb(modal_theme.surface_hover))
                                .text_color(rgb(modal_theme.text))
                                .text_sm()
                                .cursor_pointer()
                                .child("Edit theme…")
                                .on_click(cx.listener(
                                    |this, event: &gpui_kit::ClickEvent, window, cx| {
                                        this.open_theme_editor(event.position(), true, window, cx);
                                    },
                                ))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .pt_1()
                        .child(
                            div()
                                .id("popover-cancel-btn")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .text_xs()
                                .text_color(rgb(0xa1a4af))
                                .hover(move |s| {
                                    s.bg(rgb(modal_theme.surface_hover))
                                        .text_color(rgb(modal_theme.text))
                                })
                                .cursor_pointer()
                                .child("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.space_modal = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("popover-commit-btn")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(rgb(modal_theme.accent))
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(modal_theme.base))
                                .hover(|s| s.bg(rgb(0xb4bcff)))
                                .cursor_pointer()
                                .child(action_label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.commit_space_modal(cx);
                                    cx.notify();
                                })),
                        ),
                );

            overlays.push(
                popover
                    .with_animation(
                        "space-popover-entrance",
                        Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                            .with_easing(ui::motion::ease_out_quint),
                        |popover, progress| {
                            popover
                                .w(px(224.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                                .opacity(ui::motion::clamp_percentage(progress))
                        },
                    )
                    .into_any_element(),
            );
        }

        if let Some(modal) = &self.folder_modal {
            overlays.push(
                div()
                    .id("folder-popover-dismiss-layer")
                    .occlude()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        gpui_kit::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.folder_modal = None;
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
            );
            let is_rename = matches!(modal.mode, FolderModalMode::Rename { .. });
            let title = if is_rename {
                "Rename Folder"
            } else {
                "New Folder"
            };
            let action_label = if is_rename { "Save" } else { "Create" };
            let popover = div()
                .id("folder-popover-card")
                .occlude()
                .absolute()
                .top(px(modal.anchor_y - 12.0 * UI_SCALE))
                .left(px(modal.anchor_x + 12.0 * UI_SCALE))
                .w(px(224.0 * UI_SCALE))
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .rounded_xl()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(theme.muted))
                                .child("Name"),
                        )
                        .child(
                            div()
                                .id("folder-name-input-container")
                                .w_full()
                                .px_0()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.border))
                                .bg(rgb(theme.surface))
                                .cursor_text()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(modal) = &this.folder_modal {
                                        modal.name_input.update(cx, |input, cx| {
                                            input.focus(window, cx);
                                        });
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    Input::new(&modal.name_input)
                                        .id("folder-name-input")
                                        .w_full()
                                        .appearance(false),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .pt_1()
                        .child(
                            div()
                                .id("folder-popover-cancel-btn")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .text_xs()
                                .text_color(rgb(0xa1a4af))
                                .hover(move |style| {
                                    style
                                        .bg(rgb(theme.surface_hover))
                                        .text_color(rgb(theme.text))
                                })
                                .cursor_pointer()
                                .child("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.folder_modal = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("folder-popover-commit-btn")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(rgb(theme.accent))
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.base))
                                .hover(|style| style.bg(rgb(0xb4bcff)))
                                .cursor_pointer()
                                .child(action_label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.commit_folder_modal(cx);
                                    cx.notify();
                                })),
                        ),
                );
            overlays.push(
                popover
                    .with_animation(
                        "folder-popover-entrance",
                        Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                            .with_easing(ui::motion::ease_out_quint),
                        |popover, progress| {
                            popover
                                .w(px(224.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                                .opacity(ui::motion::clamp_percentage(progress))
                        },
                    )
                    .into_any_element(),
            );
        }

        overlays
    }
}
