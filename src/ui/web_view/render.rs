use gpui_kit::{AnimationExt as _, Context, Render, Window, div, prelude::*, px, rgb};

use crate::app::CARD_RADIUS;

use super::pinch::clamp_pinch_offset;
use super::*;

impl Render for WebViewport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for image in self.retired_images.drain(..) {
            let _ = window.drop_image(image);
        }

        let tab_id_down = self.tab_id.clone();
        let tab_id_up = self.tab_id.clone();
        let tab_id_move = self.tab_id.clone();
        let tab_id_scroll = self.tab_id.clone();
        let tab_id_exit = self.tab_id.clone();
        let down_sidebar = self.sidebar_width;
        let pane_offset = self.pane_offset;
        let pane_index = self.pane_index;
        let up_sidebar = self.sidebar_width;
        let move_sidebar = self.sidebar_width;
        let scroll_sidebar = self.sidebar_width;
        let parent_down = self.parent.clone();
        let parent_up = self.parent.clone();
        let parent_move = self.parent.clone();
        let parent_scroll = self.parent.clone();
        let parent_right_down = self.parent.clone();
        let parent_right_up = self.parent.clone();
        let parent_middle_down = self.parent.clone();
        let parent_middle_up = self.parent.clone();
        let parent_exit = self.parent.clone();
        let parent_back = self.parent.clone();
        let parent_forward = self.parent.clone();
        let parent_key_down = self.parent.clone();
        let parent_key_up = self.parent.clone();
        let key_tab_id = self.tab_id.clone();
        let back_tab_id = self.tab_id.clone();
        let forward_tab_id = self.tab_id.clone();
        let key_up_tab_id = self.tab_id.clone();
        let tab_id_right_down = self.tab_id.clone();
        let tab_id_right_up = self.tab_id.clone();
        let tab_id_middle_down = self.tab_id.clone();
        let tab_id_middle_up = self.tab_id.clone();
        let sidebar_right_down = self.sidebar_width;
        let sidebar_right_up = self.sidebar_width;
        let sidebar_middle_down = self.sidebar_width;
        let sidebar_middle_up = self.sidebar_width;
        let fullscreen = self.fullscreen;
        let origin_override = self.origin_override;
        let peek_mode = self.peek_mode;
        let drop_parent = self.parent.clone();
        let drop_tab_id = self.tab_id.clone();
        let drop_sidebar = self.sidebar_width;

        let mut viewport = div()
            .id("web-view-viewport")
            .relative()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(self.drag_edge_color.unwrap_or(self.theme.surface)))
            .rounded(px(if fullscreen { 0.0 } else { CARD_RADIUS }))
            .overflow_hidden()
            .cursor(match self.cursor {
                1 => gpui_kit::CursorStyle::IBeam,
                2 => gpui_kit::CursorStyle::Crosshair,
                3 => gpui_kit::CursorStyle::PointingHand,
                4 => gpui_kit::CursorStyle::Arrow,
                5 => gpui_kit::CursorStyle::ResizeLeftRight,
                6 => gpui_kit::CursorStyle::ResizeUpDown,
                7 => gpui_kit::CursorStyle::OpenHand,
                8 => gpui_kit::CursorStyle::ClosedHand,
                9 => gpui_kit::CursorStyle::OperationNotAllowed,
                10 => gpui_kit::CursorStyle::DragLink,
                11 => gpui_kit::CursorStyle::DragCopy,
                12 => gpui_kit::CursorStyle::ContextualMenu,
                _ => gpui_kit::CursorStyle::Arrow,
            })
            .on_key_down(
                cx.listener(move |_, event: &gpui_kit::KeyDownEvent, window, cx| {
                    let handled = parent_key_down
                        .update(cx, |this, cx| {
                            let focus_matches = if peek_mode {
                                this.peek_focused
                            } else {
                                this.focused_pane == pane_index
                            };
                            if !this.browser_focused
                                || !focus_matches
                                || Lumen::is_app_shortcut_for_view(
                                    &event.keystroke.key.to_lowercase(),
                                    event.keystroke.modifiers.control
                                        || event.keystroke.modifiers.platform,
                                    event.keystroke.modifiers.shift,
                                    event.keystroke.modifiers.alt,
                                )
                            {
                                return false;
                            }

                            let key = event.keystroke.key.to_lowercase();
                            let is_ctrl = event.keystroke.modifiers.control
                                || event.keystroke.modifiers.platform;
                            let is_plain_ctrl = is_ctrl
                                && !event.keystroke.modifiers.shift
                                && !event.keystroke.modifiers.alt;

                            if is_plain_ctrl && key == "v" {
                                let clip_text = cx
                                    .read_from_clipboard()
                                    .and_then(|item| item.text().map(|s| s.to_string()))
                                    .or_else(crate::engine::clipboard::read_system_clipboard);

                                if let Some(text) = clip_text {
                                    let script = format!(
                                        r#"(function() {{
                                            const text = {};
                                            const active = document.activeElement;
                                            let handled = false;
                                            try {{
                                                const event = new ClipboardEvent('paste', {{
                                                    clipboardData: new DataTransfer(),
                                                    bubbles: true,
                                                    cancelable: true
                                                }});
                                                event.clipboardData.setData('text/plain', text);
                                                if (active) {{
                                                    handled = !active.dispatchEvent(event);
                                                }} else {{
                                                    handled = !document.dispatchEvent(event);
                                                }}
                                            }} catch (_) {{}}
                                            if (!handled) {{
                                                try {{
                                                    document.execCommand('insertText', false, text);
                                                }} catch (_) {{}}
                                            }}
                                        }})();"#,
                                        serde_json::to_string(&text).unwrap_or_default()
                                    );
                                    this.engine.execute_script(&key_tab_id, &script);
                                }
                                this.engine.paste(&key_tab_id);
                                return true;
                            }

                            if is_plain_ctrl && key == "c" {
                                this.engine.send_key_down(&key_tab_id, event);
                                this.engine.copy(&key_tab_id);
                                let script = r#"(function() {
                                    try {
                                        function getSel() {
                                            let text = window.getSelection ? window.getSelection().toString() : '';
                                            if (text) return text;
                                            let el = document.activeElement;
                                            while (el && el.shadowRoot && el.shadowRoot.activeElement) {
                                                el = el.shadowRoot.activeElement;
                                            }
                                            if (el && 'selectionStart' in el && 'selectionEnd' in el && typeof el.value === 'string') {
                                                const start = Math.min(el.selectionStart, el.selectionEnd);
                                                const end = Math.max(el.selectionStart, el.selectionEnd);
                                                if (end > start) return el.value.substring(start, end);
                                            }
                                            return '';
                                        }
                                        const s = getSel();
                                        if (s) {
                                            console.log("[ANARC_CB_COPY]" + encodeURIComponent(s));
                                        }
                                    } catch (_) {}
                                })();"#;
                                this.engine.execute_script(&key_tab_id, script);
                                return true;
                            }

                            if is_plain_ctrl && key == "x" {
                                this.engine.send_key_down(&key_tab_id, event);
                                this.engine.cut(&key_tab_id);
                                let script = r#"(function() {
                                    try {
                                        function getSel() {
                                            let text = window.getSelection ? window.getSelection().toString() : '';
                                            if (text) return text;
                                            let el = document.activeElement;
                                            while (el && el.shadowRoot && el.shadowRoot.activeElement) {
                                                el = el.shadowRoot.activeElement;
                                            }
                                            if (el && 'selectionStart' in el && 'selectionEnd' in el && typeof el.value === 'string') {
                                                const start = Math.min(el.selectionStart, el.selectionEnd);
                                                const end = Math.max(el.selectionStart, el.selectionEnd);
                                                if (end > start) return el.value.substring(start, end);
                                            }
                                            return '';
                                        }
                                        const s = getSel();
                                        if (s) {
                                            console.log("[ANARC_CB_COPY]" + encodeURIComponent(s));
                                        }
                                    } catch (_) {}
                                })();"#;
                                this.engine.execute_script(&key_tab_id, script);
                                return true;
                            }

                            if is_plain_ctrl && key == "a" {
                                this.engine.select_all(&key_tab_id);
                                return true;
                            }

                            this.engine.send_key_down(&key_tab_id, event);
                            true
                        })
                        .unwrap_or(false);
                    if handled {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                }),
            )
            .on_key_up(
                cx.listener(move |_, event: &gpui_kit::KeyUpEvent, window, cx| {
                    let handled = parent_key_up
                        .update(cx, |this, _| {
                            let focus_matches = if peek_mode {
                                this.peek_focused
                            } else {
                                this.focused_pane == pane_index
                            };
                            if !this.browser_focused
                                || !focus_matches
                                || Lumen::is_app_shortcut_for_view(
                                    &event.keystroke.key.to_lowercase(),
                                    event.keystroke.modifiers.control
                                        || event.keystroke.modifiers.platform,
                                    event.keystroke.modifiers.shift,
                                    event.keystroke.modifiers.alt,
                                )
                            {
                                return false;
                            }
                            this.engine.send_key_up(&key_up_tab_id, event);
                            true
                        })
                        .unwrap_or(false);
                    if handled {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    this.scroll_motion.cancel();
                    let parent = parent_down.clone();
                    let tab_id = tab_id_down.clone();
                    let (x, y) = this.map_browser_point(browser_point_at(
                        event.position,
                        down_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    ));
                    parent
                        .update(cx, |this, cx| {
                            if peek_mode {
                                this.focus_peek_browser(window, cx);
                            } else {
                                this.set_focused_pane(pane_index);
                                this.focus_browser(window, cx);
                            }
                            this.engine.send_mouse_click(
                                &tab_id,
                                x,
                                y,
                                cef::MouseButtonType::LEFT,
                                false,
                                event.click_count as i32,
                                &event.modifiers,
                            );
                            cx.notify();
                        })
                        .ok();
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Left,
                cx.listener(move |this, event: &gpui_kit::MouseUpEvent, _, cx| {
                    let (x, y) = this.map_browser_point(browser_point_at(
                        event.position,
                        up_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    ));
                    parent_up
                        .update(cx, |this, _| {
                            this.engine.send_mouse_click(
                                &tab_id_up,
                                x,
                                y,
                                cef::MouseButtonType::LEFT,
                                true,
                                event.click_count as i32,
                                &event.modifiers,
                            );
                        })
                        .ok();
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Right,
                cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let (x, y) = this.map_browser_point(browser_point_at(
                        event.position,
                        sidebar_right_down,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    ));
                    let screen_x = f32::from(event.position.x);
                    let screen_y = f32::from(event.position.y);
                    let menu_tab_id = tab_id_right_down.clone();
                    parent_right_down
                        .update(cx, |this, cx| {
                            if peek_mode {
                                this.focus_peek_browser(window, cx);
                            } else {
                                this.set_focused_pane(pane_index);
                                this.focus_browser(window, cx);
                            }
                            this.engine.send_mouse_click(
                                &tab_id_right_down,
                                x,
                                y,
                                cef::MouseButtonType::RIGHT,
                                false,
                                event.click_count as i32,
                                &event.modifiers,
                            );
                            this.open_web_context_menu(menu_tab_id, screen_x, screen_y);
                            cx.notify();
                        })
                        .ok();
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Right,
                cx.listener(move |this, event: &gpui_kit::MouseUpEvent, _, cx| {
                    let (x, y) = this.map_browser_point(browser_point_at(
                        event.position,
                        sidebar_right_up,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    ));
                    parent_right_up
                        .update(cx, |this, _| {
                            this.engine.send_mouse_click(
                                &tab_id_right_up,
                                x,
                                y,
                                cef::MouseButtonType::RIGHT,
                                true,
                                event.click_count as i32,
                                &event.modifiers,
                            )
                        })
                        .ok();
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Middle,
                cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let (x, y) = this.map_browser_point(browser_point_at(
                        event.position,
                        sidebar_middle_down,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    ));
                    parent_middle_down
                        .update(cx, |this, cx| {
                            if peek_mode {
                                this.focus_peek_browser(window, cx);
                            } else {
                                this.set_focused_pane(pane_index);
                                this.focus_browser(window, cx);
                            }
                            this.engine.send_mouse_click(
                                &tab_id_middle_down,
                                x,
                                y,
                                cef::MouseButtonType::MIDDLE,
                                false,
                                event.click_count as i32,
                                &event.modifiers,
                            )
                        })
                        .ok();
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Middle,
                cx.listener(move |this, event: &gpui_kit::MouseUpEvent, _, cx| {
                    let (x, y) = this.map_browser_point(browser_point_at(
                        event.position,
                        sidebar_middle_up,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    ));
                    parent_middle_up
                        .update(cx, |this, _| {
                            this.engine.send_mouse_click(
                                &tab_id_middle_up,
                                x,
                                y,
                                cef::MouseButtonType::MIDDLE,
                                true,
                                event.click_count as i32,
                                &event.modifiers,
                            )
                        })
                        .ok();
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Back),
                cx.listener(move |_, _, _, cx| {
                    parent_back
                        .update(cx, |this, _| this.engine.go_back(&back_tab_id))
                        .ok();
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Navigate(gpui_kit::NavigationDirection::Forward),
                cx.listener(move |_, _, _, cx| {
                    parent_forward
                        .update(cx, |this, _| this.engine.go_forward(&forward_tab_id))
                        .ok();
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &gpui_kit::MouseMoveEvent, _, cx| {
                    let (x, y) = this.map_browser_point(browser_point_at(
                        event.position,
                        move_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    ));
                    parent_move
                        .update(cx, |this, _| {
                            this.engine.send_mouse_move(
                                &tab_id_move,
                                x,
                                y,
                                false,
                                &event.modifiers,
                                event.pressed_button,
                            )
                        })
                        .ok();
                }),
            )
            .on_mouse_exit(cx.listener(move |_, _, _, cx| {
                parent_exit
                    .update(cx, |this, _| {
                        this.engine.send_mouse_move(
                            &tab_id_exit,
                            0,
                            0,
                            true,
                            &Default::default(),
                            None,
                        )
                    })
                    .ok();
            }))
            .on_pinch(cx.listener(
                move |this, event: &gpui_kit::PinchEvent, window, cx| {
                    let (x, y) = browser_point_at(
                        event.position,
                        this.sidebar_width,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
                    let viewport_size = this.image.as_ref().map_or((1.0, 1.0), |image| {
                        let size = image.size(0);
                        frame_logical_size(
                            u32::from(size.width),
                            u32::from(size.height),
                            window.scale_factor(),
                        )
                    });
                    match event.phase {
                        gpui_kit::TouchPhase::Started => {
                            this.begin_pinch((x as f32, y as f32));
                        }
                        gpui_kit::TouchPhase::Ended | gpui_kit::TouchPhase::Cancelled => {
                            this.pinch_active = false;
                        }
                        _ => {
                            if this.pinch_active {
                                this.step_pinch(
                                    event.delta,
                                    (x as f32, y as f32),
                                    viewport_size,
                                    cx,
                                );
                            }
                        }
                    }
                },
            ))
            .on_scroll(
                cx.listener(move |this, event: &gpui_kit::ScrollEvent, window, cx| {
                    let (x, y) = browser_point_at(
                        event.position,
                        scroll_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
                    let (dx, dy, is_precision) = match event.delta {
                        gpui_kit::ScrollDelta::Pixels(p) => {
                            (f32::from(p.x), f32::from(p.y), true)
                        }
                        gpui_kit::ScrollDelta::Lines(l) => {
                            let continuous = l.x.fract().abs().max(l.y.fract().abs()) > 0.001
                                || this.scroll_motion.continuous_recent();
                            (l.x * 100.0, l.y * 100.0, continuous)
                        }
                    };
                    let (dx, dy) = (dx * WHEEL_SCROLL_SCALE, dy * WHEEL_SCROLL_SCALE);
                    if event.touch_phase == gpui_kit::TouchPhase::Cancelled {
                        this.scroll_motion.cancel();
                        return;
                    }
                    // Platforms can finish a trackpad gesture with a zero-delta
                    // event. It releases the fingers, not the inertial driver.
                    if dx == 0.0 && dy == 0.0 {
                        return;
                    }
                    if this
                        .pinch_preview
                        .as_ref()
                        .is_some_and(|preview| preview.scale > 1.0 + 1e-3)
                    {
                        let viewport_size = this.image.as_ref().map_or((1.0, 1.0), |image| {
                            let size = image.size(0);
                            frame_logical_size(
                                u32::from(size.width),
                                u32::from(size.height),
                                window.scale_factor(),
                            )
                        });
                        this.pan_image((-dx, -dy), viewport_size, cx);
                        return;
                    }
                    if is_precision
                        && event.source == gpui_kit::ScrollSource::Pointer
                        && (dx != 0.0 || dy != 0.0)
                    {
                        this.scroll_motion.record_pointer_scroll(
                            (dx, dy), (x, y), event.modifiers, window, cx,
                        );
                        return;
                    }
                    this.scroll_motion.interrupt();
                    let (step_x, step_y) = this.scroll_motion.accumulate((dx, dy));
                    if step_x != 0 || step_y != 0 {
                        parent_scroll.update(cx, |this, _| {
                            this.engine.send_mouse_wheel(
                                &tab_id_scroll, x, y, step_x, step_y,
                                &event.modifiers, is_precision,
                            );
                            this.engine.send_external_begin_frame(&tab_id_scroll);
                        }).ok();
                    }
                }),
            )
            .on_drop(
                cx.listener(move |this, drag: &gpui_kit::ExternalPaths, window, cx| {
                    let paths = drag.paths().to_vec();
                    if paths.is_empty() {
                        return;
                    }
                    let (x, y) = this.map_browser_point(browser_point_at(
                        window.mouse_position(),
                        drop_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    ));
                    let _ = drop_parent.update(cx, |this, _| {
                        this.engine.send_drag_drop_files(&drop_tab_id, &paths, x, y);
                    });
                }),
            );

        let transition_id = self.transition_id;
        let transitioning = self.transition_tab.as_deref() == Some(self.tab_id.as_str())
            && !self.transition_waiting_for_image
            && !crate::ui::motion::reduced_motion();
        if transitioning && !self.transition_cleanup_scheduled {
            self.transition_cleanup_scheduled = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(crate::ui::motion::content().duration())
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.transition_id == transition_id {
                        this.previous_image = None;
                        this.transition_tab = None;
                        this.transition_cleanup_scheduled = false;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        if transitioning {
            if let Some(image) = self.previous_image.clone() {
                viewport = viewport.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .overflow_hidden()
                        .with_animation(
                            format!("web-view-outgoing-{transition_id}"),
                            crate::ui::motion::content().animation(),
                            |element, progress| {
                                element.opacity(crate::ui::motion::clamp_percentage(1.0 - progress))
                            },
                        )
                        .child(gpui_kit::img(image).size_full()),
                );
            }
        } else if self.transition_waiting_for_image
            && self.transition_tab.as_deref() == Some(self.tab_id.as_str())
        {
            if let Some(image) = self.previous_image.clone() {
                viewport = viewport.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .overflow_hidden()
                        .child(gpui_kit::img(image).size_full()),
                );
            }
        } else if self.transition_tab.as_deref() == Some(self.tab_id.as_str()) {
            self.previous_image = None;
        }

        if let Some(image) = self.image.clone() {
            let size = image.size(0);
            let pane_scale = window.scale_factor();
            let (width, height) =
                frame_logical_size(u32::from(size.width), u32::from(size.height), pane_scale);
            let (pinch_factor, pinch_offset) =
                self.pinch_preview
                    .as_ref()
                    .map_or((1.0, (0.0, 0.0)), |preview| {
                        (
                            preview.scale as f32,
                            clamp_pinch_offset(
                                preview.offset,
                                preview.scale as f32,
                                (width, height),
                            ),
                        )
                    });
            let current = if pinch_factor > 1.0 + 1e-4 {
                div()
                    .absolute()
                    .top(px(pinch_offset.1.min(0.0)))
                    .left(px(pinch_offset.0.min(0.0)))
                    .w(px(width * pinch_factor))
                    .h(px(height * pinch_factor))
                    .overflow_hidden()
                    .child(gpui_kit::img(image).size_full())
            } else {
                div()
                    .size_full()
                    .overflow_hidden()
                    .child(gpui_kit::img(image).size_full())
            };
            let current = if transitioning {
                current
                    .with_animation(
                        format!("web-view-incoming-{transition_id}"),
                        crate::ui::motion::content().animation(),
                        |element, progress| {
                            element.opacity(crate::ui::motion::clamp_percentage(progress))
                        },
                    )
                    .into_any_element()
            } else {
                current.into_any_element()
            };
            viewport = viewport.child(current);
        }

        // A captured frame may be wider than its pane while split/sidebar
        // dividers are moving. Use a rounded border ring whose inner edge
        // matches the pane's corner arc; the pane's rectangular clip trims
        // the rest of each ring. The peek overlay draws its own card border.
        if !peek_mode {
            let radius = CARD_RADIUS;
            let ring_size = px(radius * 3.0);
            let ring_offset = px(-radius);
            let outer_corner_radius = px(radius * 2.0);
            let border_width = px(radius);
            let outer_color = rgb(self.theme.base);
            let inner_color = rgb(self.theme.surface);
            let (top_left_color, top_right_color, bottom_left_color, bottom_right_color) =
                if !self.split_layout {
                    (outer_color, outer_color, outer_color, outer_color)
                } else if self.pane_index == 0 {
                    (outer_color, inner_color, outer_color, inner_color)
                } else {
                    (inner_color, outer_color, inner_color, outer_color)
                };
            viewport = viewport
                .child(
                    div()
                        .absolute()
                        .top(ring_offset)
                        .left(ring_offset)
                        .w(ring_size)
                        .h(ring_size)
                        .border_t(border_width)
                        .border_l(border_width)
                        .border_color(top_left_color)
                        .rounded_tl(outer_corner_radius),
                )
                .child(
                    div()
                        .absolute()
                        .top(ring_offset)
                        .right(ring_offset)
                        .w(ring_size)
                        .h(ring_size)
                        .border_t(border_width)
                        .border_r(border_width)
                        .border_color(top_right_color)
                        .rounded_tr(outer_corner_radius),
                )
                .child(
                    div()
                        .absolute()
                        .bottom(ring_offset)
                        .left(ring_offset)
                        .w(ring_size)
                        .h(ring_size)
                        .border_b(border_width)
                        .border_l(border_width)
                        .border_color(bottom_left_color)
                        .rounded_bl(outer_corner_radius),
                )
                .child(
                    div()
                        .absolute()
                        .bottom(ring_offset)
                        .right(ring_offset)
                        .w(ring_size)
                        .h(ring_size)
                        .border_b(border_width)
                        .border_r(border_width)
                        .border_color(bottom_right_color)
                        .rounded_br(outer_corner_radius),
                );
        }
        viewport
    }
}
