use super::*;

impl Lumen {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_shell(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        theme: visual::SpaceTheme,
        target_theme: visual::SpaceTheme,
        tabs: &[(
            String,
            String,
            Option<std::sync::Arc<gpui_kit::RenderImage>>,
        )],
        sidebar_content: gpui_kit::AnyElement,
        outgoing_space_overlay: Option<gpui_kit::AnyElement>,
    ) -> gpui_kit::AnyElement {
        if self.fullscreen_tab.is_some() {
            if self.fullscreen_window_was_fullscreen.is_none() {
                self.fullscreen_window_was_fullscreen = Some(window.is_fullscreen());
            }
            if !window.is_fullscreen() {
                window.toggle_fullscreen();
            }
        } else if let Some(was_fullscreen) = self.fullscreen_window_was_fullscreen.take()
            && window.is_fullscreen() != was_fullscreen
        {
            window.toggle_fullscreen();
        }
        let sidebar_width_f32 = self.sidebar_width_transition.map_or_else(
            || self.effective_sidebar_width(),
            |(from, to, started, _)| {
                let progress = started.elapsed().as_secs_f32()
                    / ui::motion::MOTION_SLOW.as_secs_f32().max(f32::EPSILON);
                from + (to - from) * ui::motion::ease_out_quint(progress)
            },
        );
        let active_space = self.active_space().clone();
        let active_tab_id = active_space.active.clone();
        let active_tab_id_string = active_tab_id.clone().unwrap_or_default();
        let active_tab = active_space
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == active_tab_id.as_ref());
        let active_tab_url = active_tab.map(|tab| tab.url.clone()).unwrap_or_default();
        let split_pair = crate::plugins::split_view_enabled()
            .then(|| self.current_split_pair())
            .flatten();
        let layout_preset = visual::active_layout_preset();
        let topbar_mode = layout_preset
            .as_ref()
            .and_then(|p| p.topbar.as_deref())
            .unwrap_or("default");
        let topbar_height = visual::active_topbar_height();
        let sidebar_on_right = layout_preset
            .as_ref()
            .and_then(|p| p.sidebar_side.as_deref())
            == Some("right");

        let root = div()
            .relative()
            .flex()
            .size_full()
            .bg(rgb(theme.base))
            .child(visual::space_backdrop(
                self.space_modal
                    .as_ref()
                    .map_or(&active_space, |modal| &modal.theme_draft),
            ))
            .text_color(rgb(theme.text))
            .on_mouse_move(cx.listener(
                move |this, event: &gpui_kit::MouseMoveEvent, window, cx| {
                    if this.split_resizing {
                        let bounds = window.bounds();
                        let sidebar = this.effective_sidebar_width();
                        let card_left = if sidebar_on_right {
                            CONTENT_BEZEL_X
                        } else {
                            sidebar + CONTENT_BEZEL_X
                        };
                        let card_width =
                            f32::from(bounds.size.width) - sidebar - 2.0 * CONTENT_BEZEL_X;
                        this.split_ratio = split_view::ratio_at_pointer(
                            f32::from(event.position.x),
                            card_left,
                            card_width,
                        );
                        cx.notify();
                    }
                    if this.sidebar_resizing {
                        let new_width = if sidebar_on_right {
                            f32::from(window.bounds().size.width)
                                - f32::from(event.position.x)
                                - CONTENT_BEZEL_X
                        } else {
                            f32::from(event.position.x) - CONTENT_BEZEL_X
                        };
                        this.sidebar_width = new_width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                        cx.notify();
                    }
                },
            ))
            .on_mouse_up(
                gpui_kit::MouseButton::Left,
                cx.listener(move |this, _, _window, cx| {
                    #[cfg(all(target_os = "linux", not(test)))]
                    let finish_split_resize = this.split_resizing;
                    #[cfg(all(target_os = "linux", not(test)))]
                    if this.sidebar_resizing || finish_split_resize {
                        let bounds = _window.bounds();
                        let sidebar = this.effective_sidebar_width();
                        let content_width =
                            (f32::from(bounds.size.width) - sidebar - 2.0 * CONTENT_BEZEL_X)
                                .max(100.0);
                        let height =
                            (f32::from(bounds.size.height) - topbar_height - CONTENT_BEZEL_BOTTOM)
                                .max(100.0) as i32;
                        let exact_sizes = if let Some(pair) = this.current_split_pair() {
                            let (left, right) =
                                split_view::pane_widths(content_width, this.split_ratio);
                            vec![
                                split_view::BrowserSize {
                                    tab_id: pair.first,
                                    width: left.max(1.0) as i32,
                                    height,
                                },
                                split_view::BrowserSize {
                                    tab_id: pair.second,
                                    width: right.max(1.0) as i32,
                                    height,
                                },
                            ]
                        } else {
                            this.active_space()
                                .active
                                .clone()
                                .map(|tab_id| {
                                    vec![split_view::BrowserSize {
                                        tab_id,
                                        width: content_width as i32,
                                        height,
                                    }]
                                })
                                .unwrap_or_default()
                        };
                        this.finish_browser_resize_drag(
                            exact_sizes,
                            if finish_split_resize {
                                "split"
                            } else {
                                "sidebar"
                            },
                        );
                    }
                    if this.sidebar_resizing || this.split_resizing {
                        this.sidebar_resizing = false;
                        this.split_resizing = false;
                        this.save_state();
                        cx.notify();
                    }
                }),
            );

        let main_column = div()
            .relative()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .when(topbar_mode != "hidden", |col| {
                col.child(
                    div()
                        .h(px(topbar_height))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .child(
                            div()
                                .id("navigation-controls")
                                .flex()
                                .items_center()
                                .gap_1()
                                .child({
                                    let can_go_back =
                                        self.engine.can_go_back(self.focused_tab_id());
                                    div()
                                        .id("back-btn")
                                        .size(px(NAV_BUTTON_SIZE))
                                        .rounded_lg()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(rgb(if can_go_back {
                                            theme.text
                                        } else {
                                            theme.muted
                                        }))
                                        .opacity(if can_go_back { 1.0 } else { 0.4 })
                                        .when(can_go_back, |btn| {
                                            btn.cursor_pointer().active(|s| s.opacity(0.92)).hover(
                                                move |s| {
                                                    s.bg(rgb(theme.surface_hover))
                                                        .text_color(rgb(theme.text))
                                                },
                                            )
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.address_editing = false;
                                            this.show_suggestions = false;
                                            this.selected_suggestion = None;
                                            this.engine.go_back(this.focused_tab_id());
                                            cx.notify();
                                        }))
                                        .child(
                                            Icon::new(IconName::ArrowLeft)
                                                .size(px(16.0 * UI_SCALE)),
                                        )
                                })
                                .child({
                                    let can_go_forward =
                                        self.engine.can_go_forward(self.focused_tab_id());
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
                                            btn.cursor_pointer().active(|s| s.opacity(0.92)).hover(
                                                move |s| {
                                                    s.bg(rgb(theme.surface_hover))
                                                        .text_color(rgb(theme.text))
                                                },
                                            )
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.address_editing = false;
                                            this.show_suggestions = false;
                                            this.selected_suggestion = None;
                                            this.engine.go_forward(this.focused_tab_id());
                                            cx.notify();
                                        }))
                                        .child(
                                            Icon::new(IconName::ArrowRight)
                                                .size(px(16.0 * UI_SCALE)),
                                        )
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
                                .bg(rgb(theme.surface))
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
                                        .text_color(rgb(0x777b85))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(Icon::new(IconName::Search)),
                                )
                                .child(
                                    Input::new(&self.address)
                                        .id("address-bar")
                                        .w_full()
                                        .appearance(false),
                                ),
                        )
                        .child(
                            div()
                                .id("plugins-btn")
                                .cursor_pointer()
                                .active(|style| style.opacity(0.92))
                                .child(space_switcher::plugins_button(theme))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.plugins_menu_open = !this.plugins_menu_open;
                                    cx.notify();
                                })),
                        )
                        .when(crate::plugins::downloads_enabled(), |row| {
                            row.child(
                                div()
                                    .id("downloads-btn")
                                    .cursor_pointer()
                                    .active(|style| style.opacity(0.92))
                                    .child(space_switcher::downloads_button(theme))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app_page = Some(AppPage::Downloads);
                                        cx.notify();
                                    })),
                            )
                        }),
                )
            })
            .child({
                let viewport_sidebar = if sidebar_on_right {
                    0.0
                } else {
                    sidebar_width_f32
                };
                let total_width = f32::from(window.bounds().size.width)
                    - viewport_sidebar
                    - 2.0 * CONTENT_BEZEL_X;
                let (left_width, right_width) =
                    split_view::pane_widths(total_width, self.split_ratio);
                let left_tab_id = split_pair
                    .as_ref()
                    .map(|pair| pair.first.clone())
                    .unwrap_or_else(|| active_tab_id_string.clone());
                let left_image = self.engine.get_image(&left_tab_id);
                let left_cursor = self.engine.get_cursor(&left_tab_id);
                #[cfg(all(target_os = "linux", not(test)))]
                let left_frame_matches_target = self
                    .resize_drag
                    .target_size(&left_tab_id)
                    .is_some_and(|(w, h)| self.engine.frame_matches_size(&left_tab_id, w, h));
                #[cfg(all(target_os = "linux", not(test)))]
                let left_edge_color = self
                    .resize_drag
                    .edge_color_until_frame(&left_tab_id, left_frame_matches_target);
                #[cfg(any(not(target_os = "linux"), test))]
                let left_edge_color = None;
                self.web_viewport.update(cx, |viewport, cx| {
                    viewport.set_theme(theme);
                    if left_image.is_none()
                        && (active_tab_url.is_empty()
                            || active_tab_url == "about:blank"
                            || active_tab_url == "lumen://welcome"
                            || active_tab_url == "anarc://welcome")
                    {
                        viewport.clear_transition_image();
                    }
                    viewport.set_frame(
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
                    cx.notify();
                });
                let right_tab_id = split_pair.as_ref().map(|pair| pair.second.clone());
                if let Some(right_tab_id) = right_tab_id {
                    let right_image = self.engine.get_image(&right_tab_id);
                    let right_cursor = self.engine.get_cursor(&right_tab_id);
                    #[cfg(all(target_os = "linux", not(test)))]
                    let right_frame_matches_target = self
                        .resize_drag
                        .target_size(&right_tab_id)
                        .is_some_and(|(w, h)| self.engine.frame_matches_size(&right_tab_id, w, h));
                    #[cfg(all(target_os = "linux", not(test)))]
                    let right_edge_color = self
                        .resize_drag
                        .edge_color_until_frame(&right_tab_id, right_frame_matches_target);
                    #[cfg(any(not(target_os = "linux"), test))]
                    let right_edge_color = None;
                    self.second_web_viewport.update(cx, |viewport, cx| {
                        viewport.set_theme(theme);
                        viewport.set_frame(
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
                        cx.notify();
                    });
                }
                div()
                    .id("web-content-card")
                    .flex_1()
                    .mx(px(CONTENT_BEZEL_X))
                    .mb(px(CONTENT_BEZEL_BOTTOM))
                    .rounded(px(visual::active_card_radius()))
                    .bg(rgb(theme.surface))
                    .overflow_hidden()
                    .child(if let Some(split_pair) = split_pair.as_ref() {
                        div()
                            .id("split-viewports")
                            .size_full()
                            .flex()
                            .child(
                                div()
                                    .h_full()
                                    .w(px(left_width))
                                    .with_animation(
                                        format!("split-left-enter-{}", split_pair.first),
                                        Animation::new(ui::motion::duration(
                                            ui::motion::MOTION_SLOW,
                                        ))
                                        .with_easing(ui::motion::ease_out_quint),
                                        |pane, progress| {
                                            pane.opacity(ui::motion::clamp_percentage(progress))
                                        },
                                    )
                                    .child(
                                        AnyView::from(self.web_viewport.clone())
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
                                            #[cfg(all(target_os = "linux", not(test)))]
                                            this.begin_browser_resize_drag(_window, true);
                                            cx.notify();
                                        }),
                                    )
                                    .with_animation(
                                        "split-divider-enter",
                                        Animation::new(ui::motion::duration(
                                            ui::motion::MOTION_SLOW,
                                        ))
                                        .with_easing(ui::motion::ease_out_quint),
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
                                    .with_animation(
                                        format!("split-right-enter-{}", split_pair.second),
                                        Animation::new(ui::motion::duration(
                                            ui::motion::MOTION_SLOW,
                                        ))
                                        .with_easing(ui::motion::ease_out_quint),
                                        |pane, progress| {
                                            pane.opacity(ui::motion::clamp_percentage(progress))
                                        },
                                    )
                                    .child(
                                        AnyView::from(self.second_web_viewport.clone())
                                            .cached(StyleRefinement::default().size_full()),
                                    ),
                            )
                    } else if active_tab_url == "lumen://welcome"
                        || active_tab_url == "anarc://welcome"
                        || active_tab_url == "about:blank"
                        || active_tab_url.is_empty()
                    {
                        div().id("empty-space").size_full().on_mouse_down(
                            gpui_kit::MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                if this.show_suggestions {
                                    this.show_suggestions = false;
                                    this.selected_suggestion = None;
                                    cx.notify();
                                }
                            }),
                        )
                    } else {
                        div().id("web-viewport-entity").size_full().child(
                            AnyView::from(self.web_viewport.clone())
                                .cached(StyleRefinement::default().size_full()),
                        )
                    })
            });

        let mut root = if sidebar_on_right {
            root.child(main_column)
                .children(outgoing_space_overlay)
                .child(sidebar_content)
        } else {
            root.child(sidebar_content)
                .children(outgoing_space_overlay)
                .child(main_column)
        };

        if let Some(page) = self
            .app_page
            .filter(|page| *page != AppPage::Downloads || crate::plugins::downloads_enabled())
        {
            let active_url = self
                .active_space()
                .tabs
                .iter()
                .find(|tab| tab.id == self.active_tab_id())
                .map(|tab| tab.url.as_str())
                .unwrap_or_default();
            let zoom = crate::shield::url_host(active_url)
                .and_then(|host| self.zoom_levels.get(&host).copied())
                .map(|level| 1.2_f64.powf(level))
                .unwrap_or(self.preferences.default_zoom_factor);
            let site_protection_enabled = self.engine.shield_enabled_for_url(active_url);
            let has_web_site = crate::shield::url_host(active_url).is_some();
            let content = ui::app_pages::render(
                page,
                theme,
                self.engine.downloads(),
                zoom,
                site_protection_enabled,
                has_web_site,
                self.settings_section,
                self.preferences.search_engine,
                self.active_space().color,
                self.plugin_rows(),
                self.pending_plugin_review(),
                self.plugin_action_error().map(str::to_owned),
                cx,
            );
            root = root.child(
                div()
                    .id("app-page-overlay")
                    .absolute()
                    .top(px(topbar_height))
                    .bottom(px(CONTENT_BEZEL_BOTTOM))
                    .left(px(if sidebar_on_right {
                        CONTENT_BEZEL_X
                    } else {
                        sidebar_width_f32 + CONTENT_BEZEL_X
                    }))
                    .right(px(if sidebar_on_right {
                        sidebar_width_f32 + CONTENT_BEZEL_X
                    } else {
                        CONTENT_BEZEL_X
                    }))
                    .rounded(px(visual::active_card_radius()))
                    .bg(rgb(theme.surface))
                    .overflow_hidden()
                    .child(content),
            );
        }

        if !self.is_sidebar_collapsed() {
            let handle_w = 10.0 * UI_SCALE;
            let cut_x = if sidebar_on_right {
                f32::from(window.bounds().size.width) - sidebar_width_f32 - CONTENT_BEZEL_X
            } else {
                sidebar_width_f32 + CONTENT_BEZEL_X
            };
            root = root.child(
                div()
                    .id("sidebar-resize-handle")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(cut_x - handle_w / 2.0))
                    .w(px(handle_w))
                    .cursor_col_resize()
                    .on_mouse_down(
                        gpui_kit::MouseButton::Left,
                        cx.listener(|this, _, _window, cx| {
                            this.sidebar_resizing = true;
                            #[cfg(all(target_os = "linux", not(test)))]
                            this.begin_browser_resize_drag(_window, false);
                            cx.notify();
                        }),
                    ),
            );
        }

        let address_focused = window.focused(cx) == Some(self.address.read(cx).focus_handle(cx));
        let suggestions_open =
            self.show_suggestions && address_focused && !self.suggestions.is_empty();
        if suggestions_open {
            if !self.suggestions_were_visible {
                self.suggestion_animation_serial = self.suggestion_animation_serial.wrapping_add(1);
                self.suggestions_were_visible = true;
                self.suggestion_closing = None;
            }
            self.suggestion_snapshot.clone_from(&self.suggestions);
        } else if self.suggestions_were_visible {
            self.suggestions_were_visible = false;
            self.suggestion_animation_serial = self.suggestion_animation_serial.wrapping_add(1);
            let serial = self.suggestion_animation_serial;
            self.suggestion_closing = Some((self.suggestion_snapshot.clone(), serial));
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(ui::motion::duration(ui::motion::MOTION_FAST))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this
                        .suggestion_closing
                        .as_ref()
                        .is_some_and(|(_, current)| *current == serial)
                    {
                        this.suggestion_closing = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        let suggestion_items = if suggestions_open {
            self.suggestions.clone()
        } else {
            self.suggestion_closing
                .as_ref()
                .map(|(items, _)| items.clone())
                .unwrap_or_default()
        };
        let closing_suggestions = self.suggestion_closing.as_ref().map(|(_, serial)| *serial);
        let suggestion_left = (if sidebar_on_right {
            0.0
        } else {
            sidebar_width_f32
        }) + 16.0 * UI_SCALE
            + NAV_BUTTON_SIZE * 2.0
            + 12.0 * UI_SCALE;
        let suggestion_right = (if sidebar_on_right {
            sidebar_width_f32
        } else {
            0.0
        }) + 16.0 * UI_SCALE
            + 36.0 * UI_SCALE
            + if crate::plugins::downloads_enabled() {
                36.0 * UI_SCALE
            } else {
                0.0
            };
        let suggestion_width =
            (f32::from(window.bounds().size.width) - suggestion_left - suggestion_right).max(1.0);

        root = root.when(suggestions_open || closing_suggestions.is_some(), |this| {
            let mut list = div()
                .id("autocomplete-suggestions")
                .occlude()
                .absolute()
                .top(px(topbar_height + 4.0 * UI_SCALE))
                .left(px(suggestion_left))
                .w(px(suggestion_width))
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .rounded_xl()
                .p_1p5()
                .flex()
                .flex_col()
                .gap_1();

            for (idx, item) in suggestion_items.iter().enumerate() {
                let target = item.target.clone();
                let title = item.title.clone();
                let icon_type = item.icon_type.clone();
                list = list.child(
                    div()
                        .id(format!("suggestion-item-{idx}"))
                        .w_full()
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .gap_3()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme.surface_hover)))
                        .when(
                            closing_suggestions.is_none() && self.selected_suggestion == Some(idx),
                            |row| row.bg(rgb(theme.surface_hover)),
                        )
                        .child(
                            div()
                                .text_color(rgb(0x8b8f9a))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(16.0 * UI_SCALE))
                                .child(match icon_type.as_str() {
                                    "globe" => Icon::new(IconName::Globe),
                                    "zap" => Icon::new(IconName::Zap),
                                    _ => Icon::new(IconName::Search),
                                }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_sm()
                                .text_color(rgb(theme.text))
                                .child(title),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate_to(&target, window, cx);
                            cx.notify();
                        })),
                );
            }

            let list = if let Some(serial) = closing_suggestions {
                list.with_animation(
                    format!("autocomplete-suggestions-exit-{serial}"),
                    Animation::new(ui::motion::duration(ui::motion::MOTION_FAST))
                        .with_easing(ui::motion::ease_out_quint),
                    move |panel, progress| {
                        let remaining = ui::motion::clamp_percentage(1.0 - progress);
                        panel
                            .w(px(suggestion_width * (0.96 + 0.04 * remaining)))
                            .opacity(remaining)
                    },
                )
                .into_any_element()
            } else {
                list.with_animation(
                    format!(
                        "autocomplete-suggestions-entrance-{}",
                        self.suggestion_animation_serial
                    ),
                    Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                        .with_easing(ui::motion::ease_out_quint),
                    move |panel, progress| {
                        panel
                            .w(px(suggestion_width * (0.96 + 0.04 * progress)))
                            .opacity(ui::motion::clamp_percentage(progress))
                    },
                )
                .into_any_element()
            };
            this.child(list)
        });

        root = root.when(self.show_find, |root| {
            root.child(
                div()
                    .id("find-in-page")
                    .absolute()
                    .top(px(8.0 * UI_SCALE))
                    .right(px(16.0 * UI_SCALE))
                    .w(px(300.0 * UI_SCALE))
                    .h(px(36.0 * UI_SCALE))
                    .px_3()
                    .rounded_lg()
                    .overflow_hidden()
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .child(
                        Input::new(&self.find_input)
                            .id("find-input")
                            .w_full()
                            .appearance(false),
                    ),
            )
        });

        if self.show_command_palette {
            let query = self.command_input.read(cx).value().to_string();
            let items = self.command_items(&query);
            self.command_palette_scroll
                .scroll_to_item(self.selected_command, gpui_kit::ScrollStrategy::Nearest);
            let selected_command = self.selected_command;
            let list_height = items.len().min(8) as f32 * 52.0 * UI_SCALE;
            let surface_hover = theme.surface_hover;
            let text = theme.text;
            let muted = theme.muted;
            let list = gpui_kit::uniform_list(
                "command-palette-items",
                items.len(),
                cx.processor(move |_this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| {
                            let item = items[index].clone();
                            let click_item = item.clone();
                            div()
                                .id(format!("command-palette-item-{index}"))
                                .w_full()
                                .h(px(52.0 * UI_SCALE))
                                .px_3()
                                .py_2()
                                .rounded_lg()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .cursor_pointer()
                                .when(selected_command == index, |row| row.bg(rgb(surface_hover)))
                                .hover(|style| style.bg(rgb(surface_hover)))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div().text_sm().text_color(rgb(text)).child(item.title),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(muted))
                                                .child(item.detail),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.execute_palette_item(click_item.clone(), window, cx);
                                }))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.command_palette_scroll)
            .p_1p5()
            .h(px(list_height));
            let panel = div()
                .id("command-palette")
                .w(px(640.0 * UI_SCALE))
                .max_h(px(520.0 * UI_SCALE))
                .overflow_hidden()
                .rounded_xl()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .shadow_lg()
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .overflow_hidden()
                        .child(
                            Input::new(&self.command_input)
                                .id("command-palette-input")
                                .w_full()
                                .appearance(false),
                        ),
                )
                .child(list)
                .with_animation(
                    "command-palette-entrance",
                    Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                        .with_easing(ui::motion::ease_out_quint),
                    |panel, progress| {
                        panel
                            .w(px(640.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                            .opacity(ui::motion::clamp_percentage(progress))
                    },
                );
            root = root.child(
                div()
                    .absolute()
                    .top(px(88.0 * UI_SCALE))
                    .left(px(0.0))
                    .right(px(0.0))
                    .flex()
                    .justify_center()
                    .occlude()
                    .child(panel),
            );
        }

        root = root
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.to_lowercase();
                let modifiers = &event.keystroke.modifiers;
                let browser_modifier = modifiers.control || modifiers.platform;
                if !event.is_held
                    && browser_modifier
                    && !modifiers.shift
                    && key == "w"
                    && this.app_page.is_some()
                {
                    this.close_app_page();
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !event.is_held
                    && matches!(key.as_str(), "escape" | "esc")
                    && this.tab_context_menu.is_some()
                {
                    this.tab_context_menu = None;
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if browser_modifier && key == "k" && !modifiers.shift && !event.is_held {
                    this.open_command_palette(window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !event.is_held
                    && browser_modifier
                    && modifiers.shift
                    && key == "s"
                    && crate::plugins::split_view_enabled()
                {
                    this.split_active_with_next();
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !event.is_held
                    && browser_modifier
                    && modifiers.shift
                    && key == "w"
                    && crate::plugins::split_view_enabled()
                {
                    this.exit_split();
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !event.is_held
                    && modifiers.alt
                    && browser_modifier
                    && crate::plugins::split_view_enabled()
                    && this.current_split_pair().is_some()
                    && matches!(key.as_str(), "left" | "arrowleft" | "right" | "arrowright")
                {
                    let pane_index = if matches!(key.as_str(), "left" | "arrowleft") {
                        0
                    } else {
                        1
                    };
                    this.set_focused_pane(pane_index);
                    if let Some(pair) = this.current_split_pair() {
                        this.engine.set_focus(&pair.first, false);
                        this.engine.set_focus(&pair.second, false);
                    }
                    let focused = this.focused_tab_id().to_string();
                    this.engine.set_focus(&focused, true);
                    this.browser_focused = true;
                    window.focus(&this.browser_focus_handle, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.show_command_palette {
                    if event.is_held {
                        return;
                    }
                    let query = this.command_input.read(cx).value().to_string();
                    let items = this.command_items(&query);
                    match key.as_str() {
                        "escape" | "esc" => this.close_command_palette(window, cx),
                        "up" | "arrowup" => this.move_command_selection(false, items.len()),
                        "down" | "arrowdown" => this.move_command_selection(true, items.len()),
                        "enter" | "return" => {
                            if let Some(item) = items.get(this.selected_command).cloned() {
                                this.execute_palette_item(item, window, cx);
                            }
                        }
                        _ => return,
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.show_find && matches!(key.as_str(), "enter" | "return") {
                    this.find_in_page(!modifiers.shift, true, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if matches!(key.as_str(), "escape" | "esc") {
                    if this.show_find {
                        this.engine.stop_finding(this.focused_tab_id());
                        this.show_find = false;
                        this.focus_browser(window, cx);
                        window.prevent_default();
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    if this.engine.is_loading(this.focused_tab_id()) {
                        this.engine.stop_load(this.focused_tab_id());
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                }
                if browser_modifier && key == "f" && !modifiers.shift {
                    this.show_find = true;
                    this.browser_focused = false;
                    this.engine.set_focus(this.focused_tab_id(), false);
                    this.find_input
                        .update(cx, |input, cx| input.focus(window, cx));
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if modifiers.alt && matches!(key.as_str(), "left" | "arrowleft") {
                    this.address_editing = false;
                    this.show_suggestions = false;
                    this.selected_suggestion = None;
                    this.engine.go_back(this.focused_tab_id());
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if modifiers.alt && matches!(key.as_str(), "right" | "arrowright") {
                    this.address_editing = false;
                    this.show_suggestions = false;
                    this.selected_suggestion = None;
                    this.engine.go_forward(this.focused_tab_id());
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if browser_modifier && modifiers.alt && key == "r" && !modifiers.shift {
                    if reader::is_plugin_enabled() {
                        this.engine
                            .execute_script(this.active_tab_id(), reader::toggle_script());
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                let suggestions_open = this.show_suggestions && !this.suggestions.is_empty();
                let is_suggestion_arrow = suggestions_open
                    && matches!(key.as_str(), "up" | "down" | "arrowup" | "arrowdown");
                if event.is_held && !is_suggestion_arrow {
                    return;
                }
                if is_suggestion_arrow {
                    this.move_suggestion_selection(matches!(key.as_str(), "down" | "arrowdown"));
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if !event.is_held
                    && suggestions_open
                    && matches!(key.as_str(), "enter" | "return")
                    && let Some(target) = this
                        .selected_suggestion
                        .and_then(|index| this.suggestions.get(index))
                        .map(|item| item.target.clone())
                {
                    this.navigate_to(&target, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if !event.is_held
                    && matches!(key.as_str(), "enter" | "return")
                    && window.focused(cx) == Some(this.address.read(cx).focus_handle(cx))
                {
                    this.navigate(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if suggestions_open && matches!(key.as_str(), "escape" | "esc") {
                    this.show_suggestions = false;
                    this.selected_suggestion = None;
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if browser_modifier && key == "l" && !modifiers.shift {
                    let tab_id = this.focused_tab_id().to_string();
                    this.engine.set_focus(&tab_id, false);
                    this.address.update(cx, |input, cx| {
                        input.focus(window, cx);
                        input.select_all(window, cx);
                    });
                    this.browser_focused = false;
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if this.handle_shortcut(&key, browser_modifier, modifiers.shift) {
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, window, cx| {
                let modifiers = &event.keystroke.modifiers;
                let browser_modifier = modifiers.control || modifiers.platform;
                let key = event.keystroke.key.to_lowercase();
                if this.browser_focused
                    && !Self::is_app_shortcut(&key, browser_modifier, modifiers.shift)
                    && !(modifiers.alt
                        && matches!(key.as_str(), "left" | "arrowleft" | "right" | "arrowright"))
                {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }));

        root = root
            .children(self.render_dialogs(theme, cx))
            .children(self.render_context_menus(theme, tabs, cx));

        if let Some(tab_id) = self.fullscreen_tab.clone() {
            let image = self.engine.get_image(&tab_id);
            let cursor = self.engine.get_cursor(&tab_id);
            self.web_viewport.update(cx, |viewport, cx| {
                viewport.set_theme(theme);
                viewport.set_fullscreen(true);
                viewport.set_frame(
                    tab_id,
                    -CONTENT_BEZEL_X,
                    image,
                    cursor,
                    Vec::new(),
                    0.0,
                    0,
                    false,
                    None,
                );
                cx.notify();
            });
            root = root.child(
                div()
                    .id("video-fullscreen-overlay")
                    .absolute()
                    .inset_0()
                    .child(
                        AnyView::from(self.web_viewport.clone())
                            .cached(StyleRefinement::default().size_full()),
                    ),
            );
        } else {
            self.web_viewport.update(cx, |viewport, cx| {
                viewport.set_fullscreen(false);
                cx.notify();
            });
        }

        if let Some((from_theme, _, transition_id)) = self.theme_transition {
            let to_theme = target_theme;
            root.with_animation(
                format!("space-theme-transition-{transition_id}"),
                Animation::new(ui::motion::duration(ui::motion::MOTION_SPACE_SWITCH))
                    .with_easing(ui::motion::ease_out_quint),
                move |root, progress| {
                    let theme = visual::interpolate_theme(from_theme, to_theme, progress);
                    root.bg(rgb(theme.base)).text_color(rgb(theme.text))
                },
            )
            .into_any_element()
        } else {
            root.into_any_element()
        }
    }
}
