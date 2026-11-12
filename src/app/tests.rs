use super::actions::single_line_paste;
use super::*;
use gpui_kit::{Focusable, WindowOptions};

fn test_app(window: &mut Window, cx: &mut Context<Lumen>) -> Lumen {
    let address = cx.new(|cx| InputState::new(window, cx));
    address.update(cx, |input, cx| input.focus(window, cx));
    let address_subscription =
        cx.subscribe(&address, |app: &mut Lumen, _, event: &InputEvent, cx| {
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
    let command_subscription = cx.subscribe(&command_input, |this: &mut Lumen, _, event, cx| {
        if matches!(event, InputEvent::Change) {
            this.selected_command = 0;
            cx.notify();
        }
    });
    let (spaces, active_space) = create_default_space();
    let panel_registry = crate::plugin::new_panel_registry();
    let mut engine = engine::Engine::new();
    engine.set_panel_registry(panel_registry.clone());
    Lumen {
        spaces,
        active_space,
        sidebar_collapsed: false,
        sidebar_width: 240.0 * UI_SCALE,
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
        first_frame_logged: true,
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
        recent_searches: Vec::new(),
        zoom_levels: std::collections::HashMap::new(),
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
        plugins_menu_open: false,
        dismissed_download_toast_id: None,
        settings_section: SettingsSection::default(),
        preferences: Default::default(),
        plugins: {
            let mut plugins = crate::plugin::PluginHost::new(Default::default());
            plugins.mount_core().expect("core plugin mounts");
            crate::plugins::register(&mut plugins);
            plugins.mount_enabled().expect("built-ins mount");
            plugins
        },
        panel_registry,
        pending_plugin_review: None,
        plugin_action_error: None,
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

#[gpui::test]
fn split_shortcuts_create_focus_and_exit_the_pane_pair(cx: &mut gpui::TestAppContext) {
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
            Some(SplitPair {
                first: "tab-1".into(),
                second: "tab-2".into(),
            })
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
fn closing_a_non_active_tab_by_id_preserves_the_active_tab(cx: &mut gpui::TestAppContext) {
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
    let (_, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, _| {
        app.active_space_mut().tabs[0].pinned = true;
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
fn unloading_a_pinned_tab_restores_its_pinned_destination(cx: &mut gpui::TestAppContext) {
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
        assert!(tab.favorite);
        assert_eq!(tab.url, "https://favorite.example/home");
        assert_eq!(tab.title, "Favorite Home");
    });
}

#[test]
fn legacy_kept_tabs_adopt_their_saved_url_and_keep_it_after_navigation() {
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
fn transitioning_between_favorite_and_pinned_preserves_pinned_url(cx: &mut gpui::TestAppContext) {
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
fn closing_an_empty_tab_does_not_trigger_closing_ghost_animation(cx: &mut gpui::TestAppContext) {
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
fn download_toast_and_button_active_when_downloading(cx: &mut gpui::TestAppContext) {
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
    });
}

#[gpui::test]
fn closing_a_split_member_exits_split_and_selects_its_partner(cx: &mut gpui::TestAppContext) {
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
            Some(SplitPair {
                first: "tab-1".into(),
                second: "tab-2".into(),
            })
        );
    });
}

#[gpui::test]
fn favorite_pin_and_folder_sections_are_exclusive(cx: &mut gpui::TestAppContext) {
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
        assert!(!tab.pinned);
        assert!(tab.folder.is_none());

        app.pin_tab("tab-2", true);
        let tab = app
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == "tab-2")
            .unwrap();
        assert!(tab.pinned);
        assert!(!tab.favorite);

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
fn find_shortcut_opens_and_escape_closes_find(cx: &mut gpui::TestAppContext) {
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
fn focusing_web_content_moves_focus_off_the_address_bar(cx: &mut gpui::TestAppContext) {
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| test_app(window, cx))
        })
        .expect("open test window")
    });
    view.update(cx, |app, cx| {
        app.update_suggestions("youtube");
        cx.notify();
    });

    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |app, cx| app.focus_browser(window, cx));
        view.read_with(cx, |app, cx| {
            assert!(app.browser_focused);
            assert!(!app.show_suggestions);
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
    let space1 = Space::new("s1", "Personal", "compass", 0x9aa6ff);
    let space2 = Space::new("s2", "Work", "briefcase", 0x76d7b2);
    let space3 = Space::new("s3", "Fun", "gamepad", 0xf6ad55);

    assert_eq!(space1.icon, "compass");
    assert_eq!(space2.icon, "briefcase");
    assert_eq!(space3.icon, "gamepad");
}

#[test]
fn default_space_has_no_hardcoded_secondary_spaces() {
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
    assert!(SPACE_ICONS.len() >= 20);
    let mut set = std::collections::HashSet::new();
    for &icon in SPACE_ICONS {
        assert!(set.insert(icon), "Duplicate SVG icon found: {}", icon);
    }
}

#[test]
fn space_creation_and_deletion_invariants() {
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
    assert_eq!(sidebar_width_for(false, 240.0), 240.0);
    assert_eq!(sidebar_width_for(true, 240.0), SIDEBAR_COLLAPSED_WIDTH);
}

#[gpui::test]
fn theme_picker_grain_drag_and_outside_dismiss_use_rendered_bounds(cx: &mut gpui::TestAppContext) {
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
    let engine = engine::Engine::new();
    assert!(engine.take_close_requests().is_empty());
}

#[gpui::test]
fn open_popup_tab_shows_a_peek_overlay_instead_of_a_new_tab(cx: &mut gpui::TestAppContext) {
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
    assert_eq!(
        single_line_paste("https://example.com/a\n").as_deref(),
        Some("https://example.com/a")
    );
    assert_eq!(single_line_paste("").as_deref(), None);
    assert_eq!(single_line_paste("\r\n").as_deref(), None);
}
