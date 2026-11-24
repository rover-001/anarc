use super::*;

pub(super) struct ContentParams {
    pub(super) theme: visual::SpaceTheme,
    pub(super) topbar_height: f32,
    pub(super) sidebar_width_f32: f32,
    pub(super) sidebar_on_right: bool,
    pub(super) split_pair: Option<SplitPair>,
    pub(super) active_tab_id_string: String,
    pub(super) active_tab_url: String,
}

pub(super) fn render(
    this: &mut Anarc,
    window: &mut Window,
    cx: &mut Context<Anarc>,
    params: ContentParams,
) -> gpui_kit::AnyElement {
    let ContentParams {
        theme,
        topbar_height,
        sidebar_width_f32,
        sidebar_on_right,
        split_pair,
        active_tab_id_string,
        active_tab_url,
    } = params;
    let viewport_sidebar = if sidebar_on_right {
        0.0
    } else {
        sidebar_width_f32
    };
    let total_width =
        f32::from(window.bounds().size.width) - viewport_sidebar - 2.0 * CONTENT_BEZEL_X;
    let total_height =
        f32::from(window.bounds().size.height) - topbar_height - CONTENT_BEZEL_BOTTOM;
    let (left_width, right_width) = split_view::pane_widths(total_width, this.split_ratio);
    let (top_height, bottom_height) = split_view::pane_heights(total_height, this.split_ratio_y);
    let quad = split_pair.as_ref().is_some_and(|pair| pair.is_quad());
    let triple = split_pair
        .as_ref()
        .is_some_and(|pair| pair.third.is_some() && pair.fourth.is_none());
    let left_tab_id = split_pair
        .as_ref()
        .map(|pair| pair.first.clone())
        .unwrap_or_else(|| active_tab_id_string.clone());
    let left_image = this.engine.get_image(&left_tab_id);
    let left_cursor = this.engine.get_cursor(&left_tab_id);
    #[cfg(not(test))]
    let left_frame_matches_target = this
        .resize_drag
        .target_size(&left_tab_id)
        .is_some_and(|(w, h)| this.engine.frame_matches_size(&left_tab_id, w, h));
    #[cfg(not(test))]
    let left_edge_color = this
        .resize_drag
        .edge_color_until_frame(&left_tab_id, left_frame_matches_target);
    #[cfg(test)]
    let left_edge_color = None;
    if this.fullscreen_tab.is_none() {
        this.web_viewport.update(cx, |viewport, cx| {
            let mut changed = viewport.set_theme(theme);
            changed |= viewport.set_fullscreen(false);
            changed |= viewport.set_focus_pane_index(0);
            changed |= viewport.set_split_grid(quad);
            changed |= viewport.set_pane_y_offset(0.0);
            if left_image.is_none()
                && (active_tab_url.is_empty()
                    || active_tab_url == "about:blank"
                    || active_tab_url == "lumen://welcome"
                    || active_tab_url == "anarc://welcome")
            {
                changed |= viewport.clear_transition_image();
            }
            changed |= viewport.set_frame(
                left_tab_id.clone(),
                viewport_sidebar,
                left_image,
                left_cursor,
                Vec::new(),
                0.0,
                0,
                split_pair.is_some(),
                left_edge_color,
            );
            if changed {
                cx.notify();
            }
        });
    }
    let right_tab_id = split_pair.as_ref().map(|pair| pair.second.clone());
    if let Some(right_tab_id) = right_tab_id {
        let right_image = this.engine.get_image(&right_tab_id);
        let right_cursor = this.engine.get_cursor(&right_tab_id);
        #[cfg(not(test))]
        let right_frame_matches_target = this
            .resize_drag
            .target_size(&right_tab_id)
            .is_some_and(|(w, h)| this.engine.frame_matches_size(&right_tab_id, w, h));
        #[cfg(not(test))]
        let right_edge_color = this
            .resize_drag
            .edge_color_until_frame(&right_tab_id, right_frame_matches_target);
        #[cfg(test)]
        let right_edge_color = None;
        this.second_web_viewport.update(cx, |viewport, cx| {
            let mut changed = viewport.set_theme(theme);
            changed |= viewport.set_focus_pane_index(1);
            changed |= viewport.set_split_grid(quad || triple);
            changed |= viewport.set_pane_y_offset(0.0);
            changed |= viewport.set_frame(
                right_tab_id.clone(),
                viewport_sidebar,
                right_image,
                right_cursor,
                Vec::new(),
                left_width + DIVIDER_WIDTH,
                1,
                true,
                right_edge_color,
            );
            if changed {
                cx.notify();
            }
        });
    }
    if let Some(pair) = split_pair.as_ref() {
        if let Some(third_id) = pair.third.as_ref() {
            let image = this.engine.get_image(third_id);
            let cursor = this.engine.get_cursor(third_id);
            #[cfg(not(test))]
            let frame_matches = this
                .resize_drag
                .target_size(third_id)
                .is_some_and(|(w, h)| this.engine.frame_matches_size(third_id, w, h));
            #[cfg(not(test))]
            let edge_color = this
                .resize_drag
                .edge_color_until_frame(third_id, frame_matches);
            #[cfg(test)]
            let edge_color = None;
            this.third_web_viewport.update(cx, |viewport, cx| {
                let mut changed = viewport.set_theme(theme);
                changed |= viewport.set_focus_pane_index(2);
                changed |= viewport.set_split_grid(true);
                changed |= viewport.set_pane_y_offset(if quad {
                    top_height + DIVIDER_WIDTH
                } else {
                    0.0
                });
                changed |= viewport.set_frame(
                    third_id.clone(),
                    viewport_sidebar,
                    image,
                    cursor,
                    Vec::new(),
                    if quad {
                        0.0
                    } else {
                        left_width + DIVIDER_WIDTH
                    },
                    if quad { 2 } else { 3 },
                    true,
                    edge_color,
                );
                if changed {
                    cx.notify();
                }
            });
        }
        if quad && let Some(fourth_id) = pair.fourth.as_ref() {
            let image = this.engine.get_image(fourth_id);
            let cursor = this.engine.get_cursor(fourth_id);
            #[cfg(not(test))]
            let frame_matches = this
                .resize_drag
                .target_size(fourth_id)
                .is_some_and(|(w, h)| this.engine.frame_matches_size(fourth_id, w, h));
            #[cfg(not(test))]
            let edge_color = this
                .resize_drag
                .edge_color_until_frame(fourth_id, frame_matches);
            #[cfg(test)]
            let edge_color = None;
            this.fourth_web_viewport.update(cx, |viewport, cx| {
                let mut changed = viewport.set_theme(theme);
                changed |= viewport.set_focus_pane_index(3);
                changed |= viewport.set_split_grid(true);
                changed |= viewport.set_pane_y_offset(top_height + DIVIDER_WIDTH);
                changed |= viewport.set_frame(
                    fourth_id.clone(),
                    viewport_sidebar,
                    image,
                    cursor,
                    Vec::new(),
                    left_width + DIVIDER_WIDTH,
                    3,
                    true,
                    edge_color,
                );
                if changed {
                    cx.notify();
                }
            });
        }
    }
    div()
        .id("web-content-card")
        .flex_1()
        .mx(px(CONTENT_BEZEL_X))
        .mb(px(CONTENT_BEZEL_BOTTOM))
        .rounded(px(visual::active_card_radius()))
        .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
        .overflow_hidden()
        .child(if let Some(split_pair) = split_pair.as_ref() {
            if quad {
                // No per-pane entrance spring: four concurrent springs
                // force full-window animation frames on every split into
                // the grid, which is visible as jank on slower machines.
                let pane = |view: gpui_kit::AnyView, width: f32, height: f32, _key: String| {
                    div()
                        .w(px(width))
                        .h(px(height))
                        .child(view.cached(StyleRefinement::default().size_full()))
                };
                let top_row = div()
                    .id("split-row-top")
                    .w_full()
                    .h(px(top_height))
                    .flex()
                    .child(pane(
                        AnyView::from(this.web_viewport.clone()),
                        left_width,
                        top_height,
                        split_pair.first.clone(),
                    ))
                    .child(
                        div()
                            .id("split-divider")
                            .w(px(DIVIDER_HIT_WIDTH))
                            .mx(px((DIVIDER_WIDTH - DIVIDER_HIT_WIDTH) / 2.0))
                            .h_full()
                            .cursor_col_resize()
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(|this, _, _window, cx| {
                                    this.split_resizing = true;
                                    #[cfg(not(test))]
                                    this.begin_browser_resize_drag(_window, true, true);
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(pane(
                        AnyView::from(this.second_web_viewport.clone()),
                        right_width,
                        top_height,
                        split_pair.second.clone(),
                    ));
                let bottom_row = div()
                    .id("split-row-bottom")
                    .w_full()
                    .h(px(bottom_height))
                    .flex()
                    .child(pane(
                        AnyView::from(this.third_web_viewport.clone()),
                        left_width,
                        bottom_height,
                        split_pair.third.clone().unwrap_or_default(),
                    ))
                    .child(
                        div()
                            .id("split-divider-bottom")
                            .w(px(DIVIDER_HIT_WIDTH))
                            .mx(px((DIVIDER_WIDTH - DIVIDER_HIT_WIDTH) / 2.0))
                            .h_full()
                            .cursor_col_resize()
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(|this, _, _window, cx| {
                                    this.split_resizing = true;
                                    #[cfg(not(test))]
                                    this.begin_browser_resize_drag(_window, true, true);
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(pane(
                        AnyView::from(this.fourth_web_viewport.clone()),
                        right_width,
                        bottom_height,
                        split_pair.fourth.clone().unwrap_or_default(),
                    ));
                div()
                    .id("split-viewports-quad")
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(top_row)
                    .child(
                        div()
                            .id("split-divider-horizontal")
                            .h(px(DIVIDER_HIT_WIDTH))
                            .my(px((DIVIDER_WIDTH - DIVIDER_HIT_WIDTH) / 2.0))
                            .w_full()
                            .cursor_row_resize()
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(|this, _, _window, cx| {
                                    this.split_resizing_horizontal = true;
                                    #[cfg(not(test))]
                                    this.begin_browser_resize_drag(_window, true, true);
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(bottom_row)
            } else if triple {
                // Three panes: a full-height left pane and a
                // vertically stacked pair on the right.
                let pane = |view: gpui_kit::AnyView, width: f32, height: f32| {
                    div()
                        .w(px(width))
                        .h(px(height))
                        .child(view.cached(StyleRefinement::default().size_full()))
                };
                let right_column = div()
                    .id("split-column-right")
                    .w(px(right_width))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(pane(
                        AnyView::from(this.second_web_viewport.clone()),
                        right_width,
                        top_height,
                    ))
                    .child(
                        div()
                            .id("split-divider-horizontal-triple")
                            .h(px(DIVIDER_HIT_WIDTH))
                            .my(px((DIVIDER_WIDTH - DIVIDER_HIT_WIDTH) / 2.0))
                            .w_full()
                            .cursor_row_resize()
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(|this, _, _window, cx| {
                                    this.split_resizing_horizontal = true;
                                    #[cfg(not(test))]
                                    this.begin_browser_resize_drag(_window, true, true);
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(pane(
                        AnyView::from(this.third_web_viewport.clone()),
                        right_width,
                        bottom_height,
                    ));
                div()
                    .id("split-viewports-triple")
                    .size_full()
                    .flex()
                    .child(pane(
                        AnyView::from(this.web_viewport.clone()),
                        left_width,
                        total_height,
                    ))
                    .child(
                        div()
                            .id("split-divider-triple")
                            .w(px(DIVIDER_HIT_WIDTH))
                            .mx(px((DIVIDER_WIDTH - DIVIDER_HIT_WIDTH) / 2.0))
                            .h_full()
                            .cursor_col_resize()
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(|this, _, _window, cx| {
                                    this.split_resizing = true;
                                    #[cfg(not(test))]
                                    this.begin_browser_resize_drag(_window, true, true);
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(right_column)
            } else {
                div()
                    .id("split-viewports")
                    .size_full()
                    .flex()
                    .child(
                        div()
                            .h_full()
                            .w(px(left_width))
                            .with_spring(
                                format!("split-left-enter-{}", split_pair.first),
                                ui::motion::row_enter().enter(),
                                |pane, progress| {
                                    pane.opacity(ui::motion::clamp_percentage(progress))
                                },
                            )
                            .child(
                                AnyView::from(this.web_viewport.clone())
                                    .cached(StyleRefinement::default().size_full()),
                            ),
                    )
                    .child(
                        div()
                            .id("split-divider")
                            .w(px(DIVIDER_HIT_WIDTH))
                            .mx(px((DIVIDER_WIDTH - DIVIDER_HIT_WIDTH) / 2.0))
                            .h_full()
                            .cursor_col_resize()
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(|this, _, _window, cx| {
                                    this.split_resizing = true;
                                    #[cfg(not(test))]
                                    this.begin_browser_resize_drag(_window, true, true);
                                    cx.notify();
                                }),
                            )
                            .with_spring(
                                "split-divider-enter",
                                ui::motion::row_enter().enter(),
                                |divider, progress| {
                                    divider.opacity(ui::motion::clamp_percentage(
                                        0.35 + 0.65 * progress,
                                    ))
                                },
                            ),
                    )
                    .child(
                        div()
                            .h_full()
                            .w(px(right_width))
                            .with_spring(
                                format!("split-right-enter-{}", split_pair.second),
                                ui::motion::row_enter().enter(),
                                |pane, progress| {
                                    pane.opacity(ui::motion::clamp_percentage(progress))
                                },
                            )
                            .child(
                                AnyView::from(this.second_web_viewport.clone())
                                    .cached(StyleRefinement::default().size_full()),
                            ),
                    )
            }
        } else if active_tab_url == "lumen://welcome"
            || active_tab_url == "anarc://welcome"
            || active_tab_url == "about:blank"
            || active_tab_url.is_empty()
        {
            div().id("empty-space").size_full().on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus_browser(window, cx);
                    if this.show_suggestions {
                        this.show_suggestions = false;
                        this.selected_suggestion = None;
                        cx.notify();
                    }
                }),
            )
        } else {
            div().id("web-viewport-entity").size_full().child(
                AnyView::from(this.web_viewport.clone())
                    .cached(StyleRefinement::default().size_full()),
            )
        })
        .into_any_element()
}
