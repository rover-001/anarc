use std::sync::Arc;

use gpui_kit::{AnimationExt as _, Context, Render, Window, div, prelude::*, px, rgb};

use crate::app::{CARD_RADIUS, CONTENT_BEZEL_X, Lumen};
use crate::engine::{MAX_PAGE_SCALE, MIN_PAGE_SCALE};
#[cfg(test)]
use crate::ui::visual::TOPBAR_HEIGHT;
use crate::ui::{
    split_view::frame_logical_size,
    visual::{SpaceTheme, active_topbar_height, space_theme},
};

/// Crossfading old->new frame on drag release made centred pages visibly bounce
/// sideways; with live resizing the final frame differs by a few pixels, so swap directly.
const RESIZE_RELEASE_CROSSFADE: bool = false;

#[derive(Default)]
struct ResizeFrameHandoff {
    retaining_old_frame: bool,
}

impl ResizeFrameHandoff {
    fn target_frame_arrived(&mut self, retained_edge_fill: bool) -> bool {
        if retained_edge_fill {
            self.retaining_old_frame = true;
            false
        } else {
            std::mem::take(&mut self.retaining_old_frame)
        }
    }
}

/// Live pinch-to-zoom preview.
///
/// Blink re-rasterizes the page for every page-scale change — roughly 20 ms per
/// painted frame in OSR — so forwarding each trackpad step caps a gesture at
/// ~45 fps however fast the steps arrive. Instead the last painted frame is
/// scaled and re-anchored here at display rate, and the accumulated scale is
/// forwarded to Blink once, when the fingers lift.
struct PinchPreview {
    /// Tab the gesture started on, so a late release cannot move another tab.
    tab_id: String,
    /// Absolute page scale the previewed frame is drawn at.
    scale: f64,
    /// Page scale of the frame being scaled; equal to `scale` at 1:1.
    base_scale: f64,
    /// Page scale Blink is at, or will be at once an in-flight commit lands.
    /// Gestures multiply against this instead of the on-screen frame's scale,
    /// so a pinch started before the previous repaint still commits the right
    /// factor.
    page_scale: f64,
    /// Pane-local focal point in logical pixels, fixed for the whole gesture so
    /// the preview and the single commit land on the same geometry.
    anchor: (f32, f32),
    /// Last gesture update; a preview that stops updating without an end event
    /// is committed instead of being left on screen.
    last_update: std::time::Instant,
    /// Set once the scale has been forwarded and the repaint at that scale is
    /// still outstanding. Holding the preview until that frame arrives keeps
    /// the page from flashing back to the previous scale on release.
    awaiting_frame: bool,
    /// Set when a new gesture starts while the committed repaint is still
    /// outstanding: the frame that arrives is already at `page_scale`, so it
    /// becomes the new `base_scale` instead of ending the preview.
    rebase: bool,
    /// Min-scale snap animation started by the release: `(started, from)`.
    settle: Option<(std::time::Instant, f64)>,
}

pub struct WebViewport {
    parent: gpui_kit::WeakEntity<Lumen>,
    focus_handle: gpui_kit::FocusHandle,
    tab_id: String,
    sidebar_width: f32,
    pane_offset: f32,
    pane_index: usize,
    split_layout: bool,
    drag_edge_color: Option<u32>,
    resize_source_image: Option<Arc<gpui_kit::RenderImage>>,
    resize_frame_handoff: ResizeFrameHandoff,
    image: Option<Arc<gpui_kit::RenderImage>>,
    previous_image: Option<Arc<gpui_kit::RenderImage>>,
    transition_id: u64,
    transition_tab: Option<String>,
    transition_cleanup_scheduled: bool,
    transition_waiting_for_image: bool,
    retired_images: Vec<Arc<gpui_kit::RenderImage>>,
    cursor: i32,
    fullscreen: bool,
    theme: SpaceTheme,
    scroll_accum_x: f32,
    scroll_accum_y: f32,
    /// Live pinch preview; see [`PinchPreview`].
    pinch_preview: Option<PinchPreview>,
    /// Cancels the preview driver when a new gesture or tab takes over.
    pinch_serial: u64,
    /// Absolute window-space origin of this viewport. Used by the peek overlay,
    /// which floats outside the regular content column.
    origin_override: Option<(f32, f32)>,
    /// Peek viewports input focus from `peek_focused` instead of the split pane
    /// index and skip the card-edge chrome ring.
    peek_mode: bool,
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
            split_layout: false,
            drag_edge_color: None,
            resize_source_image: None,
            resize_frame_handoff: ResizeFrameHandoff::default(),
            image: None,
            previous_image: None,
            transition_id: 0,
            transition_tab: None,
            transition_cleanup_scheduled: false,
            transition_waiting_for_image: false,
            retired_images: Vec::new(),
            cursor: 0,
            fullscreen: false,
            theme: space_theme(0x9aa6ff),
            scroll_accum_x: 0.0,
            scroll_accum_y: 0.0,
            pinch_preview: None,
            pinch_serial: 0,
            origin_override: None,
            peek_mode: false,
        }
    }

    pub fn set_origin_override(&mut self, origin: Option<(f32, f32)>) {
        self.origin_override = origin;
    }

    pub fn set_peek_mode(&mut self, peek_mode: bool) {
        self.peek_mode = peek_mode;
    }

    /// Starts (or restarts) the local preview for a pinch gesture.
    ///
    /// `base_scale` is the page scale of the frame currently on screen. A
    /// preview still waiting for its committed frame keeps that frame's
    /// geometry, so two quick pinches never flash back to the old scale.
    fn begin_pinch(
        &mut self,
        tab_id: &str,
        base_scale: f64,
        anchor: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let (scale, base_scale, page_scale, rebase) = match self.pinch_preview.as_ref() {
            Some(preview) if preview.awaiting_frame => {
                (preview.scale, preview.base_scale, preview.page_scale, true)
            }
            _ => (base_scale, base_scale, base_scale, false),
        };
        self.pinch_preview = Some(PinchPreview {
            tab_id: tab_id.to_owned(),
            scale,
            base_scale,
            page_scale,
            anchor,
            last_update: std::time::Instant::now(),
            awaiting_frame: false,
            rebase,
            settle: None,
        });
        self.pinch_serial = self.pinch_serial.wrapping_add(1);
        let serial = self.pinch_serial;
        self.spawn_pinch_driver(cx, serial);
    }

    /// Folds one trackpad step into the preview. Every step repaints locally,
    /// so this is the path that runs at display rate during a gesture.
    fn step_pinch(&mut self, delta: f32, cx: &mut Context<Self>) {
        let Some(preview) = self.pinch_preview.as_mut() else {
            return;
        };
        if preview.awaiting_frame || preview.settle.is_some() {
            return;
        }
        let Some(scale) = pinch_step_scale(preview.scale, delta) else {
            return;
        };
        preview.scale = scale;
        preview.last_update = std::time::Instant::now();
        cx.notify();
    }

    /// Ends the gesture: spring back to 100 % when it stopped just above it,
    /// otherwise hand the accumulated scale to Blink in one go.
    fn release_pinch(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.pinch_preview.as_mut() else {
            return;
        };
        if preview.awaiting_frame || preview.settle.is_some() {
            return;
        }
        let scale = preview.scale;
        if (1.0..=PINCH_SNAP_TO_MIN).contains(&scale) && (scale - 1.0).abs() > 1e-3 {
            preview.settle = Some((std::time::Instant::now(), scale));
            return;
        }
        self.commit_pinch(cx);
    }

    /// Forwards the accumulated scale to Blink once and keeps the preview up
    /// until the repaint at the new scale arrives.
    fn commit_pinch(&mut self, cx: &mut Context<Self>) {
        let Some(mut preview) = self.pinch_preview.take() else {
            return;
        };
        let factor = preview.scale / preview.page_scale;
        if !factor.is_finite()
            || (preview.scale - preview.page_scale).abs() < PINCH_COMMIT_MIN_SCALE_DELTA
        {
            cx.notify();
            return;
        }
        let anchor = preview.anchor;
        let tab_id = preview.tab_id.clone();
        let forwarded = self
            .parent
            .update(cx, |lumen, _| {
                lumen.engine.pinch_zoom(
                    &tab_id,
                    anchor.0.round() as i32,
                    anchor.1.round() as i32,
                    factor,
                );
            })
            .is_ok();
        if forwarded {
            preview.page_scale = preview.scale;
            preview.awaiting_frame = true;
            preview.last_update = std::time::Instant::now();
            self.pinch_preview = Some(preview);
        }
        cx.notify();
    }

    /// Drops the preview without touching the page, e.g. on a tab switch.
    fn cancel_pinch(&mut self) {
        self.pinch_serial = self.pinch_serial.wrapping_add(1);
        self.pinch_preview = None;
    }

    /// Reconciles a freshly painted frame with a live preview.
    ///
    /// A frame that lands after a commit is already drawn at the committed
    /// scale, so it ends the preview — or, when a new gesture started while
    /// that frame was outstanding, becomes the gesture's new base. Without the
    /// re-base the preview would scale an already-scaled frame a second time.
    fn on_frame_arrived(&mut self, frame_changed: bool) {
        if !frame_changed {
            return;
        }
        if self
            .pinch_preview
            .as_ref()
            .is_some_and(|preview| preview.awaiting_frame)
        {
            self.cancel_pinch();
        } else if let Some(preview) = self.pinch_preview.as_mut()
            && preview.rebase
        {
            preview.base_scale = preview.page_scale;
            preview.rebase = false;
        }
    }

    /// Drives the preview: animates the min-scale snap, commits a gesture that
    /// stopped updating, and exits as soon as the preview is no longer live.
    fn spawn_pinch_driver(&mut self, cx: &mut Context<Self>, serial: u64) {
        cx.spawn(async move |this, cx| {
            loop {
                let next = this
                    .update(cx, |this, cx| this.pinch_driver_tick(serial, cx))
                    .unwrap_or(None);
                let Some(delay) = next else {
                    break;
                };
                cx.background_executor().timer(delay).await;
            }
        })
        .detach();
    }

    fn pinch_driver_tick(
        &mut self,
        serial: u64,
        cx: &mut Context<Self>,
    ) -> Option<std::time::Duration> {
        if self.pinch_serial != serial {
            return None;
        }
        let (awaiting_frame, settle, idle) = {
            let preview = self.pinch_preview.as_ref()?;
            (
                preview.awaiting_frame,
                preview.settle,
                preview.last_update.elapsed(),
            )
        };
        if awaiting_frame {
            if idle >= PINCH_FRAME_TIMEOUT {
                self.cancel_pinch();
                cx.notify();
                return None;
            }
            return Some(PINCH_FRAME_TICK);
        }
        if let Some((started, from)) = settle {
            let progress =
                f64::from(crate::ui::motion::pinch_settle().progress(started.elapsed())).min(1.0);
            let done = progress >= 1.0;
            if let Some(preview) = self.pinch_preview.as_mut() {
                preview.scale = if done {
                    1.0
                } else {
                    from + (1.0 - from) * progress
                };
                if done {
                    preview.settle = None;
                }
            }
            if done {
                self.commit_pinch(cx);
                return None;
            }
            cx.notify();
            return Some(PINCH_SETTLE_TICK);
        }
        if idle >= PINCH_STALE_AFTER {
            self.commit_pinch(cx);
            return None;
        }
        Some(PINCH_ACTIVE_TICK)
    }

    pub fn has_content(&self) -> bool {
        self.image.is_some() || self.previous_image.is_some() || !self.tab_id.is_empty()
    }

    /// Drops the current frame and tab binding when the peek overlay closes.
    pub fn clear_content(&mut self) {
        self.cancel_pinch();
        self.tab_id.clear();
        self.image = None;
        self.previous_image = None;
        self.resize_source_image = None;
        self.transition_tab = None;
        self.transition_waiting_for_image = false;
        self.transition_cleanup_scheduled = false;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_frame(
        &mut self,
        tab_id: String,
        effective_sidebar_width: f32,
        image: Option<Arc<gpui_kit::RenderImage>>,
        cursor: i32,
        retired_images: Vec<Arc<gpui_kit::RenderImage>>,
        pane_offset: f32,
        pane_index: usize,
        split_layout: bool,
        drag_edge_color: Option<u32>,
    ) {
        let frame_changed = match (&self.image, &image) {
            (Some(current), Some(incoming)) => !Arc::ptr_eq(current, incoming),
            (None, None) => false,
            _ => true,
        };
        if self.tab_id != tab_id {
            self.scroll_accum_x = 0.0;
            self.scroll_accum_y = 0.0;
            self.cancel_pinch();
            self.resize_source_image = None;
            self.resize_frame_handoff = ResizeFrameHandoff::default();
            // Only cross-fade when the incoming tab already has a rendered frame.
            // When opening a new tab, blank page, or waiting for a fresh navigation,
            // never preserve or flash the previous tab's content.
            if image.is_some() {
                self.previous_image = self.image.clone();
                self.transition_id = self.transition_id.wrapping_add(1);
                self.transition_tab = Some(tab_id.clone());
                self.transition_cleanup_scheduled = false;
                self.transition_waiting_for_image = false;
            } else {
                self.previous_image = None;
                self.transition_tab = None;
                self.transition_waiting_for_image = false;
                self.transition_cleanup_scheduled = false;
            }
        } else if self
            .resize_frame_handoff
            .target_frame_arrived(drag_edge_color.is_some())
            && RESIZE_RELEASE_CROSSFADE
        {
            // The edge fill is released only when CEF supplies the exact target
            // size. Keep the frozen-size image above it while the new frame fades in.
            self.previous_image = self
                .resize_source_image
                .take()
                .or_else(|| self.image.clone());
            self.transition_id = self.transition_id.wrapping_add(1);
            self.transition_tab = Some(tab_id.clone());
            self.transition_cleanup_scheduled = false;
        } else if drag_edge_color.is_some() {
            // CEF remains at the frozen size throughout the drag and until the
            // final-size frame arrives. Keep its newest frame for the handoff.
            self.resize_source_image = image.clone();
        }
        if self.transition_tab.as_deref() == Some(tab_id.as_str())
            && self.transition_waiting_for_image
            && image.is_some()
        {
            self.transition_waiting_for_image = false;
            self.transition_id = self.transition_id.wrapping_add(1);
            self.transition_cleanup_scheduled = false;
        }
        // A fresh frame may be the repaint at the committed scale: it either
        // ends the preview or becomes the new base for a gesture that already
        // restarted.
        self.on_frame_arrived(frame_changed);
        self.tab_id = tab_id;
        self.sidebar_width = effective_sidebar_width;
        self.image = image;
        self.cursor = cursor;
        self.retired_images.extend(retired_images);
        self.pane_offset = pane_offset;
        self.pane_index = pane_index;
        self.split_layout = split_layout;
        self.drag_edge_color = drag_edge_color;
    }

    #[cfg(not(test))]
    pub fn set_content(
        &mut self,
        tab_id: String,
        image: Option<Arc<gpui_kit::RenderImage>>,
        cursor: i32,
        retired_images: Vec<Arc<gpui_kit::RenderImage>>,
    ) {
        let frame_changed = match (&self.image, &image) {
            (Some(current), Some(incoming)) => !Arc::ptr_eq(current, incoming),
            (None, None) => false,
            _ => true,
        };
        if self.tab_id != tab_id {
            self.cancel_pinch();
            if image.is_some() {
                self.previous_image = self.image.clone();
                self.transition_id = self.transition_id.wrapping_add(1);
                self.transition_tab = Some(tab_id.clone());
                self.transition_cleanup_scheduled = false;
            } else {
                self.previous_image = None;
                self.transition_tab = None;
                self.transition_cleanup_scheduled = false;
            }
        }
        self.on_frame_arrived(frame_changed);
        self.tab_id = tab_id;
        self.image = image;
        self.cursor = cursor;
        self.retired_images.extend(retired_images);
    }

    pub fn set_theme(&mut self, theme: SpaceTheme) {
        self.theme = theme;
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    /// Drops the previous tab's frame before an immediate cross-space jump.
    pub fn clear_transition_image(&mut self) {
        self.cancel_pinch();
        self.image = None;
        self.previous_image = None;
        self.resize_source_image = None;
        self.transition_tab = None;
        self.transition_waiting_for_image = false;
        self.transition_cleanup_scheduled = false;
    }
}

#[cfg(test)]
fn browser_point(position: gpui_kit::Point<gpui_kit::Pixels>, sidebar_width: f32) -> (i32, i32) {
    browser_point_in_pane(position, sidebar_width, 0.0, false)
}

fn step_scroll_accum(accum: &mut f32, delta: f32) -> i32 {
    *accum += delta;
    let step = accum.round() as i32;
    if step != 0 {
        *accum -= step as f32;
    }
    step
}

/// Trackpad steps arrive faster than the window repaints, and every step is a
/// local frame transform, so nothing is coalesced here: each one updates the
/// preview and the compositor takes the newest state at its next vsync.
const PINCH_ACTIVE_TICK: std::time::Duration = std::time::Duration::from_millis(16);

/// Tick rate for the min-scale snap, matching a 120 Hz display.
const PINCH_SETTLE_TICK: std::time::Duration = std::time::Duration::from_millis(8);

/// A gesture that stops updating without an end event (focus loss, cancelled
/// platform gesture) is committed after this long instead of sticking.
const PINCH_STALE_AFTER: std::time::Duration = std::time::Duration::from_millis(1500);

/// Scale changes below this are not worth a page-scale round trip. Blink
/// ignores anything under 0.0005, and a commit it ignores would never produce
/// the repaint the preview waits for.
const PINCH_COMMIT_MIN_SCALE_DELTA: f64 = 5e-4;

/// While a commit is waiting for its repaint the preview holds its geometry;
/// it is polled on this cadence so a lost frame cannot pin the preview up.
const PINCH_FRAME_TICK: std::time::Duration = std::time::Duration::from_millis(100);

/// Longest a committed preview waits for the repaint before giving up.
const PINCH_FRAME_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(700);

/// A released pinch that stopped this close to 100 % snaps back to it, matching
/// Blink's min-scale snap threshold.
const PINCH_SNAP_TO_MIN: f64 = 1.05;

/// Absolute page scale after one trackpad step. Steps are relative factors
/// (0.05 == 5 % larger), so they multiply; the result stays inside Blink's
/// visual-viewport range, and non-finite or sub-pixel steps are ignored.
fn pinch_step_scale(current: f64, delta: f32) -> Option<f64> {
    if !delta.is_finite() || !current.is_finite() || current <= 0.0 {
        return None;
    }
    let factor = f64::from((1.0 + delta).clamp(0.5, 2.0));
    let next = (current * factor).clamp(MIN_PAGE_SCALE, MAX_PAGE_SCALE);
    ((next - current).abs() > 1e-6).then_some(next)
}

/// Top-left offset of the previewed frame inside the pane. Scaling around the
/// focal point keeps the page content under the cursor stationary, which is
/// the same geometry the single commit re-anchors to.
fn pinch_preview_offset(anchor: (f32, f32), factor: f32) -> (f32, f32) {
    let travel = factor - 1.0;
    (-anchor.0 * travel, -anchor.1 * travel)
}

fn browser_point_in_pane(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    sidebar_width: f32,
    pane_offset: f32,
    fullscreen: bool,
) -> (i32, i32) {
    let x = (f32::from(position.x) - sidebar_width - CONTENT_BEZEL_X - pane_offset).max(0.0) as i32;
    let top = if fullscreen {
        0.0
    } else {
        active_topbar_height()
    };
    let y = (f32::from(position.y) - top).max(0.0) as i32;
    (x, y)
}

/// Floating viewports (peek) know their exact window-space origin instead of
/// deriving it from the sidebar/topbar layout.
fn browser_point_at(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    sidebar_width: f32,
    pane_offset: f32,
    fullscreen: bool,
    origin_override: Option<(f32, f32)>,
) -> (i32, i32) {
    if let Some((left, top)) = origin_override {
        (
            (f32::from(position.x) - left).max(0.0) as i32,
            (f32::from(position.y) - top).max(0.0) as i32,
        )
    } else {
        browser_point_in_pane(position, sidebar_width, pane_offset, fullscreen)
    }
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
        let tab_id_pinch = self.tab_id.clone();
        let tab_id_exit = self.tab_id.clone();
        let down_sidebar = self.sidebar_width;
        let pane_offset = self.pane_offset;
        let pane_index = self.pane_index;
        let up_sidebar = self.sidebar_width;
        let move_sidebar = self.sidebar_width;
        let scroll_sidebar = self.sidebar_width;
        let pinch_sidebar = self.sidebar_width;
        let parent_down = self.parent.clone();
        let parent_up = self.parent.clone();
        let parent_move = self.parent.clone();
        let parent_scroll = self.parent.clone();
        let parent_pinch = self.parent.clone();
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
                cx.listener(move |_, event: &gpui_kit::MouseDownEvent, window, cx| {
                    let parent = parent_down.clone();
                    let tab_id = tab_id_down.clone();
                    let (x, y) = browser_point_at(
                        event.position,
                        down_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
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
                cx.listener(move |_, event: &gpui_kit::MouseUpEvent, _, cx| {
                    let (x, y) = browser_point_at(
                        event.position,
                        up_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
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
                    let (x, y) = browser_point_at(
                        event.position,
                        sidebar_right_down,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
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
                cx.listener(move |_, event: &gpui_kit::MouseUpEvent, _, cx| {
                    let (x, y) = browser_point_at(
                        event.position,
                        sidebar_right_up,
                        pane_offset,
                        fullscreen,
                        origin_override,
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
                    let (x, y) = browser_point_at(
                        event.position,
                        sidebar_middle_down,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
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
                cx.listener(move |_, event: &gpui_kit::MouseUpEvent, _, cx| {
                    let (x, y) = browser_point_at(
                        event.position,
                        sidebar_middle_up,
                        pane_offset,
                        fullscreen,
                        origin_override,
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
                cx.listener(move |_, event: &gpui_kit::MouseMoveEvent, _, cx| {
                    let (x, y) = browser_point_at(
                        event.position,
                        move_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
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
                move |this, event: &gpui_kit::PinchEvent, _, cx| {
                    let (x, y) = browser_point_at(
                        event.position,
                        pinch_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
                    match event.phase {
                        gpui_kit::TouchPhase::Started => {
                            let base_scale = parent_pinch
                                .update(cx, |lumen, _| lumen.engine.page_scale(&tab_id_pinch))
                                .unwrap_or(1.0);
                            this.begin_pinch(
                                &tab_id_pinch,
                                base_scale,
                                (x as f32, y as f32),
                                cx,
                            );
                        }
                        gpui_kit::TouchPhase::Ended | gpui_kit::TouchPhase::Cancelled => {
                            this.release_pinch(cx);
                        }
                        _ => {
                            if this.pinch_preview.is_none() {
                                let base_scale = parent_pinch
                                    .update(cx, |lumen, _| lumen.engine.page_scale(&tab_id_pinch))
                                    .unwrap_or(1.0);
                                this.begin_pinch(
                                    &tab_id_pinch,
                                    base_scale,
                                    (x as f32, y as f32),
                                    cx,
                                );
                            }
                            this.step_pinch(event.delta, cx);
                        }
                    }
                },
            ))
            .on_scroll(
                cx.listener(move |this, event: &gpui_kit::ScrollEvent, _, cx| {
                    let (x, y) = browser_point_at(
                        event.position,
                        scroll_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
                    let is_precision = matches!(event.delta, gpui_kit::ScrollDelta::Pixels(_));
                    let (dx, dy) = match event.delta {
                        gpui_kit::ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
                        gpui_kit::ScrollDelta::Lines(l) => (l.x * 100.0, l.y * 100.0),
                    };
                    let step_x = step_scroll_accum(&mut this.scroll_accum_x, dx);
                    let step_y = step_scroll_accum(&mut this.scroll_accum_y, dy);
                    if step_x != 0 || step_y != 0 {
                        parent_scroll
                            .update(cx, |this, _| {
                                this.engine.send_mouse_wheel(
                                    &tab_id_scroll,
                                    x,
                                    y,
                                    step_x,
                                    step_y,
                                    &event.modifiers,
                                    is_precision,
                                )
                            })
                            .ok();
                    }
                }),
            )
            .on_drop(
                cx.listener(move |_, drag: &gpui_kit::ExternalPaths, window, cx| {
                    let paths = drag.paths().to_vec();
                    if paths.is_empty() {
                        return;
                    }
                    let (x, y) = browser_point_at(
                        window.mouse_position(),
                        drop_sidebar,
                        pane_offset,
                        fullscreen,
                        origin_override,
                    );
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
                        .rounded(px(CARD_RADIUS))
                        .overflow_hidden()
                        .with_animation(
                            format!("web-view-outgoing-{transition_id}"),
                            crate::ui::motion::content().animation(),
                            |element, progress| {
                                element.opacity(crate::ui::motion::clamp_percentage(1.0 - progress))
                            },
                        )
                        .child(gpui_kit::img(image).size_full().rounded(px(CARD_RADIUS))),
                );
            }
        } else if self.transition_waiting_for_image
            && self.transition_tab.as_deref() == Some(self.tab_id.as_str())
        {
            if let Some(image) = self.previous_image.clone() {
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
                        .rounded(px(CARD_RADIUS))
                        .overflow_hidden()
                        .child(gpui_kit::img(image).size_full().rounded(px(CARD_RADIUS))),
                );
            }
        } else if self.transition_tab.as_deref() == Some(self.tab_id.as_str()) {
            self.previous_image = None;
        }

        if let Some(image) = self.image.clone() {
            let size = image.size(0);
            let (width, height) = frame_logical_size(
                u32::from(size.width),
                u32::from(size.height),
                window.scale_factor(),
            );
            // A live pinch scales this frame around the gesture focal point at
            // display rate; Blink only repaints once, on release.
            let (pinch_factor, pinch_offset) =
                self.pinch_preview
                    .as_ref()
                    .map_or((1.0, (0.0, 0.0)), |preview| {
                        let factor = (preview.scale / preview.base_scale) as f32;
                        if !factor.is_finite() || (factor - 1.0).abs() <= 1e-4 {
                            (1.0, (0.0, 0.0))
                        } else {
                            (factor, pinch_preview_offset(preview.anchor, factor))
                        }
                    });
            let current = div()
                .absolute()
                .top(px(pinch_offset.1))
                .left(px(pinch_offset.0))
                .w(px(width * pinch_factor))
                .h(px(height * pinch_factor))
                .rounded(px(CARD_RADIUS))
                .overflow_hidden()
                .child(gpui_kit::img(image).size_full().rounded(px(CARD_RADIUS)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_handoff_waits_for_edge_fill_release_then_starts_once() {
        let mut handoff = ResizeFrameHandoff::default();
        assert!(!handoff.target_frame_arrived(true));
        assert!(!handoff.target_frame_arrived(true));
        assert!(handoff.target_frame_arrived(false));
        // Further CEF paints update the incoming frame without restarting the fade.
        assert!(!handoff.target_frame_arrived(false));
    }

    #[test]
    fn viewport_coordinates_remove_sidebar_and_topbar_before_scaling() {
        let position = gpui_kit::point(px(340.0), px(72.0));
        assert_eq!(browser_point(position, 320.0), (16, 27));
        assert_eq!(browser_point(position, 64.0), (272, 27));
    }

    #[test]
    fn viewport_mouse_origin_matches_scaled_topbar_height() {
        let at_origin = gpui_kit::point(px(100.0), px(TOPBAR_HEIGHT));
        let below_origin = gpui_kit::point(px(100.0), px(TOPBAR_HEIGHT + 10.0));
        assert_eq!(browser_point(at_origin, 0.0).1, 0);
        assert_eq!(browser_point(below_origin, 0.0).1, 10);
    }

    #[test]
    fn pane_coordinates_are_local_to_the_selected_viewport() {
        let position = gpui_kit::point(px(520.0), px(72.0));
        assert_eq!(
            browser_point_in_pane(position, 320.0, 100.0, false),
            (96, 27)
        );
    }

    #[test]
    fn scroll_accumulator_preserves_subpixel_deltas_across_events() {
        let mut accum = 0.0f32;
        assert_eq!(step_scroll_accum(&mut accum, 0.4), 0);
        assert!((accum - 0.4).abs() < 1e-5);
        assert_eq!(step_scroll_accum(&mut accum, 0.7), 1);
        assert!((accum - 0.1).abs() < 1e-5);
        assert_eq!(step_scroll_accum(&mut accum, -1.3), -1);
        assert!((accum - (-0.2)).abs() < 1e-5);
    }

    #[test]
    fn pinch_steps_scale_around_the_gesture_and_ignore_noise() {
        assert_eq!(pinch_step_scale(1.0, f32::NAN), None);
        assert_eq!(pinch_step_scale(1.0, -1.0), None);
        assert!((pinch_step_scale(1.0, 0.1).unwrap() - 1.1).abs() < 1e-6);
        // Blink clamps the visual viewport to 100 %..400 %.
        assert_eq!(pinch_step_scale(1.0, 5.0), Some(2.0));
        assert_eq!(pinch_step_scale(4.0, 1.0), None);
        // Steps smaller than the tracked precision leave the frame alone.
        assert_eq!(pinch_step_scale(1.5, 1e-9), None);
    }

    #[test]
    fn pinch_preview_keeps_the_focal_point_stationary() {
        let (offset_x, offset_y) = pinch_preview_offset((100.0, 40.0), 2.0);
        assert!((offset_x + 100.0).abs() < 1e-4);
        assert!((offset_y + 40.0).abs() < 1e-4);
        // 1:1 leaves the frame exactly where it is drawn without a preview.
        assert_eq!(pinch_preview_offset((100.0, 40.0), 1.0), (0.0, 0.0));
    }
}
