use super::*;

fn is_space_key(key: &str) -> bool {
    key == " " || key.eq_ignore_ascii_case("space")
}

impl Engine {
    /// Returns true if any CEF-generated events are pending (plugin events,
    /// popups, close requests, fullscreen changes, downloads, or created browsers).
    /// Used by the render loop to skip heavy event processing when idle.
    pub fn has_pending_events(&self) -> bool {
        !self.agent_receiver.is_empty()
            || self.events.lock().is_ok_and(|events| {
                !events.plugin_events.is_empty()
                    || !events.popups.is_empty()
                    || !events.close_requests.is_empty()
                    || events.live_share_space_request.is_some()
                    || events.live_share_switch_space.is_some()
                    || !events.fullscreen.is_empty()
                    || !events.created.is_empty()
                    || events.downloads_dirty
            })
    }

    pub(crate) fn take_downloads_dirty(&self) -> bool {
        self.events
            .lock()
            .map(|mut events| std::mem::take(&mut events.downloads_dirty))
            .unwrap_or(false)
    }

    /// Start a download from a user action (e.g. "Save link as..." in the web
    /// context menu). CEF routes the request through `DownloadHandler`, which
    /// records it for the Downloads page and progress toast.
    pub fn start_download(&self, tab_id: &str, url: &str) -> bool {
        let Some(browser) = self.browsers.get(tab_id).map(|entry| entry.browser.clone()) else {
            return false;
        };
        let Some(host) = browser.host() else {
            return false;
        };
        host.start_download(Some(&CefString::from(url)));
        true
    }

    /// Link/image URL captured by CEF's context menu handler for this tab.
    pub(crate) fn context_menu_download_url(&self, tab_id: &str) -> Option<String> {
        let shared = self.browsers.get(tab_id).map(|entry| entry.state.clone())?;
        let mut shared = shared.lock().ok()?;
        shared.last_context_download_url.take()
    }

    pub fn take_plugin_events(&self) -> Vec<(String, String)> {
        self.events
            .lock()
            .map(|mut events| std::mem::take(&mut events.plugin_events))
            .unwrap_or_default()
    }

    pub fn take_popups(&self) -> Vec<PopupRequest> {
        self.events
            .lock()
            .map(|mut events| std::mem::take(&mut events.popups))
            .unwrap_or_default()
    }
    pub fn take_close_requests(&self) -> Vec<String> {
        self.events
            .lock()
            .map(|mut events| {
                let requests = std::mem::take(&mut events.close_requests);
                let expected = &mut events.expected_close_requests;
                requests
                    .into_iter()
                    .filter(|tab_id| match expected.get_mut(tab_id) {
                        Some(count) if *count > 0 => {
                            *count -= 1;
                            false
                        }
                        _ => true,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(crate) fn take_live_share_space_request(&self) -> Option<LiveShareSpaceRequest> {
        self.events.lock().ok()?.live_share_space_request.take()
    }
    pub(crate) fn take_live_share_switch_space(&self) -> Option<String> {
        self.events.lock().ok()?.live_share_switch_space.take()
    }
    pub fn take_fullscreen_changes(&self) -> Vec<(String, bool)> {
        self.events
            .lock()
            .map(|mut events| std::mem::take(&mut events.fullscreen))
            .unwrap_or_default()
    }
    pub fn downloads(&self) -> Vec<DownloadRecord> {
        self.events
            .lock()
            .map(|events| events.downloads.clone())
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub(crate) fn push_download_test(&self, record: DownloadRecord) {
        if let Ok(mut events) = self.events.lock() {
            if events.downloads.len() >= crate::persist::MAX_DOWNLOAD_RECORDS {
                events.downloads.remove(0);
            }
            events.downloads.push(record);
            events.downloads_dirty = true;
        }
    }
    pub fn go_back(&self, tab_id: &str) {
        if let Some(browser) = self.browsers.get(tab_id).map(|entry| entry.browser.clone())
            && browser.can_go_back() != 0
        {
            browser.go_back();
        }
    }
    pub fn go_forward(&self, tab_id: &str) {
        if let Some(browser) = self.browsers.get(tab_id).map(|entry| entry.browser.clone())
            && browser.can_go_forward() != 0
        {
            browser.go_forward();
        }
    }
    pub fn stop_load(&self, tab_id: &str) {
        if let Some(browser) = self.browsers.get(tab_id) {
            browser.browser.stop_load();
        }
    }
    pub fn is_loading(&self, tab_id: &str) -> bool {
        self.browsers.get(tab_id).is_some_and(|entry| {
            entry
                .state
                .lock()
                .map(|state| state.is_loading)
                .unwrap_or(false)
        })
    }
    pub fn current_url(&self, id: &str) -> Option<String> {
        let entry = self.browsers.get(id)?;
        let state = entry.state.lock().ok()?;
        state.current_url.clone()
    }
    pub fn can_go_back(&self, tab_id: &str) -> bool {
        self.browsers.get(tab_id).is_some_and(|entry| {
            entry
                .state
                .lock()
                .map(|state| state.can_go_back)
                .unwrap_or(false)
        })
    }
    pub fn can_go_forward(&self, tab_id: &str) -> bool {
        self.browsers.get(tab_id).is_some_and(|entry| {
            entry
                .state
                .lock()
                .map(|state| state.can_go_forward)
                .unwrap_or(false)
        })
    }
    pub fn find(&self, tab_id: &str, query: &str, forward: bool, next: bool) {
        if let Some(host) = self
            .browsers
            .get(tab_id)
            .and_then(|entry| entry.browser.host())
        {
            host.find(
                Some(&CefString::from(query)),
                forward as i32,
                0,
                next as i32,
            );
        }
    }
    pub fn stop_finding(&self, tab_id: &str) {
        if let Some(host) = self
            .browsers
            .get(tab_id)
            .and_then(|entry| entry.browser.host())
        {
            host.stop_finding(1);
        }
    }
    pub fn set_zoom_level(&self, tab_id: &str, level: f64) {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Ok(mut state) = entry.state.lock() {
                state.target_zoom_level = Some(level);
            }
            if let Some(host) = entry.browser.host() {
                host.set_zoom_level(level.clamp(-5.0, 5.0));
                host.invalidate(cef::PaintElementType::VIEW);
            }
        } else if let Some(state) = self.pending_browsers.get(tab_id)
            && let Ok(mut state) = state.lock()
        {
            state.target_zoom_level = Some(level);
        }
    }
    pub fn navigate(&mut self, id: &str, url: &str) {
        if let Some(entry) = self.browsers.get(id) {
            let new_host = crate::shield::url_host(url);
            if let Ok(mut state) = entry.state.lock() {
                state.current_url = Some(url.to_string());
                let host_changed = match (&state.current_host, &new_host) {
                    (Some(curr), Some(new_h)) => !same_favicon_host(curr, new_h),
                    (None, Some(_)) => true,
                    (Some(_), None) => true,
                    (None, None) => false,
                };
                if host_changed {
                    state.current_host = new_host.clone();
                    state.favicon = None;
                    state.favicon_host = None;
                    state.favicon_preferred = false;
                    if let Some(ref host) = new_host
                        && let Some(cached) = load_favicon_from_disk(host)
                    {
                        state.favicon = Some(cached);
                        state.favicon_host = Some(host.clone());
                    }
                    state.root_dirty = true;
                }
            }
            if let Some(frame) = entry.browser.main_frame() {
                frame.load_url(Some(&CefString::from(url)));
            }
            let _ = self.ui_update_wake.try_send(());
        } else if let Some(state) = self.pending_browsers.get(id) {
            if let Ok(mut state) = state.lock() {
                state.current_url = Some(url.to_string());
                state.current_host = crate::shield::url_host(url);
                state.root_dirty = true;
            }
            let _ = self.ui_update_wake.try_send(());
        }
    }
    pub fn navigate_http_first(&mut self, id: &str, https_url: &str) {
        if let Ok(mut state) = self.https_state.lock() {
            remember_https_upgrade(&mut state, https_url);
        }
        self.navigate(id, https_url);
    }
    pub fn toggle_shield_for_tab(&mut self, id: &str, url: &str) -> bool {
        let Some(host) = crate::shield::url_host(url) else {
            return false;
        };
        let allowed = self.shield.toggle_site(&host);
        self.reload(id, false);
        !allowed
    }
    pub fn shield_enabled_for_url(&self, url: &str) -> bool {
        crate::shield::url_host(url).is_some_and(|host| self.shield.site_protection_enabled(&host))
    }
    pub fn blocked_request_counts(&self) -> HashMap<String, u64> {
        self.shield.blocked_counts()
    }
    pub fn ad_blocker_enabled(&self) -> bool {
        self.shield.enabled()
    }
    /// CSS/scriptlet injection that hides inline ad containers for `url`.
    pub fn shield_cosmetic_script(&self, url: &str) -> Option<String> {
        self.shield.cosmetic_script(url)
    }
    pub fn set_ad_blocker_enabled(&self, enabled: bool) {
        self.shield.set_enabled(enabled);
    }
    pub fn execute_script(&self, id: &str, script: &str) {
        if let Some(frame) = self.browsers.get(id).and_then(|entry| {
            entry
                .browser
                .focused_frame()
                .or_else(|| entry.browser.main_frame())
        }) {
            frame.execute_java_script(Some(&CefString::from(script)), None, 1);
        }
    }
    pub fn reload(&self, id: &str, ignore_cache: bool) {
        if let Some(entry) = self.browsers.get(id) {
            if ignore_cache {
                entry.browser.reload_ignore_cache();
            } else {
                entry.browser.reload();
            }
        }
    }
    pub fn set_focus(&self, id: &str, focused: bool) {
        if let Some(entry) = self.browsers.get(id) {
            if !focused
                && let Ok(mut state) = entry.state.lock()
            {
                state.space_held = false;
            }
            if let Some(host) = entry.browser.host() {
                host.set_focus(focused as i32);
            }
        }
    }
    pub fn copy(&self, id: &str) {
        if let Some(frame) = self.browsers.get(id).and_then(|entry| {
            entry
                .browser
                .focused_frame()
                .or_else(|| entry.browser.main_frame())
        }) {
            frame.copy();
        }
    }
    pub fn paste(&self, id: &str) {
        if let Some(frame) = self.browsers.get(id).and_then(|entry| {
            entry
                .browser
                .focused_frame()
                .or_else(|| entry.browser.main_frame())
        }) {
            frame.paste();
        }
    }
    pub fn cut(&self, id: &str) {
        if let Some(frame) = self.browsers.get(id).and_then(|entry| {
            entry
                .browser
                .focused_frame()
                .or_else(|| entry.browser.main_frame())
        }) {
            frame.cut();
        }
    }
    pub fn select_all(&self, id: &str) {
        if let Some(frame) = self.browsers.get(id).and_then(|entry| {
            entry
                .browser
                .focused_frame()
                .or_else(|| entry.browser.main_frame())
        }) {
            frame.select_all();
        }
    }
    pub fn send_key_down(&self, id: &str, event: &KeyDownEvent) {
        let Some(entry) = self.browsers.get(id) else {
            return;
        };
        let Some(host) = entry.browser.host() else {
            return;
        };
        let is_space = is_space_key(&event.keystroke.key);
        let focus_on_editable_field = match entry.state.lock() {
            Ok(mut state) => {
                // Passing held Space keydowns through makes video sites toggle
                // play/pause instead of staying in hold-for-2x mode. Drop both
                // OS-flagged repeats (`event.is_held`) and any duplicate
                // keydowns while Space is already held (e.g. X11 where
                // `is_held` is always false).
                if is_space && !state.focus_on_editable_field {
                    if event.is_held || state.space_held {
                        return;
                    }
                    state.space_held = true;
                }
                state.focus_on_editable_field
            }
            Err(_) => false,
        };
        let key = &event.keystroke.key;
        let is_control_or_platform =
            event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        let character = event
            .keystroke
            .key_char
            .as_deref()
            .and_then(|value| value.encode_utf16().next())
            .or_else(|| {
                let mut chars = key.chars();
                let ch = chars.next()?;
                if chars.next().is_none() && !ch.is_control() {
                    let mut buf = [0u16; 2];
                    ch.encode_utf16(&mut buf);
                    Some(buf[0])
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let base = cef_key_event(
            key,
            &event.keystroke.modifiers,
            KeyEventType::RAWKEYDOWN,
            character,
            focus_on_editable_field,
            event.is_held,
        );
        host.send_key_event(Some(&base));
        if character != 0 && !is_control_or_platform {
            let text = cef_key_event(
                key,
                &event.keystroke.modifiers,
                KeyEventType::CHAR,
                character,
                focus_on_editable_field,
                event.is_held,
            );
            host.send_key_event(Some(&text));
        }
    }
    pub fn send_key_up(&self, id: &str, event: &KeyUpEvent) {
        let Some(entry) = self.browsers.get(id) else {
            return;
        };
        let Some(host) = entry.browser.host() else {
            return;
        };
        let is_space = is_space_key(&event.keystroke.key);
        let focus_on_editable_field = match entry.state.lock() {
            Ok(mut state) => {
                if is_space {
                    state.space_held = false;
                }
                state.focus_on_editable_field
            }
            Err(_) => false,
        };
        let key = &event.keystroke.key;
        let key_event = cef_key_event(
            key,
            &event.keystroke.modifiers,
            KeyEventType::KEYUP,
            0,
            focus_on_editable_field,
            false,
        );
        host.send_key_event(Some(&key_event));
    }

    pub fn evaluate(
        &self,
        tab_id: &str,
        script: &str,
    ) -> async_channel::Receiver<Result<String, String>> {
        let (tx, rx) = async_channel::bounded(1);
        let Some(entry) = self.browsers.get(tab_id) else {
            let _ = tx.try_send(Err(format!("tab '{tab_id}' not found")));
            return rx;
        };
        let Some(host) = entry.browser.host() else {
            let _ = tx.try_send(Err(format!("browser host for '{tab_id}' not available")));
            return rx;
        };
        let Ok(mut mgr) = self.devtools.lock() else {
            let _ = tx.try_send(Err("DevTools lock error".into()));
            return rx;
        };
        mgr.evaluate(tab_id, &host, script, devtools::DEFAULT_DEVTOOLS_TIMEOUT)
    }

    pub fn devtools(
        &self,
        tab_id: &str,
        method: &str,
        params: Option<&str>,
    ) -> async_channel::Receiver<Result<String, String>> {
        let (tx, rx) = async_channel::bounded(1);
        let Some(entry) = self.browsers.get(tab_id) else {
            let _ = tx.try_send(Err(format!("tab '{tab_id}' not found")));
            return rx;
        };
        let Some(host) = entry.browser.host() else {
            let _ = tx.try_send(Err(format!("browser host for '{tab_id}' not available")));
            return rx;
        };
        let Ok(mut mgr) = self.devtools.lock() else {
            let _ = tx.try_send(Err("DevTools lock error".into()));
            return rx;
        };
        mgr.execute_method(
            tab_id,
            &host,
            method,
            params,
            devtools::DEFAULT_DEVTOOLS_TIMEOUT,
        )
    }

    pub fn input_click(
        &mut self,
        tab_id: &str,
        x: i32,
        y: i32,
        button: Option<cef::MouseButtonType>,
    ) {
        let btn = button.unwrap_or(cef::MouseButtonType::LEFT);
        let mods = gpui_kit::Modifiers::default();
        self.send_mouse_move(tab_id, x, y, false, &mods, None);
        self.send_mouse_click(tab_id, x, y, btn, false, 1, &mods);
        self.send_mouse_click(tab_id, x, y, btn, true, 1, &mods);
    }

    pub fn input_move(&self, tab_id: &str, x: i32, y: i32) {
        let mods = gpui_kit::Modifiers::default();
        self.send_mouse_move(tab_id, x, y, false, &mods, None);
    }

    pub fn input_type(
        &self,
        tab_id: &str,
        text: &str,
    ) -> async_channel::Receiver<Result<String, String>> {
        let params = serde_json::json!({ "text": text });
        let p_str = serde_json::to_string(&params).unwrap_or_default();
        self.devtools(tab_id, "Input.insertText", Some(&p_str))
    }

    pub fn input_key(
        &self,
        tab_id: &str,
        key: &str,
        modifiers: &gpui_kit::Modifiers,
        key_down: bool,
    ) {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Some(host) = entry.browser.host()
        {
            let focus_on_editable_field = entry
                .state
                .lock()
                .map(|s| s.focus_on_editable_field)
                .unwrap_or(false);
            let char_val = if key.len() == 1 {
                key.chars().next().map(|c| c as u16).unwrap_or(0)
            } else if key.eq_ignore_ascii_case("enter") || key.eq_ignore_ascii_case("return") {
                13
            } else {
                0
            };
            if key_down {
                let down_ev = cef_key_event(
                    key,
                    modifiers,
                    KeyEventType::RAWKEYDOWN,
                    char_val,
                    focus_on_editable_field,
                    false,
                );
                host.send_key_event(Some(&down_ev));
                // A CHAR event for Enter inserts a newline in contenteditable
                // editors even after their keydown handler sent the message.
                if char_val != 0
                    && !key.eq_ignore_ascii_case("enter")
                    && !key.eq_ignore_ascii_case("return")
                    && !modifiers.control
                    && !modifiers.platform
                {
                    let char_ev = cef_key_event(
                        key,
                        modifiers,
                        KeyEventType::CHAR,
                        char_val,
                        focus_on_editable_field,
                        false,
                    );
                    host.send_key_event(Some(&char_ev));
                }
            } else {
                let up_ev = cef_key_event(
                    key,
                    modifiers,
                    KeyEventType::KEYUP,
                    0,
                    focus_on_editable_field,
                    false,
                );
                host.send_key_event(Some(&up_ev));
            }
        }
    }

    pub fn input_scroll(&self, tab_id: &str, x: i32, y: i32, delta_x: f32, delta_y: f32) {
        let mods = gpui_kit::Modifiers::default();
        self.send_mouse_wheel(tab_id, x, y, delta_x as i32, delta_y as i32, &mods, false);
    }

    pub fn take_agent_requests(
        &self,
    ) -> Vec<(
        crate::agent::protocol::AgentRequest,
        async_channel::Sender<crate::agent::protocol::AgentResponse>,
    )> {
        let mut requests = Vec::new();
        while let Ok(item) = self.agent_receiver.try_recv() {
            requests.push(item);
        }
        requests
    }

    pub fn agent_state(&self) -> Arc<Mutex<crate::agent::plugin::AgentState>> {
        self.agent_state.clone()
    }
}
