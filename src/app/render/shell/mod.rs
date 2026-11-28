use super::*;

use super::super::actions::peek::{PEEK_HEADER_HEIGHT, peek_layout};

mod content;
mod input;
mod topbar;

impl Anarc {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_shell(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        theme: visual::SpaceTheme,
        target_theme: visual::SpaceTheme,
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
        let sidebar_width_f32 = self
            .sidebar_width_transition
            .map_or_else(|| self.effective_sidebar_width(), |transition| transition.width());
        // Keep a borrowed view of the active space. Cloning it here also
        // clones every tab and its strings on every browser frame render.
        let active_space = self.active_space();
        let active_tab_id = active_space.active.clone();
        let active_tab_id_string = active_tab_id.clone().unwrap_or_default();
        let active_tab = active_space
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == active_tab_id.as_ref());
        let active_tab_url = active_tab.map(|tab| tab.url.clone()).unwrap_or_default();
        let address_focused = window.focused(cx) == Some(self.address.read(cx).focus_handle(cx));
        let address_value = self.address.read(cx).value().to_string();
        let show_address_preview =
            !address_focused && !self.address_editing && !address_value.is_empty();
        let address_preview = address_value
            .strip_prefix("https://")
            .unwrap_or(&address_value)
            .to_owned();
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

        let root = input::with_pointer_handlers(
            div()
                .relative()
                .flex()
                .size_full()
                .bg(rgb(theme.base))
                .child(visual::space_backdrop(
                    self.space_modal
                        .as_ref()
                        .map_or(active_space, |modal| &modal.theme_draft),
                ))
                .text_color(rgb(theme.text)),
            cx,
            topbar_height,
            sidebar_on_right,
        );

        // When the sidebar collapses into an icon strip, its top band is empty.
        // Let the top bar occupy it: shift it over the strip and stretch it to
        // the full window width so the address bar gains that space back.
        let topbar_expands = self.is_sidebar_collapsed();
        let window_width = f32::from(window.bounds().size.width);
        let topbar_left = if topbar_expands && !sidebar_on_right {
            -sidebar_width_f32
        } else {
            0.0
        };

        let main_column = div()
            .relative()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .when(topbar_mode != "hidden", |col| {
                col.child(topbar::render(
                    self,
                    window,
                    cx,
                    topbar::TopbarParams {
                        theme,
                        topbar_height,
                        topbar_expands,
                        topbar_left,
                        window_width,
                        sidebar_width_f32,
                        sidebar_on_right,
                        show_address_preview,
                        address_preview: address_preview.clone(),
                        active_tab_url: active_tab_url.clone(),
                    },
                ))
            })
            .child(content::render(
                self,
                window,
                cx,
                content::ContentParams {
                    theme,
                    topbar_height,
                    sidebar_width_f32,
                    sidebar_on_right,
                    split_pair,
                    active_tab_id_string: active_tab_id_string.clone(),
                    active_tab_url: active_tab_url.clone(),
                },
            ));

        let mut root = if sidebar_on_right {
            root.child(main_column)
                .children(outgoing_space_overlay)
                .child(sidebar_content)
        } else {
            root.child(sidebar_content)
                .children(outgoing_space_overlay)
                .child(main_column)
        };

        let visible_app_page = self
            .app_page
            .filter(|page| *page != AppPage::Downloads || crate::plugins::downloads_enabled());
        if let Some(page) = visible_app_page {
            if self.app_page_was_open != Some(page) {
                self.app_page_serial = self.app_page_serial.wrapping_add(1);
                self.app_page_was_open = Some(page);
                self.app_page_closing = None;
            }
        } else if let Some(prev_page) = self.app_page_was_open.take() {
            self.app_page_serial = self.app_page_serial.wrapping_add(1);
            let serial = self.app_page_serial;
            if !ui::motion::reduced_motion() {
                self.app_page_closing = Some((prev_page, serial));
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(ui::motion::sheet().duration())
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.app_page_closing.is_some_and(|(_, id)| id == serial) {
                            this.app_page_closing = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }

        if let Some((page, is_closing, serial)) = visible_app_page
            .map(|page| (page, false, self.app_page_serial))
            .or_else(|| {
                self.app_page_closing
                    .map(|(page, serial)| (page, true, serial))
            })
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
            let has_web_site = crate::shield::url_host(active_url).is_some();
            let content = ui::app_pages::render(
                page,
                theme,
                self.engine.downloads(),
                zoom,
                self.engine.ad_blocker_enabled(),
                self.preferences.uncapped_fps,
                has_web_site,
                self.settings_section,
                self.preferences.search_engine,
                self.preferences.sidebar_mode,
                self.active_space().color,
                self.plugin_rows(),
                self.pending_plugin_review(),
                self.plugin_action_error().map(str::to_owned),
                self.expanded_plugins.clone(),
                cx,
            );
            let spring = if is_closing {
                ui::motion::sheet().between(1.0, 0.0)
            } else {
                ui::motion::sheet().enter()
            };
            let overlay = div()
                .id("app-page-overlay")
                .when(!is_closing, |el| el.occlude())
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
                .bg(rgb(theme.base))
                .overflow_hidden()
                .child(content)
                .with_spring(
                    format!(
                        "app-page-overlay-{}-{serial}",
                        if is_closing { "exit" } else { "enter" }
                    ),
                    spring,
                    move |overlay, progress| {
                        let p = ui::motion::clamp_percentage(progress);
                        overlay
                            .top(px(topbar_height + 18.0 * UI_SCALE * (1.0 - p)))
                            .opacity(p)
                    },
                );
            root = root.child(overlay);
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
                    .active(|style| style.bg(rgb(theme.accent)))
                    .on_mouse_down(
                        gpui_kit::MouseButton::Left,
                        cx.listener(|this, _, _window, cx| {
                            if let Some(transition) = this.sidebar_width_transition.take() {
                                this.sidebar_width =
                                    transition.width().clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                                this.sidebar_transition_serial =
                                    this.sidebar_transition_serial.wrapping_add(1);
                            }
                            this.sidebar_resize_tracker.clear();
                            this.sidebar_resizing = true;
                            #[cfg(not(test))]
                            this.begin_browser_resize_drag(_window, false, false);
                            cx.notify();
                        }),
                    ),
            );
        }

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
                    .timer(ui::motion::pop().duration())
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
                .bg(rgb(visual::panel_surface(theme.surface)))
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
                        .hover(|s| {
                            s.bg(crate::ui::visual::translucent_surface(
                                theme.surface_hover,
                                0x40,
                            ))
                        })
                        .when(
                            closing_suggestions.is_none() && self.selected_suggestion == Some(idx),
                            |row| {
                                row.bg(crate::ui::visual::translucent_surface(
                                    theme.surface_hover,
                                    0x40,
                                ))
                            },
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
                list.with_spring(
                    format!("autocomplete-suggestions-exit-{serial}"),
                    ui::motion::pop().between(1.0, 0.0),
                    move |panel, remaining| {
                        let remaining = ui::motion::clamp_percentage(remaining);
                        panel
                            .w(px(suggestion_width * (0.96 + 0.04 * remaining)))
                            .opacity(remaining)
                    },
                )
                .into_any_element()
            } else {
                list.with_spring(
                    format!(
                        "autocomplete-suggestions-entrance-{}",
                        self.suggestion_animation_serial
                    ),
                    ui::motion::popover().enter(),
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

        if self.show_find {
            if !self.find_was_open {
                self.find_serial = self.find_serial.wrapping_add(1);
                self.find_was_open = true;
                self.find_closing = None;
            }
        } else if self.find_was_open {
            self.find_was_open = false;
            self.find_serial = self.find_serial.wrapping_add(1);
            let serial = self.find_serial;
            if !ui::motion::reduced_motion() {
                self.find_closing = Some(serial);
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(ui::motion::popover().duration())
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.find_closing == Some(serial) {
                            this.find_closing = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }

        if let Some((is_closing, serial)) = if self.show_find {
            Some((false, self.find_serial))
        } else {
            self.find_closing.map(|serial| (true, serial))
        } {
            let spring = if is_closing {
                ui::motion::popover().between(1.0, 0.0)
            } else {
                ui::motion::popover().enter()
            };
            root = root.child(
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
                    .bg(rgb(visual::panel_surface(theme.surface)))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .child(
                        Input::new(&self.find_input)
                            .id("find-input")
                            .w_full()
                            .appearance(false),
                    )
                    .with_spring(
                        format!(
                            "find-in-page-{}-{serial}",
                            if is_closing { "exit" } else { "enter" }
                        ),
                        spring,
                        |bar, progress| {
                            let p = ui::motion::clamp_percentage(progress);
                            bar.top(px((8.0 - 8.0 * (1.0 - p)) * UI_SCALE)).opacity(p)
                        },
                    ),
            );
        }

        if self.show_command_palette {
            if !self.command_palette_was_open {
                self.command_palette_serial = self.command_palette_serial.wrapping_add(1);
                self.command_palette_was_open = true;
                self.command_palette_closing = None;
            }
        } else if self.command_palette_was_open {
            self.command_palette_was_open = false;
            self.command_palette_serial = self.command_palette_serial.wrapping_add(1);
            let serial = self.command_palette_serial;
            if !ui::motion::reduced_motion() {
                self.command_palette_closing = Some(serial);
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(ui::motion::popover().duration())
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.command_palette_closing == Some(serial) {
                            this.command_palette_closing = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }

        if let Some((is_closing, serial)) = if self.show_command_palette {
            Some((false, self.command_palette_serial))
        } else {
            self.command_palette_closing.map(|serial| (true, serial))
        } {
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
                                .active(|style| style.opacity(0.88))
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
                .bg(rgb(visual::panel_surface(theme.surface)))
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
                .with_spring(
                    format!(
                        "command-palette-{}-{serial}",
                        if is_closing { "exit" } else { "enter" }
                    ),
                    if is_closing {
                        ui::motion::popover().between(1.0, 0.0)
                    } else {
                        ui::motion::popover().enter()
                    },
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
                    .when(!is_closing, |el| el.occlude())
                    .child(panel),
            );
        }

        let visible_peek = self
            .peek
            .clone()
            .filter(|_| self.fullscreen_tab.is_none());
        if let Some(peek) = &visible_peek {
            if self
                .peek_last
                .as_ref()
                .is_none_or(|prev| prev.id != peek.id)
            {
                self.peek_closing_serial = self.peek_closing_serial.wrapping_add(1);
            }
            self.peek_last = Some(peek.clone());
            self.peek_closing = None;
        } else if let Some(prev_peek) = self.peek_last.take() {
            self.peek_closing_serial = self.peek_closing_serial.wrapping_add(1);
            let serial = self.peek_closing_serial;
            if !ui::motion::reduced_motion() && self.fullscreen_tab.is_none() {
                self.peek_closing = Some((prev_peek, serial));
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(ui::motion::sheet().duration())
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.peek_closing.as_ref().is_some_and(|(_, id)| *id == serial) {
                            this.peek_closing = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }

        if let Some((peek, is_closing, serial)) = visible_peek
            .map(|peek| (peek, false, self.peek_closing_serial))
            .or_else(|| {
                self.peek_closing
                    .clone()
                    .filter(|_| self.fullscreen_tab.is_none())
                    .map(|(peek, serial)| (peek, true, serial))
            })
        {
            let (window_width, window_height) = {
                let bounds = window.bounds();
                (f32::from(bounds.size.width), f32::from(bounds.size.height))
            };
            let layout = peek_layout(
                window_width,
                window_height,
                sidebar_width_f32,
                sidebar_on_right,
                topbar_height,
            );
            let image = self.engine.get_image(&peek.id);
            let cursor = self.engine.get_cursor(&peek.id);
            self.peek_web_viewport.update(cx, |viewport, cx| {
                let mut changed = viewport.set_theme(theme);
                changed |= viewport.set_peek_mode(true);
                changed |=
                    viewport.set_origin_override(Some((layout.viewport_x, layout.viewport_y)));
                changed |= viewport.set_frame(
                    peek.id.clone(),
                    0.0,
                    image,
                    cursor,
                    Vec::new(),
                    0.0,
                    0,
                    false,
                    None,
                );
                if changed {
                    cx.notify();
                }
            });

            let title = if peek.title.trim().is_empty() {
                peek.url.clone()
            } else {
                peek.title.clone()
            };
            let favicon = self.engine.get_favicon_image(&peek.id, Some(&peek.url));
            let favicon_element = tab_list::favicon(favicon);
            let header_button = |id: &'static str, icon: IconName| {
                div()
                    .id(id)
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
                    .active(|style| style.opacity(0.9))
                    .child(Icon::new(icon).size(px(15.0 * UI_SCALE)))
            };

            let inner_radius = (CARD_RADIUS - 1.0).max(0.0);
            let card = div()
                .id("peek-overlay")
                .when(!is_closing, |el| el.occlude())
                .absolute()
                .left(px(layout.card_x))
                .top(px(layout.card_y))
                .w(px(layout.card_w))
                .h(px(layout.card_h))
                .rounded(px(CARD_RADIUS))
                .bg(rgb(visual::panel_surface(theme.surface)))
                .border_1()
                .border_color(rgb(theme.border))
                .shadow_2xl()
                .overflow_hidden()
                .flex()
                .flex_col()
                .with_spring(
                    format!(
                        "peek-overlay-{}-{}-{serial}",
                        if is_closing { "exit" } else { "enter" },
                        peek.id
                    ),
                    if is_closing {
                        ui::motion::sheet().between(1.0, 0.0)
                    } else {
                        ui::motion::sheet().enter()
                    },
                    move |card, progress| {
                        let p = ui::motion::clamp_percentage(progress);
                        card.top(px(layout.card_y + 18.0 * UI_SCALE * (1.0 - p)))
                            .opacity(p)
                    },
                )
                .child(
                    div()
                        .id("peek-header")
                        .flex_none()
                        .h(px(PEEK_HEADER_HEIGHT))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px(px(10.0 * UI_SCALE))
                        .bg(rgb(visual::panel_surface(theme.surface)))
                        .rounded_t(px(inner_radius))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(favicon_element)
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_ellipsis()
                                        .text_color(rgb(theme.text))
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_ellipsis()
                                        .text_color(rgb(theme.muted))
                                        .child(peek.url.clone()),
                                ),
                        )
                        .child(
                            header_button("peek-open-in-tab", IconName::ExternalLink).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.open_peek_in_tab();
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            header_button("peek-close", IconName::X).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.close_peek();
                                    cx.notify();
                                },
                            )),
                        ),
                )
                .child(
                    div()
                        .id("peek-viewport")
                        .flex_1()
                        .relative()
                        .rounded_b(px(inner_radius))
                        .overflow_hidden()
                        .child(
                            AnyView::from(self.peek_web_viewport.clone())
                                .cached(StyleRefinement::default().size_full()),
                        ),
                );
            root = root.child(card);
        } else {
            if self.peek_web_viewport.read(cx).has_content() {
                self.peek_web_viewport.update(cx, |viewport, cx| {
                    viewport.clear_content();
                    cx.notify();
                });
            }
        }

        // Key-up events must reach the focused web viewport. Pages such as
        // YouTube use them to undo temporary keyboard actions (for example,
        // holding Space temporarily raises playback speed).
        root = input::with_key_handler(root, cx);

        let split_menu_tabs = if self
            .tab_context_menu
            .as_ref()
            .is_some_and(|menu| menu.showing_split_targets)
            && crate::plugins::split_view_enabled()
        {
            self.active_space()
                .tabs
                .iter()
                .map(|tab| {
                    (
                        tab.id.clone(),
                        tab.title.clone(),
                        self.engine.get_favicon_image(&tab.id, Some(&tab.url)),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

        root = root
            .children(self.render_dialogs(theme, cx))
            .children(self.render_context_menus(theme, &split_menu_tabs, cx));

        if let Some(tab_id) = self.fullscreen_tab.clone() {
            let image = self.engine.get_image(&tab_id);
            let cursor = self.engine.get_cursor(&tab_id);
            self.web_viewport.update(cx, |viewport, cx| {
                let mut changed = viewport.set_theme(theme);
                changed |= viewport.set_fullscreen(true);
                changed |= viewport.set_frame(
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
                if changed {
                    cx.notify();
                }
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
        }

        if let Some((from_theme, _, transition_id)) = self.theme_transition {
            let to_theme = target_theme;
            root.with_spring(
                format!("space-theme-transition-{transition_id}"),
                ui::motion::theme().enter(),
                move |root, progress| {
                    let progress = ui::motion::clamp_percentage(progress);
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
