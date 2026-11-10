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
        if !self.browsers.contains_key(id) && !self.pending_browsers.contains_key(id) {
            let initial_host = crate::shield::url_host(url);
            let initial_favicon = initial_host.as_deref().and_then(load_favicon_from_disk);
            let shared = Arc::new(Mutex::new(BrowserSharedState {
                width: width.max(1),
                height: height.max(1),
                image: None,
                edge_color: None,
                last_edge_color_instant: None,
                dirty: false,
                root_dirty: false,
                title: None,
                url: None,
                is_loading: true,
                can_go_back: false,
                can_go_forward: false,
                favicon: initial_favicon,
                favicon_host: initial_host.clone(),
                favicon_preferred: false,
                current_url: Some(url.to_string()),
                current_host: initial_host,
                retired: Vec::new(),
                screen_width: width.max(1),
                screen_height: height.max(1),
                screen_scale: 1.0,
                screen_origin_x: 0,
                screen_origin_y: 0,
                cursor: 0,
                target_zoom_level: Some(initial_zoom_level),
            }));
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
                self.windowless_frame_rate,
                initial_zoom_level,
            );
            let download_handler =
                LumenDownloadHandler::new(self.events.clone(), self.ui_update_wake.clone());
            let permission_handler = LumenPermissionHandler::new(self.events.clone());
            let load_handler = LumenLoadHandler::new(
                shared.clone(),
                self.https_state.clone(),
                self.events.clone(),
                id.to_owned(),
            );
            let context_menu_handler = LumenContextMenuHandler::new(self.events.clone());
            let request_handler = LumenRequestHandler::new(
                self.shield.clone(),
                id.to_owned(),
                self.https_state.clone(),
                self.events.clone(),
                self.panel_packages.clone(),
            );
            let mut client = LumenClient::new(
                render_handler,
                display_handler,
                life_span_handler,
                download_handler,
                permission_handler,
                load_handler,
                context_menu_handler,
                request_handler,
            );

            let window_info = WindowInfo::default().set_as_windowless(0);
            let start_url = CefString::from(url);

            if !self.contexts.contains_key(space_id) {
                let cache_dir = crate::paths::profile_dir(space_id);
                let _ = std::fs::create_dir_all(&cache_dir);

                let settings = RequestContextSettings {
                    cache_path: CefString::from(cache_dir.to_string_lossy().as_ref()),
                    persist_session_cookies: 1,
                    ..Default::default()
                };

                if let Some(ctx) = cef::request_context_create_context(Some(&settings), None) {
                    panel_scheme::register_panel_factory(&ctx, self.panel_packages.clone());
                    self.contexts.insert(space_id.to_string(), ctx);
                }
            }

            let request_ctx = self.contexts.get_mut(space_id);

            // CEF's OSR default is 30 fps, which makes scrolling and animations visibly choppy.
            let browser_settings = BrowserSettings {
                background_color,
                windowless_frame_rate: self.windowless_frame_rate,
                ..Default::default()
            };

            let (count, _) = &*self.close_state;
            *count.lock().unwrap() += 1;
            let started = cef::browser_host_create_browser(
                Some(&window_info),
                Some(&mut client),
                Some(&start_url),
                Some(&browser_settings),
                None,
                request_ctx,
            );
            if started == 0 {
                *count.lock().unwrap() -= 1;
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
            if let Some(host) = entry.browser.host() {
                host.was_hidden((!next.contains(id)) as i32);
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
            if let Some(host) = entry.browser.host() {
                host.was_hidden(1);
            }
        }
    }
    pub fn close_browser(&mut self, tab_id: &str) {
        self.close_browser_entry(tab_id);
        self.lifecycle.closed(tab_id);
        self.resize_state.remove(tab_id);
        self.visible.retain(|id| id != tab_id);
    }
    fn close_browser_entry(&mut self, tab_id: &str) {
        if self.pending_browsers.contains_key(tab_id) {
            self.canceled_browsers.insert(tab_id.to_owned());
        }
        if let Some(entry) = self.browsers.remove(tab_id) {
            if let Some(host) = entry.browser.host() {
                host.close_browser(1);
            }
            if let Ok(mut state) = entry.state.lock() {
                self.retired_images.append(&mut state.retired);
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

    /// Drains frames replaced since the last call so the caller can evict them from
    /// GPUI's sprite atlas (each OSR frame is a distinct RenderImage/texture).
    pub fn take_retired_images(&mut self) -> Vec<Arc<RenderImage>> {
        let mut retired = std::mem::take(&mut self.retired_images);
        retired.extend(
            self.browsers
                .values()
                .flat_map(|entry| {
                    entry
                        .state
                        .lock()
                        .map(|mut state| std::mem::take(&mut state.retired))
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>(),
        );
        retired
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
                    && let Some(img) = load_favicon_from_disk(host)
                {
                    state.favicon = Some(img.clone());
                    state.favicon_host = Some(host.clone());
                    state.favicon_preferred = false;
                    return Some(img);
                }
            }
        } else if let Some(ref host) = fallback_host {
            return load_favicon_from_disk(host);
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
        &self,
        tab_id: &str,
        x: i32,
        y: i32,
        button: MouseButtonType,
        mouse_up: bool,
        click_count: i32,
        modifiers: &gpui_kit::Modifiers,
    ) {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Some(host) = entry.browser.host()
        {
            let event = MouseEvent {
                x,
                y,
                modifiers: cef_mouse_modifiers(modifiers),
            };
            host.send_mouse_click_event(Some(&event), button, mouse_up as i32, click_count);
        }
    }
    pub fn send_mouse_move(
        &self,
        tab_id: &str,
        x: i32,
        y: i32,
        mouse_leave: bool,
        modifiers: &gpui_kit::Modifiers,
    ) {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Some(host) = entry.browser.host()
        {
            let event = MouseEvent {
                x,
                y,
                modifiers: cef_mouse_modifiers(modifiers),
            };
            host.send_mouse_move_event(Some(&event), mouse_leave as i32);
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
    ) {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Some(host) = entry.browser.host()
        {
            let event = MouseEvent {
                x,
                y,
                modifiers: cef_mouse_modifiers(modifiers),
            };
            host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
        }
    }
}
