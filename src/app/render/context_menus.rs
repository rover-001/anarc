use super::*;

impl Anarc {
    pub(in crate::app) fn render_context_menus(
        &mut self,
        theme: visual::SpaceTheme,
        tabs: &[(
            String,
            String,
            Option<std::sync::Arc<gpui_kit::RenderImage>>,
        )],
        cx: &mut Context<Self>,
    ) -> Vec<gpui_kit::AnyElement> {
        let mut overlays = Vec::new();
        if self.plugins_menu_open {
            let ad_blocker_enabled = self.engine.ad_blocker_enabled();
            let active_count = if ad_blocker_enabled { 1 } else { 0 };
            let dismiss = div()
                .id("plugins-menu-dismiss")
                .occlude()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.plugins_menu_open = false;
                        if this.app_page.is_none() {
                            this.focus_browser(window, cx);
                        }
                        cx.notify();
                    }),
                );
            let panel = div()
                .id("plugins-menu-panel")
                .occlude()
                .absolute()
                .top(px(visual::active_topbar_height()))
                .right(px(8.0 * UI_SCALE))
                .w(px(300.0 * UI_SCALE))
                .max_h(px(480.0 * UI_SCALE))
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
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(visual::panel_muted(theme)))
                                .child("Extensions"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(visual::panel_muted(theme)))
                                .child(format!("{active_count} active")),
                        ),
                )
                .child(
                    div()
                        .id("toggle-ad-blocker")
                        .px_2()
                        .py_2()
                        .w_full()
                        .min_w_0()
                        .rounded_md()
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .flex_1()
                                .child(Icon::new(IconName::Shield).text_color(rgb(
                                    if ad_blocker_enabled {
                                        0x76d7b2
                                    } else {
                                        theme.muted
                                    },
                                )))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text))
                                                .whitespace_nowrap()
                                                .child("uBlock Origin (Built-in)"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(visual::panel_muted(theme)))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .child("Blocks ads, popups & YouTube ads"),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .id("ad-blocker-toggle-switch")
                                .flex_shrink_0()
                                .w(px(34.0 * UI_SCALE))
                                .h(px(20.0 * UI_SCALE))
                                .rounded_full()
                                .bg(rgb(if ad_blocker_enabled {
                                    theme.accent
                                } else {
                                    theme.border
                                }))
                                .flex()
                                .items_center()
                                .justify_start()
                                .px(px(2.0 * UI_SCALE))
                                .child(
                                    div()
                                        .size(px(16.0 * UI_SCALE))
                                        .rounded_full()
                                        .bg(rgb(0xffffff))
                                        .when(ad_blocker_enabled, |thumb| {
                                            thumb.ml(px(14.0 * UI_SCALE))
                                        }),
                                )
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let enabled = !this.engine.ad_blocker_enabled();
                                    this.set_ad_blocker_enabled(enabled);
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .id("manage-extensions-btn")
                        .px_2()
                        .py_2()
                        .w_full()
                        .min_w_0()
                        .rounded_md()
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Puzzle))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(theme.text))
                                        .whitespace_nowrap()
                                        .child("Manage extensions & plugins"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(visual::panel_muted(theme)))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child("Install packs, userstyles & userscripts"),
                                ),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.plugins_menu_open = false;
                            this.settings_section = SettingsSection::Plugins;
                            this.app_page = Some(AppPage::Settings);
                            cx.notify();
                        })),
                );
            overlays.push(dismiss.child(panel).into_any_element());
        }
        let downloads = self.engine.downloads();
        if let Some(latest) = downloads.last() {
            let is_active = !latest.complete && !latest.canceled;
            let show_toast = (is_active || latest.complete)
                && self.dismissed_download_toast_id != Some(latest.id)
                && self.app_page != Some(AppPage::Downloads);

            if show_toast {
                let filename = latest
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "download".to_owned());
                let download_id = latest.id;
                let is_complete = latest.complete;
                let pct = if latest.total_bytes > 0 {
                    (latest.received_bytes as f32 / latest.total_bytes as f32).clamp(0.0, 1.0)
                } else if is_complete {
                    1.0
                } else {
                    0.0
                };
                let status_text = if is_complete {
                    "Download complete · Saved to ~/Downloads".to_string()
                } else if latest.total_bytes > 0 {
                    format!(
                        "Downloading... {} / {}",
                        ui::app_pages::format_bytes(latest.received_bytes),
                        ui::app_pages::format_bytes(latest.total_bytes)
                    )
                } else {
                    format!(
                        "Downloading... {}",
                        ui::app_pages::format_bytes(latest.received_bytes)
                    )
                };
                let bar_width = 250.0 * UI_SCALE;
                let fill_width = (bar_width * pct).max(if is_active { 4.0 } else { 0.0 });

                let toast = div()
                    .id("download-toast")
                    .occlude()
                    .absolute()
                    .top(px(visual::active_topbar_height() + 4.0 * UI_SCALE))
                    .right(px(8.0 * UI_SCALE))
                    .w(px(280.0 * UI_SCALE))
                    .bg(rgb(visual::panel_surface(theme.surface)))
                    .border_1()
                    .border_color(rgb(if is_active {
                        theme.accent
                    } else {
                        theme.border
                    }))
                    .rounded_xl()
                    .p_3()
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.app_page = Some(AppPage::Downloads);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .w_full()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(Icon::new(IconName::Download).text_color(rgb(
                                        if is_complete { 0x76d7b2 } else { theme.accent },
                                    )))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(theme.text))
                                            .child(if is_complete {
                                                "Download Complete"
                                            } else {
                                                "Download Started"
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("dismiss-download-toast")
                                    .px_1()
                                    .py_0p5()
                                    .rounded_md()
                                    .hover(move |s| s.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                                    .text_color(rgb(visual::panel_muted(theme)))
                                    .text_xs()
                                    .child("✕")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.dismissed_download_toast_id = Some(download_id);
                                        cx.stop_propagation();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(filename),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(if is_complete { 0x76d7b2 } else { theme.muted }))
                            .child(status_text),
                    )
                    .when(!is_complete, |parent| {
                        parent.child(
                            div()
                                .w(px(bar_width))
                                .h(px(4.0 * UI_SCALE))
                                .rounded_full()
                                .bg(rgb(theme.border))
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .rounded_full()
                                        .bg(rgb(theme.accent))
                                        .w(px(fill_width)),
                                ),
                        )
                    });

                overlays.push(toast.into_any_element());
            }
        }
        if let Some(menu) = &self.sidebar_context_menu {
            let x = menu.x;
            let y = menu.y;
            let width = menu.width;
            let height = menu.height;
            let dismiss = div()
                .id("sidebar-context-dismiss")
                .occlude()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.sidebar_context_menu = None;
                        if this.app_page.is_none() {
                            this.focus_browser(window, cx);
                        }
                        cx.notify();
                    }),
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
                    div()
                        .id("sidebar-create-folder")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::FolderPlus))
                        .child("Create folder")
                        .on_click(cx.listener(
                            move |this, event: &gpui_kit::ClickEvent, window, cx| {
                                this.sidebar_context_menu = None;
                                this.open_create_folder_modal(None, event.position(), window, cx);
                                cx.notify();
                            },
                        )),
                )
                .child(
                    div()
                        .id("sidebar-change-theme")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Star))
                        .child("Change theme")
                        .on_click(cx.listener(
                            move |this, event: &gpui_kit::ClickEvent, window, cx| {
                                this.open_theme_editor(event.position(), false, window, cx);
                                cx.notify();
                            },
                        )),
                );
            overlays.push(dismiss.child(panel).into_any_element());
        }

        if let Some(editor) = &self.theme_editor {
            let x = editor.x;
            let y = editor.y;
            let selected = editor.selected_color;
            let colors = if self.theme_picker_space().theme_colors.is_empty() {
                vec![self.theme_picker_space().color]
            } else {
                self.theme_picker_space().theme_colors.clone()
            };
            let dismiss = div()
                .id("theme-editor-dismiss")
                .occlude()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.theme_editor = None;
                        if this.app_page.is_none() {
                            this.focus_browser(window, cx);
                        }
                        cx.notify();
                    }),
                );
            let panel = self.render_theme_picker(theme, x, y, selected, &colors, cx);
            overlays.push(dismiss.child(panel).into_any_element());
        }

        if let Some(menu) = &self.space_context_menu
            && let Some(target_space) = self.spaces.iter().find(|s| s.id == menu.space_id)
        {
            let menu_layer = div()
                .id("space-context-dismiss-layer")
                .occlude()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.space_context_menu = None;
                        if this.app_page.is_none() {
                            this.focus_browser(window, cx);
                        }
                        cx.notify();
                    }),
                );
            let target_id = target_space.id.clone();
            let target_id_edit = target_id.clone();
            let target_id_del = target_id.clone();
            let can_delete = self.spaces.len() > 1;

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
                        .child(space_switcher::render_space_icon(&target_space.icon))
                        .child(target_space.name.clone()),
                )
                .child(
                    div()
                        .id("ctx-edit-space")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Pencil))
                        .child("Rename & Change Icon")
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
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Trash))
                        .child("Delete Space")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.delete_space(&target_id_del);
                            cx.notify();
                        })),
                );
            }

            let ctx_menu = ctx_menu.with_spring(
                format!("space-menu-entrance-{}", target_space.id),
                ui::motion::popover().enter(),
                |menu, progress| {
                    menu.w(px(224.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                        .opacity(ui::motion::clamp_percentage(progress))
                },
            );
            overlays.push(menu_layer.child(ctx_menu).into_any_element());
        }

        if let Some(menu) = &self.tab_context_menu {
            let target_id = menu.tab_id.clone();
            let menu_width = menu.width;
            let target_tab = self
                .active_space()
                .tabs
                .iter()
                .find(|tab| tab.id == target_id);
            let is_split = self
                .active_space()
                .split
                .as_ref()
                .is_some_and(|pair| pair.panes().contains(&target_id.as_str()));
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
            let menu_layer = div()
                .id("tab-context-dismiss-layer")
                .occlude()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.tab_context_menu = None;
                        if this.app_page.is_none() {
                            this.focus_browser(window, cx);
                        }
                        cx.notify();
                    }),
                );
            let mut panel = div()
                .id("tab-context-menu")
                .occlude()
                .absolute()
                .top(px(menu.y))
                .left(px(menu.x))
                .w(px(menu.width))
                .max_h(px(menu.height))
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

            if menu.showing_split_targets && crate::plugins::split_view_enabled() {
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
                    if id == &target_id {
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
                            .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
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
            } else {
                let pin_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-pin")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(if target_kept {
                            IconName::PinOff
                        } else {
                            IconName::Pin
                        }))
                        .child(if target_kept {
                            "Unpin tab"
                        } else {
                            "Pin tab"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pin_tab(&pin_id, !target_kept);
                            this.tab_context_menu = None;
                            cx.notify();
                        })),
                );
                if target_grouped {
                    let detach_id = target_id.clone();
                    panel = panel.child(
                        div()
                            .id("ctx-detach-kept")
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::PanelTop))
                            .child("Move to own row")
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
                    let reset_id = target_id.clone();
                    panel = panel.child(
                        div()
                            .id("ctx-reset-pinned")
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::RotateCw))
                            .child("Reset to pinned URL")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.reset_tab_to_pinned_url(&reset_id);
                                this.tab_context_menu = None;
                                cx.notify();
                            })),
                    );
                }
                panel = panel.child(div().h(px(1.0)).mx_1().bg(rgb(theme.border)));
                panel = panel.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(visual::panel_muted(theme)))
                        .child("Folders"),
                );
                for (folder_id, folder_name) in &target_folders {
                    if target_folder.as_deref() == Some(folder_id.as_str()) {
                        continue;
                    }
                    let move_id = target_id.clone();
                    let move_folder = folder_id.clone();
                    panel = panel.child(
                        div()
                            .id(format!("ctx-folder-{folder_id}"))
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::Folder))
                            .child(format!("Move to {folder_name}"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.move_tab_to_folder(&move_id, &move_folder);
                                this.tab_context_menu = None;
                                cx.notify();
                            })),
                    );
                }
                if target_folder.is_some() {
                    let remove_id = target_id.clone();
                    panel = panel.child(
                        div()
                            .id("ctx-remove-folder")
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::FolderX))
                            .child("Remove from folder")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_tab_from_folder(&remove_id);
                                this.tab_context_menu = None;
                                cx.notify();
                            })),
                    );
                }
                let new_folder_target = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-new-folder")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::FolderPlus))
                        .child("New folder…")
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
                panel = panel.child(div().h(px(1.0)).mx_1().bg(rgb(theme.border)));
                panel = panel.child(
                    div()
                        .id("ctx-split-with")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Columns2))
                        .child("Split with…")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(menu) = &mut this.tab_context_menu {
                                menu.showing_split_targets = true;
                            }
                            cx.notify();
                        })),
                );
                if is_split && crate::plugins::split_view_enabled() {
                    panel = panel.child(
                        div()
                            .id("ctx-unsplit")
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::Columns2))
                            .child("Unsplit")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.exit_split();
                                this.tab_context_menu = None;
                                cx.notify();
                            })),
                    );
                }
                let reload_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-reload")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::RotateCw))
                        .child("Reload")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.engine.reload(&reload_id, false);
                            this.tab_context_menu = None;
                            cx.notify();
                        })),
                );
                let duplicate_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-duplicate")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::CopyPlus))
                        .child("Duplicate tab")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.duplicate_tab(&duplicate_id);
                            cx.notify();
                        })),
                );
                let copy_url = target_tab.map(|tab| tab.url.clone()).unwrap_or_default();
                panel = panel.child(
                    div()
                        .id("ctx-copy-link")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Copy))
                        .child("Copy link")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                copy_url.clone(),
                            ));
                            this.tab_context_menu = None;
                            cx.notify();
                        })),
                );
                let close_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-close-tab")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::X))
                        .child("Close tab")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_section_tab(&close_id);
                            this.tab_context_menu = None;
                            cx.notify();
                        })),
                );
                let other_ids: Vec<String> = self
                    .active_space()
                    .tabs
                    .iter()
                    .filter(|tab| tab.id != target_id)
                    .map(|tab| tab.id.clone())
                    .collect();
                let keep_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-close-others")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::X))
                        .child("Close other tabs")
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
                );
            }
            let panel = panel.with_spring(
                format!("tab-menu-entrance-{}", menu.tab_id),
                ui::motion::popover().enter(),
                move |panel, progress| {
                    panel
                        .w(px(menu_width * (0.96 + 0.04 * progress)))
                        .opacity(ui::motion::clamp_percentage(progress))
                },
            );
            overlays.push(menu_layer.child(panel).into_any_element());
        }

        if let Some(menu) = &self.folder_context_menu {
            let folder_id = menu.folder_id.clone();
            let folder_name = self
                .active_space()
                .folders
                .iter()
                .find(|folder| folder.id == folder_id)
                .map(|folder| folder.name.clone())
                .unwrap_or_default();
            let menu_layer = div()
                .id("folder-context-dismiss-layer")
                .occlude()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.folder_context_menu = None;
                        if this.app_page.is_none() {
                            this.focus_browser(window, cx);
                        }
                        cx.notify();
                    }),
                );
            let mut panel = div()
                .id("folder-context-menu")
                .occlude()
                .absolute()
                .top(px(menu.y))
                .left(px(menu.x))
                .w(px(menu.width))
                .max_h(px(menu.height))
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
                );
            let rename_id = folder_id.clone();
            panel = panel.child(
                div()
                    .id("ctx-rename-folder")
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Pencil))
                    .child("Rename folder")
                    .on_click(cx.listener(
                        move |this, event: &gpui_kit::ClickEvent, window, cx| {
                            this.open_rename_folder_modal(&rename_id, event.position(), window, cx);
                            cx.notify();
                        },
                    )),
            );
            let delete_id = folder_id.clone();
            panel = panel.child(
                div()
                    .id("ctx-delete-folder")
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Trash))
                    .child("Delete folder")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete_folder(&delete_id);
                        this.folder_context_menu = None;
                        cx.notify();
                    })),
            );
            overlays.push(menu_layer.child(panel).into_any_element());
        }

        if let Some(menu) = &self.web_context_menu {
            let menu_tab_id = menu.tab_id.clone();
            let menu_layer = div()
                .id("web-context-dismiss-layer")
                .occlude()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.web_context_menu = None;
                        if this.app_page.is_none() {
                            this.focus_browser(window, cx);
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    gpui_kit::MouseButton::Right,
                    cx.listener(|this, _, window, cx| {
                        this.web_context_menu = None;
                        if this.app_page.is_none() {
                            this.focus_browser(window, cx);
                        }
                        cx.notify();
                    }),
                );

            let mut panel = div()
                .id("web-context-menu-panel")
                .occlude()
                .absolute()
                .left(px(menu.x))
                .top(px(menu.y))
                .w(px(180.0 * UI_SCALE))
                .bg(rgb(visual::panel_surface(theme.surface)))
                .border_1()
                .border_color(rgb(theme.border))
                .rounded_lg()
                .shadow_lg()
                .p_1()
                .flex()
                .flex_col()
                .gap_0p5();

            let copy_id = menu_tab_id.clone();
            panel = panel.child(
                div()
                    .id("ctx-web-copy")
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Copy))
                    .child("Copy")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(text) = this.engine.get_selected_text(&copy_id) {
                            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text.clone()));
                            crate::engine::clipboard::write_system_clipboard(&text);
                        }
                        this.engine.copy(&copy_id);
                        let script = r#"(function() {
                            try {
                                function getSel() {
                                    let text = window.getSelection ? window.getSelection().toString() : '';
                                    if (text) return text;
                                    let el = document.activeElement;
                                    while (el && el.shadowRoot && el.shadowRoot.activeElement) {
                                        el = el.shadowRoot.activeElement;
                                    }
                                    if (el && 'selectionStart' in el && 'selectionEnd' in el && typeof el.value === 'string') {
                                        const start = Math.min(el.selectionStart, el.selectionEnd);
                                        const end = Math.max(el.selectionStart, el.selectionEnd);
                                        if (end > start) return el.value.substring(start, end);
                                    }
                                    return '';
                                }
                                const s = getSel();
                                if (s) {
                                    console.log("[ANARC_CB_COPY]" + encodeURIComponent(s));
                                }
                            } catch (_) {}
                        })();"#;
                        this.engine.execute_script(&copy_id, script);
                        this.web_context_menu = None;
                        cx.notify();
                    })),
            );

            let paste_id = menu_tab_id.clone();
            panel = panel.child(
                div()
                    .id("ctx-web-paste")
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Clipboard))
                    .child("Paste")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.engine.paste(&paste_id);
                        this.web_context_menu = None;
                        cx.notify();
                    })),
            );

            let select_all_id = menu_tab_id.clone();
            panel = panel.child(
                div()
                    .id("ctx-web-select-all")
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Check))
                    .child("Select All")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.engine.select_all(&select_all_id);
                        this.web_context_menu = None;
                        cx.notify();
                    })),
            );

            panel = panel.child(div().h(px(1.0)).mx_1().my_0p5().bg(rgb(theme.border)));

            let reload_id = menu_tab_id.clone();
            panel = panel.child(
                div()
                    .id("ctx-web-reload")
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::RotateCw))
                    .child("Reload")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.engine.reload(&reload_id, false);
                        this.web_context_menu = None;
                        cx.notify();
                    })),
            );

            let back_id = menu_tab_id.clone();
            panel = panel.child(
                div()
                    .id("ctx-web-back")
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::ArrowLeft))
                    .child("Back")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.engine.go_back(&back_id);
                        this.web_context_menu = None;
                        cx.notify();
                    })),
            );

            let fwd_id = menu_tab_id.clone();
            panel = panel.child(
                div()
                    .id("ctx-web-forward")
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::ArrowRight))
                    .child("Forward")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.engine.go_forward(&fwd_id);
                        this.web_context_menu = None;
                        cx.notify();
                    })),
            );

            overlays.push(menu_layer.child(panel).into_any_element());
        }

        overlays
    }
}
