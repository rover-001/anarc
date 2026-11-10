mod folders;
mod navigation;
mod palette;
mod panels;
mod plugins;
mod resize;
mod shortcuts;
mod spaces;
mod tabs;

use super::*;

impl Lumen {
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
        if let Some(space) = spaces.iter_mut().find(|space| space.id == active_space) {
            let blank_tab = space
                .tabs
                .iter()
                .find(|tab| {
                    (tab.url == "about:blank" || tab.url == "lumen://welcome")
                        && !tab.favorite
                        && !tab.pinned
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
        if let Err(error) =
            crate::plugin::register_packs(&mut plugins, &crate::paths::data_dir().join("plugins"))
        {
            eprintln!("Failed to scan plugin packs: {error}");
        }
        if let Err(error) = crate::plugin::register_panel_plugins(
            &mut plugins,
            &crate::paths::data_dir().join("plugins"),
            panel_registry.clone(),
        ) {
            eprintln!("Failed to scan panel plugins: {error}");
        }
        #[cfg(feature = "wasm-plugins")]
        if let Err(error) = crate::plugin::register_wasm_plugins(
            &mut plugins,
            &crate::paths::data_dir().join("plugins"),
        ) {
            eprintln!("Failed to scan WASM plugins: {error}");
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
        let address_subscription =
            cx.subscribe(&address, |this: &mut Lumen, _, event, cx| match event {
                InputEvent::PressEnter { .. } => {
                    this.navigate(cx);
                    cx.notify();
                }
                InputEvent::Change => {
                    this.address_editing = true;
                    let text = this.address.read(cx).value().to_string();
                    this.update_suggestions(&text);
                    this.selected_suggestion = None;
                    cx.notify();
                }
                _ => {}
            });
        let command_subscription =
            cx.subscribe(&command_input, |this: &mut Lumen, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.selected_command = 0;
                    cx.notify();
                }
            });

        let mut engine = crate::engine::Engine::new();
        engine.set_panel_registry(panel_registry.clone());
        engine.set_ad_blocker_enabled(preferences.ad_blocker_enabled);
        if let Some(url) = initial_http_first
            && let Some(tab_id) = spaces
                .iter()
                .find(|space| space.id == active_space)
                .and_then(|space| space.active.as_deref())
        {
            engine.navigate_http_first(tab_id, &url);
        }

        let app = Self {
            spaces,
            active_space,
            sidebar_collapsed,
            sidebar_width,
            sidebar_resizing: false,
            sidebar_width_transition: None,
            sidebar_transition_serial: 0,
            theme_transition: None,
            theme_transition_id: 0,
            space_switch_transition: None,
            space_switch_transition_id: 0,
            #[cfg(all(target_os = "linux", not(test)))]
            resize_drag: split_view::ResizeDrag::default(),
            started_at: Instant::now(),
            first_frame_logged: false,
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
            focused_pane: 0,
            split_ratio: 0.5,
            split_resizing: false,
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
            #[cfg(all(target_os = "linux", not(test)))]
            active_zoom_target: None,
            show_suggestions: false,
            engine,
            space_modal: None,
            space_context_menu: None,
            sidebar_context_menu: None,
            theme_editor: None,
            tab_context_menu: None,
            folder_context_menu: None,
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
            plugins_menu_open: false,
            dismissed_download_toast_id: None,
            settings_section: SettingsSection::default(),
            preferences,
            plugins,
            panel_registry,
            pending_plugin_review: None,
            plugin_action_error: None,
            #[cfg(not(test))]
            cef_task_started: false,
        };
        app.plugins.emit(
            "app.ready",
            crate::plugin::EventMode::Parallel,
            crate::plugin::EventPayload::Empty,
        );
        app
    }
    pub(crate) fn close_app_page(&mut self) {
        self.app_page = None;
    }
    pub(crate) fn set_settings_section(&mut self, section: SettingsSection) {
        self.settings_section = section;
    }
    pub(crate) fn active_space(&self) -> &Space {
        self.spaces
            .iter()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist")
    }
    pub(in crate::app) fn effective_sidebar_width(&self) -> f32 {
        sidebar_width_for(self.is_sidebar_collapsed(), self.sidebar_width)
    }
    pub(crate) fn is_sidebar_collapsed(&self) -> bool {
        crate::plugin::active_layout_preset()
            .and_then(|preset| preset.sidebar_collapsed)
            .unwrap_or(self.sidebar_collapsed)
    }
    #[cfg(all(target_os = "linux", not(test)))]
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
            let (left, right) = split_view::pane_widths(content_width, self.split_ratio);
            vec![
                split_view::BrowserSize {
                    tab_id: pair.first,
                    width: left.max(1.0) as i32,
                    height,
                },
                split_view::BrowserSize {
                    tab_id: pair.second,
                    width: right.max(1.0) as i32,
                    height,
                },
            ]
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
    #[cfg(all(target_os = "linux", not(test)))]
    pub(in crate::app) fn begin_browser_resize_drag(
        &mut self,
        window: &Window,
        split_divider: bool,
    ) {
        let pair = self.current_split_pair();
        let bounds = window.bounds();
        let width = f32::from(bounds.size.width);
        let height =
            (f32::from(bounds.size.height) - visual::active_topbar_height() - CONTENT_BEZEL_BOTTOM)
                .max(100.0) as i32;
        let sidebar_width = self.effective_sidebar_width();
        let content_width = (width - sidebar_width - CONTENT_BEZEL_X * 2.0).max(100.0);
        let fallback_widths = if pair.is_some() {
            let (first, second) = split_view::pane_widths(content_width, self.split_ratio);
            vec![first.max(1.0) as i32, second.max(1.0) as i32]
        } else if self.active_space().active.is_some() {
            vec![content_width.max(1.0) as i32]
        } else {
            Vec::new()
        };
        let tab_ids = pair
            .map(|pair| vec![pair.first, pair.second])
            .unwrap_or_else(|| self.active_space().active.clone().into_iter().collect());
        let fallback_surface = visual::theme_for_space(self.active_space()).surface;
        let sizes = tab_ids
            .iter()
            .enumerate()
            .map(|(index, tab_id)| {
                let (current_width, current_height) = self
                    .engine
                    .browser_size(tab_id)
                    .unwrap_or((fallback_widths[index], height));
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
        self.resize_drag.start(sizes.clone(), edge_colors);
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
    pub(in crate::app) fn active_tab_id(&self) -> &str {
        self.active_space().active.as_deref().unwrap_or_default()
    }
    pub(in crate::app) fn save_state(&self) {
        #[cfg(not(test))]
        persist::save_state(
            &self.spaces,
            &self.active_space,
            self.sidebar_width
                .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH),
            self.sidebar_collapsed,
        );
        #[cfg(test)]
        let _ = self;
    }
    pub(crate) fn focus_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browser_focused = true;
        self.show_suggestions = false;
        self.selected_suggestion = None;
        window.focus(&self.browser_focus_handle, cx);
        if let Some(pair) = self.current_split_pair() {
            self.engine.set_focus(&pair.first, false);
            self.engine.set_focus(&pair.second, false);
        }
        let tab_id = self.focused_tab_id().to_string();
        self.engine.set_focus(&tab_id, true);
    }
    pub(in crate::app) fn focused_tab_id(&self) -> &str {
        let space = self.active_space();
        let active = space.active.as_deref().unwrap_or_default();
        let Some(pair) = space.split.as_ref().filter(|pair| {
            (active == pair.first || active == pair.second)
                && pair.first != pair.second
                && space.tabs.iter().any(|tab| tab.id == pair.first)
                && space.tabs.iter().any(|tab| tab.id == pair.second)
        }) else {
            return active;
        };
        split_view::tab_for_pane(&pair.first, &pair.second, self.focused_pane)
    }
    pub(crate) fn set_focused_pane(&mut self, pane_index: usize) {
        self.focused_pane = pane_index.min(1);
        if let Some(pair) = self.stored_split_pair() {
            self.active_space_mut().active = Some(
                split_view::tab_for_pane(&pair.first, &pair.second, self.focused_pane).to_owned(),
            );
        }
    }
    pub(in crate::app) fn activate_split_group(&mut self) {
        if let Some(pair) = self.stored_split_pair() {
            self.focused_pane = 0;
            self.active_space_mut().active = Some(pair.first);
        }
    }
    pub(in crate::app) fn current_split_pair(&self) -> Option<SplitPair> {
        let pair = self.stored_split_pair()?;
        let active = self.active_tab_id();
        (active == pair.first || active == pair.second).then_some(pair)
    }
    pub(in crate::app) fn stored_split_pair(&self) -> Option<SplitPair> {
        let space = self.active_space();
        let pair = space.split.as_ref()?;
        let has_first = space.tabs.iter().any(|tab| tab.id == pair.first);
        let has_second = space.tabs.iter().any(|tab| tab.id == pair.second);
        (has_first && has_second && pair.first != pair.second).then(|| pair.clone())
    }
    pub(in crate::app) fn split_active_with_next(&mut self) {
        let space = self.active_space_mut();
        let Some(first) = space.active.clone() else {
            return;
        };
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
            space.split = Some(SplitPair { first, second });
            self.focused_pane = 0;
            self.split_ratio = 0.5;
            self.save_state();
        }
    }
    pub(in crate::app) fn exit_split(&mut self) {
        if self.active_space().split.is_some() {
            self.active_space_mut().split = None;
            self.focused_pane = 0;
            self.save_state();
        }
    }
    pub(in crate::app) fn active_space_mut(&mut self) -> &mut Space {
        self.spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist")
    }
}
