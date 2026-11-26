//! Sidebar rendering orchestrator, transitions, and shared context-menu helper.

mod collapsed_rail;
mod controls;
mod folders_section;
mod kept_section;
mod tabs_section;

use super::*;

pub(super) use controls::sidebar_logo_toggle_button;

/// (tab id, title, favicon) reused by the sidebar tab lists.
pub(super) type SidebarTab = (String, String, Option<std::sync::Arc<gpui_kit::RenderImage>>);
/// A kept tab row: an optional group label plus its member tabs.
pub(super) type KeptTabRow = (Option<String>, Vec<SidebarTab>);

pub(super) fn open_sidebar_context_menu(
    this: &mut Anarc,
    event: &gpui_kit::MouseDownEvent,
    window: &mut Window,
    cx: &mut Context<Anarc>,
) {
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
}

impl Anarc {
    pub(super) fn collect_sidebar_tabs(&self) -> Vec<SidebarTab> {
        self.active_space()
            .tabs
            .iter()
            .map(|tab| {
                let display_title = if tab.is_pinned()
                    && let Some(pinned_title) = &tab.pinned_title
                {
                    pinned_title.clone()
                } else {
                    tab.title.clone()
                };
                let display_url = if tab.is_pinned()
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
            .collect()
    }

    pub(super) fn render_sidebar(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let active_space_id = self.active_space.clone();
        // Sidebar rendering runs alongside browser frame updates; borrow the
        // active space instead of deep-cloning all of its tabs each time.
        let active_tab_id = self.active_space().active.clone();
        let tabs = self.collect_sidebar_tabs();
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

        let sidebar_collapsed = self.is_sidebar_collapsed();
        let active_space = self.active_space();
        let mut sidebar_content = div()
            .id("sidebar")
            .relative()
            .flex()
            .flex_col()
            .w(px(sidebar_width_f32))
            .h_full()
            .when(sidebar_collapsed, |el| el.px_2())
            .when(!sidebar_collapsed, |el| el.px_3())
            .py_3()
            .gap_2()
            .bg(rgb(theme.base))
            .child(visual::space_backdrop(
                self.space_modal
                    .as_ref()
                    .map_or(active_space, |modal| &modal.theme_draft),
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
                    if !this.is_sidebar_collapsed() {
                        open_sidebar_context_menu(this, event, window, cx);
                    }
                }),
            );

        if !self.is_sidebar_collapsed() || self.sidebar_hovered {
            sidebar_content = self.render_expanded_sidebar(
                sidebar_content,
                theme,
                &active_space_id,
                active_tab_id.as_deref(),
                &tabs,
                stored_split_pair.as_ref(),
                sidebar_width_f32,
                sidebar_collapsed,
                space_switch_transition.is_some(),
                cx,
            );
        } else {
            sidebar_content = self.render_collapsed_sidebar(
                sidebar_content,
                theme,
                active_tab_id.as_deref(),
                &tabs,
                cx,
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

        self.apply_sidebar_transition(sidebar_content, space_switch_transition)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_expanded_sidebar(
        &self,
        mut sidebar_content: gpui_kit::Stateful<gpui_kit::Div>,
        theme: visual::SpaceTheme,
        active_space_id: &str,
        active_tab_id: Option<&str>,
        tabs: &[SidebarTab],
        stored_split_pair: Option<&SplitPair>,
        sidebar_width_f32: f32,
        sidebar_collapsed: bool,
        in_space_transition: bool,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        sidebar_content = sidebar_content
            .child(controls::render_sidebar_header(sidebar_collapsed, theme, cx))
            .child(self.render_kept_section(
                theme,
                active_tab_id,
                tabs,
                stored_split_pair,
                cx,
            ))
            .child(self.render_folders_section(
                theme,
                active_tab_id,
                tabs,
                stored_split_pair,
                cx,
            ));

        if let Some(pair) = stored_split_pair {
            sidebar_content = self.render_split_group_section(
                sidebar_content,
                theme,
                active_tab_id,
                tabs,
                pair,
                in_space_transition,
                cx,
            );
        }

        sidebar_content = sidebar_content.child(self.render_regular_tabs_section(
            theme,
            active_space_id,
            active_tab_id,
            tabs,
            stored_split_pair,
            in_space_transition,
            cx,
        ));

        if let Some(el) = self.render_closing_tab_overlay(
            theme,
            active_space_id,
            stored_split_pair,
            sidebar_width_f32,
        ) {
            sidebar_content = sidebar_content.child(el);
        }

        sidebar_content
            .child(controls::render_new_tab_action(theme, cx))
            .child(
                // Empty sidebar canvas opens the same quick actions as a space menu.
                div()
                    .id("sidebar-empty-space")
                    .flex_1()
                    .min_h(px(8.0 * UI_SCALE))
                    .on_mouse_down(
                        gpui_kit::MouseButton::Right,
                        cx.listener(open_sidebar_context_menu),
                    ),
            )
            .child(self.render_sidebar_bottom_bar(theme, active_space_id, cx))
    }

    fn apply_sidebar_transition(
        &self,
        sidebar_content: gpui_kit::Stateful<gpui_kit::Div>,
        space_switch_transition: Option<(Space, space_switcher::SpaceSwitchState, Instant, u64)>,
    ) -> gpui_kit::AnyElement {
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
