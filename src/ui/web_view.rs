use std::sync::Arc;

use gpui_kit::{Context, Render, Window, div, prelude::*, px, rgb};

use crate::{CONTENT_BEZEL_X, Lumen};

pub struct WebViewport {
    parent: gpui_kit::WeakEntity<Lumen>,
    focus_handle: gpui_kit::FocusHandle,
    tab_id: String,
    sidebar_width: f32,
    sidebar_collapsed: bool,
    image: Option<Arc<gpui_kit::RenderImage>>,
    cursor: i32,
}

impl WebViewport {
    pub fn new(parent: gpui_kit::WeakEntity<Lumen>, focus_handle: gpui_kit::FocusHandle) -> Self {
        Self {
            parent,
            focus_handle,
            tab_id: String::new(),
            sidebar_width: 320.0,
            sidebar_collapsed: false,
            image: None,
            cursor: 0,
        }
    }

    pub fn set_frame(
        &mut self,
        tab_id: String,
        sidebar_width: f32,
        sidebar_collapsed: bool,
        image: Option<Arc<gpui_kit::RenderImage>>,
        cursor: i32,
    ) {
        self.tab_id = tab_id;
        self.sidebar_width = sidebar_width;
        self.sidebar_collapsed = sidebar_collapsed;
        self.image = image;
        self.cursor = cursor;
    }
}

fn browser_point(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    scale: f32,
    sidebar_width: f32,
    sidebar_collapsed: bool,
) -> (i32, i32) {
    let sidebar = if sidebar_collapsed {
        64.0
    } else {
        sidebar_width
    };
    let x = ((f32::from(position.x) - sidebar - CONTENT_BEZEL_X).max(0.0) * scale) as i32;
    let y = ((f32::from(position.y) - 52.0).max(0.0) * scale) as i32;
    (x, y)
}

impl Render for WebViewport {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_id_down = self.tab_id.clone();
        let tab_id_up = self.tab_id.clone();
        let tab_id_move = self.tab_id.clone();
        let tab_id_scroll = self.tab_id.clone();
        let tab_id_exit = self.tab_id.clone();
        let down_sidebar = self.sidebar_width;
        let up_sidebar = self.sidebar_width;
        let move_sidebar = self.sidebar_width;
        let scroll_sidebar = self.sidebar_width;
        let collapsed_down = self.sidebar_collapsed;
        let collapsed_up = self.sidebar_collapsed;
        let collapsed_move = self.sidebar_collapsed;
        let collapsed_scroll = self.sidebar_collapsed;
        let parent_down = self.parent.clone();
        let parent_up = self.parent.clone();
        let parent_move = self.parent.clone();
        let parent_scroll = self.parent.clone();
        let parent_right_down = self.parent.clone();
        let parent_right_up = self.parent.clone();
        let parent_middle_down = self.parent.clone();
        let parent_middle_up = self.parent.clone();
        let parent_exit = self.parent.clone();
        let parent_key_down = self.parent.clone();
        let parent_key_up = self.parent.clone();
        let key_tab_id = self.tab_id.clone();
        let key_up_tab_id = self.tab_id.clone();
        let tab_id_right_down = self.tab_id.clone();
        let tab_id_right_up = self.tab_id.clone();
        let tab_id_middle_down = self.tab_id.clone();
        let tab_id_middle_up = self.tab_id.clone();
        let sidebar_right_down = self.sidebar_width;
        let sidebar_right_up = self.sidebar_width;
        let sidebar_middle_down = self.sidebar_width;
        let sidebar_middle_up = self.sidebar_width;
        let collapsed_right_down = self.sidebar_collapsed;
        let collapsed_right_up = self.sidebar_collapsed;
        let collapsed_middle_down = self.sidebar_collapsed;
        let collapsed_middle_up = self.sidebar_collapsed;

        let mut viewport = div()
            .id("web-view-viewport")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(0x15161a))
            .rounded(px(crate::CARD_RADIUS))
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
                        .update(cx, |this, _| {
                            if !this.browser_focused {
                                return false;
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
                            if !this.browser_focused
                                || Lumen::is_app_shortcut_for_view(
                                    &event.keystroke.key.to_lowercase(),
                                    event.keystroke.modifiers.control
                                        || event.keystroke.modifiers.platform,
                                    event.keystroke.modifiers.shift,
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
                cx.listener(move |_, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let parent = parent_down.clone();
                    let tab_id = tab_id_down.clone();
                    let (x, y) = browser_point(
                        event.position,
                        window.scale_factor(),
                        down_sidebar,
                        collapsed_down,
                    );
                    parent
                        .update(cx, |this, cx| {
                            this.focus_browser(window, cx);
                            this.engine.send_mouse_click(
                                &tab_id,
                                x,
                                y,
                                cef::MouseButtonType::LEFT,
                                false,
                                event.click_count as i32,
                                &event.modifiers,
                            );
                        })
                        .ok();
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Left,
                cx.listener(move |_, event: &gpui_kit::MouseUpEvent, window, cx| {
                    let (x, y) = browser_point(
                        event.position,
                        window.scale_factor(),
                        up_sidebar,
                        collapsed_up,
                    );
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
                            )
                        })
                        .ok();
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Right,
                cx.listener(move |_, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let (x, y) = browser_point(
                        event.position,
                        window.scale_factor(),
                        sidebar_right_down,
                        collapsed_right_down,
                    );
                    parent_right_down
                        .update(cx, |this, _| {
                            this.engine.send_mouse_click(
                                &tab_id_right_down,
                                x,
                                y,
                                cef::MouseButtonType::RIGHT,
                                false,
                                event.click_count as i32,
                                &event.modifiers,
                            )
                        })
                        .ok();
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Right,
                cx.listener(move |_, event: &gpui_kit::MouseUpEvent, window, cx| {
                    let (x, y) = browser_point(
                        event.position,
                        window.scale_factor(),
                        sidebar_right_up,
                        collapsed_right_up,
                    );
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
                cx.listener(move |_, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let (x, y) = browser_point(
                        event.position,
                        window.scale_factor(),
                        sidebar_middle_down,
                        collapsed_middle_down,
                    );
                    parent_middle_down
                        .update(cx, |this, _| {
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
                cx.listener(move |_, event: &gpui_kit::MouseUpEvent, window, cx| {
                    let (x, y) = browser_point(
                        event.position,
                        window.scale_factor(),
                        sidebar_middle_up,
                        collapsed_middle_up,
                    );
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
            .on_mouse_move(
                cx.listener(move |_, event: &gpui_kit::MouseMoveEvent, window, cx| {
                    let (x, y) = browser_point(
                        event.position,
                        window.scale_factor(),
                        move_sidebar,
                        collapsed_move,
                    );
                    parent_move
                        .update(cx, |this, _| {
                            this.engine
                                .send_mouse_move(&tab_id_move, x, y, false, &event.modifiers)
                        })
                        .ok();
                }),
            )
            .on_mouse_exit(cx.listener(move |_, _, _, cx| {
                parent_exit
                    .update(cx, |this, _| {
                        this.engine
                            .send_mouse_move(&tab_id_exit, 0, 0, true, &Default::default())
                    })
                    .ok();
            }))
            .on_scroll(
                cx.listener(move |_, event: &gpui_kit::ScrollEvent, window, cx| {
                    let scale = window.scale_factor();
                    let (x, y) =
                        browser_point(event.position, scale, scroll_sidebar, collapsed_scroll);
                    let (dx, dy) = match event.delta {
                        gpui_kit::ScrollDelta::Pixels(p) => {
                            (f32::from(p.x) * scale, f32::from(p.y) * scale)
                        }
                        gpui_kit::ScrollDelta::Lines(l) => (l.x * 40.0 * scale, l.y * 40.0 * scale),
                    };
                    parent_scroll
                        .update(cx, |this, _| {
                            this.engine.send_mouse_wheel(
                                &tab_id_scroll,
                                x,
                                y,
                                dx as i32,
                                dy as i32,
                                &event.modifiers,
                            )
                        })
                        .ok();
                }),
            );

        if let Some(image) = self.image.clone() {
            viewport = viewport.child(
                gpui_kit::img(image)
                    .size_full()
                    .rounded(px(crate::CARD_RADIUS)),
            );
        }
        viewport
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_coordinates_remove_sidebar_and_topbar_before_scaling() {
        let position = gpui_kit::point(px(340.0), px(72.0));
        assert_eq!(browser_point(position, 2.0, 320.0, false), (32, 40));
        assert_eq!(browser_point(position, 1.0, 320.0, true), (272, 20));
    }
}
