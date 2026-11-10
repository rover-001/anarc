use super::*;

impl Engine {
    pub fn take_plugin_events(&self) -> Vec<(String, String)> {
        self.events
            .lock()
            .map(|mut events| std::mem::take(&mut events.plugin_events))
            .unwrap_or_default()
    }

    pub fn take_popups(&self) -> Vec<(String, String)> {
        self.events
            .lock()
            .map(|mut events| std::mem::take(&mut events.popups))
            .unwrap_or_default()
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
            events.downloads.push(record);
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
    pub fn set_ad_blocker_enabled(&self, enabled: bool) {
        self.shield.set_enabled(enabled);
    }
    pub fn execute_script(&self, id: &str, script: &str) {
        if let Some(frame) = self
            .browsers
            .get(id)
            .and_then(|entry| entry.browser.main_frame())
        {
            frame.execute_java_script(
                Some(&CefString::from(script)),
                Some(&CefString::from("lumen://reader-mode")),
                1,
            );
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
        if let Some(host) = self.browsers.get(id).and_then(|entry| entry.browser.host()) {
            host.set_focus(focused as i32);
        }
    }
    pub fn send_key_down(&self, id: &str, event: &KeyDownEvent) {
        let Some(host) = self.browsers.get(id).and_then(|entry| entry.browser.host()) else {
            return;
        };
        let key = &event.keystroke.key;
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
            true,
        );
        host.send_key_event(Some(&base));
        if character != 0 {
            let text = cef_key_event(
                key,
                &event.keystroke.modifiers,
                KeyEventType::CHAR,
                character,
                true,
            );
            host.send_key_event(Some(&text));
        }
    }
    pub fn send_key_up(&self, id: &str, event: &KeyUpEvent) {
        if let Some(host) = self.browsers.get(id).and_then(|entry| entry.browser.host()) {
            let key = &event.keystroke.key;
            let key_event = cef_key_event(
                key,
                &event.keystroke.modifiers,
                KeyEventType::KEYUP,
                0,
                true,
            );
            host.send_key_event(Some(&key_event));
        }
    }
}
