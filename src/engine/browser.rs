use super::*;

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub fn ensure_browser(
        &mut self,
        space_id: &str,
        id: &str,
        url: &str,
        width: i32,
        height: i32,
        background_color: u32,
        initial_zoom_level: f64,
    ) -> Result<(), String> {
        if !self.initialized {
            return Ok(());
        }
        if let Some(entry) = self.browsers.get(id)
            && entry.space_id != space_id {
                if let Some(host) = entry.browser.host() {
                    host.close_browser(1);
                }
                self.browsers.remove(id);
            }
        if !self.browsers.contains_key(id) && !self.pending_browsers.contains_key(id) {
            let shared = Arc::new(Mutex::new(new_shared_state(
                space_id,
                width,
                height,
                url,
                initial_zoom_level,
            )));
            let render_handler =
                LumenRenderHandler::new(shared.clone(), self.ui_update_wake.clone(), id.to_owned());
            let display_handler = LumenDisplayHandler::new(
                shared.clone(),
                id.to_owned(),
                self.ui_update_wake.clone(),
                self.events.clone(),
            );
            let life_span_handler = LumenLifeSpanHandler::new(
                self.events.clone(),
                space_id.to_owned(),
                id.to_owned(),
                self.ui_update_wake.clone(),
                self.close_state.clone(),
                self.shutdown_requested.clone(),
                self.shield.clone(),
                self.https_state.clone(),
                self.panel_packages.clone(),
                self.windowless_frame_rate,
                initial_zoom_level,
                self.popup_serial.clone(),
            );
            let download_handler =
                LumenDownloadHandler::new(self.events.clone(), self.ui_update_wake.clone());
            let permission_handler = LumenPermissionHandler::new(self.events.clone());
            let load_handler = LumenLoadHandler::new(
                shared.clone(),
                self.https_state.clone(),
                self.events.clone(),
                self.shield.clone(),
                id.to_owned(),
            );
            let context_menu_handler =
                LumenContextMenuHandler::new(shared.clone(), self.events.clone());
            let request_handler = LumenRequestHandler::new(
                self.shield.clone(),
                id.to_owned(),
                self.https_state.clone(),
                self.events.clone(),
                self.panel_packages.clone(),
                self.ui_update_wake.clone(),
            );
            let dialog_handler = LumenDialogHandler::new(self.ui_update_wake.clone());
            let mut client = LumenClient::new(
                render_handler,
                display_handler,
                life_span_handler,
                download_handler,
                permission_handler,
                load_handler,
                context_menu_handler,
                request_handler,
                dialog_handler,
            );

            let mut window_info = WindowInfo::default().set_as_windowless(empty_window_handle());
            // Drive CEF's compositor clock from GPUI's per-frame pump so OSR
            // paints land in the same display frame as the wheel delta. A free
            // running CEF timer drifts against the window and produces
            // alternating fresh/stale frames during a fling. Uncapped mode
            // keeps CEF's own free-running clock for benchmark parity.
            let vsync_paced = !super::uncapped_fps_requested(
                std::env::var_os("ANARC_UNCAPPED_FPS").as_deref(),
                crate::settings::load_preferences().uncapped_fps,
            );
            window_info.external_begin_frame_enabled = i32::from(vsync_paced);
            window_info.runtime_style = cef::RuntimeStyle::ALLOY;
            let start_url = CefString::from(url);

            let effective_space = space_id;

            if !self.contexts.contains_key(effective_space) {
                let cache_dir = crate::paths::profile_dir(effective_space);
                let _ = std::fs::create_dir_all(&cache_dir);

                let settings = RequestContextSettings {
                    cache_path: CefString::from(cache_dir.to_string_lossy().as_ref()),
                    persist_session_cookies: 1,
                    ..Default::default()
                };

                if let Some(ctx) = cef::request_context_create_context(Some(&settings), None) {
                    panel_scheme::register_panel_factory(&ctx, self.panel_packages.clone());
                    self.contexts.insert(effective_space.to_string(), ctx);
                }
            }

            let request_ctx = self.contexts.get_mut(effective_space);

            // CEF's OSR default is 30 fps, which makes scrolling and animations visibly choppy.
            let browser_settings = BrowserSettings {
                background_color,
                windowless_frame_rate: self.windowless_frame_rate,
                javascript_access_clipboard: State::ENABLED,
                javascript_dom_paste: State::ENABLED,
                ..Default::default()
            };

            let (count, _) = &*self.close_state;
            *count.lock().map_err(|_| "CEF close counter unavailable")? += 1;
            let started = cef::browser_host_create_browser(
                Some(&window_info),
                Some(&mut client),
                Some(&start_url),
                Some(&browser_settings),
                None,
                request_ctx,
            );
            if started == 0 {
                *count.lock().map_err(|_| "CEF close counter unavailable")? -= 1;
                return Err("CEF failed to start asynchronous browser creation".into());
            }
            self.pending_browsers.insert(id.to_owned(), shared);
        } else {
            self.resize_browser(id, width, height);
        }
        Ok(())
    }

    /// Marks only `tab_id` as visible. Hidden OSR browsers stop producing frames,
    /// so background tabs no longer burn CPU or trigger UI re-renders.
    pub fn set_visible(&mut self, tab_id: &str) {
        self.set_visible_tabs(&[tab_id]);
    }
    pub fn set_visible_tabs(&mut self, tab_ids: &[&str]) {
        // Steady state is one visible tab on every paint wake; avoid rebuilding
        // the owned Vec before discovering nothing changed.
        if tab_ids.len() == self.visible.len()
            && tab_ids
                .iter()
                .zip(self.visible.iter())
                .all(|(id, current)| *id == current.as_str())
        {
            return;
        }
        let next = tab_ids
            .iter()
            .filter(|id| self.browsers.contains_key(**id))
            .map(|id| (*id).to_owned())
            .collect::<Vec<_>>();
        if self.visible == next {
            return;
        }
        let now = Instant::now();
        for previous in &self.visible {
            if !next.contains(previous) {
                self.lifecycle.deactivated(previous, now);
            }
        }
        for (id, entry) in &self.browsers {
            let visible = next.contains(id);
            if let Ok(mut state) = entry.state.lock() {
                let was_visible = state.visible;
                if !visible && was_visible {
                    state.pending_image = None;
                }
                state.visible = visible;
                if visible && !was_visible {
                    state.force_next_paint = true;
                    state.last_accepted_paint = None;
                }
            }
            if let Some(host) = entry.browser.host() {
                host.was_hidden((!visible) as i32);
            }
        }
        for tab_id in &next {
            if !self.visible.contains(tab_id) {
                self.lifecycle.activated(tab_id, now);
            }
        }
        self.visible = next;
    }
    pub fn hide_all(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        let previous = std::mem::take(&mut self.visible);
        let now = Instant::now();
        for id in previous {
            self.lifecycle.deactivated(&id, now);
        }
        for entry in self.browsers.values() {
            if let Ok(mut state) = entry.state.lock() {
                state.visible = false;
                state.pending_image = None;
            }
            if let Some(host) = entry.browser.host() {
                host.was_hidden(1);
            }
        }
    }
    pub fn close_browser(&mut self, tab_id: &str) {
        // Record the expected CEF echo before asking CEF to close: `do_close`
        // runs on another thread and may enqueue the request immediately.
        if let Ok(mut events) = self.events.lock() {
            *events
                .expected_close_requests
                .entry(tab_id.to_owned())
                .or_insert(0) += 1;
        }
        self.close_browser_entry(tab_id);
        self.lifecycle.closed(tab_id);
        self.resize_state.remove(tab_id);
        self.visible.retain(|id| id != tab_id);
    }
    /// Drops the bookkeeping for a browser CEF is already tearing down itself
    /// (a page called `window.close()`). Closing it again would double-close.
    pub fn forget_closed_browser(&mut self, tab_id: &str) {
        if self.pending_browsers.remove(tab_id).is_some() {
            // The browser CEF is still creating; close it once it arrives.
            self.canceled_browsers.insert(tab_id.to_owned());
        } else {
            self.browsers.remove(tab_id);
        }
        self.resize_state.remove(tab_id);
        self.pressed_mouse_buttons.remove(tab_id);
        self.visible.retain(|id| id != tab_id);
        self.lifecycle.closed(tab_id);
        if let Ok(mut dt) = self.devtools.lock() {
            dt.cleanup_tab(tab_id);
        }
    }
    fn close_browser_entry(&mut self, tab_id: &str) {
        if self.pending_browsers.contains_key(tab_id) {
            self.canceled_browsers.insert(tab_id.to_owned());
        }
        if let Ok(mut dt) = self.devtools.lock() {
            dt.cleanup_tab(tab_id);
        }
        if let Some(entry) = self.browsers.remove(tab_id) {
            if let Some(host) = entry.browser.host() {
                host.close_browser(1);
            }
            if let Ok(mut state) = entry.state.lock() {
                state.pending_image = None;
                if let Some(image) = state.image.take() {
                    self.retired_images.push(image);
                }
                if let Some(image) = state.favicon.take() {
                    self.retired_images.push(image);
                }
            }
        }
    }
    pub fn discard_inactive(&mut self) -> Vec<String> {
        let expired = self
            .lifecycle
            .discard_expired_many(Instant::now(), &self.visible);
        for tab_id in &expired {
            self.close_browser_entry(tab_id);
        }
        expired
    }
    pub fn tab_state(&self, tab_id: &str) -> Option<TabState> {
        self.lifecycle.state(tab_id)
    }
    pub(crate) fn agent_browser_pending(&self, tab_id: &str) -> bool {
        self.pending_browsers.contains_key(tab_id) || self.canceled_browsers.contains(tab_id)
    }
    pub(crate) fn browser_key_count(&self) -> usize {
        self.browsers.len()
    }
    pub(crate) fn log_agent_browser_miss(&self, tab_id: &str) {
        eprintln!("anarc-agent: browser miss key={tab_id:?} live={:?} pending={:?} canceled={:?}",
            self.browsers.keys(), self.pending_browsers.keys(), self.canceled_browsers);
    }
    pub fn has_browser(&self, tab_id: &str) -> bool {
        self.browsers.contains_key(tab_id)
    }
    pub fn set_screen_metrics(
        &self,
        tab_id: &str,
        screen_width: i32,
        screen_height: i32,
        scale: f32,
        origin_x: i32,
        origin_y: i32,
    ) {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Ok(mut state) = entry.state.lock()
        {
            let changed = state.screen_width != screen_width.max(1)
                || state.screen_height != screen_height.max(1)
                || (state.screen_scale - scale.max(1.0)).abs() > f32::EPSILON
                || state.screen_origin_x != origin_x
                || state.screen_origin_y != origin_y;
            state.screen_width = screen_width.max(1);
            state.screen_height = screen_height.max(1);
            state.screen_scale = scale.max(1.0);
            state.screen_origin_x = origin_x;
            state.screen_origin_y = origin_y;
            drop(state);
            if changed && let Some(host) = entry.browser.host() {
                host.notify_screen_info_changed();
            }
        }
    }
    pub fn get_image(&self, tab_id: &str) -> Option<Arc<RenderImage>> {
        self.browsers.get(tab_id).and_then(|entry| {
            entry
                .state
                .lock()
                .ok()
                .and_then(|state| state.image.clone())
        })
    }
    pub fn browser_size(&self, tab_id: &str) -> Option<(i32, i32)> {
        self.browsers
            .get(tab_id)
            .and_then(|entry| {
                entry
                    .state
                    .lock()
                    .ok()
                    .map(|state| (state.width, state.height))
            })
            .or_else(|| {
                self.pending_browsers
                    .get(tab_id)
                    .and_then(|state| state.lock().ok().map(|state| (state.width, state.height)))
            })
    }
    pub fn frame_edge_color(&self, tab_id: &str) -> Option<u32> {
        self.browsers
            .get(tab_id)
            .and_then(|entry| entry.state.lock().ok().and_then(|state| state.edge_color))
    }
    pub fn frame_matches_size(&self, tab_id: &str, width: i32, height: i32) -> bool {
        self.browsers
            .get(tab_id)
            .and_then(|entry| {
                entry.state.lock().ok().and_then(|state| {
                    state.image.as_ref().map(|image| {
                        let image_size = image.size(0);
                        let scale = state.screen_scale.max(f32::EPSILON);
                        (u32::from(image_size.width) as f32 / scale - width as f32).abs() < 1.0
                            && (u32::from(image_size.height) as f32 / scale - height as f32).abs()
                                < 1.0
                    })
                })
            })
            .unwrap_or(false)
    }
    pub fn get_cursor(&self, tab_id: &str) -> i32 {
        self.browsers
            .get(tab_id)
            .and_then(|entry| entry.state.lock().ok().map(|state| state.cursor))
            .unwrap_or(0)
    }
    /// Promote the latest pending frame and sample its cursor in one lock.
    pub fn take_viewport_frame(
        &self,
        tab_id: &str,
        retired_images: &mut Vec<Arc<RenderImage>>,
    ) -> (Option<Arc<RenderImage>>, i32) {
        self.browsers
            .get(tab_id)
            .and_then(|entry| entry.state.lock().ok())
            .map(|mut state| {
                state.dirty = false;
                if let Some(image) = state.pending_image.take() {
                    // The forced paint has now been published; allow the
                    // normal interval gate to resume.
                    state.force_next_paint = false;
                    if let Some(previous) = state.image.replace(image) {
                        retired_images.push(previous);
                    }
                }
                (state.image.clone(), state.cursor)
            })
            .unwrap_or_default()
    }

    pub fn get_selected_text(&self, tab_id: &str) -> Option<String> {
        self.browsers.get(tab_id).and_then(|entry| {
            entry
                .state
                .lock()
                .ok()
                .and_then(|state| state.last_selected_text.clone())
        })
    }

    /// Drains images from closed browsers for eviction from GPUI's atlas.
    /// Images replaced by new paints are handed directly to the viewport.
    pub fn take_retired_images(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.retired_images)
    }
    pub fn get_favicon_image(
        &self,
        tab_id: &str,
        fallback_url: Option<&str>,
    ) -> Option<Arc<RenderImage>> {
        let fallback_host = fallback_url.and_then(crate::shield::url_host);
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Ok(mut state) = entry.state.lock() {
                // The engine's navigation state is authoritative. The tab model's
                // URL can lag behind for background tabs, and trusting it would
                // resurrect an old favicon from the disk cache after a navigation.
                let target_host = state
                    .current_host
                    .clone()
                    .or_else(|| {
                        state
                            .current_url
                            .as_deref()
                            .and_then(crate::shield::url_host)
                    })
                    .or_else(|| state.url.as_deref().and_then(crate::shield::url_host))
                    .or(fallback_host.clone());

                let stale = match (state.favicon_host.as_deref(), target_host.as_deref()) {
                    (Some(current), Some(target)) => !same_favicon_host(current, target),
                    _ => false,
                };
                if stale && state.favicon.is_some() {
                    state.favicon = None;
                    state.favicon_host = None;
                    state.favicon_preferred = false;
                }

                if let Some(img) = state.favicon.clone() {
                    return Some(img);
                }

                if let Some(ref host) = target_host
                    && let Some(img) = render_favicon(host)
                {
                    state.favicon = Some(img.clone());
                    state.favicon_host = Some(host.clone());
                    state.favicon_preferred = false;
                    return Some(img);
                }
            }
        } else if let Some(ref host) = fallback_host {
            return render_favicon(host);
        }
        None
    }
    pub fn take_updates(&self, tab_id: &str) -> Option<(Option<String>, Option<String>)> {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Ok(mut state) = entry.state.lock()
            && (state.title.is_some() || state.url.is_some())
        {
            let title = state.title.take();
            let url = state.url.take();
            return Some((title, url));
        }
        None
    }
    pub fn resize_browser(&mut self, tab_id: &str, width: i32, height: i32) {
        self.resize_browser_inner(tab_id, width, height, false);
    }
    pub fn flush_resize_browser(&mut self, tab_id: &str, width: i32, height: i32) {
        self.resize_browser_inner(tab_id, width, height, true);
    }
    fn resize_browser_inner(&mut self, tab_id: &str, width: i32, height: i32, force: bool) {
        let w = width.max(1);
        let h = height.max(1);
        let now = Instant::now();
        let should_resize = resize_should_send(self.resize_state.get(tab_id), (w, h), now, force);
        if should_resize {
            resize_debug(format_args!("resize tab={tab_id} requested={w}x{h}"));
            self.resize_state.insert(tab_id.to_owned(), (now, w, h));
            if let Some(entry) = self.browsers.get(tab_id) {
                let mut state = entry.state.lock().unwrap();
                state.width = w;
                state.height = h;
                drop(state);
                if let Some(host) = entry.browser.host() {
                    host.was_resized();
                }
            } else if let Some(state) = self.pending_browsers.get(tab_id) {
                let mut state = state.lock().unwrap();
                state.width = w;
                state.height = h;
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn send_mouse_click(
        &mut self,
        tab_id: &str,
        x: i32,
        y: i32,
        button: MouseButtonType,
        mouse_up: bool,
        click_count: i32,
        modifiers: &gpui_kit::Modifiers,
    ) {
        let Some(entry) = self.browsers.get(tab_id) else {
            return;
        };
        let Some(host) = entry.browser.host() else {
            return;
        };
        let button_flag = match button {
            MouseButtonType::LEFT => 1 << 4,
            MouseButtonType::MIDDLE => 1 << 5,
            MouseButtonType::RIGHT => 1 << 6,
            _ => 0,
        };
        if button_flag != 0 {
            let current = self.pressed_mouse_buttons.get(tab_id).copied().unwrap_or(0);
            let updated = update_pressed_button_flags(current, button_flag, mouse_up);
            if updated == 0 {
                self.pressed_mouse_buttons.remove(tab_id);
            } else {
                self.pressed_mouse_buttons
                    .insert(tab_id.to_owned(), updated);
            }
        }
        let pressed_button = if button == MouseButtonType::LEFT {
            Some(gpui_kit::MouseButton::Left)
        } else if button == MouseButtonType::MIDDLE {
            Some(gpui_kit::MouseButton::Middle)
        } else if button == MouseButtonType::RIGHT {
            Some(gpui_kit::MouseButton::Right)
        } else {
            None
        };
        let event = MouseEvent {
            x,
            y,
            // CEF derives the button flag for click events itself, but keeping
            // press/release symmetrical here costs nothing and stays correct if
            // that ever changes.
            modifiers: cef_mouse_flags(modifiers, if mouse_up { None } else { pressed_button }),
        };
        host.send_mouse_click_event(Some(&event), button, mouse_up as i32, click_count);
    }
    pub fn send_mouse_move(
        &self,
        tab_id: &str,
        x: i32,
        y: i32,
        mouse_leave: bool,
        modifiers: &gpui_kit::Modifiers,
        pressed_button: Option<gpui_kit::MouseButton>,
    ) {
        let held_flags = self.pressed_mouse_buttons.get(tab_id).copied().unwrap_or(0);
        let event_flags = cef_mouse_flags(modifiers, pressed_button) | held_flags;
        if let Some(entry) = self.browsers.get(tab_id)
            && let Some(host) = entry.browser.host()
        {
            let event = MouseEvent {
                x,
                y,
                modifiers: event_flags,
            };
            host.send_mouse_move_event(Some(&event), mouse_leave as i32);
        }
    }
    /// Drive CEF's compositor clock from the GPUI frame pump. Hidden tabs are
    /// left alone: CEF does not paint them and an unaligned BeginFrame would
    /// only queue work.
    pub fn send_external_begin_frame(&self, tab_id: &str) {
        if let Some(entry) = self.browsers.get(tab_id)
            && entry.state.lock().is_ok_and(|state| state.visible)
            && let Some(host) = entry.browser.host()
        {
            host.send_external_begin_frame();
        }
    }

    pub fn send_mouse_wheel(
        &self,
        tab_id: &str,
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        modifiers: &gpui_kit::Modifiers,
        precision: bool,
    ) {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Some(host) = entry.browser.host()
        {
            // A wheel event means a fresh frame is imminent. Never let the
            // paint-interval throttle discard it: that would leave scroll
            // input with no visible frame at the end of a gesture.
            if let Ok(mut state) = entry.state.lock() {
                state.force_next_paint = true;
            }
            let mut flags = cef_mouse_modifiers(modifiers);
            if precision {
                #[allow(clippy::unnecessary_cast)]
                {
                    flags |=
                        cef::sys::cef_event_flags_t::EVENTFLAG_PRECISION_SCROLLING_DELTA.0 as u32;
                }
            }
            let event = MouseEvent {
                x,
                y,
                modifiers: flags,
            };
            host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
        }
    }

    pub fn send_drag_drop_files(
        &self,
        tab_id: &str,
        file_paths: &[std::path::PathBuf],
        x: i32,
        y: i32,
    ) {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Some(host) = entry.browser.host()
            && let Some(mut drag_data) = cef::drag_data_create()
        {
            for path in file_paths {
                if let Some(path_str) = path.to_str() {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    drag_data.add_file(
                        Some(&CefString::from(path_str)),
                        Some(&CefString::from(file_name)),
                    );
                }
            }
            let event = MouseEvent { x, y, modifiers: 0 };
            let ops = DragOperationsMask::from(
                cef::sys::cef_drag_operations_mask_t::DRAG_OPERATION_EVERY,
            );
            host.drag_target_drag_enter(Some(&mut drag_data), Some(&event), ops);
            host.drag_target_drag_over(Some(&event), ops);
            host.drag_target_drop(Some(&event));
            host.drag_source_ended_at(x, y, ops);
            let _ = self.ui_update_wake.try_send(());
        }
    }
}
