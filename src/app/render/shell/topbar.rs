use super::*;

pub(super) struct TopbarParams {
    pub(super) theme: visual::SpaceTheme,
    pub(super) topbar_height: f32,
    pub(super) topbar_expands: bool,
    pub(super) topbar_left: f32,
    pub(super) window_width: f32,
    pub(super) sidebar_width_f32: f32,
    pub(super) sidebar_on_right: bool,
    pub(super) show_address_preview: bool,
    pub(super) address_preview: String,
    pub(super) active_tab_url: String,
}

pub(super) fn render(
    this: &mut Anarc,
    window: &mut Window,
    cx: &mut Context<Anarc>,
    params: TopbarParams,
) -> gpui_kit::AnyElement {
    let TopbarParams {
        theme,
        topbar_height,
        topbar_expands,
        topbar_left,
        window_width,
        sidebar_width_f32,
        sidebar_on_right,
        show_address_preview,
        address_preview,
        active_tab_url,
    } = params;
    div()
        .id("topbar-container")
        .relative()
        .h(px(topbar_height))
        .flex()
        .items_center()
        .gap_2()
        .px_4()
        .when(topbar_expands, |bar| {
            bar.ml(px(topbar_left)).w(px(window_width))
        })
        .when(topbar_expands, |bar| {
            bar.child(
                div()
                    .absolute()
                    .left(px(14.0 + 4.0 * UI_SCALE))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(sidebar::sidebar_logo_toggle_button(
                        "topbar-sidebar-toggle",
                        theme,
                        cx,
                    )),
            )
        })
        .child(
            div()
                .id("navigation-controls")
                .flex()
                .items_center()
                .gap_1()
                .when(topbar_expands && !sidebar_on_right, |bar| {
                    bar.pl(px(sidebar_width_f32))
                })
                .child({
                    let can_go_back = this.engine.can_go_back(this.focused_tab_id());
                    div()
                        .id("back-btn")
                        .size(px(NAV_BUTTON_SIZE))
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(if can_go_back { theme.text } else { theme.muted }))
                        .opacity(if can_go_back { 1.0 } else { 0.4 })
                        .when(can_go_back, |btn| {
                            btn.cursor_pointer()
                                .active(|s| s.opacity(0.92))
                                .hover(move |s| {
                                    s.bg(crate::ui::visual::translucent_surface(
                                        theme.surface_hover,
                                        0x40,
                                    ))
                                    .text_color(rgb(theme.text))
                                })
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_app_page();
                            this.address_editing = false;
                            this.show_suggestions = false;
                            this.selected_suggestion = None;
                            this.engine.go_back(this.focused_tab_id());
                            this.focus_browser(window, cx);
                            cx.notify();
                        }))
                        .child(Icon::new(IconName::ArrowLeft).size(px(16.0 * UI_SCALE)))
                })
                .child({
                    let can_go_forward = this.engine.can_go_forward(this.focused_tab_id());
                    div()
                        .id("forward-btn")
                        .size(px(NAV_BUTTON_SIZE))
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(if can_go_forward {
                            theme.text
                        } else {
                            theme.muted
                        }))
                        .opacity(if can_go_forward { 1.0 } else { 0.4 })
                        .when(can_go_forward, |btn| {
                            btn.cursor_pointer()
                                .active(|s| s.opacity(0.92))
                                .hover(move |s| {
                                    s.bg(crate::ui::visual::translucent_surface(
                                        theme.surface_hover,
                                        0x40,
                                    ))
                                    .text_color(rgb(theme.text))
                                })
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_app_page();
                            this.address_editing = false;
                            this.show_suggestions = false;
                            this.selected_suggestion = None;
                            this.engine.go_forward(this.focused_tab_id());
                            this.focus_browser(window, cx);
                            cx.notify();
                        }))
                        .child(Icon::new(IconName::ArrowRight).size(px(16.0 * UI_SCALE)))
                })
                .child({
                    div()
                        .id("reload-btn")
                        .size(px(NAV_BUTTON_SIZE))
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(theme.muted))
                        .cursor_pointer()
                        .active(|s| s.opacity(0.92))
                        .hover(move |s| {
                            s.bg(crate::ui::visual::translucent_surface(
                                theme.surface_hover,
                                0x40,
                            ))
                            .text_color(rgb(theme.text))
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_app_page();
                            this.address_editing = false;
                            this.show_suggestions = false;
                            this.selected_suggestion = None;
                            this.reload_active(false);
                            this.focus_browser(window, cx);
                            cx.notify();
                        }))
                        .child(Icon::new(IconName::RotateCw).size(px(15.0 * UI_SCALE)))
                }),
        )
        .child(
            div()
                .id("address-bar-container")
                .flex_1()
                .h(px(ADDRESS_BAR_HEIGHT))
                .px_3()
                .rounded_lg()
                .overflow_hidden()
                .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
                .flex()
                .items_center()
                .gap_2()
                .cursor_text()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.browser_focused = false;
                    let tab_id = this.active_tab_id().to_string();
                    this.engine.set_focus(&tab_id, false);
                    this.address.update(cx, |input, cx| {
                        input.focus(window, cx);
                    });
                    let text = this.address.read(cx).value().to_string();
                    this.update_suggestions(&text);
                    cx.notify();
                }))
                .child(
                    div()
                        .text_color(rgb(theme.muted))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::Search)),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .child(
                            Input::new(&this.address)
                                .id("address-bar")
                                .w_full()
                                .appearance(false)
                                .text_color(if show_address_preview {
                                    gpui_kit::rgba(0x00000000)
                                } else {
                                    rgb(theme.text)
                                }),
                        )
                        .when(show_address_preview, |field| {
                            field.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_sm()
                                    .text_color(rgb(theme.text))
                                    .child(address_preview.clone()),
                            )
                        }),
                )
                .child({
                    let copy_url = active_tab_url.clone();
                    div()
                        .id("copy-page-link")
                        .size(px(24.0 * UI_SCALE))
                        .flex_shrink_0()
                        .rounded_md()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(theme.muted))
                        .cursor_pointer()
                        .hover(|style| {
                            style
                                .bg(crate::ui::visual::translucent_surface(
                                    theme.surface_hover,
                                    0x40,
                                ))
                                .text_color(rgb(theme.text))
                        })
                        .active(|style| style.opacity(0.82))
                        .child(Icon::new(IconName::Copy).size(px(14.0 * UI_SCALE)))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            if !copy_url.is_empty() {
                                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                    copy_url.clone(),
                                ));
                                crate::engine::clipboard::write_system_clipboard(&copy_url);
                            }
                            cx.stop_propagation();
                        }))
                }),
        )
        .child(
            div()
                .id("plugins-btn")
                .cursor_pointer()
                .active(|style| style.opacity(0.92))
                .child(space_switcher::plugins_button(theme))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.plugins_menu_open = !this.plugins_menu_open;
                    this.downloads_bubble_open = false;
                    cx.notify();
                })),
        )
        .when(crate::plugins::downloads_enabled(), |row| {
            let is_downloading = this
                .engine
                .downloads()
                .iter()
                .any(|d| !d.complete && !d.canceled);
            row.child(
                div()
                    .id("downloads-btn")
                    .cursor_pointer()
                    .active(|style| style.opacity(0.92))
                    .child(space_switcher::downloads_button(theme, is_downloading))
                    .on_click(cx.listener(|this, _, _, cx| {
                        // Chromium-style: the toolbar icon opens a compact
                        // recent-downloads bubble; the full page is a link in it.
                        this.downloads_bubble_open = !this.downloads_bubble_open;
                        this.plugins_menu_open = false;
                        cx.notify();
                    })),
            )
        })
        .when(cfg!(target_os = "windows"), |row| {
            let is_max = window.is_maximized();
            row.child(
                div()
                    .id("topbar-window-controls")
                    .flex()
                    .items_center()
                    .gap_1()
                    .ml_1()
                    .child(
                        div()
                            .id("window-control-minimize")
                            .size(px(26.0 * UI_SCALE))
                            .rounded_md()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_color(rgb(theme.muted))
                            .hover(move |style| {
                                style
                                    .bg(crate::ui::visual::translucent_surface(
                                        theme.surface_hover,
                                        0x40,
                                    ))
                                    .text_color(rgb(theme.text))
                            })
                            .active(|style| style.opacity(0.8))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.minimize_window();
                            }))
                            .child(Icon::new(IconName::WindowMinimize).size(px(14.0 * UI_SCALE))),
                    )
                    .child(
                        div()
                            .id("window-control-maximize")
                            .size(px(26.0 * UI_SCALE))
                            .rounded_md()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_color(rgb(theme.muted))
                            .hover(move |style| {
                                style
                                    .bg(crate::ui::visual::translucent_surface(
                                        theme.surface_hover,
                                        0x40,
                                    ))
                                    .text_color(rgb(theme.text))
                            })
                            .active(|style| style.opacity(0.8))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.zoom_window();
                            }))
                            .child(
                                Icon::new(if is_max {
                                    IconName::WindowRestore
                                } else {
                                    IconName::WindowMaximize
                                })
                                .size(px(14.0 * UI_SCALE)),
                            ),
                    )
                    .child(
                        div()
                            .id("window-control-close")
                            .size(px(26.0 * UI_SCALE))
                            .rounded_md()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_color(rgb(theme.muted))
                            .hover(|style| style.bg(rgb(0xe53935)).text_color(rgb(0xffffff)))
                            .active(|style| style.opacity(0.85))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.remove_window();
                            }))
                            .child(Icon::new(IconName::WindowClose).size(px(14.0 * UI_SCALE))),
                    ),
            )
        })
        .into_any_element()
}
