use super::*;

pub(super) fn with_key_handler(root: gpui_kit::Div, cx: &mut Context<Anarc>) -> gpui_kit::Div {
    root.capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
        let key = event.keystroke.key.to_lowercase();
        let modifiers = &event.keystroke.modifiers;
        let browser_modifier = modifiers.control || modifiers.platform;
        let address_focused = window.focused(cx) == Some(this.address.read(cx).focus_handle(cx));
        if !event.is_held && browser_modifier && !modifiers.shift && key == "v" && address_focused {
            this.paste_into_address(window, cx);
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if !event.is_held && browser_modifier && !modifiers.shift && key == "w" {
            if this.app_page.is_some() {
                this.close_app_page();
            } else if this.peek.is_some() {
                this.close_peek();
            } else {
                this.close_active_tab();
            }
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !event.is_held && browser_modifier && !modifiers.shift && key == "t" {
            this.add_tab();
            this.address.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !event.is_held && matches!(key.as_str(), "escape" | "esc") && this.downloads_bubble_open
        {
            this.downloads_bubble_open = false;
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !event.is_held
            && matches!(key.as_str(), "escape" | "esc")
            && this.tab_context_menu.is_some()
        {
            this.tab_context_menu = None;
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if browser_modifier && key == "k" && !modifiers.shift && !event.is_held {
            this.open_command_palette(window, cx);
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !event.is_held
            && browser_modifier
            && modifiers.shift
            && key == "s"
            && crate::plugins::split_view_enabled()
        {
            this.split_active_with_next();
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !event.is_held
            && browser_modifier
            && modifiers.shift
            && key == "w"
            && crate::plugins::split_view_enabled()
        {
            this.exit_split();
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !event.is_held
            && modifiers.alt
            && browser_modifier
            && crate::plugins::split_view_enabled()
            && this.current_split_pair().is_some()
            && matches!(key.as_str(), "left" | "arrowleft" | "right" | "arrowright")
        {
            let pane_count = this
                .current_split_pair()
                .map_or(2, |pair| pair.pane_count());
            let pane_index = if matches!(key.as_str(), "left" | "arrowleft") {
                this.focused_pane.saturating_sub(1)
            } else {
                (this.focused_pane + 1).min(pane_count - 1)
            };
            this.set_focused_pane(pane_index);
            if let Some(pair) = this.current_split_pair() {
                for tab_id in pair.panes() {
                    this.engine.set_focus(tab_id, false);
                }
            }
            let focused = this.focused_tab_id().to_string();
            this.engine.set_focus(&focused, true);
            this.browser_focused = true;
            window.focus(&this.browser_focus_handle, cx);
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if this.show_command_palette {
            if event.is_held {
                return;
            }
            let query = this.command_input.read(cx).value().to_string();
            let items = this.command_items(&query);
            match key.as_str() {
                "escape" | "esc" => this.close_command_palette(window, cx),
                "up" | "arrowup" => this.move_command_selection(false, items.len()),
                "down" | "arrowdown" => this.move_command_selection(true, items.len()),
                "enter" | "return" => {
                    if let Some(item) = items.get(this.selected_command).cloned() {
                        this.execute_palette_item(item, window, cx);
                    }
                }
                _ => return,
            }
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if this.show_find && matches!(key.as_str(), "enter" | "return") {
            this.find_in_page(!modifiers.shift, true, cx);
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if !event.is_held
            && matches!(key.as_str(), "escape" | "esc")
            && this.peek.is_some()
            && !this.show_find
        {
            this.close_peek();
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if matches!(key.as_str(), "escape" | "esc") {
            if this.show_find {
                this.engine.stop_finding(this.focused_tab_id());
                this.show_find = false;
                this.focus_browser(window, cx);
                window.prevent_default();
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if this.engine.is_loading(this.focused_tab_id()) {
                this.engine.stop_load(this.focused_tab_id());
                window.prevent_default();
                cx.stop_propagation();
                return;
            }
        }
        if browser_modifier && key == "f" && !modifiers.shift {
            this.show_find = true;
            this.browser_focused = false;
            this.engine.set_focus(this.focused_tab_id(), false);
            this.find_input
                .update(cx, |input, cx| input.focus(window, cx));
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if modifiers.alt && matches!(key.as_str(), "left" | "arrowleft") {
            this.address_editing = false;
            this.show_suggestions = false;
            this.selected_suggestion = None;
            this.engine.go_back(this.focused_tab_id());
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if modifiers.alt && matches!(key.as_str(), "right" | "arrowright") {
            this.address_editing = false;
            this.show_suggestions = false;
            this.selected_suggestion = None;
            this.engine.go_forward(this.focused_tab_id());
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if browser_modifier && modifiers.alt && key == "r" && !modifiers.shift {
            if reader::is_plugin_enabled() {
                this.engine
                    .execute_script(this.active_tab_id(), reader::toggle_script());
            }
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        let suggestions_open = this.show_suggestions && !this.suggestions.is_empty();
        let is_suggestion_arrow =
            suggestions_open && matches!(key.as_str(), "up" | "down" | "arrowup" | "arrowdown");
        if event.is_held && !is_suggestion_arrow {
            return;
        }
        if is_suggestion_arrow {
            this.move_suggestion_selection(matches!(key.as_str(), "down" | "arrowdown"));
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
        } else if !event.is_held
            && matches!(
                key.as_str(),
                "enter" | "return" | "kp_enter" | "numpadenter"
            )
            && (address_focused || suggestions_open || this.address_editing)
        {
            if let Some(target) = this
                .selected_suggestion
                .and_then(|index| this.suggestions.get(index))
                .map(|item| item.target.clone())
            {
                this.navigate_to(&target, window, cx);
            } else {
                this.navigate(Some(window), cx);
            }
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
        } else if suggestions_open && matches!(key.as_str(), "escape" | "esc") {
            this.show_suggestions = false;
            this.selected_suggestion = None;
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
        } else if browser_modifier && key == "l" && !modifiers.shift {
            let tab_id = this.focused_tab_id().to_string();
            this.engine.set_focus(&tab_id, false);
            this.address.update(cx, |input, cx| {
                input.focus(window, cx);
                input.select_all(window, cx);
            });
            this.browser_focused = false;
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
        } else if browser_modifier && key == "s" && !modifiers.shift {
            this.animate_sidebar_toggle(window, cx);
            window.prevent_default();
            cx.stop_propagation();
        } else if this.handle_shortcut(&key, browser_modifier, modifiers.shift) {
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
        }
    }))
}

pub(super) fn with_pointer_handlers(
    root: gpui_kit::Div,
    cx: &mut Context<Anarc>,
    topbar_height: f32,
    sidebar_on_right: bool,
) -> gpui_kit::Div {
    root.on_mouse_move(
        cx.listener(move |this, event: &gpui_kit::MouseMoveEvent, window, cx| {
            if this.is_sidebar_collapsed()
                && this.preferences.sidebar_mode == crate::settings::SidebarMode::Hover
            {
                let window_width = f32::from(window.bounds().size.width);
                let pointer_x = f32::from(event.position.x);
                let reveal_width = if this.sidebar_hovered {
                    this.sidebar_width + 12.0 * UI_SCALE
                } else {
                    12.0 * UI_SCALE
                };
                let edge_distance = if sidebar_on_right {
                    window_width - pointer_x
                } else {
                    pointer_x
                };
                this.animate_sidebar_hover(
                    edge_distance >= 0.0 && edge_distance <= reveal_width,
                    window,
                    cx,
                );
            }
            if this.split_resizing || this.split_resizing_horizontal {
                let bounds = window.bounds();
                let sidebar = this.effective_sidebar_width();
                let card_left = if sidebar_on_right {
                    CONTENT_BEZEL_X
                } else {
                    sidebar + CONTENT_BEZEL_X
                };
                let card_width = f32::from(bounds.size.width) - sidebar - 2.0 * CONTENT_BEZEL_X;
                if this.split_resizing {
                    this.split_ratio = split_view::ratio_at_pointer(
                        f32::from(event.position.x),
                        card_left,
                        card_width,
                    );
                }
                if this.split_resizing_horizontal {
                    let card_height =
                        f32::from(bounds.size.height) - topbar_height - CONTENT_BEZEL_BOTTOM;
                    this.split_ratio_y = split_view::ratio_y_at_pointer(
                        f32::from(event.position.y),
                        topbar_height,
                        card_height,
                    );
                }
                cx.notify();
            }
            if this.sidebar_resizing {
                let new_width = if sidebar_on_right {
                    f32::from(window.bounds().size.width)
                        - f32::from(event.position.x)
                        - CONTENT_BEZEL_X
                } else {
                    f32::from(event.position.x) - CONTENT_BEZEL_X
                };
                this.sidebar_width = new_width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                cx.notify();
            }
        }),
    )
    .on_mouse_up(
        gpui_kit::MouseButton::Left,
        cx.listener(move |this, _, _window, cx| {
            #[cfg(not(test))]
            let finish_split_resize = this.split_resizing || this.split_resizing_horizontal;
            #[cfg(not(test))]
            if this.sidebar_resizing || finish_split_resize {
                let bounds = _window.bounds();
                let sidebar = this.effective_sidebar_width();
                let content_width =
                    (f32::from(bounds.size.width) - sidebar - 2.0 * CONTENT_BEZEL_X).max(100.0);
                let height = (f32::from(bounds.size.height) - topbar_height - CONTENT_BEZEL_BOTTOM)
                    .max(100.0) as i32;
                let exact_sizes = if let Some(pair) = this.current_split_pair() {
                    this.split_browser_sizes(&pair, content_width, height as f32)
                        .into_iter()
                        .map(|(tab_id, width, pane_height)| split_view::BrowserSize {
                            tab_id,
                            width,
                            height: pane_height,
                        })
                        .collect()
                } else {
                    this.active_space()
                        .active
                        .clone()
                        .map(|tab_id| {
                            vec![split_view::BrowserSize {
                                tab_id,
                                width: content_width as i32,
                                height,
                            }]
                        })
                        .unwrap_or_default()
                };
                this.finish_browser_resize_drag(
                    exact_sizes,
                    if finish_split_resize {
                        "split"
                    } else {
                        "sidebar"
                    },
                );
            }
            if this.sidebar_resizing || this.split_resizing || this.split_resizing_horizontal {
                this.sidebar_resizing = false;
                this.split_resizing = false;
                this.split_resizing_horizontal = false;
                this.save_state();
                cx.notify();
            }
        }),
    )
}
