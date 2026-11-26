//! Sidebar header, toggle controls, new-tab button, and bottom space dock.

use super::open_sidebar_context_menu;
use super::super::*;

/// Button that switches between the expanded and icon-only sidebar.
pub(in crate::app::render) fn sidebar_toggle_button(
    id: &'static str,
    collapsed: bool,
    theme: visual::SpaceTheme,
    cx: &mut Context<Anarc>,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(22.0 * UI_SCALE))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(theme.muted))
        .cursor_pointer()
        .active(|style| style.opacity(0.92))
        .hover(move |style| {
            style
                .bg(crate::ui::visual::translucent_surface(
                    theme.surface_hover,
                    0x40,
                ))
                .text_color(rgb(theme.text))
        })
        .child(
            Icon::new(if collapsed {
                IconName::PanelLeftOpen
            } else {
                IconName::PanelLeftClose
            })
            .size(px(16.0 * UI_SCALE)),
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.animate_sidebar_toggle(window, cx);
            cx.notify();
        }))
}

/// Collapsed-state control: shows the Anarc logo until hovered, then swaps to
/// the panel-open icon that expands the sidebar.
pub(in crate::app::render) fn sidebar_logo_toggle_button(
    id: &'static str,
    theme: visual::SpaceTheme,
    cx: &mut Context<Anarc>,
) -> impl IntoElement {
    let group = format!("{id}-hover");
    let hover_group = group.clone();
    div()
        .id(id)
        .size(px(24.0 * UI_SCALE))
        .relative()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .group(group)
        .cursor_pointer()
        .active(|style| style.opacity(0.92))
        .hover(move |style| {
            style.bg(crate::ui::visual::translucent_surface(
                theme.surface_hover,
                0x40,
            ))
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .group_hover(hover_group.clone(), |style| style.invisible())
                .child(visual::anarc_logo().map_or_else(
                    || Icon::new(IconName::Sparkles).into_any_element(),
                    |logo| {
                        div()
                            .size(px(24.0 * UI_SCALE))
                            .flex_shrink_0()
                            .opacity(0.92)
                            .child(gpui_kit::img(logo).size_full())
                            .into_any_element()
                    },
                )),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .invisible()
                .group_hover(hover_group, |style| style.visible())
                .child(Icon::new(IconName::PanelLeftOpen).size(px(16.0 * UI_SCALE))),
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.animate_sidebar_toggle(window, cx);
            cx.notify();
        }))
}

pub(super) fn render_sidebar_header(
    sidebar_collapsed: bool,
    theme: visual::SpaceTheme,
    cx: &mut Context<Anarc>,
) -> impl IntoElement {
    div()
        .id("sidebar-header")
        .w_full()
        .h(px(visual::sidebar_header_height()))
        .flex()
        .items_center()
        .justify_between()
        .px_1()
        .mb_2()
        .on_mouse_down(
            gpui_kit::MouseButton::Right,
            cx.listener(open_sidebar_context_menu),
        )
        .child(visual::anarc_logo().map_or_else(
            || Icon::new(IconName::Sparkles).into_any_element(),
            |logo| {
                div()
                    .size(px(24.0 * UI_SCALE))
                    .flex_shrink_0()
                    .opacity(0.92)
                    .child(gpui_kit::img(logo).size_full())
                    .into_any_element()
            },
        ))
        .child(sidebar_toggle_button(
            "sidebar-header-toggle",
            sidebar_collapsed,
            theme,
            cx,
        ))
}

pub(super) fn render_new_tab_action(
    theme: visual::SpaceTheme,
    cx: &mut Context<Anarc>,
) -> impl IntoElement {
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
        .drag_over::<TabDrag>(move |style, _, _, _| {
            style.bg(crate::ui::visual::translucent_surface(
                theme.surface_hover,
                0x40,
            ))
        })
        .on_drop::<TabDrag>(cx.listener(|this, drag: &TabDrag, _, cx| {
            this.clear_tab_section(&drag.tab_id);
            this.move_tab_to_end(&drag.tab_id);
            cx.notify();
        }))
}

impl Anarc {
    pub(super) fn render_sidebar_bottom_bar(
        &self,
        theme: visual::SpaceTheme,
        active_space_id: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active_space = self.active_space();
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
            .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.space_context_menu = None;
                        this.activate_space(&id);
                        crate::ui::visual::update_theme_caret(
                            visual::theme_for_space(this.active_space()),
                            cx,
                        );
                        this.focus_browser(window, cx);
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

        div()
            .id("sidebar-bottom-bar")
            .w_full()
            .pt_1()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("sidebar-bottom-space-name")
                    .w_full()
                    .flex()
                    .justify_center()
                    .px_1()
                    .text_xs()
                    .text_center()
                    .text_color(rgb(theme.muted))
                    .opacity(0.75)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(active_space.name.clone()),
            )
            .child(
                div()
                    .id("sidebar-bottom-actions")
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .id("settings-btn")
                            .flex_shrink_0()
                            .child(space_switcher::settings_button(theme))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app_page = Some(AppPage::Settings);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("spaces-dock-container")
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(spaces_dock),
                    )
                    .child(
                        div()
                            .id("add-space-btn")
                            .flex_shrink_0()
                            .cursor_pointer()
                            .active(|style| style.opacity(0.92))
                            .child(space_switcher::add_space_button(theme))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_create_space_modal(window, cx);
                                cx.notify();
                            })),
                    ),
            )
    }
}
