use super::super::*;

/// Fingerprint of the last cookie snapshot pushed to the daemon. Avoids a
/// disk write + peer broadcast every 3 s when nothing signed in changed.
static SHARED_COOKIE_FINGERPRINT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

impl Anarc {
    pub(crate) fn spaces(&self) -> &[Space] {
        &self.spaces
    }

    pub(crate) fn theme_picker_space(&self) -> &Space {
        if self.theme_editor.as_ref().is_some_and(|e| e.target_modal)
            && let Some(modal) = &self.space_modal
        {
            return &modal.theme_draft;
        }
        self.active_space()
    }

    pub(crate) fn theme_picker_space_mut(&mut self) -> &mut Space {
        if self.theme_editor.as_ref().is_some_and(|e| e.target_modal) && self.space_modal.is_some()
        {
            #[allow(clippy::unnecessary_unwrap)]
            return &mut self.space_modal.as_mut().unwrap().theme_draft;
        }
        self.active_space_mut()
    }

    pub(crate) fn open_theme_editor(
        &mut self,
        anchor: gpui_kit::Point<gpui_kit::Pixels>,
        target_modal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let viewport = window.viewport_size();
        let margin = 8.0 * UI_SCALE;
        self.sidebar_context_menu = None;
        self.theme_editor = Some(ThemeEditor {
            x: (f32::from(anchor.x) + margin).clamp(
                margin,
                (f32::from(viewport.width) - 304.0 * UI_SCALE - margin).max(margin),
            ),
            y: f32::from(anchor.y).clamp(
                margin,
                (f32::from(viewport.height) - 420.0 * UI_SCALE - margin).max(margin),
            ),
            selected_color: 0,
            dragging: false,
            control: 0,
            target_modal,
            grab_offset: (0.0, 0.0),
            pointer_position: None,
            panel_bounds: Default::default(),
            bounds: Default::default(),
        });
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.pointer_position = None;
        }
        let space = self.theme_picker_space_mut();
        if space.theme_colors.is_empty() {
            space.theme_colors.push(space.color);
        }
        cx.notify();
    }

    pub(in crate::app) fn add_theme_color(&mut self) {
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.pointer_position = None;
        }
        let space = self.theme_picker_space_mut();
        if space.theme_colors.is_empty() {
            space.theme_colors.push(space.color);
        }
        if space.theme_colors.len() < 3 {
            let next = match space.theme_colors.len() {
                1 => 0x76d7b2,
                _ => 0xf6ad55,
            };
            space.theme_colors.push(next);
        }
        self.save_state();
    }

    pub(in crate::app) fn remove_theme_color(&mut self, index: usize) {
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.pointer_position = None;
        }
        let space = self.theme_picker_space_mut();
        if space.theme_colors.is_empty() {
            return;
        }
        if space.theme_colors.len() > 1 && index < space.theme_colors.len() {
            space.theme_colors.remove(index);
        }
        if let Some(first) = space.theme_colors.first() {
            space.color = *first;
        }
        self.save_state();
    }

    pub(in crate::app) fn update_theme_color_from_pointer(&mut self, x: f32, y: f32) {
        let Some(editor) = self.theme_editor.as_ref() else {
            return;
        };
        if !editor.dragging {
            return;
        }
        let bounds = editor.bounds.get()[editor.control];
        let width = f32::from(bounds.size.width).max(1.0);
        let height = f32::from(bounds.size.height).max(1.0);
        let inset = 14.0 * UI_SCALE;
        let horizontal = ((x - editor.grab_offset.0 - f32::from(bounds.origin.x) - inset)
            / (width - 2.0 * inset).max(1.0))
        .clamp(0.0, 1.0);
        let vertical = ((y - editor.grab_offset.1 - f32::from(bounds.origin.y) - inset)
            / (height - 2.0 * inset).max(1.0))
        .clamp(0.0, 1.0);
        if editor.control != 0 {
            let control = editor.control;
            let value =
                slider_drag_value(x - editor.grab_offset.0, f32::from(bounds.origin.x), width);
            if control == 1 {
                self.theme_picker_space_mut().theme_intensity = value * 0.65;
            } else {
                self.theme_picker_space_mut().theme_grain = value;
            }
            self.theme_transition = None;
            return;
        }
        let color = hsv_to_rgb(horizontal * 359.9, (1.0 - vertical) * 0.8, 1.0);
        let index = editor.selected_color;
        self.theme_editor.as_mut().unwrap().pointer_position = Some((index, horizontal, vertical));
        let space = self.theme_picker_space_mut();
        if space.theme_colors.is_empty() {
            space.theme_colors.push(space.color);
        }
        if let Some(slot) = space.theme_colors.get_mut(index) {
            *slot = color;
            if index == 0 {
                space.color = color;
            }
        }
        self.theme_transition = None;
    }

    pub(crate) fn activate_space(&mut self, id: &str) {
        self.close_peek();
        self.close_app_page();
        self.plugins_menu_open = false;
        if self.active_space == id {
            return;
        }
        let previous_space = self.active_space.clone();
        if let Some(to_index) = self.spaces.iter().position(|space| space.id == id) {
            let from_index = self
                .spaces
                .iter()
                .position(|space| space.id == self.active_space)
                .unwrap_or(0);
            let current_space = self.active_space().clone();
            let (outgoing_space, switch_state) = self.space_switch_transition.as_ref().map_or_else(
                || {
                    (
                        current_space.clone(),
                        space_switcher::SpaceSwitchState::new(from_index, to_index),
                    )
                },
                |(previous_space, state, started, _)| {
                    let progress = ui::motion::space_switch().progress(started.elapsed());
                    let keep_incoming = progress >= 0.5;
                    let outgoing_space = if keep_incoming {
                        current_space.clone()
                    } else {
                        previous_space.clone()
                    };
                    (
                        outgoing_space,
                        state.retarget(to_index, progress, keep_incoming),
                    )
                },
            );
            let current_theme = visual::theme_for_space(self.active_space());
            let displayed_theme =
                self.theme_transition
                    .map_or(current_theme, |(from, started, _)| {
                        visual::interpolate_theme(
                            from,
                            current_theme,
                            ui::motion::theme().progress(started.elapsed()),
                        )
                    });
            self.theme_transition_id = self.theme_transition_id.wrapping_add(1);
            self.theme_transition =
                Some((displayed_theme, Instant::now(), self.theme_transition_id));
            self.space_switch_transition_id = self.space_switch_transition_id.wrapping_add(1);
            self.space_switch_transition = if ui::motion::reduced_motion() {
                None
            } else {
                Some((
                    outgoing_space,
                    switch_state,
                    Instant::now(),
                    self.space_switch_transition_id,
                ))
            };
        }
        self.show_suggestions = false;
        self.active_space = id.to_owned();
        self.emit_plugin_text("space.switched", format!("{previous_space}\n{id}"));
        self.save_state();
    }
    pub(in crate::app) fn activate_space_immediate(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.spaces.iter().any(|space| space.id == id) {
            self.sync_external_spaces(cx);
        }
        if !self.spaces.iter().any(|space| space.id == id) {
            return;
        }
        self.web_viewport.update(cx, |viewport, cx| {
            viewport.clear_transition_image();
            cx.notify();
        });
        self.second_web_viewport.update(cx, |viewport, cx| {
            viewport.clear_transition_image();
            cx.notify();
        });
        self.third_web_viewport.update(cx, |viewport, cx| {
            viewport.clear_transition_image();
            cx.notify();
        });
        self.fourth_web_viewport.update(cx, |viewport, cx| {
            viewport.clear_transition_image();
            cx.notify();
        });
        let previous_space = self.active_space.clone();
        self.close_peek();
        self.close_app_page();
        self.plugins_menu_open = false;
        self.active_space = id.to_owned();
        if let Some(space) = self.spaces.iter_mut().find(|s| s.id == id)
            && space.active.as_ref().is_none_or(|act| !space.tabs.iter().any(|t| t.id == *act)) {
                space.active = space.tabs.first().map(|t| t.id.clone());
            }
        let target_theme = crate::ui::visual::theme_for_space(self.active_space());
        crate::ui::visual::update_theme_caret(target_theme, cx);
        if previous_space != id {
            self.emit_plugin_text("space.switched", format!("{previous_space}\n{id}"));
        }
        self.space_switch_transition = None;
        self.theme_transition = None;
        self.show_suggestions = false;
        self.save_state();
    }
    pub(in crate::app) fn open_create_space_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space_context_menu = None;
        let index = self.spaces.len();
        let default_name = format!("Space {}", index + 1);
        let default_icon = SPACE_ICONS[index % SPACE_ICONS.len()].to_string();
        let default_color = SPACE_COLORS[index % SPACE_COLORS.len()];

        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(&default_name));
        name_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });

        let sub = cx.subscribe(&name_input, |this: &mut Anarc, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.commit_space_modal(cx);
                cx.notify();
            }
        });

        self.space_modal = Some(SpaceModal {
            mode: SpaceModalMode::Create,
            name_input,
            _name_sub: sub,
            selected_icon: default_icon,
            theme_draft: Space::new("draft", "", "", default_color),
        });
    }
    pub(in crate::app) fn open_edit_space_modal(
        &mut self,
        space_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space_context_menu = None;
        let (current_name, current_icon, current_color) = {
            let Some(space) = self.spaces.iter().find(|s| s.id == space_id) else {
                return;
            };
            (space.name.clone(), space.icon.clone(), space.color)
        };

        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Space name"));
        name_input.update(cx, |input, cx| {
            input.set_value(&current_name, window, cx);
            input.focus(window, cx);
        });

        let sub = cx.subscribe(&name_input, |this: &mut Anarc, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.commit_space_modal(cx);
                cx.notify();
            }
        });

        self.space_modal = Some(SpaceModal {
            mode: SpaceModalMode::Edit {
                space_id: space_id.to_string(),
            },
            name_input,
            _name_sub: sub,
            selected_icon: current_icon,
            theme_draft: self
                .spaces
                .iter()
                .find(|space| space.id == space_id)
                .cloned()
                .unwrap_or_else(|| Space::new("draft", "", "", current_color)),
        });
    }
    pub(in crate::app) fn commit_space_modal(&mut self, cx: &Context<Self>) {
        self.theme_editor = None;
        let Some(modal) = self.space_modal.take() else {
            return;
        };
        let input_val = modal.name_input.read(cx).value().to_string();
        let name = if input_val.trim().is_empty() {
            match &modal.mode {
                SpaceModalMode::Create => format!("Space {}", self.spaces.len() + 1),
                SpaceModalMode::Edit { .. } => "Untitled Space".to_string(),
            }
        } else {
            input_val.trim().to_string()
        };

        match modal.mode {
            SpaceModalMode::Create => {
                let id = format!("space-{}", self.spaces.len() + 1);
                let mut space = modal.theme_draft;
                space.id = id.clone();
                space.name = name.clone();
                space.icon = modal.selected_icon;
                let tab_id = format!("tab-{}-1", id);
                let new_tab = Tab::new(&tab_id, "New tab", "about:blank");
                space.tabs.push(new_tab);
                space.active = Some(tab_id);

                self.spaces.push(space);
                self.activate_space(&id);
            }
            SpaceModalMode::Edit { space_id } => {
                if let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id) {
                    space.name = name;
                    space.icon = modal.selected_icon;
                    space.color = modal.theme_draft.color;
                    space.theme_colors = modal.theme_draft.theme_colors;
                    space.theme_intensity = modal.theme_draft.theme_intensity;
                    space.theme_grain = modal.theme_draft.theme_grain;
                    space.theme_appearance = modal.theme_draft.theme_appearance;
                }
            }
        }
        self.save_state();
    }
    pub(in crate::app) fn delete_space(&mut self, space_id: &str) {
        self.space_context_menu = None;
        if self.spaces.len() <= 1 {
            return;
        }
        if let Some(idx) = self.spaces.iter().position(|s| s.id == space_id) {
            let is_shared = self.spaces[idx].name.starts_with("[Shared]")
                || self.spaces[idx].id.starts_with("shared-");
            for tab in &self.spaces[idx].tabs {
                self.engine.close_browser(&tab.id);
            }
            self.spaces.remove(idx);
            if self.active_space == space_id {
                self.active_space = self.spaces[0].id.clone();
            }
            self.save_state();
            if is_shared {
                std::thread::spawn(|| {
                    let _ = std::net::TcpStream::connect("127.0.0.1:7433").and_then(|mut stream| {
                        use std::io::Write;
                        let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(500)));
                        stream.write_all(b"POST /api/disconnect HTTP/1.0\r\nHost: 127.0.0.1:7433\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    });
                });
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn sync_external_spaces(&mut self, cx: &mut Context<Self>) {
        let Some(loaded) = crate::persist::load_state() else {
            return;
        };
        let mut changed = false;
        let mut activate_id = None;
        let mut sync_cookies = false;

        // Prune any shared spaces that are no longer in loaded.spaces (e.g. session ended or space deleted)
        let before_len = self.spaces.len();
        self.spaces.retain(|s| {
            if s.name.starts_with("[Shared]") || s.id.starts_with("shared-") {
                loaded.spaces.iter().any(|ext| ext.id == s.id)
            } else {
                true
            }
        });
        if self.spaces.len() != before_len {
            changed = true;
            if !self.spaces.iter().any(|s| s.id == self.active_space) {
                self.active_space = self.spaces.first().map(|s| s.id.clone()).unwrap_or_else(|| "personal".into());
            }
        }

        for mut external_space in loaded.spaces {
            for ext_tab in &mut external_space.tabs {
                if ext_tab.is_pinned()
                    && let Some(pinned) = &ext_tab.pinned_url
                    && !pinned.is_empty() {
                        let p_lower = pinned.to_ascii_lowercase();
                        if !p_lower.contains("workspace.google.") && !p_lower.contains("/signin") {
                            ext_tab.url = pinned.clone();
                        }
                    }
            }
            if let Some(local_space) = self.spaces.iter_mut().find(|s| s.id == external_space.id) {
                if local_space.color != external_space.color
                    || local_space.theme_colors != external_space.theme_colors
                    || local_space.theme_appearance != external_space.theme_appearance
                    || (local_space.theme_intensity - external_space.theme_intensity).abs() > 0.001
                    || (local_space.theme_grain - external_space.theme_grain).abs() > 0.001
                {
                    local_space.color = external_space.color;
                    local_space.theme_colors = external_space.theme_colors.clone();
                    local_space.theme_appearance = external_space.theme_appearance;
                    local_space.theme_intensity = external_space.theme_intensity;
                    local_space.theme_grain = external_space.theme_grain;
                    changed = true;
                }
                if local_space.peer_ips != external_space.peer_ips {
                    local_space.peer_ips = external_space.peer_ips.clone();
                    changed = true;
                    if !local_space.peer_ips.is_empty() && local_space.id != "shared-space" {
                        sync_cookies = true;
                    }
                }
                if local_space.tabs.len() != external_space.tabs.len()
                    || local_space.tabs.iter().zip(&external_space.tabs).any(|(a, b)| {
                        a.id != b.id
                            || a.url != b.url
                            || a.title != b.title
                            || a.is_pinned() != b.is_pinned()
                            || a.folder != b.folder
                    })
                {
                    for ext_tab in &external_space.tabs {
                        if let Some(loc_tab) = local_space.tabs.iter().find(|t| t.id == ext_tab.id)
                            && loc_tab.url != ext_tab.url
                                && !ext_tab.url.is_empty()
                                && ext_tab.url != "about:blank"
                            {
                                let ext_lower = ext_tab.url.to_ascii_lowercase();
                                let is_landing_or_auth = ext_lower.contains("workspace.google.com")
                                    || ext_lower.contains("google.com/gmail/about")
                                    || ext_lower.contains("google.com/intl/")
                                    || ext_lower.contains("accounts.google.")
                                    || ext_lower.contains("login.microsoft")
                                    || ext_lower.contains("login.live.com")
                                    || ext_lower.contains("appleid.apple.com")
                                    || ext_lower.contains("/signin/")
                                    || ext_lower.contains("/v3/signin/")
                                    || ext_lower.contains("/signin")
                                    || ext_lower.contains("/login")
                                    || ext_lower.contains("/logout")
                                    || ext_lower.contains("/accountchooser")
                                    || ext_lower.contains("/oauth2/")
                                    || ext_lower.contains("/oauth/")
                                    || ext_lower.contains("auth0.com")
                                    || ext_lower.contains("okta.com");
                                if !is_landing_or_auth {
                                    self.engine.navigate(&ext_tab.id, &ext_tab.url);
                                }
                            }
                    }
                    local_space.tabs = external_space.tabs;
                    if local_space.active.as_ref().is_none_or(|act| !local_space.tabs.iter().any(|t| t.id == *act)) {
                        local_space.active = local_space.tabs.first().map(|t| t.id.clone());
                    }
                    if local_space.id == "shared-space" {
                        let cookies_file = crate::paths::config_dir().join("shared-cookies.json");
                        if cookies_file.is_file()
                            && let Ok(content) = std::fs::read_to_string(&cookies_file)
                                && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                                    let arr_opt = if val.is_array() {
                                        Some(val)
                                    } else {
                                        val.get("cookies").cloned()
                                    };
                                    if let Some(arr) = arr_opt && let Some(c_arr) = arr.as_array() && !c_arr.is_empty() {
                                        let cleaned: Vec<_> = c_arr.iter().filter_map(crate::engine::devtools::sanitize_cookie_for_cdp).collect();
                                        if !cleaned.is_empty() {
                                            let params = serde_json::json!({ "cookies": cleaned }).to_string();
                                            for tab in &local_space.tabs {
                                                let _ = self.engine.devtools(&tab.id, "Network.setCookies", Some(&params));
                                            }
                                        }
                                    }
                                }
                    }
                    changed = true;
                }
            } else {
                if !crate::app::SPACE_ICONS.contains(&external_space.icon.as_str()) {
                    external_space.icon = "globe".to_string();
                }
                if external_space.active.is_none() && !external_space.tabs.is_empty() {
                    external_space.active = Some(external_space.tabs[0].id.clone());
                }
                let is_shared = external_space.name.starts_with("[Shared]")
                    || external_space.id.starts_with("shared-");
                if is_shared {
                    activate_id = Some(external_space.id.clone());
                }
                self.spaces.push(external_space);
                changed = true;
            }
        }

        if changed {
            if let Some(id) = activate_id {
                self.activate_space_immediate(&id, cx);
            }
            cx.notify();
        }

        if sync_cookies {
            self.sync_host_cookies_if_needed(false);
        }
    }

    /// Export the host's *entire* session (all account cookies and page logins
    /// in the shared space's CEF profile) to the daemon, which mirrors it to the
    /// peer. `force` is used on session transitions; otherwise an unchanged
    /// snapshot is skipped so the 3 s check never rewrites/rebroadcasts cookies.
    pub(crate) fn sync_host_cookies_if_needed(&mut self, force: bool) {
        for space in &self.spaces {
            if space.id == "shared-space" {
                continue;
            }
            let is_active_session = !space.peer_ips.is_empty();
            let is_sharing_now = force
                && self.live_share_is_host
                && self.live_share_space_id == space.id;
            if !is_active_session && !is_sharing_now {
                continue;
            }

            let tab_to_query = space
                .tabs
                .iter()
                .find(|t| self.engine.has_browser(&t.id))
                .map(|t| t.id.clone())
                .or_else(|| {
                    space
                        .active
                        .as_ref()
                        .filter(|id| self.engine.has_browser(id))
                        .cloned()
                });
            let Some(tab_id) = tab_to_query else { continue; };

            // getAllCookies (not the URL-filtered getCookies) is required for a
            // peer to actually be signed into the same Google/work accounts:
            // Google auth spans accounts.google.com, google.com, youtube.com,
            // gstatic.com and more, including hosts not present in the space.
            let rx = self.engine.devtools(&tab_id, "Network.getAllCookies", None);
            std::thread::spawn(move || {
                let Ok(Ok(raw_json)) = rx.recv_blocking() else { return; };
                let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw_json) else { return; };
                let Some(cookies) = parsed.get("cookies").and_then(|c| c.as_array()) else { return; };
                if cookies.is_empty() {
                    return;
                }
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                for c in cookies {
                    (
                        c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        c.get("value").and_then(|v| v.as_str()).unwrap_or(""),
                        c.get("domain").and_then(|v| v.as_str()).unwrap_or(""),
                        c.get("path").and_then(|v| v.as_str()).unwrap_or(""),
                    ).hash(&mut hasher);
                }
                let fingerprint = hasher.finish();
                if !force
                    && fingerprint == SHARED_COOKIE_FINGERPRINT.load(std::sync::atomic::Ordering::Relaxed)
                {
                    return;
                }
                SHARED_COOKIE_FINGERPRINT.store(fingerprint, std::sync::atomic::Ordering::Relaxed);
                let payload = serde_json::json!({ "cookies": cookies }).to_string();
                let _ = std::net::TcpStream::connect_timeout(
                    &"127.0.0.1:7433".parse::<std::net::SocketAddr>().unwrap(),
                    std::time::Duration::from_millis(200),
                ).and_then(|mut stream| {
                    use std::io::Write;
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(500)));
                    let req = format!(
                        "POST /api/cookies HTTP/1.0\r\nHost: 127.0.0.1:7433\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
                    );
                    stream.write_all(req.as_bytes())
                });
            });
        }
    }

    pub(crate) fn inject_shared_cookies_into_all_tabs(&mut self) {
        let cookies_file = crate::paths::config_dir().join("shared-cookies.json");
        if !cookies_file.is_file() {
            return;
        }
        let Ok(content) = std::fs::read_to_string(&cookies_file) else {
            return;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            return;
        };
        let arr_opt = if val.is_array() {
            Some(val)
        } else {
            val.get("cookies").cloned()
        };
        let Some(arr) = arr_opt else { return; };
        let Some(c_arr) = arr.as_array() else { return; };
        if c_arr.is_empty() {
            return;
        }
        let cleaned: Vec<_> = c_arr
            .iter()
            .filter_map(crate::engine::devtools::sanitize_cookie_for_cdp)
            .collect();
        if cleaned.is_empty() {
            return;
        }
        let params = serde_json::json!({ "cookies": cleaned }).to_string();

        let shared_space_opt = self.spaces.iter().find(|s| s.id == "shared-space");
        let Some(shared_space) = shared_space_opt else { return; };

        let mut to_recover: Vec<(String, String)> = Vec::new();
        for tab in &shared_space.tabs {
            if self.engine.has_browser(&tab.id) {
                let _ = self.engine.devtools(&tab.id, "Network.setCookies", Some(&params));
                let target_url = tab
                    .pinned_url
                    .as_deref()
                    .unwrap_or(tab.url.as_str());
                if !target_url.is_empty() && !target_url.starts_with("about:") {
                    let cur_url = self
                        .engine
                        .current_url(&tab.id)
                        .unwrap_or_else(|| tab.url.clone())
                        .to_ascii_lowercase();
                    let is_landing = cur_url.contains("workspace.google.com")
                        || cur_url.contains("google.com/gmail/about")
                        || cur_url.contains("accounts.google.")
                        || cur_url.contains("/signin")
                        || cur_url.contains("/login");
                    if is_landing {
                        to_recover.push((tab.id.clone(), target_url.to_string()));
                    }
                }
            }
        }
        for (tab_id, target_url) in to_recover {
            self.engine.navigate(&tab_id, &target_url);
        }
    }
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let chroma = value * saturation;
    let h = hue / 60.0;
    let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    let channel = |value: f32| ((value + m).clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(r) << 16) | (channel(g) << 8) | channel(b)
}

fn slider_drag_value(x: f32, origin: f32, width: f32) -> f32 {
    let inset = 8.0 * UI_SCALE;
    ((x - origin - inset) / (width - 2.0 * inset).max(1.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod theme_drag_tests {
    use super::*;

    #[test]
    fn sliders_track_the_thumb_center_and_clamp_at_both_ends() {
        let origin = 40.0 * UI_SCALE;
        let width = 128.0 * UI_SCALE;
        assert_eq!(slider_drag_value(origin, origin, width), 0.0);
        assert_eq!(
            slider_drag_value(origin + 8.0 * UI_SCALE, origin, width),
            0.0
        );
        assert!((slider_drag_value(origin + width / 2.0, origin, width) - 0.5).abs() < 0.0001);
        assert!(
            (slider_drag_value(origin + width - 8.0 * UI_SCALE, origin, width) - 1.0).abs()
                < 0.0001
        );
        assert_eq!(slider_drag_value(origin + width * 2.0, origin, width), 1.0);
    }
}
