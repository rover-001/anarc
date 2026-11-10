use super::super::*;

impl Lumen {
    pub(crate) fn theme_picker_space(&self) -> &Space {
        if self.theme_editor.as_ref().is_some_and(|e| e.target_modal) {
            if let Some(modal) = &self.space_modal {
                return &modal.theme_draft;
            }
        }
        self.active_space()
    }

    pub(crate) fn theme_picker_space_mut(&mut self) -> &mut Space {
        if self.theme_editor.as_ref().is_some_and(|e| e.target_modal) && self.space_modal.is_some()
        {
            return &mut self.space_modal.as_mut().unwrap().theme_draft;
        }
        self.active_space_mut()
    }

    pub(crate) fn open_theme_editor(
        &mut self,
        anchor: gpui_kit::Point<gpui_kit::Pixels>,
        target_modal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let viewport = window.viewport_size();
        let margin = 8.0 * UI_SCALE;
        self.sidebar_context_menu = None;
        self.theme_editor = Some(ThemeEditor {
            x: (f32::from(anchor.x) + margin).clamp(
                margin,
                (f32::from(viewport.width) - 304.0 * UI_SCALE - margin).max(margin),
            ),
            y: f32::from(anchor.y).clamp(
                margin,
                (f32::from(viewport.height) - 420.0 * UI_SCALE - margin).max(margin),
            ),
            selected_color: 0,
            dragging: false,
            control: 0,
            target_modal,
            grab_offset: (0.0, 0.0),
            pointer_position: None,
            panel_bounds: Default::default(),
            bounds: Default::default(),
        });
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.pointer_position = None;
        }
        let space = self.theme_picker_space_mut();
        if space.theme_colors.is_empty() {
            space.theme_colors.push(space.color);
        }
        cx.notify();
    }

    pub(in crate::app) fn add_theme_color(&mut self) {
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.pointer_position = None;
        }
        let space = self.theme_picker_space_mut();
        if space.theme_colors.is_empty() {
            space.theme_colors.push(space.color);
        }
        if space.theme_colors.len() < 3 {
            let next = match space.theme_colors.len() {
                1 => 0x76d7b2,
                _ => 0xf6ad55,
            };
            space.theme_colors.push(next);
        }
        self.save_state();
    }

    pub(in crate::app) fn remove_theme_color(&mut self, index: usize) {
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.pointer_position = None;
        }
        let space = self.theme_picker_space_mut();
        if space.theme_colors.is_empty() {
            return;
        }
        if space.theme_colors.len() > 1 && index < space.theme_colors.len() {
            space.theme_colors.remove(index);
        }
        if let Some(first) = space.theme_colors.first() {
            space.color = *first;
        }
        self.save_state();
    }

    pub(in crate::app) fn update_theme_color_from_pointer(&mut self, x: f32, y: f32) {
        let Some(editor) = self.theme_editor.as_ref() else {
            return;
        };
        if !editor.dragging {
            return;
        }
        let bounds = editor.bounds.get()[editor.control];
        let width = f32::from(bounds.size.width).max(1.0);
        let height = f32::from(bounds.size.height).max(1.0);
        let inset = 14.0 * UI_SCALE;
        let horizontal = ((x - editor.grab_offset.0 - f32::from(bounds.origin.x) - inset)
            / (width - 2.0 * inset).max(1.0))
        .clamp(0.0, 1.0);
        let vertical = ((y - editor.grab_offset.1 - f32::from(bounds.origin.y) - inset)
            / (height - 2.0 * inset).max(1.0))
        .clamp(0.0, 1.0);
        if editor.control != 0 {
            let control = editor.control;
            let value =
                slider_drag_value(x - editor.grab_offset.0, f32::from(bounds.origin.x), width);
            if control == 1 {
                self.theme_picker_space_mut().theme_intensity = value * 0.65;
            } else {
                self.theme_picker_space_mut().theme_grain = value;
            }
            self.theme_transition = None;
            return;
        }
        let color = hsv_to_rgb(horizontal * 359.9, (1.0 - vertical) * 0.8, 1.0);
        let index = editor.selected_color;
        self.theme_editor.as_mut().unwrap().pointer_position = Some((index, horizontal, vertical));
        let space = self.theme_picker_space_mut();
        if space.theme_colors.is_empty() {
            space.theme_colors.push(space.color);
        }
        if let Some(slot) = space.theme_colors.get_mut(index) {
            *slot = color;
            if index == 0 {
                space.color = color;
            }
        }
        self.theme_transition = None;
    }

    pub(in crate::app) fn activate_space(&mut self, id: &str) {
        if self.active_space == id {
            return;
        }
        if let Some(to_index) = self.spaces.iter().position(|space| space.id == id) {
            let from_index = self
                .spaces
                .iter()
                .position(|space| space.id == self.active_space)
                .unwrap_or(0);
            let current_space = self.active_space().clone();
            let (outgoing_space, switch_state) = self.space_switch_transition.as_ref().map_or_else(
                || {
                    (
                        current_space.clone(),
                        space_switcher::SpaceSwitchState::new(from_index, to_index),
                    )
                },
                |(previous_space, state, started, _)| {
                    let progress = ui::motion::ease_out_quint(
                        started.elapsed().as_secs_f32()
                            / ui::motion::MOTION_SPACE_SWITCH.as_secs_f32(),
                    );
                    let keep_incoming = progress >= 0.5;
                    let outgoing_space = if keep_incoming {
                        current_space.clone()
                    } else {
                        previous_space.clone()
                    };
                    (
                        outgoing_space,
                        state.retarget(to_index, progress, keep_incoming),
                    )
                },
            );
            let current_theme = visual::theme_for_space(self.active_space());
            let displayed_theme =
                self.theme_transition
                    .map_or(current_theme, |(from, started, _)| {
                        let progress = started.elapsed().as_secs_f32()
                            / ui::motion::MOTION_SPACE_SWITCH
                                .as_secs_f32()
                                .max(f32::EPSILON);
                        visual::interpolate_theme(
                            from,
                            current_theme,
                            ui::motion::ease_out_quint(progress),
                        )
                    });
            self.theme_transition_id = self.theme_transition_id.wrapping_add(1);
            self.theme_transition =
                Some((displayed_theme, Instant::now(), self.theme_transition_id));
            self.space_switch_transition_id = self.space_switch_transition_id.wrapping_add(1);
            self.space_switch_transition = if ui::motion::reduced_motion() {
                None
            } else {
                Some((
                    outgoing_space,
                    switch_state,
                    Instant::now(),
                    self.space_switch_transition_id,
                ))
            };
        }
        self.show_suggestions = false;
        self.active_space = id.to_owned();
        self.save_state();
    }
    pub(in crate::app) fn activate_space_immediate(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.spaces.iter().any(|space| space.id == id) {
            return;
        }
        self.web_viewport.update(cx, |viewport, cx| {
            viewport.clear_transition_image();
            cx.notify();
        });
        self.second_web_viewport.update(cx, |viewport, cx| {
            viewport.clear_transition_image();
            cx.notify();
        });
        self.active_space = id.to_owned();
        self.space_switch_transition = None;
        self.theme_transition = None;
        self.show_suggestions = false;
        self.save_state();
    }
    pub(in crate::app) fn open_create_space_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space_context_menu = None;
        let index = self.spaces.len();
        let default_name = format!("Space {}", index + 1);
        let default_icon = SPACE_ICONS[index % SPACE_ICONS.len()].to_string();
        let default_color = SPACE_COLORS[index % SPACE_COLORS.len()];

        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(&default_name));
        name_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });

        let sub = cx.subscribe(&name_input, |this: &mut Lumen, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.commit_space_modal(cx);
                cx.notify();
            }
        });

        self.space_modal = Some(SpaceModal {
            mode: SpaceModalMode::Create,
            name_input,
            _name_sub: sub,
            selected_icon: default_icon,
            theme_draft: Space::new("draft", "", "", default_color),
        });
    }
    pub(in crate::app) fn open_edit_space_modal(
        &mut self,
        space_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space_context_menu = None;
        let (current_name, current_icon, current_color) = {
            let Some(space) = self.spaces.iter().find(|s| s.id == space_id) else {
                return;
            };
            (space.name.clone(), space.icon.clone(), space.color)
        };

        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Space name"));
        name_input.update(cx, |input, cx| {
            input.set_value(&current_name, window, cx);
            input.focus(window, cx);
        });

        let sub = cx.subscribe(&name_input, |this: &mut Lumen, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.commit_space_modal(cx);
                cx.notify();
            }
        });

        self.space_modal = Some(SpaceModal {
            mode: SpaceModalMode::Edit {
                space_id: space_id.to_string(),
            },
            name_input,
            _name_sub: sub,
            selected_icon: current_icon,
            theme_draft: self
                .spaces
                .iter()
                .find(|space| space.id == space_id)
                .cloned()
                .unwrap_or_else(|| Space::new("draft", "", "", current_color)),
        });
    }
    pub(in crate::app) fn commit_space_modal(&mut self, cx: &Context<Self>) {
        self.theme_editor = None;
        let Some(modal) = self.space_modal.take() else {
            return;
        };
        let input_val = modal.name_input.read(cx).value().to_string();
        let name = if input_val.trim().is_empty() {
            match &modal.mode {
                SpaceModalMode::Create => format!("Space {}", self.spaces.len() + 1),
                SpaceModalMode::Edit { .. } => "Untitled Space".to_string(),
            }
        } else {
            input_val.trim().to_string()
        };

        match modal.mode {
            SpaceModalMode::Create => {
                let id = format!("space-{}", self.spaces.len() + 1);
                let mut space = modal.theme_draft;
                space.id = id.clone();
                space.name = name.clone();
                space.icon = modal.selected_icon;
                let tab_id = format!("tab-{}-1", id);
                let new_tab = Tab::new(&tab_id, "New tab", "about:blank");
                space.tabs.push(new_tab);
                space.active = Some(tab_id);

                self.spaces.push(space);
                self.activate_space(&id);
            }
            SpaceModalMode::Edit { space_id } => {
                if let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id) {
                    space.name = name;
                    space.icon = modal.selected_icon;
                    space.color = modal.theme_draft.color;
                    space.theme_colors = modal.theme_draft.theme_colors;
                    space.theme_intensity = modal.theme_draft.theme_intensity;
                    space.theme_grain = modal.theme_draft.theme_grain;
                    space.theme_appearance = modal.theme_draft.theme_appearance;
                }
            }
        }
        self.save_state();
    }
    pub(in crate::app) fn delete_space(&mut self, space_id: &str) {
        self.space_context_menu = None;
        if self.spaces.len() <= 1 {
            return;
        }
        if let Some(idx) = self.spaces.iter().position(|s| s.id == space_id) {
            for tab in &self.spaces[idx].tabs {
                self.engine.close_browser(&tab.id);
            }
            self.spaces.remove(idx);
            if self.active_space == space_id {
                self.active_space = self.spaces[0].id.clone();
            }
            self.save_state();
        }
    }
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let chroma = value * saturation;
    let h = hue / 60.0;
    let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    let channel = |value: f32| ((value + m).clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(r) << 16) | (channel(g) << 8) | channel(b)
}

fn slider_drag_value(x: f32, origin: f32, width: f32) -> f32 {
    let inset = 8.0 * UI_SCALE;
    ((x - origin - inset) / (width - 2.0 * inset).max(1.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod theme_drag_tests {
    use super::*;

    #[test]
    fn sliders_track_the_thumb_center_and_clamp_at_both_ends() {
        let origin = 40.0 * UI_SCALE;
        let width = 128.0 * UI_SCALE;
        assert_eq!(slider_drag_value(origin, origin, width), 0.0);
        assert_eq!(
            slider_drag_value(origin + 8.0 * UI_SCALE, origin, width),
            0.0
        );
        assert!((slider_drag_value(origin + width / 2.0, origin, width) - 0.5).abs() < 0.0001);
        assert!(
            (slider_drag_value(origin + width - 8.0 * UI_SCALE, origin, width) - 1.0).abs()
                < 0.0001
        );
        assert_eq!(slider_drag_value(origin + width * 2.0, origin, width), 1.0);
    }
}
