use gpui_kit::Context;

use super::WebViewport;

const MIN_IMAGE_SCALE: f64 = 1.0;
const MAX_IMAGE_SCALE: f64 = 8.0;

/// Persistent viewport magnification of the latest CEF frame. The web page
/// stays at its original scale while this image is zoomed and panned locally.
pub(super) struct PinchPreview {
    pub(super) scale: f64,
    pub(super) offset: (f32, f32),
    anchor: (f32, f32),
    gesture_scale: f32,
}

impl WebViewport {
    pub(super) fn begin_pinch(&mut self, anchor: (f32, f32)) {
        let (scale, offset) = self
            .pinch_preview
            .as_ref()
            .map_or((1.0, (0.0, 0.0)), |preview| (preview.scale, preview.offset));
        self.pinch_preview = Some(PinchPreview {
            scale,
            offset,
            anchor,
            gesture_scale: 1.0,
        });
        self.scroll_motion.cancel();
        self.pinch_active = true;
    }

    pub(super) fn step_pinch(
        &mut self,
        delta: f32,
        anchor: (f32, f32),
        viewport_size: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let Some(preview) = self.pinch_preview.as_mut() else {
            return;
        };
        #[cfg(target_os = "linux")]
        let delta = {
            // Linux adapters report the difference between consecutive absolute scales.
            let next_gesture_scale = (preview.gesture_scale + delta).max(0.01);
            let relative_delta = next_gesture_scale / preview.gesture_scale - 1.0;
            preview.gesture_scale = next_gesture_scale;
            relative_delta
        };
        let Some(next_scale) = pinch_step_scale(preview.scale, delta) else {
            return;
        };
        let factor = (next_scale / preview.scale) as f32;
        preview.offset = pinch_transform_offset(preview.anchor, anchor, preview.offset, factor);
        preview.offset = clamp_pinch_offset(preview.offset, next_scale as f32, viewport_size);
        preview.scale = next_scale;
        preview.anchor = anchor;
        if (preview.scale - 1.0).abs() < 1e-3 {
            preview.scale = 1.0;
            preview.offset = (0.0, 0.0);
        }
        cx.notify();
    }

    pub(super) fn pan_image(
        &mut self,
        delta: (f32, f32),
        viewport_size: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let Some(preview) = self.pinch_preview.as_mut() else {
            return;
        };
        if preview.scale <= 1.0 + 1e-3 {
            return;
        }
        preview.offset.0 += delta.0;
        preview.offset.1 += delta.1;
        preview.offset = clamp_pinch_offset(preview.offset, preview.scale as f32, viewport_size);
        cx.notify();
    }

    pub(super) fn map_browser_point(&self, point: (i32, i32)) -> (i32, i32) {
        let Some(preview) = self.pinch_preview.as_ref() else {
            return point;
        };
        if preview.scale <= 1.0 + 1e-3 {
            return point;
        }
        (
            ((point.0 as f32 - preview.offset.0) / preview.scale as f32).max(0.0) as i32,
            ((point.1 as f32 - preview.offset.1) / preview.scale as f32).max(0.0) as i32,
        )
    }

    pub(super) fn cancel_pinch(&mut self) {
        self.pinch_preview = None;
        self.pinch_active = false;
    }
}

/// Accumulated bitmap scale. Pinch deltas are relative factors and the page
/// itself never receives a zoom command.
fn pinch_step_scale(current: f64, delta: f32) -> Option<f64> {
    if !delta.is_finite() || !current.is_finite() || current <= 0.0 {
        return None;
    }
    let factor = f64::from((1.0 + delta).clamp(0.5, 2.0));
    let next = (current * factor).clamp(MIN_IMAGE_SCALE, MAX_IMAGE_SCALE);
    ((next - current).abs() > 1e-6).then_some(next)
}

fn pinch_transform_offset(
    old_anchor: (f32, f32),
    new_anchor: (f32, f32),
    old_offset: (f32, f32),
    factor: f32,
) -> (f32, f32) {
    (
        new_anchor.0 - (old_anchor.0 - old_offset.0) * factor,
        new_anchor.1 - (old_anchor.1 - old_offset.1) * factor,
    )
}

pub(super) fn clamp_pinch_offset(
    offset: (f32, f32),
    scale: f32,
    viewport: (f32, f32),
) -> (f32, f32) {
    let min_x = viewport.0 * (1.0 - scale);
    let min_y = viewport.1 * (1.0 - scale);
    (offset.0.clamp(min_x, 0.0), offset.1.clamp(min_y, 0.0))
}
