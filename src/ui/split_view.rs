pub const DIVIDER_WIDTH: f32 = 4.0;
pub const MIN_PANE_WIDTH: f32 = 240.0;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_panes_preserve_divider_and_minimum_width() {
        assert_eq!(pane_widths(1004.0, 0.25), (250.0, 750.0));
        assert_eq!(pane_widths(400.0, 0.1), (198.0, 198.0));
    }

    #[test]
    fn divider_drag_clamps_to_minimum_pane_width() {
        assert_eq!(ratio_at_pointer(100.0, 0.0, 1004.0), 0.24);
        assert_eq!(ratio_at_pointer(900.0, 0.0, 1004.0), 0.76);
        assert_eq!(ratio_at_pointer(100.0, 0.0, 400.0), 0.5);
    }

    #[test]
    fn pane_input_routes_to_the_tab_in_that_pane() {
        assert_eq!(tab_for_pane("left-tab", "right-tab", 0), "left-tab");
        assert_eq!(tab_for_pane("left-tab", "right-tab", 1), "right-tab");
    }
}
