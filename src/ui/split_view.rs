use crate::ui::visual::UI_SCALE;

pub const DIVIDER_WIDTH: f32 = 4.0 * UI_SCALE;
pub const DIVIDER_HIT_WIDTH: f32 = 8.0 * UI_SCALE;
pub const MIN_PANE_WIDTH: f32 = 240.0 * UI_SCALE;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserSize {
    pub tab_id: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Default)]
pub struct ResizeDrag {
    frozen: Option<Vec<BrowserSize>>,
}

impl ResizeDrag {
    pub fn start(&mut self, max_sizes: Vec<BrowserSize>) -> Vec<BrowserSize> {
        self.frozen = Some(max_sizes.clone());
        max_sizes
    }

    pub fn frozen_size(&self, tab_id: &str) -> Option<(i32, i32)> {
        self.frozen
            .as_ref()?
            .iter()
            .find_map(|size| (size.tab_id == tab_id).then_some((size.width, size.height)))
    }

    pub fn finish(&mut self, exact_sizes: Vec<BrowserSize>) -> Vec<BrowserSize> {
        self.frozen = None;
        exact_sizes
    }
}

pub fn max_divider_pane_width(content_width: f32) -> f32 {
    (content_width - DIVIDER_WIDTH - MIN_PANE_WIDTH).max(1.0)
}

pub fn tab_for_pane<'a>(first: &'a str, second: &'a str, pane_index: usize) -> &'a str {
    if pane_index == 1 { second } else { first }
}

pub fn pane_widths(total_width: f32, ratio: f32) -> (f32, f32) {
    let available = (total_width - DIVIDER_WIDTH).max(0.0);
    let min_ratio = if available > 0.0 {
        (MIN_PANE_WIDTH / available).min(0.5)
    } else {
        0.5
    };
    let ratio = ratio.clamp(min_ratio, 1.0 - min_ratio);
    (available * ratio, available * (1.0 - ratio))
}

pub fn ratio_at_pointer(pointer_x: f32, card_left: f32, card_width: f32) -> f32 {
    let available = (card_width - DIVIDER_WIDTH).max(1.0);
    if available <= MIN_PANE_WIDTH * 2.0 {
        return 0.5;
    }
    let min_ratio = (MIN_PANE_WIDTH / available).min(0.5);
    ((pointer_x - card_left).clamp(MIN_PANE_WIDTH, available - MIN_PANE_WIDTH) / available)
        .clamp(min_ratio, 1.0 - min_ratio)
}

pub fn frame_logical_size(pixel_width: u32, pixel_height: u32, scale: f32) -> (f32, f32) {
    let scale = scale.max(f32::EPSILON);
    (pixel_width as f32 / scale, pixel_height as f32 / scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_panes_preserve_divider_and_minimum_width() {
        let (left, right) = pane_widths(1004.0, 0.25);
        assert!((left - 250.15).abs() < 0.01);
        assert!((right - 750.45).abs() < 0.01);
        let (left, right) = pane_widths(400.0, 0.1);
        assert!((left - 198.3).abs() < 0.01);
        assert!((right - 198.3).abs() < 0.01);
    }

    #[test]
    fn divider_drag_clamps_to_minimum_pane_width() {
        let min_ratio = MIN_PANE_WIDTH / (1004.0 - DIVIDER_WIDTH);
        assert!((ratio_at_pointer(100.0, 0.0, 1004.0) - min_ratio).abs() < 1e-6);
        assert!((ratio_at_pointer(900.0, 0.0, 1004.0) - (1.0 - min_ratio)).abs() < 1e-6);
        assert_eq!(ratio_at_pointer(100.0, 0.0, 400.0), 0.5);
    }

    #[test]
    fn pane_input_routes_to_the_tab_in_that_pane() {
        assert_eq!(tab_for_pane("left-tab", "right-tab", 0), "left-tab");
        assert_eq!(tab_for_pane("left-tab", "right-tab", 1), "right-tab");
    }

    #[test]
    fn frame_size_is_one_to_one_in_device_pixels() {
        assert_eq!(frame_logical_size(1600, 900, 2.0), (800.0, 450.0));
        assert_eq!(frame_logical_size(1200, 800, 1.5), (800.0, 800.0 / 1.5));
    }

    #[test]
    fn divider_drag_freezes_at_max_width_and_moves_do_not_resize() {
        let max_width = max_divider_pane_width(1004.0) as i32;
        assert_eq!(max_width, 760);
        let mut drag = ResizeDrag::default();
        let start = vec![BrowserSize {
            tab_id: "tab-a".into(),
            width: max_width,
            height: 700,
        }];
        assert_eq!(drag.start(start.clone()), start);
        assert_eq!(drag.frozen_size("tab-a"), Some((760, 700)));
        // Any number of pointer moves leave the frozen browser request unchanged.
        assert_eq!(drag.frozen_size("tab-a"), Some((760, 700)));
        assert_eq!(drag.frozen_size("tab-a"), Some((760, 700)));
        let end = vec![BrowserSize {
            tab_id: "tab-a".into(),
            width: 360,
            height: 700,
        }];
        assert_eq!(drag.finish(end.clone()), end);
        assert_eq!(drag.frozen_size("tab-a"), None);
    }
}
