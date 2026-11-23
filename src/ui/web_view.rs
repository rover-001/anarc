use std::sync::Arc;

use crate::app::{CONTENT_BEZEL_X, Anarc};
#[cfg(test)]
use crate::ui::visual::TOPBAR_HEIGHT;
#[cfg(test)]
use gpui_kit::px;

use crate::ui::visual::{SpaceTheme, active_topbar_height, space_theme};

mod pinch;
mod render;
mod scroll;
use pinch::PinchPreview;
use scroll::ScrollMotion;
#[cfg(test)]
use scroll::step_scroll_accum;

/// Wheel scroll scale. Default is 1.0 (53px per notch, matching standard 3-line scroll).
/// Can be overridden via ANARC_SCROLL_SCALE env var.
pub fn wheel_scroll_scale() -> f32 {
    static SCALE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *SCALE.get_or_init(|| {
        std::env::var("ANARC_SCROLL_SCALE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0)
    })
}

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

pub struct WebViewport {
    parent: gpui_kit::WeakEntity<Anarc>,
    focus_handle: gpui_kit::FocusHandle,
    tab_id: String,
    sidebar_width: f32,
    pane_offset: f32,
    pane_y_offset: f32,
    pane_index: usize,
    /// Logical pane index used for input routing and drop targets. Unlike
    /// `pane_index` (which selects corner radii) this is always 0-based and
    /// contiguous, including the right-hand panes of a three-pane split.
    focus_pane_index: usize,
    split_layout: bool,
    /// True when panes form a 2x2 grid rather than a single split row.
    split_grid: bool,
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
    scroll_motion: ScrollMotion,
    /// Persistent bitmap magnification; see [`PinchPreview`].
    pinch_preview: Option<PinchPreview>,
    /// Whether a touch/trackpad pinch gesture is currently active between Started and Ended.
    pinch_active: bool,
    /// Absolute window-space origin of this viewport. Used by the peek overlay,
    /// which floats outside the regular content column.
    origin_override: Option<(f32, f32)>,
    /// Peek viewports input focus from `peek_focused` instead of the split pane
    /// index and skip the card-edge chrome ring.
    peek_mode: bool,
}

impl WebViewport {
    pub fn new(parent: gpui_kit::WeakEntity<Anarc>, focus_handle: gpui_kit::FocusHandle) -> Self {
        Self {
            parent,
            focus_handle,
            tab_id: String::new(),
            sidebar_width: 320.0,
            pane_offset: 0.0,
            pane_y_offset: 0.0,
            pane_index: 0,
            focus_pane_index: 0,
            split_layout: false,
            split_grid: false,
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
            scroll_motion: ScrollMotion::default(),
            pinch_preview: None,
            pinch_active: false,
            origin_override: None,
            peek_mode: false,
        }
    }

    pub fn set_origin_override(&mut self, origin: Option<(f32, f32)>) -> bool {
        std::mem::replace(&mut self.origin_override, origin) != origin
    }

    pub fn set_peek_mode(&mut self, peek_mode: bool) -> bool {
        std::mem::replace(&mut self.peek_mode, peek_mode) != peek_mode
    }

    pub fn set_split_grid(&mut self, split_grid: bool) -> bool {
        std::mem::replace(&mut self.split_grid, split_grid) != split_grid
    }

    pub fn set_focus_pane_index(&mut self, focus_pane_index: usize) -> bool {
        std::mem::replace(&mut self.focus_pane_index, focus_pane_index) != focus_pane_index
    }

    pub fn set_pane_y_offset(&mut self, pane_y_offset: f32) -> bool {
        std::mem::replace(&mut self.pane_y_offset, pane_y_offset) != pane_y_offset
    }

    pub fn has_content(&self) -> bool {
        self.image.is_some() || self.previous_image.is_some() || !self.tab_id.is_empty()
    }

    /// Drops the current frame and tab binding when the peek overlay closes.
    pub fn clear_content(&mut self) {
        self.cancel_pinch();
        self.scroll_motion.cancel();
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
    ) -> bool {
        let changed = self.tab_id != tab_id
            || self.sidebar_width != effective_sidebar_width
            || self.image != image
            || self.cursor != cursor
            || self.pane_offset != pane_offset
            || self.pane_index != pane_index
            || self.split_layout != split_layout
            || self.drag_edge_color != drag_edge_color
            || !retired_images.is_empty();
        if self.tab_id != tab_id {
            self.scroll_motion.cancel();
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
        let transition_started = self.transition_tab.as_deref() == Some(tab_id.as_str())
            && self.transition_waiting_for_image
            && image.is_some();
        if transition_started {
            self.transition_waiting_for_image = false;
            self.transition_id = self.transition_id.wrapping_add(1);
            self.transition_cleanup_scheduled = false;
        }
        self.tab_id = tab_id;
        self.sidebar_width = effective_sidebar_width;
        self.image = image;
        self.cursor = cursor;
        self.retired_images.extend(retired_images);
        self.pane_offset = pane_offset;
        self.pane_index = pane_index;
        self.split_layout = split_layout;
        self.drag_edge_color = drag_edge_color;
        changed || transition_started
    }

    #[cfg(not(test))]
    pub fn update_frame(
        &mut self,
        tab_id: String,
        image: Option<Arc<gpui_kit::RenderImage>>,
        cursor: i32,
        retired_images: Vec<Arc<gpui_kit::RenderImage>>,
        drag_edge_color: Option<u32>,
    ) -> bool {
        self.set_frame(
            tab_id,
            self.sidebar_width,
            image,
            cursor,
            retired_images,
            self.pane_offset,
            self.pane_index,
            self.split_layout,
            drag_edge_color,
        )
    }

    #[cfg(not(test))]
    pub fn set_content(
        &mut self,
        tab_id: String,
        image: Option<Arc<gpui_kit::RenderImage>>,
        cursor: i32,
        retired_images: Vec<Arc<gpui_kit::RenderImage>>,
    ) -> bool {
        let changed = self.tab_id != tab_id
            || self.image != image
            || self.cursor != cursor
            || !retired_images.is_empty();
        if self.tab_id != tab_id {
            self.scroll_motion.cancel();
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
        self.tab_id = tab_id;
        self.image = image;
        self.cursor = cursor;
        self.retired_images.extend(retired_images);
        changed
    }

    pub fn set_theme(&mut self, theme: SpaceTheme) -> bool {
        std::mem::replace(&mut self.theme, theme) != theme
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) -> bool {
        std::mem::replace(&mut self.fullscreen, fullscreen) != fullscreen
    }

    /// Drops the previous tab's frame before an immediate cross-space jump.
    pub fn clear_transition_image(&mut self) -> bool {
        let changed = self.image.is_some()
            || self.previous_image.is_some()
            || self.resize_source_image.is_some()
            || self.pinch_preview.is_some()
            || self.transition_tab.is_some()
            || self.transition_waiting_for_image;
        self.cancel_pinch();
        self.scroll_motion.cancel();
        self.image = None;
        self.previous_image = None;
        self.resize_source_image = None;
        self.transition_tab = None;
        self.transition_waiting_for_image = false;
        self.transition_cleanup_scheduled = false;
        changed
    }
}

#[cfg(test)]
fn browser_point(position: gpui_kit::Point<gpui_kit::Pixels>, sidebar_width: f32) -> (i32, i32) {
    browser_point_in_pane(position, sidebar_width, 0.0, 0.0, false)
}

fn browser_point_in_pane(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    sidebar_width: f32,
    pane_offset: f32,
    pane_y_offset: f32,
    fullscreen: bool,
) -> (i32, i32) {
    let x = (f32::from(position.x) - sidebar_width - CONTENT_BEZEL_X - pane_offset).max(0.0) as i32;
    let top = if fullscreen {
        0.0
    } else {
        active_topbar_height()
    };
    let y = (f32::from(position.y) - top - pane_y_offset).max(0.0) as i32;
    (x, y)
}

/// Floating viewports (peek) know their exact window-space origin instead of
/// deriving it from the sidebar/topbar layout.
fn browser_point_at(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    sidebar_width: f32,
    pane_offset: f32,
    pane_y_offset: f32,
    fullscreen: bool,
    origin_override: Option<(f32, f32)>,
) -> (i32, i32) {
    if let Some((left, top)) = origin_override {
        (
            (f32::from(position.x) - left).max(0.0) as i32,
            (f32::from(position.y) - top - pane_y_offset).max(0.0) as i32,
        )
    } else {
        browser_point_in_pane(position, sidebar_width, pane_offset, pane_y_offset, fullscreen)
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
        let expected_y = (72.0 - active_topbar_height()) as i32;
        assert_eq!(browser_point(position, 320.0), (16, expected_y));
        assert_eq!(browser_point(position, 64.0), (272, expected_y));
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
        let expected_y = (72.0 - active_topbar_height()) as i32;
        assert_eq!(
            browser_point_in_pane(position, 320.0, 100.0, 0.0, false),
            (96, expected_y)
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
}
