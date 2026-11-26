//! Extensions & ad-blocker toolbar popover.

use super::dismiss_backdrop;
use super::super::*;

impl Anarc {
    pub(super) fn render_plugins_menu(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        if !self.plugins_menu_open {
            return None;
        }
        let ad_blocker_enabled = self.engine.ad_blocker_enabled();
        let active_count = if ad_blocker_enabled { 1 } else { 0 };
        let dismiss = dismiss_backdrop(
            "plugins-menu-dismiss",
            |this, _, _| {
                this.plugins_menu_open = false;
            },
            cx,
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
            .child(render_ad_blocker_row(theme, ad_blocker_enabled, cx))
            .child(render_manage_extensions_row(theme, cx));
        Some(dismiss.child(panel).into_any_element())
    }
}

fn render_ad_blocker_row(
    theme: visual::SpaceTheme,
    ad_blocker_enabled: bool,
    cx: &mut Context<Anarc>,
) -> impl IntoElement {
    div()
        .id("toggle-ad-blocker")
        .px_2()
        .py_2()
        .w_full()
        .min_w_0()
        .rounded_md()
        .hover(move |style| {
            style.bg(crate::ui::visual::translucent_surface(
                theme.surface_hover,
                0x40,
            ))
        })
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
                        .when(ad_blocker_enabled, |thumb| thumb.ml(px(14.0 * UI_SCALE))),
                )
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    let enabled = !this.engine.ad_blocker_enabled();
                    this.set_ad_blocker_enabled(enabled);
                    cx.notify();
                })),
        )
}

fn render_manage_extensions_row(
    theme: visual::SpaceTheme,
    cx: &mut Context<Anarc>,
) -> impl IntoElement {
    div()
        .id("manage-extensions-btn")
        .px_2()
        .py_2()
        .w_full()
        .min_w_0()
        .rounded_md()
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
        }))
}
