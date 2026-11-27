use super::actions::single_line_paste;
use super::*;
use crate::app::state::OverlayTransition;
use gpui_kit::{Focusable, WindowOptions};

/// Tests that enable plugins call PluginConfig::save(), which targets the real
/// user config by default. Redirect ANARC_CONFIG_DIR to a temporary directory
/// so running the suite can never wipe the user's plugin toggles.
struct IsolatedConfigDir {
    previous: Option<std::ffi::OsString>,
    path: std::path::PathBuf,
}

impl Drop for IsolatedConfigDir {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var("ANARC_CONFIG_DIR", value),
                None => std::env::remove_var("ANARC_CONFIG_DIR"),
            }
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn isolated_config_dir(test: &str) -> IsolatedConfigDir {
    let path = std::env::temp_dir().join(format!("anarc-plugin-config-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    let previous = std::env::var_os("ANARC_CONFIG_DIR");
    unsafe { std::env::set_var("ANARC_CONFIG_DIR", &path) };
    IsolatedConfigDir { previous, path }
}

fn test_app(window: &mut Window, cx: &mut Context<Anarc>) -> Anarc {
    crate::plugins::set_split_view_enabled_for_test(true);
    let address = cx.new(|cx| InputState::new(window, cx));
    address.update(cx, |input, cx| input.focus(window, cx));
    let address_subscription =
        cx.subscribe(&address, |app: &mut Anarc, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                app.address_editing = true;
                let text = app.address.read(cx).value().to_string();
                app.update_suggestions(&text);
                app.selected_suggestion = None;
                cx.notify();
            }
        });
    let find_input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in page"));
    let command_input =
        cx.new(|cx| InputState::new(window, cx).placeholder("Search tabs, history, and actions"));
    let command_subscription = cx.subscribe(&command_input, |this: &mut Anarc, _, event, cx| {
        if matches!(event, InputEvent::Change) {
            this.selected_command = 0;
            cx.notify();
        }
    });
    let (spaces, active_space) = create_default_space();
    let panel_registry = crate::plugin::new_panel_registry();
    let mut engine = engine::Engine::new();
    engine.set_panel_registry(panel_registry.clone());
    Anarc {
        spaces,
        active_space,
        sidebar_collapsed: false,
        sidebar_hovered: false,
        kept_drag_hovered_tab: None,
        kept_drag_hover_after: false,
        kept_drag_source_id: None,
        kept_section_drag_hovered: false,
        sidebar_width: 240.0 * UI_SCALE,
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
        command_palette_scroll: gpui_kit::UniformListScrollHandle::new(),
        browser_focus_handle: cx.focus_handle(),
        web_viewport: {
            let root_entity = cx.weak_entity();
            let focus_handle = cx.focus_handle();
            cx.new(|_| WebViewport::new(root_entity, focus_handle))
        },
        second_web_viewport: {
            let root_entity = cx.weak_entity();
            let focus_handle = cx.focus_handle();
            cx.new(|_| WebViewport::new(root_entity, focus_handle))
        },
        third_web_viewport: {
            let root_entity = cx.weak_entity();
            let focus_handle = cx.focus_handle();
            cx.new(|_| WebViewport::new(root_entity, focus_handle))
        },
        fourth_web_viewport: {
            let root_entity = cx.weak_entity();
            let focus_handle = cx.focus_handle();
            cx.new(|_| WebViewport::new(root_entity, focus_handle))
        },
        peek_web_viewport: {
            let root_entity = cx.weak_entity();
            let focus_handle = cx.focus_handle();
            cx.new(|_| WebViewport::new(root_entity, focus_handle))
        },
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
        recent_searches: Vec::new(),
        zoom_levels: std::collections::HashMap::new(),
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
        folder_context_menu: None,
        folder_menu_transition: OverlayTransition::default(),
        web_context_menu: None,
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
        last_spaces_mtime: std::sync::atomic::AtomicU64::new(0),
        last_shared_cookies_mtime: std::sync::atomic::AtomicU64::new(0),
        last_cookie_sync: std::time::Instant::now(),
        last_live_share_session_mtime: std::sync::atomic::AtomicU64::new(0),
        live_share_session_generation: 0,
        live_share_is_host: false,
        live_share_space_id: String::new(),
        pending_auth_recovery: std::collections::HashMap::new(),
        preferences: Default::default(),
        plugins: {
            // Do not mount the built-in plugins here. Their mount/dispose
            // effects mutate process-wide feature flags and the shared search
            // engine registry; because GPUI entities outlive the test body,
            // those effects would run after the test's serialization guard is
            // released and race with other tests. Each test that needs a flag
            // sets it explicitly (see set_split_view_enabled_for_test above).
            let mut plugins = crate::plugin::PluginHost::new(Default::default());
            plugins.mount_core().expect("core plugin mounts");
            plugins
        },
        panel_registry,
        pending_plugin_review: None,
        plugin_action_error: None,
        expanded_plugins: std::collections::HashSet::new(),
        #[cfg(not(test))]
        cef_task_started: false,
    }
}

fn dispatch_test_key(
    cx: &mut gpui::TestAppContext,
    handle: gpui_kit::AnyWindowHandle,
    key: &'static str,
) {
    cx.update_window(handle, |_, window, cx| {
        window.draw(cx).clear(cx);
        window.dispatch_keystroke(gpui_kit::Keystroke::parse(key).unwrap(), cx);
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

#[gpui::test]
fn browser_tab_shortcuts_work_from_focused_address_bar(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    dispatch_test_key(cx, handle, "ctrl-t");
    view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-2"));
    dispatch_test_key(cx, handle, "ctrl-w");
    view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-1"));
    dispatch_test_key(cx, handle, "ctrl-shift-t");
    view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-2"));
}

#[gpui::test]
fn plugin_settings_and_extensions_menu_render_without_nested_entity_reads(
    cx: &mut gpui::TestAppContext,
) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.app_page = Some(AppPage::Settings);
        app.settings_section = SettingsSection::Plugins;
        app.plugins_menu_open = true;
    });
    cx.update_window(handle, |_, window, cx| {
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

#[cfg(debug_assertions)]
#[gpui::test]
fn clicking_plugins_keeps_settings_open_over_a_blank_tab(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, cx| {
        app.app_page = Some(AppPage::Settings);
        cx.notify();
    });

    let mut window = gpui::VisualTestContext::from_window(handle, cx);
    window.update(|window, cx| window.draw(cx).clear(cx));
    let plugins_tab = window.debug_bounds("settings-tab-Plugins").unwrap();
    window.simulate_click(plugins_tab.center(), gpui::Modifiers::none());

    view.read_with(cx, |app, _| {
        assert!(app.app_page == Some(AppPage::Settings));
        assert!(app.settings_section == SettingsSection::Plugins);
    });
}

#[cfg(debug_assertions)]
#[gpui::test]
fn opening_a_panel_from_plugins_settings_focuses_its_tab(cx: &mut gpui::TestAppContext) {
    let _config_guard = isolated_config_dir("open-panel");
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = std::env::temp_dir().join(format!("anarc-open-panel-test-{}", std::process::id()));
    let package = root.join("notes");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        package.join("anarc-plugin.toml"),
        "[plugin]\nid = 'test.example.notes'\nname = 'Notes'\nversion = '1.0.0'\napi = '1'\nkind = 'panel'\ndescription = 'A collaborative space for sharing and organizing notes, links, and references across connected browsers.'\n",
    )
    .unwrap();
    std::fs::write(package.join("index.html"), "<h1>Notes</h1>").unwrap();

    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, cx| {
        app.plugins = crate::plugin::PluginHost::new(Default::default());
        app.plugins.mount_core().unwrap();
        crate::plugin::register_panel_plugins(&mut app.plugins, &root, app.panel_registry.clone())
            .unwrap();
        app.plugins.set_enabled("test.example.notes", true).unwrap();
        app.app_page = Some(AppPage::Settings);
        app.settings_section = SettingsSection::Plugins;
        cx.notify();
    });

    let mut window = gpui::VisualTestContext::from_window(handle, cx);
    window.simulate_resize(gpui_kit::size(px(960.0), px(800.0)));
    window.update(|window, cx| window.draw(cx).clear(cx));
    let card = window
        .debug_bounds("plugin-card-test.example.notes")
        .unwrap();
    let actions = window
        .debug_bounds("plugin-actions-test.example.notes")
        .unwrap();
    assert!(actions.right() < card.right());
    let open = window
        .debug_bounds("plugin-open-test.example.notes")
        .unwrap();
    window.simulate_click(open.center(), gpui::Modifiers::none());

    let _ = std::fs::remove_dir_all(root);
    view.read_with(cx, |app, _| {
        assert_eq!(app.app_page, None);
        assert!(app.browser_focused);
        let space = app.active_space();
        let active = space.tabs.iter().find(|tab| tab.id == app.active_tab_id());
        assert_eq!(
            active.map(|tab| tab.url.as_str()),
            Some("anarc-plugin://test.example.notes/")
        );
    });
}

#[gpui::test]
fn split_shortcuts_create_focus_and_exit_the_pane_pair(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    dispatch_test_key(cx, handle, "ctrl-shift-s");
    view.read_with(cx, |app, _| {
        assert_eq!(
            app.current_split_pair(),
            Some(SplitPair::new("tab-1".into(), "tab-2".into()))
        );
    });
    dispatch_test_key(cx, handle, "ctrl-shift-w");
    view.read_with(cx, |app, _| assert!(app.current_split_pair().is_none()));
    dispatch_test_key(cx, handle, "ctrl-shift-s");
    dispatch_test_key(cx, handle, "ctrl-alt-right");
    view.read_with(cx, |app, _| {
        assert_eq!(app.focused_pane, 1);
        assert_eq!(app.focused_tab_id(), "tab-2");
    });
    view.update(cx, |app, _| app.close_active_tab());
    view.read_with(cx, |app, _| {
        assert!(app.current_split_pair().is_none());
        assert_eq!(app.active_space().tabs.len(), 1);
        assert_eq!(app.active_tab_id(), "tab-1");
    });
    view.read_with(cx, |app, _| {
        assert_eq!(app.focused_pane, 0);
        assert!(app.current_split_pair().is_none());
    });
}

#[gpui::test]
fn split_can_extend_to_four_panes(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.add_tab();
        app.active_space_mut().active = Some("tab-1".into());
        app.split_active_with_next();
        assert_eq!(app.current_split_pair().unwrap().pane_count(), 2);

        app.split_active_with_next();
        let pair = app.current_split_pair().unwrap();
        assert!(pair.is_quad());
        assert_eq!(pair.panes().len(), 4);

        // A fifth split is a no-op; focus cycles through all four panes.
        app.split_active_with_next();
        assert_eq!(app.current_split_pair().unwrap().pane_count(), 4);
        app.set_focused_pane(3);
        assert_eq!(app.focused_tab_id(), pair.panes()[3]);

        app.exit_split();
        assert!(app.current_split_pair().is_none());
        assert_eq!(app.focused_pane, 0);
    });
}

#[gpui::test]
fn closing_a_pane_in_a_four_way_split_only_removes_that_pane(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.add_tab();
        app.active_space_mut().active = Some("tab-1".into());
        app.split_active_with_next();
        app.split_active_with_next();
        assert_eq!(
            app.current_split_pair().unwrap().panes(),
            ["tab-1", "tab-2", "tab-3", "tab-4"]
        );

        // Ctrl+W in the bottom-right pane must leave the other three split.
        app.set_focused_pane(3);
        app.close_active_tab();
        let pair = app.current_split_pair().unwrap();
        assert_eq!(pair.panes(), ["tab-1", "tab-2", "tab-3"]);
        assert!(!pair.is_quad());
        assert_eq!(app.focused_pane, 2);
        assert_eq!(app.focused_tab_id(), "tab-3");
        assert_eq!(app.active_tab_id(), "tab-3");

        // Closing a non-focused pane keeps the current pane focused.
        app.close_tab_by_id("tab-1");
        let pair = app.current_split_pair().unwrap();
        assert_eq!(pair.panes(), ["tab-2", "tab-3"]);
        assert_eq!(app.focused_pane, 1);
        assert_eq!(app.focused_tab_id(), "tab-3");

        app.close_active_tab();
        assert!(app.current_split_pair().is_none());
        assert_eq!(app.focused_pane, 0);
        assert_eq!(app.active_tab_id(), "tab-2");
    });
}

#[gpui::test]
fn unloading_a_kept_pane_keeps_the_rest_of_the_split(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.add_tab();
        app.active_space_mut().active = Some("tab-1".into());
        app.split_active_with_next();
        app.split_active_with_next();
        app.active_space_mut()
            .tabs
            .iter_mut()
            .find(|tab| tab.id == "tab-4")
            .unwrap()
            .set_pinned(true);
        app.set_focused_pane(3);
        app.close_active_tab();

        let pair = app.current_split_pair().unwrap();
        assert_eq!(pair.panes(), ["tab-1", "tab-2", "tab-3"]);
        assert_eq!(app.focused_pane, 2);
        assert_eq!(app.active_tab_id(), "tab-3");
    });
}

#[gpui::test]
fn dropping_a_tab_on_a_pane_builds_the_split(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.add_tab();
        app.add_tab();
        for tab in &mut app.active_space_mut().tabs {
            if tab.url == "about:blank" {
                tab.url = "https://example.com/".into();
            }
        }
        app.active_space_mut().active = Some("tab-1".into());
        app.drop_tab_into_pane("tab-2", 1);
        let pair = app.current_split_pair().unwrap();
        assert_eq!(pair.first, "tab-1");
        assert_eq!(pair.second, "tab-2");
        assert_eq!(app.focused_pane, 1);

        app.drop_tab_into_pane("tab-3", 1);
        let pair = app.current_split_pair().unwrap();
        assert!(pair.is_quad());
        assert_eq!(pair.fourth.as_deref(), Some("tab-3"));
    });
}

#[gpui::test]
fn closing_a_non_active_tab_by_id_preserves_the_active_tab(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.active_space_mut().active = Some("tab-1".into());
        assert!(app.close_tab_by_id("tab-2"));
    });
    view.read_with(cx, |app, _| {
        assert_eq!(app.active_tab_id(), "tab-1");
        assert_eq!(app.active_space().tabs.len(), 1);
    });
}

#[gpui::test]
fn closing_a_pinned_tab_request_keeps_it_pinned_and_unloads_it(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.active_space_mut().tabs[0].set_pinned(true);
        app.active_space_mut().tabs[0].url = "https://example.com".into();
        app.close_section_tab("tab-1");
    });
    view.read_with(cx, |app, _| {
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-1")
            .unwrap();
        assert!(tab.pinned);
        assert_eq!(tab.state, TabState::Discarded);
    });
}

#[gpui::test]
fn closing_a_folder_tab_keeps_it_in_its_folder(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        let folder_id = app.create_folder("a", Some("tab-2")).unwrap();
        app.active_space_mut()
            .tabs
            .iter_mut()
            .find(|tab| tab.id == "tab-2")
            .unwrap()
            .url = "https://example.com/folder".into();
        app.active_space_mut().active = Some("tab-2".into());

        app.close_active_tab();

        let space = app.active_space();
        let tab = space
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .expect("closing a folder tab must keep its sidebar entry");
        assert_eq!(tab.folder.as_deref(), Some(folder_id.as_str()));
        assert_eq!(tab.state, TabState::Discarded);
        assert!(space.folders[0].expanded);
        assert_ne!(space.active.as_deref(), Some("tab-2"));
    });
}

#[gpui::test]
fn closing_a_folder_tab_by_id_keeps_it_in_its_folder(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        let folder_id = app.create_folder("a", Some("tab-2")).unwrap();
        app.active_space_mut().active = Some("tab-1".into());

        assert!(app.close_tab_by_id("tab-2"));

        let space = app.active_space();
        let tab = space
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .expect("targeted close must not remove a folder tab");
        assert_eq!(tab.folder.as_deref(), Some(folder_id.as_str()));
        assert_eq!(tab.state, TabState::Discarded);
        assert_eq!(space.active.as_deref(), Some("tab-1"));
    });
}

#[gpui::test]
fn closing_a_folder_tab_from_the_sidebar_keeps_it_in_its_folder(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        let folder_id = app.create_folder("a", Some("tab-2")).unwrap();
        app.active_space_mut().active = Some("tab-1".into());

        app.close_section_tab("tab-2");

        let space = app.active_space();
        let tab = space
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .expect("the sidebar close button must not remove a folder tab");
        assert_eq!(tab.folder.as_deref(), Some(folder_id.as_str()));
        assert_eq!(tab.state, TabState::Discarded);
        assert_eq!(space.active.as_deref(), Some("tab-1"));
    });
}

#[gpui::test]
fn closing_a_pinned_tab_does_not_activate_another_pinned_tab(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.active_space_mut().tabs[1].url = "https://example.org".into();
        app.pin_tab("tab-2", true);
        app.active_space_mut().tabs[0].set_pinned(true);
        app.active_space_mut().active = Some("tab-1".into());
        app.close_active_tab();
    });
    view.read_with(cx, |app, _| {
        let space = app.active_space();
        let other_pinned = space.tabs.iter().find(|tab| tab.id == "tab-2").unwrap();
        assert_eq!(
            other_pinned.state,
            TabState::Discarded,
            "a second pinned tab must not be opened as the fallback"
        );
        assert_eq!(app.active_tab_id(), "tab-3");
    });
}

#[gpui::test]
fn unloading_a_pinned_tab_restores_its_pinned_destination(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.active_space_mut().tabs[0].url = "https://pinned.example/home".into();
        app.active_space_mut().tabs[0].title = "Pinned Home".into();
        app.pin_tab("tab-1", true);
        app.active_space_mut().tabs[0].url = "https://pinned.example/changed".into();
        app.active_space_mut().tabs[0].title = "Navigated Away".into();
        app.unload_kept_tab("tab-1");
    });
    view.read_with(cx, |app, _| {
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-1")
            .unwrap();
        assert_eq!(tab.url, "https://pinned.example/home");
        assert_eq!(tab.title, "Pinned Home");
    });
}

#[gpui::test]
fn unloading_a_favorite_tab_restores_its_pinned_destination(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.active_space_mut().tabs[0].url = "https://favorite.example/home".into();
        app.active_space_mut().tabs[0].title = "Favorite Home".into();
        app.favorite_tab("tab-1", true);
        app.active_space_mut().tabs[0].url = "https://favorite.example/changed".into();
        app.active_space_mut().tabs[0].title = "Changed Title".into();
        app.unload_kept_tab("tab-1");
    });
    view.read_with(cx, |app, _| {
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-1")
            .unwrap();
        assert!(tab.is_pinned());
        assert_eq!(tab.url, "https://favorite.example/home");
        assert_eq!(tab.title, "Favorite Home");
    });
}

#[test]
fn legacy_kept_tabs_adopt_their_saved_url_and_keep_it_after_navigation() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut tab = Tab::new("tab-1", "ChatGPT", "https://chatgpt.com/");
    tab.favorite = true;
    super::actions::seed_pinned_destination(&mut tab);
    assert_eq!(tab.pinned_url.as_deref(), Some("https://chatgpt.com/"));
    assert_eq!(tab.pinned_title.as_deref(), Some("ChatGPT"));

    // Later navigations must not move the pinned destination.
    tab.url = "https://chatgpt.com/c/6af52c5a".into();
    tab.title = "A conversation".into();
    super::actions::seed_pinned_destination(&mut tab);
    assert_eq!(tab.pinned_url.as_deref(), Some("https://chatgpt.com/"));
    assert_eq!(tab.pinned_title.as_deref(), Some("ChatGPT"));

    // Regular tabs are untouched.
    let mut regular = Tab::new("tab-2", "Scratch", "https://example.com");
    super::actions::seed_pinned_destination(&mut regular);
    assert_eq!(regular.pinned_url, None);
}

#[gpui::test]
fn favorite_and_pin_are_aliases_preserving_pinned_url(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.active_space_mut().tabs[0].url = "https://original.example".into();
        app.favorite_tab("tab-1", true);
        // User navigates
        app.active_space_mut().tabs[0].url = "https://original.example/updated".into();
        // User then pins it
        app.pin_tab("tab-1", true);
    });
    view.read_with(cx, |app, _| {
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-1")
            .unwrap();
        assert!(tab.pinned);
        assert!(tab.favorite);
        assert_eq!(tab.pinned_url.as_deref(), Some("https://original.example"));
    });
    // Unloading restores the original pinned link
    view.update(cx, |app, _| {
        app.unload_kept_tab("tab-1");
    });
    view.read_with(cx, |app, _| {
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-1")
            .unwrap();
        assert_eq!(tab.url, "https://original.example");
    });
}

#[gpui::test]
fn activating_already_active_pinned_tab_resets_to_pinned_destination(
    cx: &mut gpui::TestAppContext,
) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.active_space_mut().tabs[0].url = "https://pinned.example".into();
        app.pin_tab("tab-1", true);
        app.active_space_mut().tabs[0].url = "https://pinned.example/subpage".into();
        assert_eq!(app.active_space().active.as_deref(), Some("tab-1"));
        // Clicking active tab again triggers reset
        app.activate_tab_in_active_space("tab-1");
    });
    view.read_with(cx, |app, _| {
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-1")
            .unwrap();
        assert_eq!(tab.url, "https://pinned.example");
    });
}

#[gpui::test]
fn switching_back_to_pinned_tab_preserves_current_url(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.active_space_mut().tabs[0].url = "https://youtube.com".into();
        app.active_space_mut().tabs[0].title = "YouTube".into();
        app.pin_tab("tab-1", true);
        app.active_space_mut().tabs[0].url = "https://youtube.com/watch?v=123".into();
        app.active_space_mut().tabs[0].title = "Playing Video".into();
        app.add_tab();
        let second_tab_id = app.active_space().active.clone().unwrap();
        assert_ne!(second_tab_id, "tab-1");
        // Switching back to the pinned tab from another tab must preserve its current URL.
        app.activate_tab_in_active_space("tab-1");
    });
    view.read_with(cx, |app, _| {
        assert_eq!(app.active_space().active.as_deref(), Some("tab-1"));
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-1")
            .unwrap();
        assert_eq!(tab.url, "https://youtube.com/watch?v=123");
        assert_eq!(tab.title, "Playing Video");
    });
}

#[gpui::test]
fn closing_an_empty_tab_does_not_trigger_closing_ghost_animation(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        // Tab-1 is about:blank
        assert!(app.close_tab_by_id("tab-1"));
        assert!(
            app.closing_tab.is_none(),
            "closing an empty blank tab must not set closing_tab"
        );

        // Now set a real url on the new tab
        let active_id = app.active_tab_id().to_string();
        app.active_space_mut()
            .tabs
            .iter_mut()
            .find(|t| t.id == active_id)
            .unwrap()
            .url = "https://example.com".into();
        assert!(app.close_tab_by_id(&active_id));
        assert!(
            app.closing_tab.is_some(),
            "closing a real tab must record closing_tab"
        );
    });
}

#[gpui::test]
fn closing_last_regular_tab_keeps_favorite_unloaded_and_spawns_new_tab(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        let space = app.active_space_mut();
        space.tabs.clear();
        let mut fav = Tab::new("fav-1", "Google", "https://google.com");
        fav.favorite = true;
        space.tabs.push(fav);
        let reg = Tab::new("reg-1", "New tab", "about:blank");
        space.tabs.push(reg);
        space.active = Some("reg-1".to_string());

        app.close_active_tab();

        let active_space = app.active_space();
        assert_eq!(active_space.tabs.len(), 2);
        assert_eq!(active_space.tabs[0].id, "fav-1");
        assert_ne!(active_space.active.as_deref(), Some("fav-1"));
        assert!(!active_space.tabs[1].favorite);
        assert_eq!(active_space.tabs[1].url, "about:blank");
    });
}

#[gpui::test]
fn download_toast_and_button_active_when_downloading(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, cx| {
        let theme = crate::ui::visual::theme_for_space(app.active_space());
        assert!(app.engine.downloads().is_empty());
        let initial_overlays = app.render_context_menus(theme, &[], cx);
        assert_eq!(initial_overlays.len(), 0);

        // Simulate an active download starting
        app.engine
            .push_download_test(crate::engine::DownloadRecord {
                id: 101,
                url: "https://example.com/archive.tar.gz".into(),
                path: std::path::PathBuf::from("/home/rover/Downloads/archive.tar.gz"),
                received_bytes: 512,
                total_bytes: 4096,
                complete: false,
                canceled: false,
            });

        assert_eq!(app.engine.downloads().len(), 1);
        let active = app
            .engine
            .downloads()
            .iter()
            .any(|d| !d.complete && !d.canceled);
        assert!(active);

        // Context menus should now render the download toast overlay
        let overlays = app.render_context_menus(theme, &[], cx);
        assert_eq!(
            overlays.len(),
            1,
            "download toast overlay should be rendered when downloading"
        );

        // Dismissing the toast should prevent it from rendering
        app.dismissed_download_toast_id = Some(101);
        let overlays_after_dismiss = app.render_context_menus(theme, &[], cx);
        assert_eq!(
            overlays_after_dismiss.len(),
            0,
            "dismissed download toast must not render"
        );

        // The toolbar icon opens a compact bubble instead of the full page.
        crate::plugins::set_downloads_enabled_for_test(true);
        app.dismissed_download_toast_id = None;
        app.downloads_bubble_open = true;
        let overlays_with_bubble = app.render_context_menus(theme, &[], cx);
        assert_eq!(
            overlays_with_bubble.len(),
            1,
            "recent-download bubble must render and replace the toast"
        );
        app.downloads_bubble_open = false;
        app.dismissed_download_toast_id = Some(101);
        // The bubble plays a short close animation; once it finishes, neither
        // the bubble nor the dismissed toast should render.
        app.downloads_bubble_was_open = false;
        app.downloads_bubble_closing = None;
        assert_eq!(app.render_context_menus(theme, &[], cx).len(), 0);
        crate::plugins::set_downloads_enabled_for_test(false);
    });
}

#[gpui::test]
fn closing_a_split_member_exits_split_and_selects_its_partner(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        assert!(app.split_tabs("tab-1", "tab-2"));
        assert!(app.close_tab_by_id("tab-1"));
    });
    view.read_with(cx, |app, _| {
        assert!(app.current_split_pair().is_none());
        assert_eq!(app.active_tab_id(), "tab-2");
    });
}

#[gpui::test]
fn split_with_context_flow_uses_the_clicked_tab_and_selected_target(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.tab_context_menu = Some(TabContextMenu {
            tab_id: "tab-1".into(),
            showing_split_targets: true,
            x: 8.0 * UI_SCALE,
            y: 8.0 * UI_SCALE,
            width: 240.0 * UI_SCALE,
            height: 520.0 * UI_SCALE,
        });
        assert!(app.choose_split_target("tab-2"));
        assert!(app.tab_context_menu.is_none());
    });
    view.read_with(cx, |app, _| {
        assert_eq!(
            app.current_split_pair(),
            Some(SplitPair::new("tab-1".into(), "tab-2".into()))
        );
    });
}

#[gpui::test]
fn kept_and_folder_sections_are_exclusive(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();

        app.favorite_tab("tab-2", true);
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .unwrap();
        assert!(tab.favorite);
        assert!(tab.pinned);
        assert!(tab.folder.is_none());

        app.pin_tab("tab-2", true);
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .unwrap();
        assert!(tab.pinned);
        assert!(tab.favorite);

        app.favorite_tab("tab-2", false);
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .unwrap();
        assert!(!tab.pinned);
        assert!(!tab.favorite);

        app.favorite_tab("tab-2", true);
        let folder_id = app.create_folder("Work", Some("tab-2")).unwrap();
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .unwrap();
        assert_eq!(tab.folder.as_deref(), Some(folder_id.as_str()));
        assert!(!tab.pinned);
        assert!(!tab.favorite);
        assert_eq!(app.active_space().folders.len(), 1);
        assert!(app.active_space().folders[0].expanded);

        app.delete_folder(&folder_id);
        assert!(app.active_space().folders.is_empty());
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .unwrap();
        assert!(tab.folder.is_none());
    });
}

#[gpui::test]
fn dropping_a_regular_tab_on_a_kept_tab_joins_the_same_row(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.pin_tab("tab-1", true);
        // Drag tab-2 onto tab-1 (drop before the target).
        app.drop_tab_in_target_section("tab-2", "tab-1", false);
    });
    view.read_with(cx, |app, _| {
        let tabs = &app.active_space().tabs;
        assert_eq!(tabs.len(), 2);
        assert!(tabs.iter().all(|tab| tab.is_pinned()));
        assert_eq!(tabs[0].id, "tab-2");
        assert_eq!(tabs[1].id, "tab-1");
        assert_eq!(tabs[0].kept_group.as_deref(), Some("tab-1"));
        assert_eq!(tabs[1].kept_group.as_deref(), Some("tab-1"));
    });
}

#[gpui::test]
fn dropping_on_a_slot_beside_an_ungrouped_tab_forms_a_two_tab_group(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.pin_tab("tab-1", true);
        // The hover slot beside an ungrouped tab carries group = None. Dropping
        // tab-2 there must still create a group shared by both tabs.
        app.drop_kept_tab_in_slot("tab-2", "tab-1", None, false);
    });
    view.read_with(cx, |app, _| {
        let tabs = &app.active_space().tabs;
        assert_eq!(tabs.len(), 2);
        assert!(tabs.iter().all(|tab| tab.is_pinned()));
        assert!(tabs
            .iter()
            .all(|tab| tab.kept_group.as_deref() == Some("tab-1")));
    });
}

#[gpui::test]
fn dropping_on_the_lower_strip_keeps_a_single_web_per_row(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.pin_tab("tab-1", true);
        // First drop joins the group via the box body.
        app.drop_tab_in_target_section("tab-2", "tab-1", false);
        app.add_tab();
        // Second drop on the lower strip gives tab-3 its own full-width row.
        app.drop_tab_in_target_section("tab-3", "tab-1", true);
    });
    view.read_with(cx, |app, _| {
        let tabs = &app.active_space().tabs;
        assert!(tabs.iter().all(|tab| tab.is_pinned()));
        let tab_1 = tabs.iter().find(|tab| tab.id == "tab-1").unwrap();
        let tab_2 = tabs.iter().find(|tab| tab.id == "tab-2").unwrap();
        let tab_3 = tabs.iter().find(|tab| tab.id == "tab-3").unwrap();
        assert_eq!(tab_1.kept_group.as_deref(), Some("tab-1"));
        assert_eq!(tab_2.kept_group.as_deref(), Some("tab-1"));
        assert_eq!(tab_3.kept_group, None);
    });
}

#[gpui::test]
fn detaching_a_grouped_tab_keeps_it_pinned_in_its_own_row(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.add_tab();
        app.pin_tab("tab-1", true);
        app.drop_tab_in_target_section("tab-2", "tab-1", false);
        // Drop on the section background / use "Move to own row".
        app.set_kept_group("tab-2", None);
    });
    view.read_with(cx, |app, _| {
        let tabs = &app.active_space().tabs;
        let detached = tabs.iter().find(|tab| tab.id == "tab-2").unwrap();
        assert!(detached.is_pinned());
        assert_eq!(detached.kept_group, None);
    });
}

#[gpui::test]
fn find_shortcut_opens_and_escape_closes_find(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    dispatch_test_key(cx, handle, "ctrl-f");
    view.read_with(cx, |app, _| assert!(app.show_find));
    dispatch_test_key(cx, handle, "escape");
    view.read_with(cx, |app, _| assert!(!app.show_find));
}

#[gpui::test]
fn command_palette_opens_with_ctrl_k_and_closes_with_escape(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    dispatch_test_key(cx, handle, "ctrl-k");
    view.read_with(cx, |app, _| assert!(app.show_command_palette));
    dispatch_test_key(cx, handle, "down");
    view.read_with(cx, |app, _| assert_eq!(app.selected_command, 1));
    dispatch_test_key(cx, handle, "up");
    view.read_with(cx, |app, _| assert_eq!(app.selected_command, 0));
    dispatch_test_key(cx, handle, "escape");
    view.read_with(cx, |app, _| assert!(!app.show_command_palette));
}

#[gpui::test]
fn command_palette_enter_runs_the_matching_action(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    dispatch_test_key(cx, handle, "ctrl-k");
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |app, cx| {
            app.command_input
                .update(cx, |input, cx| input.set_value("New tab", window, cx));
        });
    })
    .unwrap();
    dispatch_test_key(cx, handle, "enter");
    view.read_with(cx, |app, _| {
        assert_eq!(app.active_tab_id(), "tab-2");
        assert!(!app.show_command_palette);
    });
}

#[gpui::test]
fn browser_tab_cycle_and_number_shortcuts_select_the_expected_tab(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = test_app(window, cx);
                app.add_tab();
                app.add_tab();
                app
            })
        })
        .expect("open test window")
    });
    dispatch_test_key(cx, handle, "ctrl-1");
    view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-1"));
    dispatch_test_key(cx, handle, "ctrl-tab");
    view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-2"));
    dispatch_test_key(cx, handle, "ctrl-shift-tab");
    view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-1"));
}

#[gpui::test]
fn new_tab_clears_and_focuses_the_address_bar(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    for key in ["g", "i", "t"] {
        dispatch_test_key(cx, handle, key);
    }
    dispatch_test_key(cx, handle, "ctrl-t");
    view.read_with(cx, |app, cx| {
        assert_eq!(app.active_tab_id(), "tab-2");
        assert_eq!(app.address.read(cx).value().to_string(), "");
        assert!(!app.show_suggestions);
    });
    cx.update_window(handle, |_, window, cx| {
        view.read_with(cx, |app, cx| {
            assert_eq!(
                window.focused(cx),
                Some(app.address.read(cx).focus_handle(cx))
            );
        });
    })
    .unwrap();
}

#[gpui::test]
fn tab_switch_syncs_address_to_the_active_tab(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = test_app(window, cx);
                app.add_tab();
                app.add_tab();
                app.active_space_mut().tabs[0].url = "https://one.example".into();
                app.active_space_mut().tabs[1].url = "https://two.example".into();
                app.active_space_mut().active = Some("tab-1".into());
                app
            })
        })
        .expect("open test window")
    });
    dispatch_test_key(cx, handle, "ctrl-2");
    view.read_with(cx, |app, cx| {
        assert_eq!(app.active_tab_id(), "tab-2");
        assert_eq!(
            app.address.read(cx).value().to_string(),
            "https://two.example"
        );
    });
    dispatch_test_key(cx, handle, "ctrl-1");
    view.read_with(cx, |app, cx| {
        assert_eq!(
            app.address.read(cx).value().to_string(),
            "https://one.example"
        );
    });
    view.update(cx, |app, cx| {
        let mut other = Space::new("other", "Other", "code", 0x123456);
        other
            .tabs
            .push(Tab::new("tab-1", "Other page", "https://other.example"));
        other.active = Some("tab-1".into());
        app.spaces.push(other);
        app.activate_space("other");
        cx.notify();
    });
    view.read_with(cx, |app, cx| {
        assert_eq!(
            app.address.read(cx).value().to_string(),
            "https://other.example"
        );
    });
}

#[gpui::test]
fn active_tab_navigation_updates_address_bar(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = test_app(window, cx);
                app.active_space_mut().tabs[0].url = "https://initial.example".into();
                app
            })
        })
        .expect("open test window")
    });

    view.update(cx, |app, cx| {
        app.active_space_mut().tabs[0].url = "https://navigated.example".into();
        cx.notify();
    });

    view.read_with(cx, |app, cx| {
        assert_eq!(
            app.address.read(cx).value().to_string(),
            "https://navigated.example"
        );
    });
}

#[gpui::test]
fn new_tab_temporarily_leaves_a_split_and_address_targets_it(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = test_app(window, cx);
                app.add_tab();
                app.active_space_mut().tabs[0].url = "https://a.example".into();
                app.active_space_mut().tabs[1].url = "https://b.example".into();
                assert!(app.split_tabs("tab-1", "tab-2"));
                app
            })
        })
        .expect("open test window")
    });

    dispatch_test_key(cx, handle, "ctrl-t");
    view.read_with(cx, |app, _| {
        assert_eq!(app.active_tab_id(), "tab-3");
        assert!(app.current_split_pair().is_none());
        assert_eq!(
            app.stored_split_pair()
                .map(|pair| (pair.first, pair.second)),
            Some(("tab-1".into(), "tab-2".into()))
        );
    });
    dispatch_test_key(cx, handle, "x");
    view.read_with(cx, |app, cx| {
        assert_eq!(app.address.read(cx).value().to_string(), "x");
    });
    dispatch_test_key(cx, handle, "enter");
    view.read_with(cx, |app, _| {
        assert_eq!(app.active_tab_id(), "tab-3");
        assert_eq!(app.active_space().tabs[2].url, "https://x.com");
        assert!(app.current_split_pair().is_none());
        assert!(app.stored_split_pair().is_some());
    });

    view.update(cx, |app, cx| {
        app.activate_split_group();
        cx.notify();
    });
    view.read_with(cx, |app, _| {
        assert_eq!(app.active_tab_id(), "tab-1");
        assert!(app.current_split_pair().is_some());
        assert!(app.stored_split_pair().is_some());
    });
    view.update(cx, |app, cx| {
        app.active_space_mut().active = Some("tab-3".into());
        app.set_focused_pane(1);
        cx.notify();
    });
    view.read_with(cx, |app, _| {
        assert_eq!(app.active_tab_id(), "tab-2");
        assert!(app.current_split_pair().is_some());
    });
}

#[gpui::test]
fn autocomplete_arrows_select_a_suggestion_and_enter_opens_it(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, cx| {
        app.update_suggestions("example.com");
        cx.notify();
    });

    dispatch_test_key(cx, handle, "down");
    view.read_with(cx, |app, _| assert_eq!(app.selected_suggestion, Some(0)));
    dispatch_test_key(cx, handle, "down");
    view.read_with(cx, |app, _| assert_eq!(app.selected_suggestion, Some(1)));
    dispatch_test_key(cx, handle, "up");
    view.read_with(cx, |app, _| assert_eq!(app.selected_suggestion, Some(0)));
    dispatch_test_key(cx, handle, "down");
    dispatch_test_key(cx, handle, "enter");
    view.read_with(cx, |app, _| {
        assert_eq!(app.active_space().tabs[0].url, "https://example.com");
        assert!(!app.show_suggestions);
    });
}

#[gpui::test]
fn typing_in_the_address_bar_opens_autocomplete(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });

    for key in ["y", "o", "u"] {
        dispatch_test_key(cx, handle, key);
    }

    view.read_with(cx, |app, cx| {
        assert_eq!(app.address.read(cx).value().to_string(), "you");
        assert!(app.show_suggestions);
        assert!(app.suggestions.iter().any(|suggestion| {
            suggestion.title.contains("YouTube") && suggestion.target == "https://youtube.com"
        }));
    });
}

#[gpui::test]
fn pressing_enter_after_typing_search_navigates(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });

    for key in ["y", "o", "u"] {
        dispatch_test_key(cx, handle, key);
    }
    dispatch_test_key(cx, handle, "enter");

    view.read_with(cx, |app, _cx| {
        assert_eq!(
            app.active_space().tabs[0].url,
            "https://www.google.com/search?q=you"
        );
        assert!(!app.show_suggestions);
    });
}

#[gpui::test]
fn pressing_enter_with_popular_keyword_navigates_to_domain(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });

    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |app, cx| {
            app.address.update(cx, |input, cx| {
                input.set_value("youtube", window, cx);
            });
        });
    })
    .unwrap();
    dispatch_test_key(cx, handle, "enter");

    view.read_with(cx, |app, _cx| {
        assert_eq!(app.active_space().tabs[0].url, "https://youtube.com");
    });
}

#[gpui::test]
fn focusing_web_content_moves_focus_off_the_address_bar(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, cx| {
        app.update_suggestions("youtube");
        app.address_editing = true;
        cx.notify();
    });

    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |app, cx| app.focus_browser(window, cx));
        view.read_with(cx, |app, cx| {
            assert!(app.browser_focused);
            assert!(!app.show_suggestions);
            assert!(!app.address_editing);
            assert_eq!(window.focused(cx), Some(app.browser_focus_handle.clone()));
            assert_ne!(
                window.focused(cx),
                Some(app.address.read(cx).focus_handle(cx))
            );
        });
    })
    .unwrap();
}

#[gpui::test]
fn recent_searches_appear_when_the_address_bar_is_empty(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, cx| {
        app.recent_searches = vec!["lumen browser shortcuts".to_string()];
        app.update_suggestions("");
        cx.notify();
    });
    view.read_with(cx, |app, _| {
        assert_eq!(app.suggestions.len(), 1);
        assert_eq!(app.suggestions[0].title, "lumen browser shortcuts");
    });
    dispatch_test_key(cx, handle, "down");
    dispatch_test_key(cx, handle, "enter");
    view.read_with(cx, |app, _| {
        assert_eq!(
            app.active_space().tabs[0].url,
            "https://www.google.com/search?q=lumen+browser+shortcuts"
        );
    });
}

#[test]
fn test_all_assets_has_all_space_icons() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for &name in SPACE_ICONS {
        let icon_name = match name {
            "globe" => IconName::Globe,
            "compass" => IconName::Compass,
            "code" => IconName::Code,
            "terminal" => IconName::Terminal,
            "folder" => IconName::Folder,
            "book" => IconName::BookOpen,
            "briefcase" => IconName::Briefcase,
            "layers" => IconName::Layers,
            "box" => IconName::Box,
            "sparkles" => IconName::Sparkles,
            "star" => IconName::Star,
            "heart" => IconName::Heart,
            "bookmark" => IconName::Bookmark,
            "music" => IconName::Music,
            "gamepad" => IconName::Gamepad2,
            "cpu" => IconName::Cpu,
            "zap" => IconName::Zap,
            "hash" => IconName::Hash,
            "shield" => IconName::Shield,
            "palette" => IconName::Palette,
            "rocket" => IconName::Rocket,
            "bell" => IconName::Bell,
            _ => IconName::Compass,
        };
        let path = icon_name.path();
        assert!(
            gpui_kit::assets::AllAssets::get(&path).is_some(),
            "Icon {name} with path {path} not found in AllAssets"
        );
    }
}

#[test]
fn space_has_icon_and_isolated_identity() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let space1 = Space::new("s1", "Personal", "compass", 0x9aa6ff);
    let space2 = Space::new("s2", "Work", "briefcase", 0x76d7b2);
    let space3 = Space::new("s3", "Fun", "gamepad", 0xf6ad55);

    assert_eq!(space1.icon, "compass");
    assert_eq!(space2.icon, "briefcase");
    assert_eq!(space3.icon, "gamepad");
}

#[test]
fn default_space_has_no_hardcoded_secondary_spaces() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (spaces, active) = create_default_space();
    assert_eq!(spaces.len(), 1);
    assert_eq!(spaces[0].id, "personal");
    assert_eq!(spaces[0].icon, "compass");
    assert_eq!(spaces[0].tabs[0].url, "about:blank");
    assert_eq!(spaces[0].tabs[0].title, "New tab");
    assert_eq!(active, "personal");
    assert!(!spaces.iter().any(|s| s.id == "work"));
}

#[test]
fn svg_icon_palette_has_plenty_of_distinct_choices() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert!(SPACE_ICONS.len() >= 20);
    let mut set = std::collections::HashSet::new();
    for &icon in SPACE_ICONS {
        assert!(set.insert(icon), "Duplicate SVG icon found: {}", icon);
    }
}

#[test]
fn space_creation_and_deletion_invariants() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (mut spaces, mut active) = create_default_space();
    assert_eq!(spaces.len(), 1);
    assert_eq!(active, "personal");

    // Add second space
    let mut dev_space = Space::new("dev", "Dev Workspace", "code", 0x63b3ed);
    dev_space
        .tabs
        .push(Tab::new("tab-dev-1", "GitHub", "https://github.com"));
    dev_space.active = Some("tab-dev-1".into());
    spaces.push(dev_space);
    active = "dev".into();
    assert_eq!(spaces.len(), 2);
    assert_eq!(active, "dev");

    // Delete active space: active should fallback to remaining space
    let delete_idx = spaces.iter().position(|s| s.id == "dev").unwrap();
    spaces.remove(delete_idx);
    active = spaces[0].id.clone();
    assert_eq!(spaces.len(), 1);
    assert_eq!(active, "personal");

    // Single space cannot be deleted
    assert_eq!(spaces.len(), 1);
}

#[test]
fn sidebar_width_has_minimum_and_maximum_clamp() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let requested_shrink = 50.0_f32;
    let clamped = requested_shrink.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
    assert_eq!(clamped, SIDEBAR_MIN_WIDTH);

    let requested_grow = 500.0_f32;
    let clamped_grow = requested_grow.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
    assert_eq!(clamped_grow, SIDEBAR_MAX_WIDTH);

    let mid = 240.0_f32;
    assert_eq!(mid.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH), 240.0);
}

#[test]
fn sidebar_width_depends_only_on_manual_collapse_state() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert_eq!(sidebar_width_for(false, 240.0), 240.0);
    assert_eq!(sidebar_width_for(true, 240.0), SIDEBAR_COLLAPSED_WIDTH);
}

#[gpui::test]
fn theme_picker_grain_drag_and_outside_dismiss_use_rendered_bounds(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |app, cx| {
            app.open_theme_editor(gpui_kit::point(px(40.0), px(40.0)), false, window, cx)
        });
        window.draw(cx).clear(cx);
        let area = view.read(cx).theme_editor.as_ref().unwrap().bounds.get()[2];
        assert!(area.size.width > px(0.0) && area.size.height > px(0.0));
        let start = area.center();
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                position: start,
                button: gpui_kit::MouseButton::Left,
                ..Default::default()
            }),
            cx,
        );
        assert!(view.read(cx).theme_editor.as_ref().unwrap().dragging);
        window.draw(cx).clear(cx);
        let end = start + gpui_kit::point(px(30.0), px(-20.0));
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: end,
                pressed_button: Some(gpui_kit::MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert!(view.read(cx).active_space().theme_grain > 0.2);
        window.draw(cx).clear(cx);
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                position: end,
                button: gpui_kit::MouseButton::Left,
                ..Default::default()
            }),
            cx,
        );
        assert!(!view.read(cx).theme_editor.as_ref().unwrap().dragging);
        window.draw(cx).clear(cx);
        // Both controls use the same horizontal track geometry and retain capture outside it.
        for control in [1, 2] {
            let area = view.read(cx).theme_editor.as_ref().unwrap().bounds.get()[control];
            window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    position: area.center(),
                    button: gpui_kit::MouseButton::Left,
                    ..Default::default()
                }),
                cx,
            );
            window.draw(cx).clear(cx);
            let end = area.origin + gpui_kit::point(area.size.width + px(80.0), px(-40.0));
            window.dispatch_event(
                gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                    position: end,
                    pressed_button: Some(gpui_kit::MouseButton::Left),
                    ..Default::default()
                }),
                cx,
            );
            let space = view.read(cx).active_space();
            assert_eq!(
                if control == 1 {
                    space.theme_intensity / 0.65
                } else {
                    space.theme_grain
                },
                1.0
            );
            window.draw(cx).clear(cx);
            window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    position: area.origin - gpui_kit::point(px(80.0), px(40.0)),
                    button: gpui_kit::MouseButton::Left,
                    ..Default::default()
                }),
                cx,
            );
            assert!(!view.read(cx).theme_editor.as_ref().unwrap().dragging);
            let space = view.read(cx).active_space();
            assert_eq!(
                if control == 1 {
                    space.theme_intensity / 0.65
                } else {
                    space.theme_grain
                },
                0.0
            );
            window.draw(cx).clear(cx);
        }
        let pad = view.read(cx).theme_editor.as_ref().unwrap().bounds.get()[0];
        let center = pad.center();
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                position: center,
                button: gpui_kit::MouseButton::Left,
                ..Default::default()
            }),
            cx,
        );
        window.draw(cx).clear(cx);
        let destination = center + gpui_kit::point(px(25.0), px(15.0));
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: destination,
                pressed_button: Some(gpui_kit::MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let (_, x, y) = view
            .read(cx)
            .theme_editor
            .as_ref()
            .unwrap()
            .pointer_position
            .unwrap();
        assert!(x > 0.5 && y > 0.5, "dot tracks pointer continuously");
        window.draw(cx).clear(cx);
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                position: destination,
                button: gpui_kit::MouseButton::Left,
                ..Default::default()
            }),
            cx,
        );
        window.draw(cx).clear(cx);
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                position: gpui_kit::point(px(2.0), px(2.0)),
                button: gpui_kit::MouseButton::Left,
                ..Default::default()
            }),
            cx,
        );
        assert!(view.read(cx).theme_editor.is_none());
    })
    .unwrap();
}

#[gpui::test]
fn closing_a_tab_in_non_active_space_succeeds_and_updates_model(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        // Create a secondary space with a tab
        let mut work_space = Space::new("work", "Work", "briefcase", 0x4f46e5);
        let tab = Tab::new("work-tab-1", "Work Tab", "https://example.com");
        work_space.tabs.push(tab);
        work_space.active = Some("work-tab-1".to_string());
        app.spaces.push(work_space);

        // Keep active space as "personal"
        assert_eq!(app.active_space, "personal");

        // Close the tab in the "work" space
        assert!(app.close_tab_by_id("work-tab-1"));

        // Verify the tab was removed from the work space
        let work = app.spaces.iter().find(|s| s.id == "work").unwrap();
        assert_eq!(work.tabs.len(), 1); // space.tabs replaced with new about:blank tab when empty
        assert_ne!(work.tabs[0].id, "work-tab-1");
    });
}

#[test]
fn take_close_requests_drains_queued_tabs() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let engine = engine::Engine::new();
    assert!(engine.take_close_requests().is_empty());
}

#[gpui::test]
fn open_popup_tab_shows_a_peek_overlay_instead_of_a_new_tab(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        let tabs_before = app.active_space().tabs.len();
        app.open_popup_tab("personal", "https://accounts.example.com/oauth".to_string());
        assert_eq!(app.active_space, "personal");
        assert_eq!(app.active_space().tabs.len(), tabs_before);
        let peek = app.peek.as_ref().expect("popup opens a peek overlay");
        assert_eq!(peek.url, "https://accounts.example.com/oauth");
        assert!(peek.id.starts_with("personal-peek-"));
        assert_eq!(app.active_tab_id(), "tab-1");
    });
}

#[gpui::test]
fn peek_promotes_to_a_live_tab_keeping_its_browser_id(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.open_popup_tab("personal", "https://example.com/docs".to_string());
        let peek_id = app.peek.as_ref().expect("peek open").id.clone();
        app.open_peek_in_tab();
        assert!(app.peek.is_none());
        assert_eq!(app.active_tab_id(), peek_id);
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == peek_id)
            .expect("promoted tab exists");
        assert_eq!(tab.url, "https://example.com/docs");
        assert_eq!(tab.state, TabState::Live);
    });
}

#[gpui::test]
fn closing_a_peek_removes_it_and_restores_the_active_tab(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.open_popup_tab("personal", "https://example.com".to_string());
        assert!(app.peek.is_some());
        app.close_peek();
        assert!(app.peek.is_none());
        assert_eq!(app.active_tab_id(), "tab-1");
        assert_eq!(app.active_space().tabs.len(), 1);
    });
}

#[gpui::test]
fn address_bar_select_all_then_paste_replaces_the_text(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |app, cx| {
            app.address.update(cx, |input, cx| {
                input.set_value("https://example.com/old", window, cx);
                input.focus(window, cx);
            });
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                "https://pasted.example/new".to_string(),
            ));
        });
    })
    .unwrap();
    dispatch_test_key(cx, handle, "ctrl-a");
    dispatch_test_key(cx, handle, "ctrl-v");
    cx.run_until_parked();
    view.read_with(cx, |app, cx| {
        assert_eq!(app.address.read(cx).value(), "https://pasted.example/new");
    });
}

#[gpui::test]
fn sign_in_popups_show_their_cef_window_in_the_peek_overlay(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        let tabs_before = app.active_space().tabs.len();
        app.show_popup_window(
            "personal",
            "https://accounts.google.com/v3/signin/accountchooser?client_id=abc&redirect_uri=https%3A%2F%2Fclaude.ai"
                .to_string(),
            "personal-popup-7".to_string(),
        );
        assert_eq!(app.active_space, "personal");
        assert_eq!(app.active_space().tabs.len(), tabs_before);
        let peek = app.peek.as_ref().expect("popup window shows in the peek overlay");
        assert_eq!(peek.id, "personal-popup-7");
        assert_eq!(
            peek.url,
            "https://accounts.google.com/v3/signin/accountchooser?client_id=abc&redirect_uri=https%3A%2F%2Fclaude.ai"
        );
        assert_eq!(app.active_tab_id(), "tab-1");
    });
}

#[gpui::test]
fn page_closing_its_popup_window_only_dismisses_the_peek(cx: &mut gpui::TestAppContext) {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        let tabs_before = app.active_space().tabs.len();
        app.show_popup_window(
            "personal",
            "https://accounts.google.com/signin".to_string(),
            "personal-popup-8".to_string(),
        );
        app.close_peek_from_page();
        assert!(app.peek.is_none());
        assert_eq!(app.active_space().tabs.len(), tabs_before);
        assert_eq!(app.active_tab_id(), "tab-1");
    });
}

#[test]
fn single_line_paste_drops_line_breaks_and_empty_clipboards() {
    let _global_guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert_eq!(
        single_line_paste("https://example.com/a\n").as_deref(),
        Some("https://example.com/a")
    );
    assert_eq!(single_line_paste("").as_deref(), None);
    assert_eq!(single_line_paste("\r\n").as_deref(), None);
}
