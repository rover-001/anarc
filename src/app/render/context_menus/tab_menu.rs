//! Tab context menu, split-target picker, and folder/pin actions.

use super::{context_menu_item, dismiss_backdrop};
use super::super::*;

impl Anarc {
    pub(super) fn render_tab_context_menu(
        &mut self,
        theme: visual::SpaceTheme,
        tabs: &[(
            String,
            String,
            Option<std::sync::Arc<gpui_kit::RenderImage>>,
        )],
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let menu = self.tab_context_menu.as_ref()?;
        let target_id = menu.tab_id.clone();
        let menu_width = menu.width;
        let showing_split_targets = menu.showing_split_targets;
        let menu_x = menu.x;
        let menu_y = menu.y;
        let menu_height = menu.height;

        let menu_layer = dismiss_backdrop(
            "tab-context-dismiss-layer",
            |this, _, _| {
                this.tab_context_menu = None;
            },
            cx,
        );
        let mut panel = div()
            .id("tab-context-menu")
            .occlude()
            .absolute()
            .top(px(menu_y))
            .left(px(menu_x))
            .w(px(menu_width))
            .max_h(px(menu_height))
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
            );

        if showing_split_targets && crate::plugins::split_view_enabled() {
            panel = self.append_split_target_items(panel, theme, &target_id, tabs, cx);
        } else {
            panel = self.append_tab_menu_items(panel, theme, &target_id, cx);
        }

        let panel = panel.with_spring(
            format!("tab-menu-entrance-{target_id}"),
            ui::motion::popover().enter(),
            move |panel, progress| {
                panel
                    .w(px(menu_width * (0.96 + 0.04 * progress)))
                    .opacity(ui::motion::clamp_percentage(progress))
            },
        );
        Some(menu_layer.child(panel).into_any_element())
    }

    fn append_split_target_items(
        &self,
        mut panel: gpui_kit::Stateful<gpui_kit::Div>,
        theme: visual::SpaceTheme,
        target_id: &str,
        tabs: &[(
            String,
            String,
            Option<std::sync::Arc<gpui_kit::RenderImage>>,
        )],
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        panel = panel.child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(visual::panel_muted(theme)))
                .child("Split with…"),
        );
        for (id, title, favicon) in tabs {
            if id == target_id {
                continue;
            }
            let other_id = id.clone();
            panel = panel.child(
                div()
                    .id(format!("split-target-{id}"))
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| {
                        style.bg(crate::ui::visual::translucent_surface(
                            theme.surface_hover,
                            0x40,
                        ))
                    })
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(tab_list::favicon(favicon.clone()))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title.clone()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_split_target(&other_id);
                        cx.notify();
                    })),
            );
        }
        panel
    }

    fn append_tab_menu_items(
        &self,
        mut panel: gpui_kit::Stateful<gpui_kit::Div>,
        theme: visual::SpaceTheme,
        target_id: &str,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let target_tab = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == target_id);
        let is_split = self
            .active_space()
            .split
            .as_ref()
            .is_some_and(|pair| pair.panes().contains(&target_id));
        // Favorites and pins are one section; `fav`/`pin` stay aliases.
        let target_kept = target_tab.is_some_and(|tab| tab.is_pinned());
        let target_grouped = target_tab.is_some_and(|tab| tab.kept_group.is_some());
        let target_folder = target_tab.and_then(|tab| tab.folder.clone());
        let target_folders = self
            .active_space()
            .folders
            .iter()
            .map(|folder| (folder.id.clone(), folder.name.clone()))
            .collect::<Vec<_>>();

        let pin_id = target_id.to_owned();
        panel = panel.child(
            context_menu_item(
                "ctx-pin",
                theme,
                if target_kept {
                    IconName::PinOff
                } else {
                    IconName::Pin
                },
                if target_kept { "Unpin tab" } else { "Pin tab" },
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.pin_tab(&pin_id, !target_kept);
                this.tab_context_menu = None;
                cx.notify();
            })),
        );
        if target_grouped {
            let detach_id = target_id.to_owned();
            panel = panel.child(
                context_menu_item(
                    "ctx-detach-kept",
                    theme,
                    IconName::PanelTop,
                    "Move to own row",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_kept_group(&detach_id, None);
                    this.tab_context_menu = None;
                    cx.notify();
                })),
            );
        }
        if target_kept
            && target_tab
                .and_then(|t| t.pinned_url.as_deref())
                .is_some_and(|p_url| target_tab.is_some_and(|t| t.url != p_url))
        {
            let reset_id = target_id.to_owned();
            panel = panel.child(
                context_menu_item(
                    "ctx-reset-pinned",
                    theme,
                    IconName::RotateCw,
                    "Reset to pinned URL",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.reset_tab_to_pinned_url(&reset_id);
                    this.tab_context_menu = None;
                    cx.notify();
                })),
            );
        }

        panel = panel
            .child(div().h(px(1.0)).mx_1().bg(rgb(theme.border)))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(visual::panel_muted(theme)))
                    .child("Spaces"),
            );
        for (folder_id, folder_name) in &target_folders {
            if target_folder.as_deref() == Some(folder_id.as_str()) {
                continue;
            }
            let move_id = target_id.to_owned();
            let move_folder = folder_id.clone();
            panel = panel.child(
                context_menu_item(
                    format!("ctx-folder-{folder_id}"),
                    theme,
                    IconName::Folder,
                    format!("Move to {folder_name}"),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.move_tab_to_folder(&move_id, &move_folder);
                    this.tab_context_menu = None;
                    cx.notify();
                })),
            );
        }
        if target_folder.is_some() {
            let remove_id = target_id.to_owned();
            panel = panel.child(
                context_menu_item(
                    "ctx-remove-folder",
                    theme,
                    IconName::FolderX,
                    "Remove from space",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.remove_tab_from_folder(&remove_id);
                    this.tab_context_menu = None;
                    cx.notify();
                })),
            );
        }
        let new_folder_target = target_id.to_owned();
        panel = panel.child(
            context_menu_item("ctx-new-folder", theme, IconName::FolderPlus, "New space…")
                .on_click(cx.listener(
                    move |this, event: &gpui_kit::ClickEvent, window, cx| {
                        this.open_create_folder_modal(
                            Some(new_folder_target.clone()),
                            event.position(),
                            window,
                            cx,
                        );
                        cx.notify();
                    },
                )),
        );

        panel = panel
            .child(div().h(px(1.0)).mx_1().bg(rgb(theme.border)))
            .child(
                context_menu_item("ctx-split-with", theme, IconName::Columns2, "Split with…")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(menu) = &mut this.tab_context_menu {
                            menu.showing_split_targets = true;
                        }
                        cx.notify();
                    })),
            );
        if is_split && crate::plugins::split_view_enabled() {
            panel = panel.child(
                context_menu_item("ctx-unsplit", theme, IconName::Columns2, "Unsplit").on_click(
                    cx.listener(|this, _, _, cx| {
                        this.exit_split();
                        this.tab_context_menu = None;
                        cx.notify();
                    }),
                ),
            );
        }

        let reload_id = target_id.to_owned();
        let duplicate_id = target_id.to_owned();
        let copy_url = target_tab.map(|tab| tab.url.clone()).unwrap_or_default();
        let close_id = target_id.to_owned();
        let other_ids: Vec<String> = self
            .active_space()
            .tabs
            .iter()
            .filter(|tab| tab.id != target_id)
            .map(|tab| tab.id.clone())
            .collect();
        let keep_id = target_id.to_owned();

        panel
            .child(
                context_menu_item("ctx-reload", theme, IconName::RotateCw, "Reload").on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.engine.reload(&reload_id, false);
                        this.tab_context_menu = None;
                        cx.notify();
                    }),
                ),
            )
            .child(
                context_menu_item("ctx-duplicate", theme, IconName::CopyPlus, "Duplicate tab")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.duplicate_tab(&duplicate_id);
                        cx.notify();
                    })),
            )
            .child(
                context_menu_item("ctx-copy-link", theme, IconName::Copy, "Copy link").on_click(
                    cx.listener(move |this, _, _, cx| {
                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                            copy_url.clone(),
                        ));
                        this.tab_context_menu = None;
                        cx.notify();
                    }),
                ),
            )
            .child(
                context_menu_item("ctx-close-tab", theme, IconName::X, "Close tab").on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.close_section_tab(&close_id);
                        this.tab_context_menu = None;
                        cx.notify();
                    }),
                ),
            )
            .child(
                context_menu_item("ctx-close-others", theme, IconName::X, "Close other tabs")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        for id in &other_ids {
                            this.close_section_tab(id);
                        }
                        this.active_space_mut().active = Some(keep_id.clone());
                        this.emit_plugin_text("tab.activated", keep_id.clone());
                        this.active_space_mut().split = None;
                        this.focused_pane = 0;
                        this.tab_context_menu = None;
                        this.save_state();
                        cx.notify();
                    })),
            )
    }
}
