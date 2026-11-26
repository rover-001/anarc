//! Sidebar, theme editor, space, and folder context menu overlays.

use super::{context_menu_item, dismiss_backdrop, sync_overlay_transition};
use super::super::*;

impl Anarc {
    pub(super) fn render_sidebar_context_menu(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let mut transition = std::mem::take(&mut self.sidebar_menu_transition);
        let synced = sync_overlay_transition(
            &mut transition,
            self.sidebar_context_menu.as_ref(),
            ui::motion::popover(),
            |this, serial| {
                if this.sidebar_menu_transition.closing.as_ref().is_some_and(|(_, s)| *s == serial) {
                    this.sidebar_menu_transition.closing = None;
                }
            },
            cx,
        );
        self.sidebar_menu_transition = transition;
        let (menu, is_closing, serial) = synced?;
        let x = menu.x;
        let y = menu.y;
        let width = menu.width;
        let height = menu.height;
        let dismiss = dismiss_backdrop(
            "sidebar-context-dismiss",
            |this, _, _| {
                this.sidebar_context_menu = None;
            },
            cx,
        );
        let panel = div()
            .id("sidebar-context-menu")
            .occlude()
            .absolute()
            .top(px(y))
            .left(px(x))
            .w(px(width))
            .max_h(px(height))
            .bg(rgb(visual::panel_surface(theme.surface)))
            .border_1()
            .border_color(rgb(theme.border))
            .rounded_xl()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(visual::panel_muted(theme)))
                    .child("Sidebar"),
            )
            .child(
                context_menu_item("sidebar-create-folder", theme, IconName::FolderPlus, "Create space")
                    .on_click(cx.listener(
                        move |this, event: &gpui_kit::ClickEvent, window, cx| {
                            this.sidebar_context_menu = None;
                            this.open_create_folder_modal(None, event.position(), window, cx);
                            cx.notify();
                        },
                    )),
            )
            .child(
                context_menu_item("sidebar-change-theme", theme, IconName::Star, "Change theme")
                    .on_click(cx.listener(
                        move |this, event: &gpui_kit::ClickEvent, window, cx| {
                            this.open_theme_editor(event.position(), false, window, cx);
                            cx.notify();
                        },
                    )),
            );
        let panel = panel.with_spring(
            format!(
                "sidebar-menu-{}-{serial}",
                if is_closing { "exit" } else { "enter" }
            ),
            if is_closing {
                ui::motion::popover().between(1.0, 0.0)
            } else {
                ui::motion::popover().enter()
            },
            move |menu, progress| {
                menu.w(px(width * (0.96 + 0.04 * progress)))
                    .opacity(ui::motion::clamp_percentage(progress))
            },
        );
        if is_closing {
            Some(panel.into_any_element())
        } else {
            Some(dismiss.child(panel).into_any_element())
        }
    }

    pub(super) fn render_theme_editor_overlay(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let mut transition = std::mem::take(&mut self.theme_editor_transition);
        let synced = sync_overlay_transition(
            &mut transition,
            self.theme_editor.as_ref(),
            ui::motion::popover(),
            |this, serial| {
                if this
                    .theme_editor_transition
                    .closing
                    .as_ref()
                    .is_some_and(|(_, s)| *s == serial)
                {
                    this.theme_editor_transition.closing = None;
                }
            },
            cx,
        );
        self.theme_editor_transition = transition;
        let (editor, is_closing, serial) = synced?;
        let x = editor.x;
        let y = editor.y;
        let selected = editor.selected_color;
        let bounds = editor.bounds.clone();
        let panel_bounds = editor.panel_bounds.clone();
        let colors = if self.theme_picker_space().theme_colors.is_empty() {
            vec![self.theme_picker_space().color]
        } else {
            self.theme_picker_space().theme_colors.clone()
        };
        let dismiss = dismiss_backdrop(
            "theme-editor-dismiss",
            |this, _, _| {
                this.theme_editor = None;
            },
            cx,
        );
        let panel = self.render_theme_picker(
            theme,
            x,
            y,
            selected,
            &colors,
            bounds,
            panel_bounds,
            is_closing,
            serial,
            cx,
        );
        if is_closing {
            Some(panel)
        } else {
            Some(dismiss.child(panel).into_any_element())
        }
    }

    pub(super) fn render_space_context_menu(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let mut transition = std::mem::take(&mut self.space_menu_transition);
        let synced = sync_overlay_transition(
            &mut transition,
            self.space_context_menu.as_ref(),
            ui::motion::popover(),
            |this, serial| {
                if this
                    .space_menu_transition
                    .closing
                    .as_ref()
                    .is_some_and(|(_, s)| *s == serial)
                {
                    this.space_menu_transition.closing = None;
                }
            },
            cx,
        );
        self.space_menu_transition = transition;
        let (menu, is_closing, serial) = synced?;
        let target_space = self.spaces.iter().find(|s| s.id == menu.space_id)?;
        let target_id = target_space.id.clone();
        let target_id_edit = target_id.clone();
        let target_id_del = target_id.clone();
        let space_icon = space_switcher::render_space_icon(&target_space.icon);
        let space_name = target_space.name.clone();
        let can_delete = self.spaces.len() > 1;

        let menu_layer = dismiss_backdrop(
            "space-context-dismiss-layer",
            |this, _, _| {
                this.space_context_menu = None;
            },
            cx,
        );
        let mut ctx_menu = div()
            .id("space-context-menu")
            .occlude()
            .absolute()
            .bottom(px(48.0 * UI_SCALE))
            .left(px(8.0 * UI_SCALE))
            .w(px(224.0 * UI_SCALE))
            .bg(rgb(visual::panel_surface(theme.surface)))
            .border_1()
            .border_color(rgb(theme.border))
            .rounded_xl()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(visual::panel_muted(theme)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(space_icon)
                    .child(space_name),
            )
            .child(
                context_menu_item(
                    "ctx-edit-space",
                    theme,
                    IconName::Pencil,
                    "Rename & Change Icon",
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_edit_space_modal(&target_id_edit, window, cx);
                    cx.notify();
                })),
            );

        if can_delete {
            ctx_menu = ctx_menu.child(
                div()
                    .id("ctx-delete-space")
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(0xff6b6b))
                    .hover(|style| style.bg(rgb(0x381e22)))
                    .active(|style| style.opacity(0.85))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Trash))
                    .child("Delete Workspace")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete_space(&target_id_del);
                        cx.notify();
                    })),
            );
        }

        let ctx_menu = ctx_menu.with_spring(
            format!(
                "space-menu-{}-{target_id}-{serial}",
                if is_closing { "exit" } else { "enter" }
            ),
            if is_closing {
                ui::motion::popover().between(1.0, 0.0)
            } else {
                ui::motion::popover().enter()
            },
            |menu, progress| {
                menu.w(px(224.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                    .opacity(ui::motion::clamp_percentage(progress))
            },
        );
        if is_closing {
            Some(ctx_menu.into_any_element())
        } else {
            Some(menu_layer.child(ctx_menu).into_any_element())
        }
    }

    pub(super) fn render_folder_context_menu(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let mut transition = std::mem::take(&mut self.folder_menu_transition);
        let synced = sync_overlay_transition(
            &mut transition,
            self.folder_context_menu.as_ref(),
            ui::motion::popover(),
            |this, serial| {
                if this
                    .folder_menu_transition
                    .closing
                    .as_ref()
                    .is_some_and(|(_, s)| *s == serial)
                {
                    this.folder_menu_transition.closing = None;
                }
            },
            cx,
        );
        self.folder_menu_transition = transition;
        let (menu, is_closing, serial) = synced?;
        let folder_id = menu.folder_id;
        let x = menu.x;
        let y = menu.y;
        let width = menu.width;
        let height = menu.height;
        let folder_name = self
            .active_space()
            .folders
            .iter()
            .find(|folder| folder.id == folder_id)
            .map(|folder| folder.name.clone())
            .unwrap_or_default();
        let menu_layer = dismiss_backdrop(
            "folder-context-dismiss-layer",
            |this, _, _| {
                this.folder_context_menu = None;
            },
            cx,
        );
        let rename_id = folder_id.clone();
        let delete_id = folder_id.clone();
        let panel = div()
            .id("folder-context-menu")
            .occlude()
            .absolute()
            .top(px(y))
            .left(px(x))
            .w(px(width))
            .max_h(px(height))
            .overflow_y_scroll()
            .bg(rgb(visual::panel_surface(theme.surface)))
            .border_1()
            .border_color(rgb(theme.border))
            .rounded_xl()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(visual::panel_muted(theme)))
                    .child(folder_name),
            )
            .child(
                context_menu_item("ctx-rename-folder", theme, IconName::Pencil, "Rename space")
                    .on_click(cx.listener(
                        move |this, event: &gpui_kit::ClickEvent, window, cx| {
                            this.open_rename_folder_modal(&rename_id, event.position(), window, cx);
                            cx.notify();
                        },
                    )),
            )
            .child(
                context_menu_item("ctx-delete-folder", theme, IconName::Trash, "Delete space")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete_folder(&delete_id);
                        this.folder_context_menu = None;
                        cx.notify();
                    })),
            )
            .with_spring(
                format!(
                    "folder-menu-{}-{folder_id}-{serial}",
                    if is_closing { "exit" } else { "enter" }
                ),
                if is_closing {
                    ui::motion::popover().between(1.0, 0.0)
                } else {
                    ui::motion::popover().enter()
                },
                move |menu, progress| {
                    menu.w(px(width * (0.96 + 0.04 * progress)))
                        .opacity(ui::motion::clamp_percentage(progress))
                },
            );
        if is_closing {
            Some(panel.into_any_element())
        } else {
            Some(menu_layer.child(panel).into_any_element())
        }
    }
}
