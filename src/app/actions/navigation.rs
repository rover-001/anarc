use super::super::*;

impl Lumen {
    pub(crate) fn set_ad_blocker_enabled(&mut self, enabled: bool) {
        self.preferences.ad_blocker_enabled = enabled;
        self.engine.set_ad_blocker_enabled(enabled);
        self.plugins
            .service(crate::plugin::SETTINGS)
            .expect("core settings service is mounted")
            .save_preferences(&self.preferences);
    }

    pub(crate) fn toggle_shield_for_active_tab(&mut self) {
        let id = self.active_tab_id().to_owned();
        let url = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.url.clone());
        if let Some(url) = url {
            let enabled = self.engine.toggle_shield_for_tab(&id, &url);
            eprintln!(
                "Lumen shield {} for {}",
                if enabled { "enabled" } else { "disabled" },
                crate::shield::url_host(&url).as_deref().unwrap_or(&url)
            );
        }
    }
    pub(in crate::app) fn reload_active(&self, ignore_cache: bool) {
        self.engine.reload(self.focused_tab_id(), ignore_cache);
    }
    pub(in crate::app) fn find_in_page(&self, forward: bool, next: bool, cx: &Context<Self>) {
        let query = self.find_input.read(cx).value().to_string();
        if !query.is_empty() {
            self.engine
                .find(self.focused_tab_id(), &query, forward, next);
        }
    }
    pub(crate) fn change_zoom(&mut self, delta: f64) {
        let active_id = self.active_tab_id().to_string();
        self.change_zoom_for_tab(&active_id, delta);
    }

    /// URL a zoom target is showing. Covers sidebar tabs, split panes and the
    /// peek overlay (whose browser id is not part of any space).
    fn zoom_target_url(&self, target_tab_id: &str) -> Option<String> {
        self.spaces
            .iter()
            .flat_map(|space| space.tabs.iter())
            .find(|tab| tab.id == target_tab_id)
            .map(|tab| tab.url.clone())
            .or_else(|| {
                self.peek
                    .as_ref()
                    .filter(|peek| peek.id == target_tab_id)
                    .map(|peek| peek.url.clone())
            })
    }

    pub(crate) fn change_zoom_for_tab(&mut self, target_tab_id: &str, delta: f64) {
        let tab_url = self.zoom_target_url(target_tab_id).unwrap_or_default();
        let settings = self
            .plugins
            .service(crate::plugin::SETTINGS)
            .expect("core settings service is mounted");
        let host = settings.url_host(&tab_url);
        if host.is_empty() {
            self.preferences.default_zoom_factor = if delta == 0.0 {
                visual::DEFAULT_PAGE_ZOOM_FACTOR
            } else {
                let current = self.preferences.default_zoom_factor;
                let factor = (current + delta).clamp(1.2_f64.powf(-5.0), 1.2_f64.powf(5.0));
                1.2_f64.powf(visual::zoom_level_for_factor(factor))
            };
            settings.save_preferences(&self.preferences);
            return;
        }
        let default_level = visual::zoom_level_for_factor(self.preferences.default_zoom_factor);
        let level = if delta == 0.0 {
            default_level
        } else {
            let current = self
                .zoom_levels
                .get(&host)
                .copied()
                .unwrap_or(default_level);
            let factor =
                (1.2_f64.powf(current) + delta).clamp(1.2_f64.powf(-5.0), 1.2_f64.powf(5.0));
            visual::zoom_level_for_factor(factor)
        };
        self.zoom_levels.insert(host, level);
        settings.save_zooms(&self.zoom_levels);
        self.engine.set_zoom_level(target_tab_id, level);
    }
    pub(crate) fn set_search_engine(&mut self, engine: crate::settings::SearchEngine) {
        self.preferences.search_engine = engine;
        self.plugins
            .service(crate::plugin::SETTINGS)
            .expect("core settings service is mounted")
            .save_preferences(&self.preferences);
    }
    pub(crate) fn clear_history(&mut self) {
        self.recent_searches.clear();
        self.suggestions.clear();
        self.suggestion_snapshot.clear();
        self.show_suggestions = false;
        self.plugins
            .service(crate::plugin::HISTORY)
            .expect("core history service is mounted")
            .clear();
    }
    pub(in crate::app) fn navigate(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.close_peek();
        self.show_suggestions = false;
        self.selected_suggestion = None;
        self.address_editing = false;
        let input = self.address.read(cx).value().to_string();
        let input = self.plugin_text_event("address.submit", input);
        let search_engine = self.preferences.search_engine;
        let navigation = self
            .plugins
            .service(crate::plugin::NAVIGATION)
            .expect("core navigation service is mounted");
        let space = self.active_space_mut();
        let Some((id, url, http_first)) = navigation.submit_address(space, &input, search_engine)
        else {
            return;
        };
        let (url, http_first) = self.navigation_before(&id, url, http_first);
        self.app_page = None;
        self.remember_search(&input, &url);
        if http_first {
            self.engine.navigate_http_first(&id, &url);
        } else {
            self.engine.navigate(&id, &url);
        }
        if let Some(window) = window {
            self.focus_browser(window, cx);
        } else {
            self.browser_focused = true;
            let tab_id = self.focused_tab_id().to_string();
            self.engine.set_focus(&tab_id, true);
        }
        self.save_state();
    }
    pub(in crate::app) fn navigate_to(
        &mut self,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_peek();
        self.show_suggestions = false;
        self.selected_suggestion = None;
        self.address_editing = false;
        self.address.update(cx, |input, cx| {
            input.set_value(target, window, cx);
        });
        let target = self.plugin_text_event("address.submit", target.to_string());
        let search_engine = self.preferences.search_engine;
        let navigation = self
            .plugins
            .service(crate::plugin::NAVIGATION)
            .expect("core navigation service is mounted");
        let space = self.active_space_mut();
        let Some((id, url, http_first)) = navigation.submit_address(space, &target, search_engine)
        else {
            return;
        };
        let (url, http_first) = self.navigation_before(&id, url, http_first);
        self.app_page = None;
        self.remember_search(&target, &url);
        if http_first {
            self.engine.navigate_http_first(&id, &url);
        } else {
            self.engine.navigate(&id, &url);
        }
        self.focus_browser(window, cx);
        self.save_state();
    }

    /// Pastes the clipboard into the address field at the current selection.
    ///
    /// The read happens from a task instead of inside the key dispatch: on
    /// Wayland a clipboard roundtrip started while the platform queue is being
    /// dispatched comes back empty, which made Ctrl+V silently do nothing even
    /// though the select-all and copy beside it worked. The input's own
    /// `Paste` action takes the same in-dispatch read, so the address bar
    /// drives the paste itself.
    pub(in crate::app) fn paste_into_address(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let address = self.address.downgrade();
        cx.spawn_in(window, async move |this, cx| {
            let mut clipboard = read_address_clipboard(&this, cx);
            if clipboard.is_none() {
                // One more try after the dispatch has unwound.
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1))
                    .await;
                clipboard = read_address_clipboard(&this, cx);
            }
            let Some(text) = clipboard.as_deref().and_then(single_line_paste) else {
                return;
            };
            address
                .update_in(cx, |input, window, cx| {
                    if window.focused(cx) != Some(input.focus_handle(cx)) {
                        return;
                    }
                    input.replace(text, window, cx);
                })
                .ok();
        })
        .detach();
    }

    pub(super) fn plugin_text_event(&self, name: &str, value: String) -> String {
        self.plugins
            .emit(
                name,
                crate::plugin::EventMode::Waterfall,
                crate::plugin::EventPayload::Text(value.clone()),
            )
            .into_iter()
            .last()
            .and_then(|payload| match payload {
                crate::plugin::EventPayload::Text(value) => Some(value),
                _ => None,
            })
            .unwrap_or(value)
    }

    fn navigation_before(&mut self, tab_id: &str, url: String, http_first: bool) -> (String, bool) {
        let target = self.plugin_text_event("navigation.before", url.clone());
        if target != url {
            if let Some(tab) = self
                .spaces
                .iter_mut()
                .flat_map(|space| &mut space.tabs)
                .find(|tab| tab.id == tab_id)
            {
                tab.url = target.clone();
            }
            (target, false)
        } else {
            (url, http_first)
        }
    }
    pub(in crate::app) fn update_suggestions(&mut self, query: &str) {
        self.emit_plugin_text_with_mode(
            "address.suggest",
            crate::plugin::EventMode::Parallel,
            query.to_owned(),
        );
        let list = compute_suggestions(query);
        self.show_suggestions = !list.is_empty();
        self.suggestions = list;
        if !query.trim().is_empty() {
            self.suggestions.extend(
                self.plugins
                    .service(crate::plugin::HISTORY)
                    .expect("core history service is mounted")
                    .search(query, 4)
                    .into_iter()
                    .map(|entry| SuggestionItem {
                        icon_type: "globe".to_string(),
                        title: if entry.title.is_empty() {
                            entry.url.clone()
                        } else {
                            entry.title
                        },
                        target: entry.url,
                    }),
            );
        }
        self.selected_suggestion = None;
        if query.trim().is_empty() && history::plugin_enabled() {
            self.suggestions
                .extend(self.recent_searches.iter().map(|query| SuggestionItem {
                    icon_type: "search".to_string(),
                    title: query.clone(),
                    target: query.clone(),
                }));
            self.suggestions.truncate(6);
            self.show_suggestions = !self.suggestions.is_empty();
        }
    }
    pub(in crate::app) fn remember_search(&mut self, query: &str, url: &str) {
        if !history::plugin_enabled() {
            return;
        }
        let query = query.trim();
        if query.is_empty()
            || (!url.starts_with("https://www.google.com/search?q=")
                && !url.starts_with("https://google.com/search?q="))
        {
            return;
        }
        self.recent_searches
            .retain(|recent| !recent.eq_ignore_ascii_case(query));
        self.recent_searches.insert(0, query.to_string());
        self.recent_searches.truncate(10);
        #[cfg(not(test))]
        history::save_recent_searches(&self.recent_searches);
    }
    pub(in crate::app) fn move_suggestion_selection(&mut self, down: bool) {
        self.selected_suggestion =
            move_suggestion_selection(self.selected_suggestion, self.suggestions.len(), down);
    }
}

/// Clipboard text shaped for a single-line field: line breaks dropped, and
/// nothing at all when the clipboard only carried formatting or an image.
pub(in crate::app) fn single_line_paste(text: &str) -> Option<String> {
    let text = text.replace(['\n', '\r'], "");
    (!text.is_empty()).then_some(text)
}

fn read_address_clipboard(
    app: &gpui_kit::WeakEntity<Lumen>,
    cx: &mut gpui_kit::AsyncWindowContext,
) -> Option<String> {
    app.update(cx, |_app, cx| {
        cx.read_from_clipboard().and_then(|item| item.text())
    })
    .ok()
    .flatten()
    .or_else(crate::engine::clipboard::read_system_clipboard)
}
