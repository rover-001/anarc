use std::sync::Arc;

use gpui_kit::{Context, Render, Window, div, prelude::*, px, rgb};

use crate::ui::{
    split_view::frame_logical_size,
    visual::{SpaceTheme, TOPBAR_HEIGHT, space_theme},
};
use crate::{CONTENT_BEZEL_X, Lumen};

pub struct WebViewport {
    parent: gpui_kit::WeakEntity<Lumen>,
    focus_handle: gpui_kit::FocusHandle,
    tab_id: String,
    sidebar_width: f32,
    pane_offset: f32,
    pane_index: usize,
    image: Option<Arc<gpui_kit::RenderImage>>,
    retired_images: Vec<Arc<gpui_kit::RenderImage>>,
    cursor: i32,
    theme: SpaceTheme,
}

impl WebViewport {
    pub fn new(parent: gpui_kit::WeakEntity<Lumen>, focus_handle: gpui_kit::FocusHandle) -> Self {
        Self {
            parent,
            focus_handle,
            tab_id: String::new(),
            sidebar_width: 320.0,
            pane_offset: 0.0,
            pane_index: 0,
            image: None,
            retired_images: Vec::new(),
            cursor: 0,
            theme: space_theme(0x9aa6ff),
        }
    }

    pub fn set_frame(
        &mut self,
        tab_id: String,
        effective_sidebar_width: f32,
        image: Option<Arc<gpui_kit::RenderImage>>,
        cursor: i32,
        retired_images: Vec<Arc<gpui_kit::RenderImage>>,
        pane_offset: f32,
        pane_index: usize,
    ) {
        self.tab_id = tab_id;
        self.sidebar_width = effective_sidebar_width;
        self.image = image;
        self.cursor = cursor;
        self.retired_images.extend(retired_images);
        self.pane_offset = pane_offset;
        self.pane_index = pane_index;
    }

    pub fn set_content(
        &mut self,
        tab_id: String,
        image: Option<Arc<gpui_kit::RenderImage>>,
        cursor: i32,
        retired_images: Vec<Arc<gpui_kit::RenderImage>>,
    ) {
        self.tab_id = tab_id;
        self.image = image;
        self.cursor = cursor;
        self.retired_images.extend(retired_images);
    }

    pub fn set_theme(&mut self, theme: SpaceTheme) {
        self.theme = theme;
    }
}

#[cfg(test)]
fn browser_point(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    scale: f32,
    sidebar_width: f32,
) -> (i32, i32) {
    browser_point_in_pane(position, scale, sidebar_width, 0.0)
}

fn browser_point_in_pane(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    scale: f32,
    sidebar_width: f32,
    pane_offset: f32,
) -> (i32, i32) {
    let x = ((f32::from(position.x) - sidebar_width - CONTENT_BEZEL_X - pane_offset).max(0.0)
        * scale) as i32;
    let y = ((f32::from(position.y) - TOPBAR_HEIGHT).max(0.0) * scale) as i32;
    (x, y)
}

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

        let mut viewport = div()
            .id("web-view-viewport")
            .relative()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(self.theme.surface))
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
                            if !this.browser_focused
                                || this.focused_pane != pane_index
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
                                || this.focused_pane != pane_index
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
                cx.listener(move |_, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let parent = parent_down.clone();
                    let tab_id = tab_id_down.clone();
                    let (x, y) = browser_point_in_pane(
                        event.position,
                        window.scale_factor(),
                        down_sidebar,
                        pane_offset,
                    );
                    parent
                        .update(cx, |this, cx| {
                            this.set_focused_pane(pane_index);
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
                            cx.notify();
                        })
                        .ok();
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Left,
                cx.listener(move |_, event: &gpui_kit::MouseUpEvent, window, cx| {
                    let (x, y) = browser_point_in_pane(
                        event.position,
                        window.scale_factor(),
                        up_sidebar,
                        pane_offset,
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
                            );
                        })
                        .ok();
                }),
            )
            .on_mouse_down(
                gpui_kit::MouseButton::Right,
                cx.listener(move |_, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let (x, y) = browser_point_in_pane(
                        event.position,
                        window.scale_factor(),
                        sidebar_right_down,
                        pane_offset,
                    );
                    parent_right_down
                        .update(cx, |this, cx| {
                            this.set_focused_pane(pane_index);
                            this.focus_browser(window, cx);
                            this.engine.send_mouse_click(
                                &tab_id_right_down,
                                x,
                                y,
                                cef::MouseButtonType::RIGHT,
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
                gpui_kit::MouseButton::Right,
                cx.listener(move |_, event: &gpui_kit::MouseUpEvent, window, cx| {
                    let (x, y) = browser_point_in_pane(
                        event.position,
                        window.scale_factor(),
                        sidebar_right_up,
                        pane_offset,
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
                    let (x, y) = browser_point_in_pane(
                        event.position,
                        window.scale_factor(),
                        sidebar_middle_down,
                        pane_offset,
                    );
                    parent_middle_down
                        .update(cx, |this, cx| {
                            this.set_focused_pane(pane_index);
                            this.focus_browser(window, cx);
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
                    let (x, y) = browser_point_in_pane(
                        event.position,
                        window.scale_factor(),
                        sidebar_middle_up,
                        pane_offset,
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
                cx.listener(move |_, event: &gpui_kit::MouseMoveEvent, window, cx| {
                    let (x, y) = browser_point_in_pane(
                        event.position,
                        window.scale_factor(),
                        move_sidebar,
                        pane_offset,
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
                        browser_point_in_pane(event.position, scale, scroll_sidebar, pane_offset);
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
            let size = image.size(0);
            let (width, height) = frame_logical_size(
                u32::from(size.width),
                u32::from(size.height),
                window.scale_factor(),
            );
            viewport = viewport.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(width))
                    .h(px(height))
                    .child(gpui_kit::img(image).size_full()),
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
        assert_eq!(browser_point(position, 2.0, 320.0), (33, 55));
        assert_eq!(browser_point(position, 1.0, 64.0), (272, 27));
    }

    #[test]
    fn viewport_mouse_origin_matches_scaled_topbar_height() {
        let at_origin = gpui_kit::point(px(100.0), px(TOPBAR_HEIGHT));
        let below_origin = gpui_kit::point(px(100.0), px(TOPBAR_HEIGHT + 10.0));
        assert_eq!(browser_point(at_origin, 1.0, 0.0).1, 0);
        assert_eq!(browser_point(below_origin, 1.0, 0.0).1, 10);
    }

    #[test]
    fn pane_coordinates_are_local_to_the_selected_viewport() {
        let position = gpui_kit::point(px(520.0), px(72.0));
        assert_eq!(
            browser_point_in_pane(position, 2.0, 320.0, 100.0),
            (193, 55)
        );
    }
}
