use super::*;
use gpui_kit::{Bounds, Pixels, canvas, fill, point, size};
use std::{cell::Cell, rc::Rc};

type PickerBounds = Rc<Cell<[Bounds<Pixels>; 3]>>;

thread_local! {
    static DOT_GRID_TEXTURES: std::cell::RefCell<Vec<((u32, u32, u32), std::sync::Arc<gpui_kit::RenderImage>)>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

fn dot_grid_texture(
    width: u32,
    height: u32,
    scale: f32,
    window: &mut gpui_kit::Window,
) -> std::sync::Arc<gpui_kit::RenderImage> {
    DOT_GRID_TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        let key = (width, height, scale.to_bits());
        if let Some(index) = cache.iter().position(|(size, _)| *size == key) {
            let entry = cache.remove(index);
            let image = entry.1.clone();
            cache.push(entry);
            return image;
        }

        let mut pixels = image::RgbaImage::new(width.max(1), height.max(1));
        let step = (7.0 * UI_SCALE * scale).max(1.0);
        let pixel = image::Rgba([150, 156, 162, 255]);
        let mut y = 2.0 * scale;
        while y < height as f32 {
            let mut x = 2.0 * scale;
            while x < width as f32 {
                let pixel_x = (x.round() as u32).min(width.saturating_sub(1));
                let pixel_y = (y.round() as u32).min(height.saturating_sub(1));
                pixels.put_pixel(pixel_x, pixel_y, pixel);
                x += step;
            }
            y += step;
        }
        let image = std::sync::Arc::new(gpui_kit::RenderImage::new(smallvec::smallvec![
            image::Frame::new(pixels)
        ]));
        if cache.len() == 2 {
            let (_, retired) = cache.remove(0);
            let _ = window.drop_image(retired);
        }
        cache.push((key, image.clone()));
        image
    })
}

fn measured(bounds: PickerBounds, control: usize, dotted: bool, value: f32) -> impl IntoElement {
    canvas(
        move |area, _, _| {
            let mut all = bounds.get();
            all[control] = area;
            bounds.set(all);
        },
        move |area, _, window, _| {
            if control != 0 {
                let width = f32::from(area.size.width);
                let height = f32::from(area.size.height);
                let inset = 5.0 * UI_SCALE;
                let thumb_width = 6.0 * UI_SCALE;
                let travel = (width - 2.0 * inset - thumb_width).max(1.0);
                let thumb_x = inset + value.clamp(0.0, 1.0) * travel;
                window.paint_quad(fill(area, rgb(0x585b59)).corner_radii(px(6.0 * UI_SCALE)));
                window.paint_quad(
                    fill(
                        Bounds::new(
                            area.origin,
                            size(px(thumb_x + thumb_width), area.size.height),
                        ),
                        rgb(0x666966),
                    )
                    .corner_radii(px(6.0 * UI_SCALE)),
                );
                window.paint_quad(
                    fill(
                        Bounds::new(
                            area.origin + point(px(thumb_x), px(inset)),
                            size(px(thumb_width), px(height - 2.0 * inset)),
                        ),
                        rgb(0xffffff),
                    )
                    .corner_radii(px(thumb_width / 2.0)),
                );
            }
            if dotted {
                let scale = window.scale_factor();
                let width = (f32::from(area.size.width) * scale).ceil() as u32;
                let height = (f32::from(area.size.height) * scale).ceil() as u32;
                let image = dot_grid_texture(width, height, scale, window);
                let _ = window.paint_image(area, area, px(0.0).into(), image, 0, false);
            }
        },
    )
    .absolute()
    .inset_0()
}

fn color_position(color: u32) -> (f32, f32) {
    let r = ((color >> 16) & 255) as f32 / 255.0;
    let g = ((color >> 8) & 255) as f32 / 255.0;
    let b = (color & 255) as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta < 0.001 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } / 6.0;
    (hue, 1.0 - if max > 0.0 { delta / max / 0.8 } else { 0.0 })
}

impl Lumen {
    pub(super) fn render_theme_picker(
        &self,
        _theme: visual::SpaceTheme,
        x: f32,
        y: f32,
        selected: usize,
        colors: &[u32],
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let bounds = self.theme_editor.as_ref().unwrap().bounds.clone();
        let space = self.theme_picker_space();
        let picker_theme = visual::theme_for_space(space);
        let mut modes = div()
            .h(px(36.0 * UI_SCALE))
            .w_full()
            .flex()
            .justify_center()
            .gap_2();
        for (mode, label) in ["◐", "☀", "☾"].into_iter().enumerate() {
            modes = modes.child(
                div()
                    .id(format!("theme-mode-{mode}"))
                    .size(px(28.0 * UI_SCALE))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(picker_theme.text))
                    .cursor_pointer()
                    .when(space.theme_appearance == mode as u8, |d| {
                        d.bg(rgb(0x77727f))
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.theme_picker_space_mut().theme_appearance = mode as u8;
                        this.save_state();
                        cx.notify();
                    })),
            );
        }
        let mut pad = div()
            .id("theme-color-pad")
            .relative()
            .w(px(280.0 * UI_SCALE))
            .h(px(220.0 * UI_SCALE))
            .flex_shrink_0()
            .rounded_lg()
            .bg(rgb(picker_theme.surface))
            .overflow_hidden()
            .child(measured(bounds.clone(), 0, true, 0.0))
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|this, event: &gpui_kit::MouseDownEvent, _, cx| {
                    if let Some(editor) = this.theme_editor.as_mut() {
                        editor.dragging = true;
                        editor.control = 0;
                        editor.grab_offset = (0.0, 0.0);
                    }
                    this.update_theme_color_from_pointer(
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                    );
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        for (index, color) in colors.iter().copied().enumerate() {
            let (hue, saturation) = self
                .theme_editor
                .as_ref()
                .and_then(|editor| editor.pointer_position)
                .filter(|(i, _, _)| *i == index)
                .map(|(_, x, y)| (x, y))
                .unwrap_or_else(|| color_position(color));
            let saturation = saturation.clamp(0.0, 1.0);
            pad = pad.child(
                div()
                    .id(format!("theme-dot-{index}"))
                    .absolute()
                    .left(px(hue.clamp(0.0, 1.0) * 252.0 * UI_SCALE))
                    .top(px(saturation * 192.0 * UI_SCALE))
                    .size(px(28.0 * UI_SCALE))
                    .rounded_full()
                    .bg(rgb(color))
                    .border_2()
                    .border_color(rgb(if selected == index {
                        0xffffff
                    } else {
                        0xbcbcbc
                    }))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui_kit::MouseButton::Left,
                        cx.listener(move |this, event: &gpui_kit::MouseDownEvent, _, cx| {
                            if let Some(editor) = this.theme_editor.as_mut() {
                                let area = editor.bounds.get()[0];
                                editor.grab_offset = (
                                    f32::from(event.position.x - area.origin.x)
                                        - 14.0 * UI_SCALE
                                        - hue * (f32::from(area.size.width) - 28.0 * UI_SCALE),
                                    f32::from(event.position.y - area.origin.y)
                                        - 14.0 * UI_SCALE
                                        - saturation
                                            * (f32::from(area.size.height) - 28.0 * UI_SCALE),
                                );
                                editor.selected_color = index;
                                editor.dragging = true;
                                editor.control = 0;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }
        let buttons = div()
            .h(px(32.0 * UI_SCALE))
            .w_full()
            .flex()
            .justify_center()
            .gap_3()
            .child(
                div()
                    .id("theme-remove")
                    .size(px(28.0 * UI_SCALE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(rgb(picker_theme.text))
                    .child("−")
                    .on_mouse_down(
                        gpui_kit::MouseButton::Left,
                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_theme_color(selected);
                        let last = this
                            .theme_picker_space()
                            .theme_colors
                            .len()
                            .saturating_sub(1);
                        if let Some(editor) = this.theme_editor.as_mut() {
                            editor.selected_color = selected.min(last);
                        }
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("theme-add")
                    .size(px(28.0 * UI_SCALE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(rgb(picker_theme.text))
                    .child("+")
                    .on_mouse_down(
                        gpui_kit::MouseButton::Left,
                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_theme_color();
                        let last = this
                            .theme_picker_space()
                            .theme_colors
                            .len()
                            .saturating_sub(1);
                        if let Some(editor) = this.theme_editor.as_mut() {
                            editor.selected_color = last;
                        }
                        cx.notify();
                    })),
            );
        let mut presets = div()
            .w_full()
            .flex()
            .justify_between()
            .items_center()
            .py_3();
        for (index, color) in [
            0xf3f1e9, 0xe5a7c1, 0xad7ba5, 0xdf697c, 0xee975f, 0xf4d76d, 0x77d7a1, 0x76b8c9,
            0x7976af,
        ]
        .into_iter()
        .enumerate()
        {
            presets = presets.child(
                div()
                    .id(format!("theme-preset-{index}"))
                    .size(px(19.0 * UI_SCALE))
                    .rounded_full()
                    .bg(rgb(color))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(editor) = this.theme_editor.as_mut() {
                            editor.pointer_position = None;
                        }
                        let space = this.theme_picker_space_mut();
                        if space.theme_colors.is_empty() {
                            space.theme_colors.push(color);
                        } else if let Some(slot) = space.theme_colors.get_mut(selected) {
                            *slot = color;
                        }
                        space.color = space.theme_colors[0];
                        this.save_state();
                        cx.notify();
                    })),
            );
        }
        let mut controls = div().flex().items_center().gap_3();
        for (control, value, label) in [
            (1, space.theme_intensity / 0.65, "Intensity"),
            (2, space.theme_grain, "Grain"),
        ] {
            controls = controls.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_xs()
                    .text_color(rgb(picker_theme.muted))
                    .child(label)
                    .child(
                        div()
                            .id(format!("theme-slider-{control}"))
                            .relative()
                            .w_full()
                            .h(px(24.0 * UI_SCALE))
                            .rounded_lg()
                            .cursor_pointer()
                            .child(measured(bounds.clone(), control, false, value))
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(
                                    move |this, event: &gpui_kit::MouseDownEvent, _, cx| {
                                        let value = if control == 1 {
                                            this.theme_picker_space().theme_intensity / 0.65
                                        } else {
                                            this.theme_picker_space().theme_grain
                                        };
                                        if let Some(editor) = this.theme_editor.as_mut() {
                                            editor.dragging = true;
                                            editor.control = control;
                                            let area = editor.bounds.get()[control];
                                            let inset = 8.0 * UI_SCALE;
                                            let travel =
                                                (f32::from(area.size.width) - 2.0 * inset).max(1.0);
                                            let thumb_x =
                                                f32::from(area.origin.x) + inset + value * travel;
                                            let offset = f32::from(event.position.x) - thumb_x;
                                            // Grabbing the handle preserves its position; clicking the track jumps to it.
                                            editor.grab_offset = (
                                                if offset.abs() <= inset { offset } else { 0.0 },
                                                0.0,
                                            );
                                        }
                                        this.update_theme_color_from_pointer(
                                            f32::from(event.position.x),
                                            f32::from(event.position.y),
                                        );
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                ),
                            ),
                    ),
            );
        }
        let move_entity = cx.entity().downgrade();
        let up_entity = move_entity.clone();
        let dismiss_entity = move_entity.clone();
        let panel_bounds = self.theme_editor.as_ref().unwrap().panel_bounds.clone();
        let tracking = canvas(
            move |area, _, _| panel_bounds.set(area),
            move |_, _, window, _| {
                window.on_mouse_event::<gpui_kit::MouseDownEvent>(move |event, phase, _, cx| {
                    if phase != gpui_kit::DispatchPhase::Capture {
                        return;
                    }
                    let _ = dismiss_entity.update(cx, |this, cx| {
                        let outside = this.theme_editor.as_ref().is_some_and(|editor| {
                            !editor.panel_bounds.get().contains(&event.position)
                        });
                        if outside {
                            this.theme_editor = None;
                            this.save_state();
                            cx.notify();
                            cx.stop_propagation();
                        }
                    });
                });
                window.on_mouse_event::<gpui_kit::MouseMoveEvent>(move |event, phase, _, cx| {
                    if phase != gpui_kit::DispatchPhase::Capture {
                        return;
                    }
                    let _ = move_entity.update(cx, |this, cx| {
                        if !this.theme_editor.as_ref().is_some_and(|e| e.dragging) {
                            return;
                        }
                        if event.pressed_button == Some(gpui_kit::MouseButton::Left) {
                            this.update_theme_color_from_pointer(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                            );
                        } else {
                            if let Some(editor) = this.theme_editor.as_mut() {
                                editor.dragging = false;
                            }
                            this.save_state();
                        }
                        cx.notify();
                    });
                });
                window.on_mouse_event::<gpui_kit::MouseUpEvent>(move |event, phase, _, cx| {
                    if phase != gpui_kit::DispatchPhase::Capture
                        || event.button != gpui_kit::MouseButton::Left
                    {
                        return;
                    }
                    let _ = up_entity.update(cx, |this, cx| {
                        if this.theme_editor.as_ref().is_some_and(|e| e.dragging) {
                            this.update_theme_color_from_pointer(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                            );
                            if let Some(editor) = this.theme_editor.as_mut() {
                                editor.dragging = false;
                            }
                            this.save_state();
                            cx.notify();
                        }
                    });
                });
            },
        )
        .absolute()
        .inset_0();
        div()
            .id("theme-editor-panel")
            .occlude()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(304.0 * UI_SCALE + 2.0))
            .p(px(12.0 * UI_SCALE))
            .rounded_xl()
            .overflow_hidden()
            .bg(rgb(picker_theme.base))
            .border_1()
            .border_color(rgb(picker_theme.border))
            .flex()
            .flex_col()
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(visual::space_backdrop_rounded(space, 12.0 * UI_SCALE))
            .child(tracking)
            .child(modes)
            .child(pad)
            .child(buttons)
            .child(presets)
            .child(controls)
            .into_any_element()
    }
}
