use super::*;

mod context_menus;
mod dialogs;
mod shell;
mod sidebar;
mod theme_picker;

impl Render for Lumen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        _window.set_rem_size(px(16.0 * UI_SCALE));
        stats::record_ui_render();
        let auto_light = matches!(
            _window.appearance(),
            gpui_kit::WindowAppearance::Light | gpui_kit::WindowAppearance::VibrantLight
        );
        if let Some(modal) = self.space_modal.as_mut() {
            modal.theme_draft.theme_auto_light = auto_light;
        }
        for space in &mut self.spaces {
            space.theme_auto_light = auto_light;
        }
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            eprintln!(
                "Lumen time-to-first-frame: {:.1} ms",
                self.started_at.elapsed().as_secs_f64() * 1000.0
            );
        }

        #[cfg(not(test))]
        if !self.cef_task_started {
            self.cef_task_started = true;
            let refresh_rate = engine::frame_rate_for_refresh(_window.refresh_interval(cx));
            if std::env::var_os("LUMEN_STATS").is_some_and(|value| value == "1") {
                eprintln!("Lumen OSR requested frame rate: {refresh_rate} fps");
            }
            self.engine.set_windowless_frame_rate(refresh_rate);
            // Synthetic input for repeatable OSR benchmarks on Wayland, where X11
            // window automation cannot target this GPUI window.
            if std::env::var_os("LUMEN_AUTOSCROLL").is_some_and(|value| value == "1") {
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(5))
                        .await;
                    let mut direction = -1;
                    let mut direction_started = Instant::now();
                    loop {
                        if direction_started.elapsed() >= std::time::Duration::from_secs(2) {
                            direction = -direction;
                            direction_started = Instant::now();
                        }
                        if this
                            .update(cx, |this, _| {
                                this.engine.send_mouse_wheel(
                                    this.active_tab_id(),
                                    300,
                                    300,
                                    0,
                                    50 * direction,
                                    &Default::default(),
                                );
                            })
                            .is_err()
                        {
                            break;
                        }
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(16))
                            .await;
                    }
                })
                .detach();
            }
            let ui_update_receiver = self.engine.ui_update_receiver();
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(180))
                    .await;
                loop {
                    while ui_update_receiver.try_recv().is_ok() {}
                    let updated = this.update(cx, |this, cx| {
                        let was_initialized = this.engine.is_initialized();
                        if let Err(error) = this.engine.initialize() {
                            eprintln!("Lumen CEF startup failed: {error}");
                            return None;
                        }
                        if this.engine.take_created_browsers() {
                            let active = this.active_tab_id().to_owned();
                            this.engine.set_visible(&active);
                            this.active_zoom_target = None;
                            cx.notify();
                        }
                        let popups = this.engine.take_popups();
                        for (space_id, url) in &popups {
                            this.open_popup_tab(space_id, url.clone());
                        }
                        if !popups.is_empty() {
                            cx.notify();
                        }
                        for (tab_id, fullscreen) in this.engine.take_fullscreen_changes() {
                            if fullscreen {
                                this.fullscreen_tab = Some(tab_id);
                            } else if this.fullscreen_tab.as_deref() == Some(tab_id.as_str()) {
                                this.fullscreen_tab = None;
                            }
                            cx.notify();
                        }
                        let visible_ids = this.current_split_pair().map_or_else(
                            || vec![this.active_tab_id().to_string()],
                            |pair| vec![pair.first, pair.second],
                        );
                        let visible_refs =
                            visible_ids.iter().map(String::as_str).collect::<Vec<_>>();
                        let (has_updates, has_root_updates) =
                            this.engine.take_dirty_for(&visible_refs);
                        if has_root_updates {
                            stats::record_root_metadata_update();
                            for tab_id in &visible_ids {
                                if let Some((title, url)) = this.engine.take_updates(tab_id)
                                    && let Some(tab) = this
                                        .active_space_mut()
                                        .tabs
                                        .iter_mut()
                                        .find(|tab| tab.id == *tab_id)
                                {
                                    if let Some(title) = title.filter(|title| !title.is_empty()) {
                                        tab.title = title;
                                    }
                                    if let Some(url) = url.filter(|url| !url.is_empty()) {
                                        tab.url = url;
                                    }
                                }
                            }
                            cx.notify();
                        }
                        if !was_initialized {
                            cx.notify();
                        } else if has_updates {
                            let retired_images = this.engine.take_retired_images();
                            let first_tab = visible_ids[0].clone();
                            let image = this.engine.get_image(&first_tab);
                            let cursor = this.engine.get_cursor(&first_tab);
                            let sidebar_width = this.effective_sidebar_width();
                            let frame_matches_target = this
                                .resize_drag
                                .target_size(&first_tab)
                                .is_some_and(|(w, h)| {
                                    this.engine.frame_matches_size(&first_tab, w, h)
                                });
                            let edge_color = this
                                .resize_drag
                                .edge_color_until_frame(&first_tab, frame_matches_target);
                            this.web_viewport.update(cx, |viewport, cx| {
                                viewport.set_frame(
                                    first_tab,
                                    sidebar_width,
                                    image,
                                    cursor,
                                    retired_images,
                                    0.0,
                                    0,
                                    visible_ids.len() > 1,
                                    edge_color,
                                );
                                cx.notify();
                            });
                            if let Some(second_tab) = visible_ids.get(1).cloned() {
                                let image = this.engine.get_image(&second_tab);
                                let cursor = this.engine.get_cursor(&second_tab);
                                this.second_web_viewport.update(cx, |viewport, cx| {
                                    viewport.set_content(second_tab, image, cursor, Vec::new());
                                    cx.notify();
                                });
                            }
                        }
                        Some(())
                    });
                    let Ok(Some(())) = updated else {
                        break;
                    };
                    let fallback_ms = ((1000.0 / refresh_rate as f64).round() as u64).max(4);
                    futures_lite::future::or(
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(fallback_ms)),
                        async {
                            let _ = ui_update_receiver.recv().await;
                        },
                    )
                    .await;
                }
            })
            .detach();
        }

        #[cfg(all(target_os = "linux", not(test)))]
        {
            let current_active_id = self.active_space().active.clone().unwrap_or_default();
            if !current_active_id.is_empty()
                && let Some((new_title, new_url)) = self.engine.take_updates(&current_active_id)
            {
                let space = self.active_space_mut();
                if let Some(tab) = space.tabs.iter_mut().find(|t| t.id == current_active_id) {
                    if let Some(t) = new_title
                        && !t.is_empty()
                    {
                        tab.title = t;
                    }
                    if let Some(u) = new_url
                        && !u.is_empty()
                    {
                        tab.url = u;
                    }
                }
            }
        }

        let expired_tabs = self.engine.discard_inactive();
        if !expired_tabs.is_empty() {
            for space in &mut self.spaces {
                for tab in &mut space.tabs {
                    if expired_tabs.iter().any(|id| id == &tab.id) {
                        tab.state = TabState::Discarded;
                    }
                }
            }
        }

        for (space_id, url) in self.engine.take_popups() {
            self.open_popup_tab(&space_id, url);

            for retired in self.engine.take_retired_images() {
                let _ = _window.drop_image(retired);
            }
        }

        let current_tab_url = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == self.active_tab_id())
            .map(|tab| tab.url.clone())
            .unwrap_or_default();
        let active_address_tab = (
            self.active_space.clone(),
            self.active_tab_id().to_owned(),
            current_tab_url.clone(),
        );
        let address_focused = _window.focused(cx) == Some(self.address.read(cx).focus_handle(cx));
        if !address_focused && self.show_suggestions {
            self.show_suggestions = false;
            self.selected_suggestion = None;
        }
        if self.address_active_tab.as_ref() != Some(&active_address_tab) {
            let active_url =
                if current_tab_url == "about:blank" || current_tab_url == "lumen://welcome" {
                    String::new()
                } else {
                    current_tab_url
                };
            let force_reset = self.reset_address_for_tab_change;
            if force_reset || !(self.address_editing && address_focused) {
                self.address.update(cx, |input, cx| {
                    input.set_value(&active_url, _window, cx);
                    if force_reset {
                        input.focus(_window, cx);
                    }
                });
                self.address_editing = false;
                self.show_suggestions = false;
                self.selected_suggestion = None;
            }
            self.address_active_tab = Some(active_address_tab);
            self.reset_address_for_tab_change = false;
        }

        let active_space_id = self.active_space.clone();
        let active_space = self.active_space().clone();
        #[cfg(all(target_os = "linux", not(test)))]
        let active_tab_id = active_space.active.clone();
        let target_theme = self.space_modal.as_ref().map_or_else(
            || visual::theme_for_space(&active_space),
            |modal| match &modal.mode {
                SpaceModalMode::Create => visual::theme_for_space(&modal.theme_draft),
                SpaceModalMode::Edit { space_id } if space_id == &active_space_id => {
                    visual::theme_for_space(&modal.theme_draft)
                }
                _ => visual::theme_for_space(&active_space),
            },
        );
        let theme = if ui::motion::reduced_motion() {
            target_theme
        } else {
            self.theme_transition
                .map_or(target_theme, |(from, started, _)| {
                    let progress = started.elapsed().as_secs_f32()
                        / ui::motion::MOTION_SPACE_SWITCH
                            .as_secs_f32()
                            .max(f32::EPSILON);
                    visual::interpolate_theme(
                        from,
                        target_theme,
                        ui::motion::ease_out_quint(progress),
                    )
                })
        };
        let space_switch_transition =
            self.space_switch_transition
                .clone()
                .filter(|(_, _, started, _)| {
                    started.elapsed() < ui::motion::duration(ui::motion::MOTION_SPACE_SWITCH)
                });
        let tabs: Vec<(
            String,
            String,
            Option<std::sync::Arc<gpui_kit::RenderImage>>,
        )> = active_space
            .tabs
            .iter()
            .map(|tab| {
                (
                    tab.id.clone(),
                    tab.title.clone(),
                    self.engine.get_favicon_image(&tab.id, Some(&tab.url)),
                )
            })
            .collect();
        #[cfg(all(target_os = "linux", not(test)))]
        let active_tab = active_space
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == active_tab_id.as_ref());
        #[cfg(all(target_os = "linux", not(test)))]
        let active_tab_id_string = active_tab_id.clone().unwrap_or_default();
        #[cfg(all(target_os = "linux", not(test)))]
        let active_tab_url = active_tab.map(|tab| tab.url.clone()).unwrap_or_default();
        #[cfg(all(target_os = "linux", not(test)))]
        let split_pair = self.current_split_pair();

        #[cfg(all(target_os = "linux", not(test)))]
        let scale = _window.scale_factor();
        let sidebar_width_f32 = self.sidebar_width_transition.map_or_else(
            || self.effective_sidebar_width(),
            |(from, to, started, _)| {
                let progress = started.elapsed().as_secs_f32()
                    / ui::motion::MOTION_SLOW.as_secs_f32().max(f32::EPSILON);
                from + (to - from) * ui::motion::ease_out_quint(progress)
            },
        );
        #[cfg(all(target_os = "linux", not(test)))]
        let bounds = _window.bounds();
        #[cfg(all(target_os = "linux", not(test)))]
        let width = f32::from(bounds.size.width);
        #[cfg(all(target_os = "linux", not(test)))]
        let height = f32::from(bounds.size.height);
        #[cfg(all(target_os = "linux", not(test)))]
        let browser_fullscreen =
            self.fullscreen_tab.as_deref() == Some(active_tab_id_string.as_str());
        #[cfg(all(target_os = "linux", not(test)))]
        let content_w = if browser_fullscreen {
            width.max(100.0) as i32
        } else {
            (width - sidebar_width_f32 - (CONTENT_BEZEL_X * 2.0)).max(100.0) as i32
        };
        #[cfg(all(target_os = "linux", not(test)))]
        let content_h = if browser_fullscreen {
            height.max(100.0) as i32
        } else {
            (height - TOPBAR_HEIGHT - CONTENT_BEZEL_BOTTOM).max(100.0) as i32
        };

        #[cfg(all(target_os = "linux", not(test)))]
        if self.engine.is_initialized()
            && !active_tab_id_string.is_empty()
            && (split_pair.is_some()
                || (active_tab_url != "lumen://welcome"
                    && active_tab_url != "about:blank"
                    && !active_tab_url.is_empty()))
        {
            let (left_width, right_width) =
                split_view::pane_widths(content_w as f32, self.split_ratio);
            let left_width_dip = left_width as i32;
            let right_width_dip = right_width as i32;
            let active_width = if browser_fullscreen {
                content_w
            } else {
                split_pair.as_ref().map_or(content_w, |pair| {
                    if pair.first == active_tab_id_string {
                        left_width_dip
                    } else if pair.second == active_tab_id_string {
                        right_width_dip
                    } else {
                        content_w
                    }
                })
            };
            let active_width = self
                .resize_drag
                .frozen_size(&active_tab_id_string)
                .map_or(active_width, |(width, _)| width);
            if active_tab_url != "lumen://welcome"
                && active_tab_url != "about:blank"
                && !active_tab_url.is_empty()
                && let Err(error) = self.engine.ensure_browser(
                    &active_space_id,
                    &active_tab_id_string,
                    &active_tab_url,
                    active_width.max(1),
                    content_h,
                    0xff000000 | theme.surface,
                    self.zoom_levels
                        .get(&url_host(&active_tab_url))
                        .copied()
                        .unwrap_or_else(|| {
                            visual::zoom_level_for_factor(self.preferences.default_zoom_factor)
                        }),
                )
            {
                eprintln!("Lumen CEF browser creation failed: {error}");
            }
            if let Some(pair) = &split_pair {
                if let Some(tab) = active_space.tabs.iter().find(|tab| tab.id == pair.first)
                    && tab.url != "lumen://welcome"
                    && tab.url != "about:blank"
                    && !tab.url.is_empty()
                    && let Err(error) = self.engine.ensure_browser(
                        &active_space_id,
                        &pair.first,
                        &tab.url,
                        self.resize_drag
                            .frozen_size(&pair.first)
                            .map_or(left_width_dip.max(1), |(width, _)| width),
                        content_h,
                        0xff000000 | theme.surface,
                        self.zoom_levels
                            .get(&url_host(&tab.url))
                            .copied()
                            .unwrap_or_else(|| {
                                visual::zoom_level_for_factor(self.preferences.default_zoom_factor)
                            }),
                    )
                {
                    eprintln!("Lumen split browser creation failed: {error}");
                }
                if let Some(tab) = active_space.tabs.iter().find(|tab| tab.id == pair.second)
                    && tab.url != "lumen://welcome"
                    && tab.url != "about:blank"
                    && !tab.url.is_empty()
                    && let Err(error) = self.engine.ensure_browser(
                        &active_space_id,
                        &pair.second,
                        &tab.url,
                        self.resize_drag
                            .frozen_size(&pair.second)
                            .map_or(right_width_dip.max(1), |(width, _)| width),
                        content_h,
                        0xff000000 | theme.surface,
                        self.zoom_levels
                            .get(&url_host(&tab.url))
                            .copied()
                            .unwrap_or_else(|| {
                                visual::zoom_level_for_factor(self.preferences.default_zoom_factor)
                            }),
                    )
                {
                    eprintln!("Lumen split browser creation failed: {error}");
                }
            }
            if let Some(pair) = &split_pair {
                let left_w = self
                    .resize_drag
                    .frozen_size(&pair.first)
                    .map_or(left_width_dip.max(1), |(width, _)| width);
                let right_w = self
                    .resize_drag
                    .frozen_size(&pair.second)
                    .map_or(right_width_dip.max(1), |(width, _)| width);
                self.engine.resize_browser(&pair.first, left_w, content_h);
                self.engine.resize_browser(&pair.second, right_w, content_h);
            } else {
                self.engine
                    .resize_browser(&active_tab_id_string, active_width.max(1), content_h);
            }
            let host = url_host(&active_tab_url);
            let zoom_target = (active_tab_id_string.clone(), host.clone());
            if !host.is_empty() && self.active_zoom_target.as_ref() != Some(&zoom_target) {
                let level = self.zoom_levels.get(&host).copied().unwrap_or_else(|| {
                    visual::zoom_level_for_factor(self.preferences.default_zoom_factor)
                });
                self.engine.set_zoom_level(&active_tab_id_string, level);
                if self.engine.has_browser(&active_tab_id_string) {
                    self.active_zoom_target = Some(zoom_target);
                }
            } else if host.is_empty() {
                self.active_zoom_target = None;
            }
            let visible_ids = split_pair.as_ref().map_or_else(
                || vec![active_tab_id_string.as_str()],
                |pair| vec![pair.first.as_str(), pair.second.as_str()],
            );
            self.engine.set_visible_tabs(&visible_ids);
            let display = _window.display(cx);
            let window_bounds = _window.bounds();
            let (screen_width, screen_height) = display
                .map(|display| {
                    let bounds = display.bounds();
                    (
                        f32::from(bounds.size.width) as i32,
                        f32::from(bounds.size.height) as i32,
                    )
                })
                .unwrap_or((content_w, content_h));
            for tab_id in &visible_ids {
                self.engine.set_screen_metrics(
                    tab_id,
                    screen_width,
                    screen_height,
                    scale,
                    f32::from(window_bounds.origin.x) as i32,
                    f32::from(window_bounds.origin.y) as i32,
                );
            }
            if let Some(tab) = self
                .active_space_mut()
                .tabs
                .iter_mut()
                .find(|tab| tab.id == active_tab_id_string)
            {
                tab.state = TabState::Live;
            }
        } else {
            self.engine.hide_all();
        }

        #[cfg(all(target_os = "linux", not(test)))]
        for retired in self.engine.take_retired_images() {
            let _ = _window.drop_image(retired);
        }

        let sidebar_content = self.render_sidebar(theme, cx);

        let outgoing_space_overlay =
            space_switch_transition
                .as_ref()
                .map(|(outgoing_space, state, _, serial)| {
                    let old_theme = visual::theme_for_space(outgoing_space);
                    let mut overlay = div()
                        .id(format!(
                            "space-switch-outgoing-{}-{serial}",
                            outgoing_space.id
                        ))
                        .absolute()
                        .top_0()
                        .left_0()
                        .w(px(sidebar_width_f32))
                        .h_full()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .px_3()
                        .py_3()
                        .gap_2()
                        .bg(rgb(old_theme.base))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .px_2()
                                .py_2()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(old_theme.text))
                                .child(outgoing_space.name.clone()),
                        );
                    let mut outgoing_rows = Vec::new();
                    let split_tabs = outgoing_space.split.as_ref().and_then(|pair| {
                        let first = outgoing_space
                            .tabs
                            .iter()
                            .find(|tab| tab.id == pair.first)?;
                        let second = outgoing_space
                            .tabs
                            .iter()
                            .find(|tab| tab.id == pair.second)?;
                        Some((pair, first, second))
                    });
                    if let Some((pair, first, second)) = split_tabs {
                        let active_is_member = outgoing_space.active.as_deref()
                            == Some(pair.first.as_str())
                            || outgoing_space.active.as_deref() == Some(pair.second.as_str());
                        outgoing_rows.push((
                            tab_list::group_label(&first.title, &second.title),
                            None,
                            active_is_member && !self.split_group_expanded,
                        ));
                        if self.split_group_expanded {
                            for member in [first, second] {
                                outgoing_rows.push((
                                    member.title.clone(),
                                    self.engine.get_favicon_image(&member.id, Some(&member.url)),
                                    outgoing_space.active.as_deref() == Some(member.id.as_str()),
                                ));
                            }
                        }
                    }
                    for tab in &outgoing_space.tabs {
                        if outgoing_space
                            .split
                            .as_ref()
                            .is_some_and(|pair| tab.id == pair.first || tab.id == pair.second)
                        {
                            continue;
                        }
                        outgoing_rows.push((
                            tab.title.clone(),
                            self.engine.get_favicon_image(&tab.id, Some(&tab.url)),
                            outgoing_space.active.as_deref() == Some(tab.id.as_str()),
                        ));
                    }
                    let new_tab_index = outgoing_rows.len();
                    let active_row_index = outgoing_rows.iter().position(|(_, _, active)| *active);
                    for (row_index, (title, favicon, selected)) in
                        outgoing_rows.into_iter().enumerate()
                    {
                        let _ = row_index;
                        let row = div()
                            .flex_none()
                            .w_full()
                            .h(px(visual::TAB_ROW_HEIGHT))
                            .rounded_lg()
                            .when(selected, |row| row.bg(rgb(old_theme.surface)))
                            .flex()
                            .items_center()
                            .gap(px(visual::SIDEBAR_ROW_GAP))
                            .px(px(visual::SIDEBAR_ROW_PADDING_X))
                            .text_sm()
                            .text_color(rgb(if selected {
                                old_theme.text
                            } else {
                                old_theme.muted
                            }))
                            .overflow_hidden()
                            .child(tab_list::favicon(favicon))
                            .child(title);
                        overlay = overlay.child(row);
                    }
                    let _ = active_row_index;
                    let _ = new_tab_index;
                    overlay = overlay.child(
                        div()
                            .flex_none()
                            .w_full()
                            .h(px(visual::TAB_ROW_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(visual::SIDEBAR_ROW_GAP))
                            .px(px(visual::SIDEBAR_ROW_PADDING_X))
                            .text_sm()
                            .text_color(rgb(old_theme.muted))
                            .child(Icon::new(IconName::Plus).size(px(14.0 * UI_SCALE)))
                            .child("New tab"),
                    );
                    let initial_alpha = state.outgoing_opacity_start;
                    let initial_offset = state.outgoing_offset_start;
                    let outgoing_offset = state.outgoing_direction * 24.0;
                    overlay
                        .with_animation(
                            format!("space-switch-sidebar-out-{serial}"),
                            Animation::new(ui::motion::duration(ui::motion::MOTION_SPACE_SWITCH))
                                .with_easing(ui::motion::ease_out_quint),
                            move |overlay, progress| {
                                overlay
                                    .left(px(initial_offset * (1.0 - progress)
                                        + outgoing_offset * progress))
                                    .opacity(ui::motion::clamp_percentage(
                                        initial_alpha * (1.0 - progress),
                                    ))
                            },
                        )
                        .into_any_element()
                });

        self.render_shell(
            _window,
            cx,
            theme,
            target_theme,
            &tabs,
            sidebar_content,
            outgoing_space_overlay,
        )
    }
}
