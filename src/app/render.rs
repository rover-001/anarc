use super::*;

// TEMPORARY benchmark hook.
static BENCH_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(not(test))]
use super::actions::peek::peek_layout;

mod context_menus;
mod dialogs;
mod shell;
mod sidebar;
mod theme_picker;

impl Render for Lumen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        _window.set_rem_size(px(16.0 * UI_SCALE));
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
        // TEMPORARY benchmark hook.
        if std::env::var_os("ANARC_BENCH_URL").is_some()
            && !BENCH_OPENED.swap(true, std::sync::atomic::Ordering::Relaxed)
            && let Ok(url) = std::env::var("ANARC_BENCH_URL")
        {
            let id = self.active_tab_id().to_owned();
            if let Some(tab) = self
                .active_space_mut()
                .tabs
                .iter_mut()
                .find(|tab| tab.id == id)
            {
                tab.url = url;
                tab.state = TabState::Discarded;
            }
        }

        #[cfg(not(test))]
        if !self.cef_task_started {
            self.cef_task_started = true;
            let interval = _window.refresh_interval(cx);
            let paint_cap = engine::frame_rate_for_refresh(interval);
            self.engine.set_windowless_frame_rate(paint_cap);
            cx.observe_window_display(_window, |this, window, cx| {
                let paint_cap = engine::frame_rate_for_refresh(window.refresh_interval(cx));
                this.engine.set_windowless_frame_rate(paint_cap);
            })
            .detach();

            // Synthetic input for repeatable OSR benchmarks on Wayland, where X11
            // window automation cannot target this GPUI window.
            if std::env::var_os("ANARC_AUTOSCROLL").is_some_and(|value| value == "1") {
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
                                    false,
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
                    // A queued wake can be coalesced into this update. A wake that
                    // arrives during the update stays queued for the next pass.
                    let _ = ui_update_receiver.try_recv();
                    let scheduled = this.update_in(cx, |this, window, cx| {
                        let was_initialized = this.engine.is_initialized();
                        if let Err(error) = this.engine.initialize() {
                            eprintln!("Lumen CEF startup failed: {error}");
                            return None;
                        }
                        // Fast path: skip heavy event processing when no CEF events
                        // are pending. During scrolling, this avoids blocking the
                        // async loop on plugin events, popups, close requests, etc.
                        if this.engine.has_pending_events() {
                            for (event, payload) in this.engine.take_plugin_events() {
                                if event == "page.load_end"
                                    && let Some((tab_id, url)) = payload.split_once('\n')
                                {
                                    this.inject_pack_assets(tab_id, url);
                                    if this.engine.ad_blocker_enabled() {
                                        if let Some(script) =
                                            this.engine.shield_cosmetic_script(url)
                                        {
                                            this.engine.execute_script(tab_id, &script);
                                        }
                                        if crate::shield::is_youtube_url(url) {
                                            this.engine.execute_script(
                                                tab_id,
                                                crate::shield::youtube_ad_shield_script(),
                                            );
                                        }
                                    }
                                }
                                let mode = match event.as_str() {
                                    "address.suggest" => crate::plugin::EventMode::Parallel,
                                    "navigation.before" => crate::plugin::EventMode::Waterfall,
                                    "popup.before" => crate::plugin::EventMode::Bail,
                                    "download.before" => crate::plugin::EventMode::Waterfall,
                                    "context_menu.build" => crate::plugin::EventMode::Serial,
                                    "permission.request" => crate::plugin::EventMode::Bail,
                                    _ => crate::plugin::EventMode::Emit,
                                };
                                this.emit_plugin_text_with_mode(&event, mode, payload);
                            }
                            if this.engine.take_created_browsers() {
                                let active = this.active_tab_id().to_owned();
                                this.engine.set_visible(&active);
                                this.active_zoom_target = None;
                                cx.notify();
                            }
                            let popups = this.engine.take_popups();
                            let has_popups = !popups.is_empty();
                            for popup in popups {
                                match popup.browser_id {
                                    Some(browser_id) => this.show_popup_window(
                                        &popup.space_id,
                                        popup.url,
                                        browser_id,
                                    ),
                                    None => this.open_popup_tab(&popup.space_id, popup.url),
                                }
                            }
                            if has_popups {
                                cx.notify();
                            }
                            let close_requests = this.engine.take_close_requests();
                            for tab_id in &close_requests {
                                if this.peek.as_ref().is_some_and(|peek| peek.id == *tab_id) {
                                    this.close_peek_from_page();
                                } else {
                                    // CEF can request a window close when Ctrl+W is
                                    // handled by the web view. Keep pinned/favorite
                                    // tabs in the sidebar, matching the app shortcut.
                                    this.close_section_tab(tab_id);
                                }
                            }
                            if !close_requests.is_empty() {
                                cx.notify();
                            }
                            if let Some(request) = this.engine.take_live_share_space_request() {
                                this.choose_live_share_space(request, window, cx);
                            }
                            for (tab_id, fullscreen) in this.engine.take_fullscreen_changes() {
                                if fullscreen {
                                    this.fullscreen_tab = Some(tab_id);
                                } else if this.fullscreen_tab.as_deref()
                                    == Some(tab_id.as_str())
                                {
                                    this.fullscreen_tab = None;
                                }
                                cx.notify();
                            }
                        }
                        let (visible_ids, split_active, peek_id) = this.visible_browser_panes();
                        let (dirty_tabs, has_root_updates) =
                            this.engine.collect_browser_updates(&visible_ids);
                        if has_root_updates {
                            for tab_id in &visible_ids {
                                if let Some((title, url)) = this.engine.take_updates(tab_id)
                                {
                                    if peek_id.as_deref() == Some(tab_id.as_str()) {
                                        if let Some(peek) = this.peek.as_mut() {
                                            if let Some(title) =
                                                title.filter(|t| !t.is_empty())
                                            {
                                                peek.title = title;
                                            }
                                            if let Some(url) = url.filter(|u| !u.is_empty())
                                            {
                                                peek.url = url;
                                            }
                                        }
                                        continue;
                                    }
                                    if let Some(tab) = this
                                        .active_space_mut()
                                        .tabs
                                        .iter_mut()
                                        .find(|tab| tab.id == *tab_id)
                                    {
                                        if let Some(title) =
                                            title.filter(|title| !title.is_empty())
                                        {
                                            tab.title = title.clone();
                                            if (tab.pinned || tab.favorite)
                                                && tab.pinned_title.is_none()
                                            {
                                                tab.pinned_title = Some(title);
                                            }
                                        }
                                        if let Some(url) = url.filter(|url| !url.is_empty())
                                        {
                                            tab.url = url;
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        }
                        if !was_initialized {
                            cx.notify();
                        } else if !dirty_tabs.is_empty() {
                            this.update_browser_panes(
                                &visible_ids,
                                &dirty_tabs,
                                split_active,
                                peek_id.as_deref(),
                                cx,
                            );
                        }
                        Some(())
                    });
                    if !matches!(scheduled, Ok(Some(()))) {
                        break;
                    }
                    futures_lite::future::or(
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(250)),
                        async {
                            let _ = ui_update_receiver.recv().await;
                        },
                    )
                    .await;
                }
            })
            .detach();
        }

        #[cfg(not(test))]
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
                        tab.title = t.clone();
                        if (tab.pinned || tab.favorite) && tab.pinned_title.is_none() {
                            tab.pinned_title = Some(t);
                        }
                    }
                    if let Some(u) = new_url
                        && !u.is_empty()
                    {
                        tab.url = u;
                    }
                }
            }
        }

        let expired_tabs = if self.plugins.is_active("lumen.lifecycle") {
            self.engine.discard_inactive()
        } else {
            Vec::new()
        };
        if !expired_tabs.is_empty() {
            for space in &mut self.spaces {
                for tab in &mut space.tabs {
                    if expired_tabs.iter().any(|id| id == &tab.id) {
                        tab.state = TabState::Discarded;
                    }
                }
            }
            for tab_id in expired_tabs {
                self.emit_plugin_text("tab.discarded", tab_id);
            }
        }

        for retired in self.engine.take_retired_images() {
            let _ = _window.drop_image(retired);
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
        let active_space = self.active_space();
        #[cfg(not(test))]
        let active_tab_id = active_space.active.clone();
        let target_theme = self.space_modal.as_ref().map_or_else(
            || visual::theme_for_space(active_space),
            |modal| match &modal.mode {
                SpaceModalMode::Create => visual::theme_for_space(&modal.theme_draft),
                SpaceModalMode::Edit { space_id } if space_id == &active_space_id => {
                    visual::theme_for_space(&modal.theme_draft)
                }
                _ => visual::theme_for_space(active_space),
            },
        );
        let theme = if ui::motion::reduced_motion() {
            target_theme
        } else {
            self.theme_transition
                .map_or(target_theme, |(from, started, _)| {
                    visual::interpolate_theme(
                        from,
                        target_theme,
                        ui::motion::theme().progress(started.elapsed()),
                    )
                })
        };
        let space_switch_transition = self
            .space_switch_transition
            .clone()
            .filter(|(_, _, started, _)| started.elapsed() < ui::motion::space_switch().duration());
        #[cfg(not(test))]
        let active_tab = active_space
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == active_tab_id.as_ref());
        #[cfg(not(test))]
        let active_tab_id_string = active_tab_id.clone().unwrap_or_default();
        #[cfg(not(test))]
        let active_tab_url = active_tab.map(|tab| tab.url.clone()).unwrap_or_default();
        #[cfg(not(test))]
        let split_pair = self.current_split_pair();

        #[cfg(not(test))]
        let scale = _window.scale_factor();
        let sidebar_width_f32 = self.sidebar_width_transition.map_or_else(
            || self.effective_sidebar_width(),
            |(from, to, started, _)| {
                let progress = ui::motion::sidebar().progress(started.elapsed());
                from + (to - from) * progress
            },
        );
        #[cfg(not(test))]
        let bounds = _window.bounds();
        #[cfg(not(test))]
        let width = f32::from(bounds.size.width);
        #[cfg(not(test))]
        let height = f32::from(bounds.size.height);
        #[cfg(not(test))]
        let browser_fullscreen =
            self.fullscreen_tab.as_deref() == Some(active_tab_id_string.as_str());
        #[cfg(not(test))]
        let content_w = if browser_fullscreen {
            width.max(100.0) as i32
        } else {
            (width - sidebar_width_f32 - (CONTENT_BEZEL_X * 2.0)).max(100.0) as i32
        };
        #[cfg(not(test))]
        let content_h = if browser_fullscreen {
            height.max(100.0) as i32
        } else {
            (height - visual::active_topbar_height() - CONTENT_BEZEL_BOTTOM).max(100.0) as i32
        };

        #[cfg(not(test))]
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
                if let Some(url) = self
                    .active_space()
                    .tabs
                    .iter()
                    .find(|tab| tab.id == pair.first)
                    .map(|tab| tab.url.clone())
                    && url != "lumen://welcome"
                    && url != "about:blank"
                    && !url.is_empty()
                    && let Err(error) = self.engine.ensure_browser(
                        &active_space_id,
                        &pair.first,
                        &url,
                        self.resize_drag
                            .frozen_size(&pair.first)
                            .map_or(left_width_dip.max(1), |(width, _)| width),
                        content_h,
                        0xff000000 | theme.surface,
                        self.zoom_levels
                            .get(&url_host(&url))
                            .copied()
                            .unwrap_or_else(|| {
                                visual::zoom_level_for_factor(self.preferences.default_zoom_factor)
                            }),
                    )
                {
                    eprintln!("Lumen split browser creation failed: {error}");
                }
                if let Some(url) = self
                    .active_space()
                    .tabs
                    .iter()
                    .find(|tab| tab.id == pair.second)
                    .map(|tab| tab.url.clone())
                    && url != "lumen://welcome"
                    && url != "about:blank"
                    && !url.is_empty()
                    && let Err(error) = self.engine.ensure_browser(
                        &active_space_id,
                        &pair.second,
                        &url,
                        self.resize_drag
                            .frozen_size(&pair.second)
                            .map_or(right_width_dip.max(1), |(width, _)| width),
                        content_h,
                        0xff000000 | theme.surface,
                        self.zoom_levels
                            .get(&url_host(&url))
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
            let mut visible_ids = split_pair.as_ref().map_or_else(
                || vec![active_tab_id_string.as_str()],
                |pair| vec![pair.first.as_str(), pair.second.as_str()],
            );
            let peek_visible = self.peek.as_ref().map(|peek| peek.id.clone());
            if let Some(peek_id) = peek_visible.as_deref() {
                visible_ids.push(peek_id);
            }
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

        #[cfg(not(test))]
        if let Some(peek) = self.peek.clone()
            && self.engine.is_initialized()
        {
            let sidebar_on_right = visual::active_layout_preset()
                .is_some_and(|preset| preset.sidebar_side.as_deref() == Some("right"));
            let layout = peek_layout(
                width,
                height,
                sidebar_width_f32,
                sidebar_on_right,
                visual::active_topbar_height(),
            );
            let zoom = self
                .zoom_levels
                .get(&url_host(&peek.url))
                .copied()
                .unwrap_or_else(|| {
                    visual::zoom_level_for_factor(self.preferences.default_zoom_factor)
                });
            if let Err(error) = self.engine.ensure_browser(
                &peek.space_id,
                &peek.id,
                &peek.url,
                layout.viewport_w,
                layout.viewport_h,
                0xff000000 | theme.surface,
                zoom,
            ) {
                eprintln!("Lumen peek browser creation failed: {error}");
            }
            self.engine
                .resize_browser(&peek.id, layout.viewport_w, layout.viewport_h);
            let mut ids: Vec<String> = split_pair.as_ref().map_or_else(
                || vec![active_tab_id_string.clone()],
                |pair| vec![pair.first.clone(), pair.second.clone()],
            );
            ids.push(peek.id.clone());
            let refs = ids.iter().map(String::as_str).collect::<Vec<_>>();
            self.engine.set_visible_tabs(&refs);
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
                .unwrap_or((width as i32, height as i32));
            self.engine.set_screen_metrics(
                &peek.id,
                screen_width,
                screen_height,
                scale,
                f32::from(window_bounds.origin.x) as i32,
                f32::from(window_bounds.origin.y) as i32,
            );
        }

        #[cfg(not(test))]
        for retired in self.engine.take_retired_images() {
            let _ = _window.drop_image(retired);
        }

        // TEMPORARY probe: ANARC_LIGHT_UI=2 renders nothing at all.
        if std::env::var("ANARC_LIGHT_UI").as_deref() == Ok("2") {
            return div().size_full().into_any_element();
        }
        // TEMPORARY probe: ANARC_LIGHT_UI=1 renders only the browser surface.
        if std::env::var_os("ANARC_LIGHT_UI").is_some() {
            return div()
                .size_full()
                .child(
                    AnyView::from(self.web_viewport.clone())
                        .cached(StyleRefinement::default().size_full()),
                )
                .into_any_element();
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
                            ui::motion::space_switch().animation(),
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
            sidebar_content,
            outgoing_space_overlay,
        )
    }
}

#[cfg(not(test))]
impl Lumen {
    fn visible_browser_panes(&self) -> (smallvec::SmallVec<[String; 3]>, bool, Option<String>) {
        let visible_ids = crate::plugins::split_view_enabled()
            .then(|| self.current_split_pair())
            .flatten()
            .map_or_else(
                || smallvec::smallvec![self.active_tab_id().to_string()],
                |pair| smallvec::smallvec![pair.first, pair.second],
            );
        let split_active = visible_ids.len() > 1;
        let peek_id = self.peek.as_ref().map(|peek| peek.id.clone());
        let mut visible_ids = visible_ids;
        if let Some(peek_id) = peek_id.clone() {
            visible_ids.push(peek_id);
        }
        (visible_ids, split_active, peek_id)
    }

    fn update_browser_panes(
        &mut self,
        visible_ids: &[String],
        dirty_tabs: &[&str],
        split_active: bool,
        peek_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let mut retired_images = Vec::new();
        if dirty_tabs.contains(&visible_ids[0].as_str()) {
            let first_tab = visible_ids[0].clone();
            let (image, cursor) = self.engine.take_viewport_frame(&first_tab, &mut retired_images);
            let frame_matches_target = self
                .resize_drag
                .target_size(&first_tab)
                .is_some_and(|(w, h)| self.engine.frame_matches_size(&first_tab, w, h));
            let edge_color = self
                .resize_drag
                .edge_color_until_frame(&first_tab, frame_matches_target);
            self.web_viewport.update(cx, |viewport, cx| {
                if viewport.update_frame(
                    first_tab.clone(),
                    image,
                    cursor,
                    std::mem::take(&mut retired_images),
                    edge_color,
                ) {
                    cx.notify();
                }
            });
        }
        if let Some(second_tab) = visible_ids
            .get(1)
            .filter(|id| split_active && dirty_tabs.contains(&id.as_str()))
        {
            let (image, cursor) = self.engine.take_viewport_frame(second_tab, &mut retired_images);
            self.second_web_viewport.update(cx, |viewport, cx| {
                if viewport.set_content(
                    second_tab.clone(),
                    image,
                    cursor,
                    std::mem::take(&mut retired_images),
                ) {
                    cx.notify();
                }
            });
        }
        if let Some(peek_id) = peek_id.filter(|id| dirty_tabs.contains(id)) {
            let (image, cursor) = self.engine.take_viewport_frame(peek_id, &mut retired_images);
            self.peek_web_viewport.update(cx, |viewport, cx| {
                if viewport.set_content(
                    peek_id.to_owned(),
                    image,
                    cursor,
                    std::mem::take(&mut retired_images),
                ) {
                    cx.notify();
                }
            });
        }
    }
}
