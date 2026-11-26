mod folders;
mod navigation;
mod palette;
mod panels;
pub(super) mod peek;
mod plugins;
mod presence;
mod resize;
mod shortcuts;
mod spaces;
mod tabs;

#[cfg(test)]
pub(in crate::app) use navigation::single_line_paste;

use super::*;
use crate::app::state::OverlayTransition;

/// Kept tabs remember the page they were pinned on independently of where the
/// user navigates later. Legacy tabs predate that field, so their current URL
/// is adopted once as the pinned destination.
pub(in crate::app) fn seed_pinned_destination(tab: &mut Tab) {
    if tab.is_pinned() && tab.pinned_url.is_none() {
        tab.pinned_url = Some(tab.url.clone());
        if tab.pinned_title.is_none() && !tab.title.is_empty() {
            tab.pinned_title = Some(tab.title.clone());
        }
    }
}

impl Anarc {
    pub(crate) fn plugin_rows(&self) -> Vec<crate::plugin::PluginRow> {
        self.plugins.rows()
    }

    pub(crate) fn set_plugin_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        let is_panel = self.plugins.rows().iter().any(|row| {
            row.descriptor.id == id && row.descriptor.kind == crate::plugin::PluginKind::Panel
        });
        self.plugins.set_enabled(id, enabled)?;
        if is_panel && !enabled {
            self.stop_panel_browsers(id);
        }
        Ok(())
    }

    pub(super) fn emit_plugin_text(&self, name: &str, value: impl Into<String>) {
        self.emit_plugin_text_with_mode(name, crate::plugin::EventMode::Emit, value);
    }

    pub(super) fn emit_plugin_text_with_mode(
        &self,
        name: &str,
        mode: crate::plugin::EventMode,
        value: impl Into<String>,
    ) {
        self.plugins
            .emit(name, mode, crate::plugin::EventPayload::Text(value.into()));
    }

    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (mut spaces, active_space, sidebar_width, sidebar_collapsed) =
            if let Some(mut saved) = persist::load_state() {
                if !saved.spaces.is_empty() {
                    for space in &mut saved.spaces {
                        if !SPACE_ICONS.contains(&space.icon.as_str()) {
                            space.icon = "compass".to_string();
                        }
                    }
                    let active = if saved.spaces.iter().any(|s| s.id == saved.active_space) {
                        saved.active_space
                    } else {
                        saved.spaces[0].id.clone()
                    };
                    (
                        saved.spaces,
                        active,
                        saved
                            .sidebar_width
                            .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH),
                        saved.sidebar_collapsed,
                    )
                } else {
                    let (spaces, active) = create_default_space();
                    (spaces, active, 240.0 * UI_SCALE, false)
                }
            } else {
                let (spaces, active) = create_default_space();
                (spaces, active, 240.0 * UI_SCALE, false)
            };

        make_tab_ids_unique(&mut spaces);
        for space in &mut spaces {
            for tab in &mut space.tabs {
                // Favorites and pins are one section: normalize old saves that
                // only set one of the two flags before any logic reads them.
                if tab.is_pinned() {
                    tab.set_pinned(true);
                } else {
                    tab.kept_group = None;
                }
                // Saves older than `pinned_url` (and tabs pinned before their
                // URL settled) get their destination captured now, while the
                // stored URL is still the page they represent. Capturing it on
                // close instead would adopt whatever the user navigated to.
                seed_pinned_destination(tab);
            }
        }
        for space in &mut spaces {
            for tab in &mut space.tabs {
                if tab.is_pinned()
                    && let Some(pinned_url) = &tab.pinned_url
                {
                    tab.url = pinned_url.clone();
                    if let Some(pinned_title) = &tab.pinned_title {
                        tab.title = pinned_title.clone();
                    }
                    tab.state = TabState::Discarded;
                }
            }
        }
        if let Some(space) = spaces.iter_mut().find(|space| space.id == active_space) {
            let blank_tab = space
                .tabs
                .iter()
                .find(|tab| {
                    (tab.url == "about:blank" || tab.url == "lumen://welcome")
                        && !tab.is_pinned()
                        && tab.folder.is_none()
                })
                .map(|tab| tab.id.clone());
            let tab_id = blank_tab.unwrap_or_else(|| {
                let id = next_tab_id(space);
                space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
                id
            });
            space.active = Some(tab_id);
            if let Some(active) = space.active.as_deref()
                && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == active)
            {
                tab.state = TabState::Discarded;
            }
        }

        let plugin_config = match crate::plugin::PluginConfig::load() {
            Ok(config) => config,
            Err(error) => {
                eprintln!("Failed to load plugin configuration: {error}");
                crate::plugin::PluginConfig::default()
            }
        };
        let safe_mode = std::env::args().any(|argument| argument == "--safe-mode");
        let panel_registry = crate::plugin::new_panel_registry();
        let mut plugins = crate::plugin::PluginHost::new(plugin_config);
        plugins.set_safe_mode(safe_mode);
        if let Err(error) = plugins.mount_core() {
            eprintln!("Failed to mount core plugin: {error}");
        }
        crate::plugins::register(&mut plugins);
        let data_plugins = crate::paths::data_dir().join("plugins");
        let bundled_plugins = std::path::PathBuf::from("plugins");
        let exe_plugins = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("plugins")));
        for root in [&data_plugins, &bundled_plugins]
            .into_iter()
            .chain(exe_plugins.as_ref())
        {
            if root.is_dir() {
                let _ = crate::plugin::register_packs(&mut plugins, root);
                let _ = crate::plugin::register_panel_plugins(
                    &mut plugins,
                    root,
                    panel_registry.clone(),
                );
                #[cfg(feature = "wasm-plugins")]
                let _ = crate::plugin::register_wasm_plugins(&mut plugins, root);
            }
        }
        if let Err(error) = plugins.mount_enabled() {
            eprintln!("Failed to mount enabled plugins: {error}");
        }
        let preferences = plugins
            .service(crate::plugin::SETTINGS)
            .expect("core settings service is mounted")
            .load_preferences();
        let initial_query = std::env::args().skip(1).find(|arg| !arg.starts_with("--"));
        let initial_http_first = initial_query
            .as_deref()
            .and_then(crate::shield::upgrade_http_url);
        if let Some(query) = &initial_query
            && let Some(space) = spaces.iter_mut().find(|s| s.id == active_space)
        {
            let _ = plugins
                .service(crate::plugin::NAVIGATION)
                .expect("core navigation service is mounted")
                .submit_address(space, query, preferences.search_engine);
        }

        persist::save_state(&spaces, &active_space, sidebar_width, sidebar_collapsed);

        let address = cx.new(|cx| {
            let input =
                InputState::new(window, cx).placeholder("Search or enter a website address");
            if let Some(query) = initial_query {
                input.default_value(query)
            } else {
                input
            }
        });
        address.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        let browser_focus_handle = cx.focus_handle();
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in page"));
        let command_input = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search tabs, history, and actions"));
        let command_palette_scroll = gpui_kit::UniformListScrollHandle::new();
        let root_entity = cx.weak_entity();
        let web_viewport = cx.new(|_| WebViewport::new(root_entity, browser_focus_handle.clone()));
        let root_entity = cx.weak_entity();
        let second_web_viewport =
            cx.new(|_| WebViewport::new(root_entity, browser_focus_handle.clone()));
        let root_entity = cx.weak_entity();
        let third_web_viewport =
            cx.new(|_| WebViewport::new(root_entity, browser_focus_handle.clone()));
        let root_entity = cx.weak_entity();
        let fourth_web_viewport =
            cx.new(|_| WebViewport::new(root_entity, browser_focus_handle.clone()));
        let root_entity = cx.weak_entity();
        let peek_web_viewport =
            cx.new(|_| WebViewport::new(root_entity, browser_focus_handle.clone()));
        let address_subscription =
            cx.subscribe(&address, |this: &mut Anarc, _, event, cx| match event {
                InputEvent::PressEnter { .. } => {
                    this.navigate(None, cx);
                    cx.notify();
                }
                InputEvent::Change => {
                    this.address_editing = true;
                    let text = this.address.read(cx).value().to_string();
                    this.update_suggestions(&text);
                    this.selected_suggestion = None;
                    let space = this.active_space();
                    let peer_ips = if !space.peer_ips.is_empty() {
                        &space.peer_ips
                    } else {
                        this.spaces()
                            .iter()
                            .find_map(|s| (!s.peer_ips.is_empty()).then_some(&s.peer_ips))
                            .unwrap_or(&space.peer_ips)
                    };
                    if !peer_ips.is_empty() {
                        crate::app::live_cursor::set_local_typing_text(
                            if text.is_empty() { None } else { Some(text) },
                            peer_ips,
                        );
                    }
                    cx.notify();
                }
                _ => {}
            });
        let command_subscription =
            cx.subscribe(&command_input, |this: &mut Anarc, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.selected_command = 0;
                    cx.notify();
                }
            });

        let mut engine = crate::engine::Engine::new();
        engine.set_panel_registry(panel_registry.clone());
        engine.set_ad_blocker_enabled(preferences.ad_blocker_enabled);
        crate::app::live_cursor::init_live_cursors(engine.ui_update_wake());
        if let Some(url) = initial_http_first
            && let Some(tab_id) = spaces
                .iter()
                .find(|space| space.id == active_space)
                .and_then(|space| space.active.as_deref())
        {
            engine.navigate_http_first(tab_id, &url);
        }

        let mut app = Self {
            spaces,
            active_space,
            sidebar_collapsed,
            sidebar_hovered: false,
            kept_drag_hovered_tab: None,
            kept_drag_hover_after: false,
            kept_drag_source_id: None,
            kept_section_drag_hovered: false,
            sidebar_width,
            sidebar_resizing: false,
            sidebar_resize_tracker: ui::motion::VelocityTracker1D::new(),
            sidebar_width_transition: None,
            sidebar_transition_serial: 0,
            theme_transition: None,
            theme_transition_id: 0,
            space_switch_transition: None,
            space_switch_transition_id: 0,
            #[cfg(not(test))]
            resize_drag: split_view::ResizeDrag::default(),
            address,
            address_active_tab: None,
            address_editing: false,
            reset_address_for_tab_change: false,
            find_input,
            command_input,
            command_palette_scroll,
            browser_focus_handle,
            web_viewport,
            second_web_viewport,
            third_web_viewport,
            fourth_web_viewport,
            peek_web_viewport,
            peek: None,
            peek_serial: 0,
            peek_focused: false,
            focused_pane: 0,
            split_ratio: 0.5,
            split_ratio_y: 0.5,
            split_resizing: false,
            split_resizing_horizontal: false,
            _address_subscription: address_subscription,
            _command_subscription: command_subscription,
            show_find: false,
            show_command_palette: false,
            selected_command: 0,
            suggestions: Vec::new(),
            suggestion_snapshot: Vec::new(),
            suggestions_were_visible: false,
            suggestion_closing: None,
            suggestion_animation_serial: 0,
            selected_suggestion: None,
            recent_searches: history::load_recent_searches(),
            zoom_levels: plugins
                .service(crate::plugin::SETTINGS)
                .expect("core settings service is mounted")
                .load_zooms(),
            #[cfg(not(test))]
            active_zoom_target: None,
            show_suggestions: false,
            engine,
            space_modal: None,
            space_context_menu: None,
            space_menu_transition: OverlayTransition::default(),
            sidebar_context_menu: None,
            sidebar_menu_transition: OverlayTransition::default(),
            theme_editor: None,
            theme_editor_transition: OverlayTransition::default(),
            tab_context_menu: None,
            tab_menu_transition: OverlayTransition::default(),
            web_context_menu: None,
            folder_context_menu: None,
            folder_menu_transition: OverlayTransition::default(),
            folder_modal: None,
            split_group_expanded: false,
            added_tab: None,
            split_expanded_at: None,
            recently_closed_tabs: Vec::new(),
            closing_tab: None,
            closing_tab_serial: 0,
            browser_focused: false,
            fullscreen_tab: None,
            fullscreen_window_was_fullscreen: None,
            app_page: None,
            app_page_was_open: None,
            app_page_closing: None,
            app_page_serial: 0,
            command_palette_was_open: false,
            command_palette_closing: None,
            command_palette_serial: 0,
            find_was_open: false,
            find_closing: None,
            find_serial: 0,
            peek_last: None,
            peek_closing: None,
            peek_closing_serial: 0,
            plugins_menu_open: false,
            downloads_bubble_open: false,
            downloads_bubble_was_open: false,
            downloads_bubble_closing: None,
            downloads_bubble_serial: 0,
            dismissed_download_toast_id: None,
            settings_section: SettingsSection::default(),
            last_spaces_mtime: std::sync::atomic::AtomicU64::new(
                crate::persist::spaces_mtime()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
            ),
            last_shared_cookies_mtime: std::sync::atomic::AtomicU64::new(0),
            last_cookie_sync: std::time::Instant::now(),
            last_live_share_session_mtime: std::sync::atomic::AtomicU64::new(0),
            live_share_session_generation: 0,
            live_share_is_host: false,
            live_share_space_id: String::new(),
            pending_auth_recovery: std::collections::HashMap::new(),
            preferences,
            plugins,
            panel_registry,
            pending_plugin_review: None,
            plugin_action_error: None,
            expanded_plugins: std::collections::HashSet::new(),
            #[cfg(not(test))]
            cef_task_started: false,
        };
        // Records restored from disk are history, not a fresh completion.
        app.dismissed_download_toast_id = app.engine.downloads().last().map(|record| record.id);
        app.plugins.emit(
            "app.ready",
            crate::plugin::EventMode::Parallel,
            crate::plugin::EventPayload::Empty,
        );
        let initial_theme = crate::ui::visual::theme_for_space(app.active_space());
        crate::ui::visual::update_theme_caret(initial_theme, cx);
        ensure_live_share_daemon_running();
        app.focus_browser(window, cx);
        app
    }
    pub(crate) fn close_app_page(&mut self) {
        self.app_page = None;
    }
    pub(crate) fn open_web_context_menu(
        &mut self,
        tab_id: String,
        x: f32,
        y: f32,
        download_url: Option<String>,
    ) {
        self.tab_context_menu = None;
        self.folder_context_menu = None;
        self.space_context_menu = None;
        self.web_context_menu = Some(super::state::WebContextMenu {
            tab_id,
            x,
            y,
            download_url,
        });
    }
    pub(crate) fn set_settings_section(&mut self, section: SettingsSection) {
        self.settings_section = section;
    }
    pub(crate) fn set_sidebar_mode(
        &mut self,
        mode: crate::settings::SidebarMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = if self.is_sidebar_collapsed() {
            match mode {
                crate::settings::SidebarMode::Hidden => 0.0,
                crate::settings::SidebarMode::Icons => SIDEBAR_COLLAPSED_WIDTH,
                crate::settings::SidebarMode::Hover if self.sidebar_hovered => self.sidebar_width,
                crate::settings::SidebarMode::Hover => 0.0,
            }
        } else {
            self.sidebar_width
        };
        self.animate_sidebar_width_to(target, self.sidebar_collapsed, window, cx);
        self.sidebar_hovered = mode == crate::settings::SidebarMode::Hover && self.sidebar_hovered;
        self.preferences.sidebar_mode = mode;
        crate::settings::save_preferences(&self.preferences);
        cx.notify();
    }
    pub(crate) fn active_space(&self) -> &Space {
        self.spaces
            .iter()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist")
    }
    pub(in crate::app) fn effective_sidebar_width(&self) -> f32 {
        if !self.is_sidebar_collapsed() {
            return self.sidebar_width;
        }
        match self.preferences.sidebar_mode {
            crate::settings::SidebarMode::Hidden => 0.0,
            crate::settings::SidebarMode::Icons => SIDEBAR_COLLAPSED_WIDTH,
            crate::settings::SidebarMode::Hover if self.sidebar_hovered => self.sidebar_width,
            crate::settings::SidebarMode::Hover => 0.0,
        }
    }
    pub(crate) fn is_sidebar_collapsed(&self) -> bool {
        crate::plugin::active_layout_preset()
            .and_then(|preset| preset.sidebar_collapsed)
            .unwrap_or(self.sidebar_collapsed)
    }
    /// Exact CEF sizes for every visible split pane, row-major.
    pub(in crate::app) fn split_browser_sizes(
        &self,
        pair: &crate::model::SplitPair,
        content_width: f32,
        content_height: f32,
    ) -> Vec<(String, i32, i32)> {
        let panes = pair.panes();
        let sizes = if pair.is_quad() {
            let quad = split_view::quad_pane_sizes(
                content_width,
                content_height,
                self.split_ratio,
                self.split_ratio_y,
            );
            quad.to_vec()
        } else if pair.third.is_some() {
            let triple = split_view::triple_pane_sizes(
                content_width,
                content_height,
                self.split_ratio,
                self.split_ratio_y,
            );
            triple.to_vec()
        } else {
            let (left, right) = split_view::pane_widths(content_width, self.split_ratio);
            vec![(left, content_height), (right, content_height)]
        };
        panes
            .into_iter()
            .zip(sizes)
            .map(|(tab_id, (width, height))| {
                (
                    tab_id.to_owned(),
                    width.max(1.0) as i32,
                    height.max(1.0) as i32,
                )
            })
            .collect()
    }

    #[cfg(not(test))]
    pub(in crate::app) fn browser_sizes_for_sidebar(
        &self,
        window: &Window,
        sidebar_width: f32,
    ) -> Vec<split_view::BrowserSize> {
        let bounds = window.bounds();
        let content_width =
            (f32::from(bounds.size.width) - sidebar_width - 2.0 * CONTENT_BEZEL_X).max(100.0);
        let height =
            (f32::from(bounds.size.height) - visual::active_topbar_height() - CONTENT_BEZEL_BOTTOM)
                .max(100.0) as i32;
        if let Some(pair) = self.current_split_pair() {
            let sizes = self.split_browser_sizes(&pair, content_width, height as f32);
            sizes
                .into_iter()
                .map(|(tab_id, width, pane_height)| split_view::BrowserSize {
                    tab_id,
                    width,
                    height: pane_height,
                })
                .collect()
        } else {
            self.active_space()
                .active
                .clone()
                .map(|tab_id| {
                    vec![split_view::BrowserSize {
                        tab_id,
                        width: content_width.max(1.0) as i32,
                        height,
                    }]
                })
                .unwrap_or_default()
        }
    }
    #[cfg(not(test))]
    pub(in crate::app) fn begin_browser_resize_drag(
        &mut self,
        window: &Window,
        split_divider: bool,
        freeze: bool,
    ) {
        let pair = self.current_split_pair();
        let bounds = window.bounds();
        let width = f32::from(bounds.size.width);
        let height =
            (f32::from(bounds.size.height) - visual::active_topbar_height() - CONTENT_BEZEL_BOTTOM)
                .max(100.0) as i32;
        let sidebar_width = self.effective_sidebar_width();
        let content_width = (width - sidebar_width - CONTENT_BEZEL_X * 2.0).max(100.0);
        let fallback_sizes = if let Some(pair) = &pair {
            self.split_browser_sizes(pair, content_width, height as f32)
        } else if self.active_space().active.is_some() {
            vec![(self.active_tab_id().to_owned(), content_width.max(1.0) as i32, height)]
        } else {
            Vec::new()
        };
        let tab_ids = pair
            .map(|pair| {
                pair.panes()
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| self.active_space().active.clone().into_iter().collect());
        let fallback_surface = visual::theme_for_space(self.active_space()).surface;
        let sizes = tab_ids
            .iter()
            .enumerate()
            .map(|(index, tab_id)| {
                let (current_width, current_height) = self
                    .engine
                    .browser_size(tab_id)
                    .unwrap_or((fallback_sizes[index].1, fallback_sizes[index].2));
                split_view::BrowserSize {
                    tab_id: tab_id.clone(),
                    width: current_width,
                    height: current_height,
                }
            })
            .collect::<Vec<_>>();
        let edge_colors = tab_ids
            .iter()
            .map(|tab_id| {
                (
                    tab_id.clone(),
                    self.engine
                        .frame_edge_color(tab_id)
                        .unwrap_or(fallback_surface),
                )
            })
            .collect();
        if freeze {
            self.resize_drag.start_frozen(sizes.clone(), edge_colors);
        } else {
            self.resize_drag.start(sizes.clone(), edge_colors);
        }
        for size in sizes {
            resize_debug(format_args!(
                "drag_start kind={} tab={} size={}x{}",
                if split_divider { "split" } else { "sidebar" },
                size.tab_id,
                size.width,
                size.height
            ));
        }
    }
    pub(crate) fn active_tab_id(&self) -> &str {
        self.active_space().active.as_deref().unwrap_or_default()
    }
    pub(crate) fn process_agent_requests(&mut self) {
        let requests = self.engine.take_agent_requests();
        for (req, resp_tx) in requests {
            crate::agent::plugin::handle_agent_request(self, req, resp_tx);
        }
    }
    pub(crate) fn save_state(&self) {
        #[cfg(not(test))]
        {
            let mut spaces_to_save = self.spaces.clone();
            if let Some(loaded) = persist::load_state() {
                for s in &mut spaces_to_save {
                    if let Some(ext) = loaded.spaces.iter().find(|e| e.id == s.id)
                        && s.peer_ips.is_empty() && !ext.peer_ips.is_empty() {
                            s.peer_ips = ext.peer_ips.clone();
                        }
                }
                for ext in loaded.spaces {
                    if (ext.name.starts_with("[Shared]") || ext.id.starts_with("shared-"))
                        && !spaces_to_save.iter().any(|s| s.id == ext.id)
                    {
                        spaces_to_save.push(ext);
                    }
                }
            }
            persist::save_state(
                &spaces_to_save,
                &self.active_space,
                self.sidebar_width
                    .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH),
                self.sidebar_collapsed,
            );
            if let Some(mtime) = persist::spaces_mtime()
                && let Ok(d) = mtime.duration_since(std::time::UNIX_EPOCH)
            {
                self.last_spaces_mtime
                    .store(d.as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
            }
        }
        #[cfg(test)]
        let _ = self;
    }
    pub(crate) fn focus_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browser_focused = true;
        self.peek_focused = false;
        self.show_suggestions = false;
        self.selected_suggestion = None;
        // A stale edit flag would route the next Return to address navigation
        // even after GPUI focus moved into the CEF canvas.
        self.address_editing = false;
        self.close_app_page();
        self.plugins_menu_open = false;
        self.tab_context_menu = None;
        self.space_context_menu = None;
        self.folder_context_menu = None;
        self.web_context_menu = None;
        self.theme_editor = None;
        window.focus(&self.browser_focus_handle, cx);
        if let Some(pair) = self.current_split_pair() {
            for tab_id in pair.panes() {
                self.engine.set_focus(tab_id, false);
            }
        }
        if let Some(peek_id) = self.peek.as_ref().map(|peek| peek.id.clone()) {
            self.engine.set_focus(&peek_id, false);
        }
        let tab_id = self.focused_tab_id().to_string();
        self.engine.set_focus(&tab_id, true);
    }
    pub(in crate::app) fn focused_tab_id(&self) -> &str {
        let space = self.active_space();
        let active = space.active.as_deref().unwrap_or_default();
        let Some(pair) = space.split.as_ref().filter(|pair| {
            let panes = pair.panes();
            panes.contains(&active)
                && panes.iter().all(|id| space.tabs.iter().any(|tab| &tab.id == id))
        }) else {
            return active;
        };
        pair.pane(self.focused_pane.min(pair.pane_count() - 1))
            .unwrap_or(active)
    }
    pub(crate) fn set_focused_pane(&mut self, pane_index: usize) {
        if let Some(pair) = self.stored_split_pair() {
            self.focused_pane = pane_index.min(pair.pane_count() - 1);
        } else {
            self.focused_pane = 0;
        }
        self.close_app_page();
        self.plugins_menu_open = false;
        if let Some(pair) = self.stored_split_pair()
            && let Some(tab_id) = pair.pane(self.focused_pane)
        {
            self.active_space_mut().active = Some(tab_id.to_owned());
        }
    }
    pub(in crate::app) fn activate_split_group(&mut self) {
        if let Some(pair) = self.stored_split_pair() {
            self.focused_pane = 0;
            self.close_app_page();
            self.plugins_menu_open = false;
            self.active_space_mut().active = Some(pair.first);
        }
    }
    pub(in crate::app) fn current_split_pair(&self) -> Option<SplitPair> {
        let pair = self.stored_split_pair()?;
        pair.panes()
            .contains(&self.active_tab_id())
            .then_some(pair)
    }
    pub(in crate::app) fn stored_split_pair(&self) -> Option<SplitPair> {
        let space = self.active_space();
        let pair = space.split.as_ref()?;
        let panes = pair.panes();
        let distinct = panes
            .iter()
            .enumerate()
            .all(|(index, id)| panes[index + 1..].iter().all(|other| other != id));
        (distinct
            && panes
                .iter()
                .all(|id| space.tabs.iter().any(|tab| &tab.id == id)))
        .then(|| pair.clone())
    }
    pub(crate) fn split_active_with_next(&mut self) {
        let space = self.active_space_mut();
        let Some(first) = space.active.clone() else {
            return;
        };
        // A second split fills the remaining two quadrants; beyond four panes
        // the shortcut is a no-op.
        if let Some(mut pair) = space.split.clone() {
            if pair.is_quad() {
                return;
            }
            let mut used = pair.panes().iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
            let mut extra = Vec::new();
            let needed = if pair.third.is_some() { 1 } else { 2 };
            while extra.len() < needed {
                let next = space
                    .tabs
                    .iter()
                    .map(|tab| tab.id.clone())
                    .find(|id| !used.contains(id) && !extra.contains(id));
                let id = match next {
                    Some(id) => id,
                    None => {
                        let id = next_tab_id(space);
                        space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
                        id
                    }
                };
                used.push(id.clone());
                extra.push(id);
            }
            if pair.third.is_some() {
                pair.fourth = Some(extra.remove(0));
            } else {
                pair.third = Some(extra.remove(0));
                pair.fourth = Some(extra.pop().unwrap());
            }
            space.split = Some(pair);
            self.split_ratio = 0.5;
            self.split_ratio_y = 0.5;
            self.save_state();
            return;
        }
        let Some(first_index) = space.tabs.iter().position(|tab| tab.id == first) else {
            return;
        };
        let second = if let Some(tab) = space.tabs.get(first_index + 1) {
            tab.id.clone()
        } else if space.tabs.len() > 1 {
            space.tabs[0].id.clone()
        } else {
            let id = next_tab_id(space);
            space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
            id
        };
        if first != second {
            space.split = Some(SplitPair::new(first, second));
            self.focused_pane = 0;
            self.split_ratio = 0.5;
            self.split_ratio_y = 0.5;
            self.save_state();
        }
    }
    pub(crate) fn exit_split(&mut self) {
        if self.active_space().split.is_some() {
            self.active_space_mut().split = None;
            self.focused_pane = 0;
            self.split_ratio = 0.5;
            self.split_ratio_y = 0.5;
            self.split_resizing = false;
            self.split_resizing_horizontal = false;
            self.save_state();
        }
    }
    pub(crate) fn active_space_mut(&mut self) -> &mut Space {
        self.spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist")
    }
}

pub(crate) fn ensure_live_share_daemon_running() {
    #[cfg(not(test))]
    {
        if let Ok(addr) = "127.0.0.1:7433".parse::<std::net::SocketAddr>()
            && std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(50))
                .is_ok()
        {
            return;
        }
        let exe = std::env::current_exe().ok();
        let exe_dir = exe.as_ref().and_then(|p| p.parent());
        let candidates = [
            Some(crate::paths::data_dir().join("plugins/com.anarc.live-share/daemon.py")),
            Some(crate::paths::data_dir().join("plugins/live-share/daemon.py")),
            Some(std::path::PathBuf::from("plugins/live-share/daemon.py")),
            exe_dir.map(|p| p.join("plugins/live-share/daemon.py")),
            exe_dir
                .and_then(|p| p.parent())
                .map(|p| p.join("plugins/live-share/daemon.py")),
            exe_dir
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .map(|p| p.join("plugins/live-share/daemon.py")),
            exe_dir
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .map(|p| p.join("plugins/live-share/daemon.py")),
            Some(std::path::PathBuf::from(
                "/home/rover/Projects/kreon/anarc/plugins/live-share/daemon.py",
            )),
        ];
        for path in candidates.into_iter().flatten() {
            if path.is_file() {
                let python_execs = if cfg!(windows) {
                    &["pythonw", "python", "python3"][..]
                } else {
                    &["python3", "python"][..]
                };
                for &py in python_execs {
                    let mut cmd = std::process::Command::new(py);
                    cmd.arg(&path)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null());
                    if let Some(parent) = path.parent() {
                        cmd.current_dir(parent);
                    }
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                    }
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::CommandExt;
                        cmd.process_group(0);
                    }
                    if cmd.spawn().is_ok() {
                        return;
                    }
                }
                break;
            }
        }
    }
}
