//! Collapsed icon-only sidebar rail.

use super::SidebarTab;
use super::super::*;

impl Anarc {
    pub(super) fn render_collapsed_sidebar(
        &self,
        mut sidebar_content: gpui_kit::Stateful<gpui_kit::Div>,
        theme: visual::SpaceTheme,
        active_tab_id: Option<&str>,
        tabs: &[SidebarTab],
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let active_space = self.active_space();
        let active_space_icon = active_space.icon.clone();
        if self.preferences.sidebar_mode == crate::settings::SidebarMode::Icons {
            // The expanded sidebar spends this band on the space name;
            // collapsed, the full-width top bar occupies it. Size the
            // spacer so the first favicon row starts exactly where the
            // content card on the right begins.
            let header_height = visual::sidebar_header_height();
            sidebar_content = sidebar_content.child(
                div()
                    .id("collapsed-topbar-spacer")
                    .w_full()
                    .h(px(header_height)),
            );

            // Favorites and pins are one kept-tab concept. Show them above
            // a divider so the two groups read apart without per-tab
            // badges or fills.
            let (kept_tabs, normal_tabs): (Vec<_>, Vec<_>) =
                tabs.iter().cloned().partition(|(id, _, _)| {
                    active_space
                        .tabs
                        .iter()
                        .any(|tab| tab.id == *id && tab.is_pinned())
                });
            let has_kept = !kept_tabs.is_empty();
            let has_normal = !normal_tabs.is_empty();

            for (id, _, favicon) in kept_tabs {
                let selected = active_tab_id == Some(id.as_str());
                sidebar_content =
                    sidebar_content.child(collapsed_tab_row(id, favicon, selected, theme, cx));
            }
            if has_kept && has_normal {
                sidebar_content = sidebar_content.child(
                    div()
                        .id("collapsed-pins-divider")
                        .w_full()
                        .h(px(1.0))
                        .rounded_full()
                        .bg(rgb(theme.border)),
                );
            }
            for (id, _, favicon) in normal_tabs {
                let selected = active_tab_id == Some(id.as_str());
                sidebar_content =
                    sidebar_content.child(collapsed_tab_row(id, favicon, selected, theme, cx));
            }
            sidebar_content = sidebar_content.child(
                div()
                    .id("collapsed-new-tab-action")
                    .w_full()
                    .h(px(visual::TAB_ROW_HEIGHT))
                    .rounded_lg()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .active(|style| style.opacity(0.92))
                    .hover(|row| {
                        row.bg(visual::translucent_surface(theme.surface_hover, 0x40))
                    })
                    .child(Icon::new(IconName::Plus).size(px(visual::SIDEBAR_ROW_ICON_SIZE)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_tab();
                        this.address.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                            input.focus(window, cx);
                        });
                        cx.notify();
                    })),
            );
        }
        sidebar_content.child(div().flex_1()).child(
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
        )
    }
}

fn collapsed_tab_row(
    id: String,
    favicon: Option<std::sync::Arc<gpui_kit::RenderImage>>,
    selected: bool,
    theme: visual::SpaceTheme,
    cx: &mut Context<Anarc>,
) -> impl IntoElement {
    div()
        .id(format!("collapsed-tab-{id}"))
        .w_full()
        .h(px(visual::TAB_ROW_HEIGHT))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .active(|style| style.opacity(0.92))
        .when(selected, |row| {
            row.bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
        })
        .hover(|row| {
            row.bg(crate::ui::visual::translucent_surface(
                theme.surface_hover,
                0x40,
            ))
        })
        .child(tab_list::favicon(favicon))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.activate_tab_in_active_space(&id);
            this.focus_browser(window, cx);
            cx.notify();
        }))
}
