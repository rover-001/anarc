mod engine;
mod history;
mod import;
mod lifecycle;
mod model;
mod paths;
mod persist;
mod reader;
mod shield;
mod stats;
mod ui;

use std::time::Instant;

use gpui_kit::{
    Animation, AnimationExt as _, AnyView, Bounds, Context, Focusable, FontWeight, KeyDownEvent,
    KeyUpEvent, Render, SpringAnimation, SpringConfig, StyleRefinement, Window, WindowBounds,
    WindowOptions,
    assets::IconName,
    component::Icon,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::*,
    px, rgb, size,
};
use model::{Space, SplitPair, Tab, TabState};
use ui::command_palette::{self, PaletteAction, PaletteItem, PaletteTab, PaletteTarget};
use ui::split_view::{self, DIVIDER_HIT_WIDTH, DIVIDER_WIDTH};
use ui::visual::{self, ADDRESS_BAR_HEIGHT, TOPBAR_HEIGHT, UI_SCALE};
use ui::web_view::WebViewport;
use ui::{sidebar, space_switcher, tab_list};

const SPACE_ICONS: &[&str] = &[
    "compass",
    "globe",
    "code",
    "terminal",
    "folder",
    "book",
    "briefcase",
    "layers",
    "box",
    "sparkles",
    "star",
    "heart",
    "bookmark",
    "music",
    "gamepad",
    "cpu",
    "zap",
    "hash",
    "shield",
    "palette",
    "rocket",
    "bell",
];

const SPACE_COLORS: &[u32] = &[
    0x9aa6ff, 0x76d7b2, 0xf6ad55, 0xf687b3, 0x63b3ed, 0xb794f4, 0xfbb6ce, 0x68d391,
];

#[derive(Clone, Debug, PartialEq)]
enum SpaceModalMode {
    Create,
    Edit { space_id: String },
}

struct SpaceModal {
    mode: SpaceModalMode,
    name_input: gpui_kit::Entity<InputState>,
    _name_sub: gpui_kit::Subscription,
    selected_icon: String,
    selected_color: u32,
}

struct SpaceContextMenu {
    space_id: String,
}

struct TabContextMenu {
    tab_id: String,
    showing_split_targets: bool,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

pub const SIDEBAR_MIN_WIDTH: f32 = 160.0 * UI_SCALE;
pub const SIDEBAR_MAX_WIDTH: f32 = 320.0 * UI_SCALE;
pub const SIDEBAR_COLLAPSED_WIDTH: f32 = 64.0 * UI_SCALE;
pub const CONTENT_BEZEL_X: f32 = 4.0 * UI_SCALE;
pub const CONTENT_BEZEL_BOTTOM: f32 = 4.0 * UI_SCALE;
pub const CARD_RADIUS: f32 = 16.0 * UI_SCALE;

fn tab_context_menu_position(
    event: &gpui_kit::MouseDownEvent,
    window: &Window,
) -> (f32, f32, f32, f32) {
    let bounds = window.bounds();
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let margin = 8.0 * UI_SCALE;
    let menu_width = (240.0 * UI_SCALE).min((width - margin * 2.0).max(1.0));
    let menu_height = (520.0 * UI_SCALE).min((height - margin * 2.0).max(1.0));
    let x = (f32::from(event.position.x) + margin)
        .clamp(margin, (width - menu_width - margin).max(margin));
    let y = f32::from(event.position.y).clamp(margin, (height - menu_height - margin).max(margin));
    (x, y, menu_width, menu_height)
}

#[derive(Clone, Debug, PartialEq)]
pub struct SuggestionItem {
    pub icon_type: String,
    pub title: String,
    pub target: String,
}

pub(crate) struct Lumen {
    spaces: Vec<Space>,
    active_space: String,
    sidebar_collapsed: bool,
    sidebar_width: f32,
    sidebar_resizing: bool,
    sidebar_width_transition: Option<(f32, f32, Instant, u64)>,
    sidebar_transition_serial: u64,
    theme_transition: Option<(visual::SpaceTheme, Instant, u64)>,
    theme_transition_id: u64,
    space_switch_transition: Option<(Space, space_switcher::SpaceSwitchState, Instant, u64)>,
    space_switch_transition_id: u64,
    #[cfg(all(target_os = "linux", not(test)))]
    resize_drag: split_view::ResizeDrag,
    started_at: Instant,
    first_frame_logged: bool,
    address: gpui_kit::Entity<InputState>,
    address_active_tab: Option<(String, String)>,
    address_editing: bool,
    reset_address_for_tab_change: bool,
    find_input: gpui_kit::Entity<InputState>,
    command_input: gpui_kit::Entity<InputState>,
    pub(crate) browser_focus_handle: gpui_kit::FocusHandle,
    _address_subscription: gpui_kit::Subscription,
    _command_subscription: gpui_kit::Subscription,
    show_find: bool,
    show_command_palette: bool,
    selected_command: usize,
    suggestions: Vec<SuggestionItem>,
    suggestion_snapshot: Vec<SuggestionItem>,
    suggestions_were_visible: bool,
    suggestion_closing: Option<(Vec<SuggestionItem>, u64)>,
    suggestion_animation_serial: u64,
    selected_suggestion: Option<usize>,
    recent_searches: Vec<String>,
    zoom_levels: std::collections::HashMap<String, f64>,
    #[cfg(all(target_os = "linux", not(test)))]
    active_zoom_target: Option<(String, String)>,
    show_suggestions: bool,
    pub(crate) engine: engine::Engine,
    web_viewport: gpui_kit::Entity<WebViewport>,
    second_web_viewport: gpui_kit::Entity<WebViewport>,
    focused_pane: usize,
    split_ratio: f32,
    split_resizing: bool,
    space_modal: Option<SpaceModal>,
    space_context_menu: Option<SpaceContextMenu>,
    tab_context_menu: Option<TabContextMenu>,
    split_group_expanded: bool,
    recently_closed_tabs: Vec<(String, Tab)>,
    closing_tab: Option<(String, Tab, usize, Instant, u64)>,
    closing_tab_serial: u64,
    browser_focused: bool,
    #[cfg(not(test))]
    cef_task_started: bool,
}

impl Lumen {
    pub(crate) fn is_app_shortcut_for_view(
        key: &str,
        browser_modifier: bool,
        shift: bool,
        alt: bool,
    ) -> bool {
        Self::is_app_shortcut(key, browser_modifier, shift)
            || (browser_modifier
                && alt
                && matches!(key, "left" | "arrowleft" | "right" | "arrowright"))
    }
}

fn create_default_space() -> (Vec<Space>, String) {
    let mut personal = Space::new("personal", "Personal", "compass", 0x9aa6ff);
    let mut welcome = Tab::new("tab-1", "New tab", "about:blank");
    welcome.state = TabState::Live;
    personal.active = Some(welcome.id.clone());
    personal.tabs.push(welcome);
    (vec![personal], "personal".into())
}

fn next_tab_id(space: &Space) -> String {
    let next = space
        .tabs
        .iter()
        .filter_map(|tab| tab.id.strip_prefix("tab-")?.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    format!("tab-{next}")
}

fn zoom_file() -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join(".local/share/lumen/zoom.json")
}

fn load_zooms() -> std::collections::HashMap<String, f64> {
    std::fs::read_to_string(zoom_file())
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_zooms(zooms: &std::collections::HashMap<String, f64>) {
    let path = zoom_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(contents) = serde_json::to_vec_pretty(zooms) {
        let _ = std::fs::write(path, contents);
    }
}

fn url_host(url: &str) -> String {
    let Some((scheme, remainder)) = url.split_once("://") else {
        return String::new();
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return String::new();
    }
    let authority = remainder
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let host = if authority.starts_with('[') {
        authority
            .find(']')
            .map(|end| &authority[..=end])
            .unwrap_or(authority)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if port.chars().all(|ch| ch.is_ascii_digit()) {
            host
        } else {
            authority
        }
    } else {
        authority
    };
    host.to_ascii_lowercase()
}

fn sidebar_width_for(collapsed: bool, width: f32) -> f32 {
    if collapsed {
        SIDEBAR_COLLAPSED_WIDTH
    } else {
        width
    }
}

#[cfg(all(target_os = "linux", not(test)))]
fn resize_debug(message: std::fmt::Arguments<'_>) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var("LUMEN_DEBUG_RESIZE").as_deref() == Ok("1")) {
        eprintln!("[resize-debug] {message}");
    }
}

fn move_suggestion_selection(current: Option<usize>, count: usize, down: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (current, down) {
        (None, true) => 0,
        (None, false) => count - 1,
        (Some(index), true) => (index + 1) % count,
        (Some(0), false) => count - 1,
        (Some(index), false) => index.saturating_sub(1),
    })
}

impl Lumen {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (mut spaces, active_space, sidebar_width, sidebar_collapsed) =
            if let Some(mut saved) = persist::load_state() {
                if !saved.spaces.is_empty() {
                    for space in &mut saved.spaces {
                        if !SPACE_ICONS.contains(&space.icon.as_str()) {
                            space.icon = "compass".to_string();
                        }
                    }
                    let active = if saved.spaces.iter().any(|s| s.id == saved.active_space) {
                        saved.active_space
                    } else {
                        saved.spaces[0].id.clone()
                    };
                    (
                        saved.spaces,
                        active,
                        saved
                            .sidebar_width
                            .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH),
                        saved.sidebar_collapsed,
                    )
                } else {
                    let (spaces, active) = create_default_space();
                    (spaces, active, 240.0 * UI_SCALE, false)
                }
            } else {
                let (spaces, active) = create_default_space();
                (spaces, active, 240.0 * UI_SCALE, false)
            };

        let initial_query = std::env::args().skip(1).find(|arg| !arg.starts_with("--"));
        let initial_http_first = initial_query
            .as_deref()
            .and_then(crate::shield::upgrade_http_url);
        if let Some(query) = &initial_query {
            if let Some(space) = spaces.iter_mut().find(|s| s.id == active_space) {
                let _ = submit_address(space, query);
            }
        }

        persist::save_state(&spaces, &active_space, sidebar_width, sidebar_collapsed);

        let address = cx.new(|cx| {
            let input =
                InputState::new(window, cx).placeholder("Search or enter a website address");
            if let Some(query) = initial_query {
                input.default_value(query)
            } else {
                input
            }
        });
        address.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        let browser_focus_handle = cx.focus_handle();
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in page"));
        let command_input = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search tabs, history, and actions"));
        let root_entity = cx.weak_entity();
        let web_viewport = cx.new(|_| WebViewport::new(root_entity, browser_focus_handle.clone()));
        let root_entity = cx.weak_entity();
        let second_web_viewport =
            cx.new(|_| WebViewport::new(root_entity, browser_focus_handle.clone()));
        let address_subscription =
            cx.subscribe(&address, |this: &mut Lumen, _, event, cx| match event {
                InputEvent::PressEnter { .. } => {
                    this.navigate(cx);
                    cx.notify();
                }
                InputEvent::Change => {
                    this.address_editing = true;
                    let text = this.address.read(cx).value().to_string();
                    this.update_suggestions(&text);
                    this.selected_suggestion = None;
                    cx.notify();
                }
                _ => {}
            });
        let command_subscription =
            cx.subscribe(&command_input, |this: &mut Lumen, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.selected_command = 0;
                    cx.notify();
                }
            });

        let mut engine = engine::Engine::new();
        if let Some(url) = initial_http_first
            && let Some(tab_id) = spaces
                .iter()
                .find(|space| space.id == active_space)
                .and_then(|space| space.active.as_deref())
        {
            engine.navigate_http_first(tab_id, &url);
        }

        Self {
            spaces,
            active_space,
            sidebar_collapsed,
            sidebar_width,
            sidebar_resizing: false,
            sidebar_width_transition: None,
            sidebar_transition_serial: 0,
            theme_transition: None,
            theme_transition_id: 0,
            space_switch_transition: None,
            space_switch_transition_id: 0,
            #[cfg(all(target_os = "linux", not(test)))]
            resize_drag: split_view::ResizeDrag::default(),
            started_at: Instant::now(),
            first_frame_logged: false,
            address,
            address_active_tab: None,
            address_editing: false,
            reset_address_for_tab_change: false,
            find_input,
            command_input,
            browser_focus_handle,
            web_viewport,
            second_web_viewport,
            focused_pane: 0,
            split_ratio: 0.5,
            split_resizing: false,
            _address_subscription: address_subscription,
            _command_subscription: command_subscription,
            show_find: false,
            show_command_palette: false,
            selected_command: 0,
            suggestions: Vec::new(),
            suggestion_snapshot: Vec::new(),
            suggestions_were_visible: false,
            suggestion_closing: None,
            suggestion_animation_serial: 0,
            selected_suggestion: None,
            recent_searches: history::load_recent_searches(),
            zoom_levels: load_zooms(),
            #[cfg(all(target_os = "linux", not(test)))]
            active_zoom_target: None,
            show_suggestions: false,
            engine,
            space_modal: None,
            space_context_menu: None,
            tab_context_menu: None,
            split_group_expanded: false,
            recently_closed_tabs: Vec::new(),
            closing_tab: None,
            closing_tab_serial: 0,
            browser_focused: false,
            #[cfg(not(test))]
            cef_task_started: false,
        }
    }

    fn active_space(&self) -> &Space {
        self.spaces
            .iter()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist")
    }

    fn activate_space(&mut self, id: &str) {
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
            let current_theme = visual::space_theme(self.active_space().color);
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

    fn open_create_space_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.space_context_menu = None;
        let index = self.spaces.len();
        let default_name = format!("Space {}", index + 1);
        let default_icon = SPACE_ICONS[index % SPACE_ICONS.len()].to_string();
        let default_color = SPACE_COLORS[index % SPACE_COLORS.len()];

        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Space name")
                .default_value(&default_name)
        });
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
            selected_color: default_color,
        });
    }

    fn open_edit_space_modal(
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

        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Space name")
                .default_value(&current_name)
        });
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
            mode: SpaceModalMode::Edit {
                space_id: space_id.to_string(),
            },
            name_input,
            _name_sub: sub,
            selected_icon: current_icon,
            selected_color: current_color,
        });
    }

    fn commit_space_modal(&mut self, cx: &Context<Self>) {
        let Some(modal) = self.space_modal.take() else {
            return;
        };
        let input_val = modal.name_input.read(cx).value().to_string();
        let name = if input_val.trim().is_empty() {
            "Untitled Space".to_string()
        } else {
            input_val.trim().to_string()
        };

        match modal.mode {
            SpaceModalMode::Create => {
                let id = format!("space-{}", self.spaces.len() + 1);
                let mut space = Space::new(&id, &name, &modal.selected_icon, modal.selected_color);
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
                    space.color = modal.selected_color;
                }
            }
        }
        self.save_state();
    }

    fn delete_space(&mut self, space_id: &str) {
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

    fn add_tab(&mut self) {
        self.show_suggestions = false;
        self.address_editing = false;
        self.reset_address_for_tab_change = true;
        let space = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist");
        let id = next_tab_id(space);
        let tab = Tab::new(&id, "New tab", "about:blank");
        space.tabs.push(tab);
        space.active = Some(id);
        self.save_state();
    }

    fn close_active_tab(&mut self) {
        let active_id = self.focused_tab_id().to_owned();
        if active_id.is_empty() || !self.close_tab_by_id(&active_id) {
            self.add_tab();
        }
    }

    fn close_tab_by_id(&mut self, tab_id: &str) -> bool {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return false;
        };
        let Some(index) = space.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        let was_active = space.active.as_deref() == Some(tab_id);
        let closing_space_id = space.id.clone();
        let closing_snapshot = space.tabs[index].clone();
        let split_other = space.split.as_ref().and_then(|pair| {
            if pair.first == tab_id {
                Some(pair.second.clone())
            } else if pair.second == tab_id {
                Some(pair.first.clone())
            } else {
                None
            }
        });
        if split_other.is_some() {
            space.split = None;
            self.focused_pane = 0;
        }
        self.engine.close_browser(tab_id);
        let closed = space.tabs.remove(index);
        self.recently_closed_tabs.push((space.id.clone(), closed));
        if space.tabs.is_empty() {
            let id = next_tab_id(space);
            space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
            space.active = Some(id);
        } else if let Some(other) = split_other {
            space.active = Some(other);
        } else if was_active {
            space.active = Some(space.tabs[index.min(space.tabs.len() - 1)].id.clone());
        }
        self.closing_tab_serial = self.closing_tab_serial.wrapping_add(1);
        self.closing_tab = Some((
            closing_space_id,
            closing_snapshot,
            index,
            Instant::now(),
            self.closing_tab_serial,
        ));
        self.save_state();
        true
    }

    fn split_tabs(&mut self, first: &str, second: &str) -> bool {
        if first == second
            || !self.active_space().tabs.iter().any(|tab| tab.id == first)
            || !self.active_space().tabs.iter().any(|tab| tab.id == second)
        {
            return false;
        }
        self.active_space_mut().split = Some(SplitPair {
            first: first.to_owned(),
            second: second.to_owned(),
        });
        self.focused_pane = 0;
        self.split_ratio = 0.5;
        self.save_state();
        true
    }

    fn choose_split_target(&mut self, target_id: &str) -> bool {
        let Some(menu) = self.tab_context_menu.as_ref() else {
            return false;
        };
        if !menu.showing_split_targets {
            return false;
        }
        let first_id = menu.tab_id.clone();
        let split = self.split_tabs(&first_id, target_id);
        if split {
            self.tab_context_menu = None;
        }
        split
    }

    fn duplicate_tab(&mut self, tab_id: &str) {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return;
        };
        let Some(source) = space.tabs.iter().find(|tab| tab.id == tab_id).cloned() else {
            return;
        };
        let id = next_tab_id(space);
        let mut duplicate = source;
        duplicate.id = id.clone();
        duplicate.state = TabState::Discarded;
        space.tabs.push(duplicate);
        space.active = Some(id);
        self.tab_context_menu = None;
        self.save_state();
    }

    fn reopen_closed_tab(&mut self) {
        let Some((space_id, tab)) = self.recently_closed_tabs.pop() else {
            return;
        };
        if let Some(space) = self.spaces.iter_mut().find(|space| space.id == space_id) {
            let id = if space.tabs.iter().any(|existing| existing.id == tab.id) {
                next_tab_id(space)
            } else {
                tab.id.clone()
            };
            let mut tab = tab;
            tab.id = id.clone();
            space.tabs.push(tab);
            space.active = Some(id);
            let _ = space;
            self.activate_space(&space_id);
        }
        self.save_state();
    }

    fn open_popup_tab(&mut self, space_id: &str, url: String) {
        {
            let Some(space) = self.spaces.iter_mut().find(|space| space.id == space_id) else {
                return;
            };
            let id = next_tab_id(space);
            let mut tab = Tab::new(&id, "New tab", url);
            tab.state = TabState::Live;
            space.tabs.push(tab);
            space.active = Some(id);
        }
        self.activate_space(space_id);
        self.save_state();
    }

    fn cycle_tab(&mut self, reverse: bool) {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return;
        };
        if space.tabs.len() < 2 {
            return;
        }
        let current = space
            .active
            .as_ref()
            .and_then(|id| space.tabs.iter().position(|tab| &tab.id == id))
            .unwrap_or(0);
        let next = if reverse {
            (current + space.tabs.len() - 1) % space.tabs.len()
        } else {
            (current + 1) % space.tabs.len()
        };
        space.active = Some(space.tabs[next].id.clone());
        self.save_state();
    }

    fn activate_tab_number(&mut self, number: usize) {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return;
        };
        let index = if number == 9 {
            space.tabs.len().checked_sub(1)
        } else {
            number.checked_sub(1)
        };
        if let Some(tab) = index.and_then(|index| space.tabs.get(index)) {
            space.active = Some(tab.id.clone());
            self.save_state();
        }
    }

    fn is_app_shortcut(key: &str, control: bool, shift: bool) -> bool {
        control
            && ((key == "l" && !shift)
                || (key == "b" && shift)
                || (key == "k" && !shift)
                || key == "f"
                || key == "t"
                || key == "tab"
                || key == "r"
                || (shift && matches!(key, "s" | "w"))
                || (key == "w" && !shift)
                || matches!(key, "=" | "+" | "-" | "0")
                || (!shift && key.len() == 1 && matches!(key.as_bytes()[0], b'1'..=b'9')))
    }

    fn handle_shortcut(&mut self, key: &str, control: bool, shift: bool) -> bool {
        if !control {
            return false;
        }
        match (key, shift) {
            ("b", true) => self.toggle_shield_for_active_tab(),
            ("t", true) => self.reopen_closed_tab(),
            ("t", false) => self.add_tab(),
            ("w", false) => self.close_active_tab(),
            ("tab", shift) => self.cycle_tab(shift),
            (digit, false) if digit.len() == 1 && matches!(digit.as_bytes()[0], b'1'..=b'9') => {
                self.activate_tab_number(key.parse().unwrap_or(1));
            }
            ("r", false) => self.reload_active(false),
            ("r", true) => self.reload_active(true),
            ("=", _) | ("+", _) => self.change_zoom(1.0),
            ("-", false) => self.change_zoom(-1.0),
            ("0", false) => self.change_zoom(0.0),
            _ => return false,
        }
        true
    }

    fn toggle_shield_for_active_tab(&mut self) {
        let id = self.active_tab_id().to_owned();
        let url = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.url.clone());
        if let Some(url) = url {
            let enabled = self.engine.toggle_shield_for_tab(&id, &url);
            eprintln!(
                "Lumen shield {} for {}",
                if enabled { "enabled" } else { "disabled" },
                crate::shield::url_host(&url).as_deref().unwrap_or(&url)
            );
        }
    }

    fn reload_active(&self, ignore_cache: bool) {
        self.engine.reload(self.focused_tab_id(), ignore_cache);
    }

    fn find_in_page(&self, forward: bool, next: bool, cx: &Context<Self>) {
        let query = self.find_input.read(cx).value().to_string();
        if !query.is_empty() {
            self.engine
                .find(self.focused_tab_id(), &query, forward, next);
        }
    }

    fn change_zoom(&mut self, delta: f64) {
        let tab_id_and_host = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == self.active_tab_id());
        let Some((tab_id, host)) = tab_id_and_host.map(|tab| (tab.id.clone(), url_host(&tab.url)))
        else {
            return;
        };
        if host.is_empty() {
            return;
        }
        let level = visual::zoom_after_delta(self.zoom_levels.get(&host).copied(), delta);
        self.zoom_levels.insert(host, level);
        save_zooms(&self.zoom_levels);
        self.engine.set_zoom_level(&tab_id, level);
    }

    fn effective_sidebar_width(&self) -> f32 {
        sidebar_width_for(self.sidebar_collapsed, self.sidebar_width)
    }

    #[cfg(all(target_os = "linux", not(test)))]
    fn browser_sizes_for_sidebar(
        &self,
        window: &Window,
        sidebar_width: f32,
    ) -> Vec<split_view::BrowserSize> {
        let bounds = window.bounds();
        let content_width =
            (f32::from(bounds.size.width) - sidebar_width - 2.0 * CONTENT_BEZEL_X).max(100.0);
        let height = (f32::from(bounds.size.height) - TOPBAR_HEIGHT - CONTENT_BEZEL_BOTTOM)
            .max(100.0) as i32;
        if let Some(pair) = self.current_split_pair() {
            let (left, right) = split_view::pane_widths(content_width, self.split_ratio);
            vec![
                split_view::BrowserSize {
                    tab_id: pair.first,
                    width: left.max(1.0) as i32,
                    height,
                },
                split_view::BrowserSize {
                    tab_id: pair.second,
                    width: right.max(1.0) as i32,
                    height,
                },
            ]
        } else {
            self.active_space()
                .active
                .clone()
                .map(|tab_id| {
                    vec![split_view::BrowserSize {
                        tab_id,
                        width: content_width.max(1.0) as i32,
                        height,
                    }]
                })
                .unwrap_or_default()
        }
    }

    fn animate_sidebar_toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(test)]
        let _ = window;
        let from = self.sidebar_width_transition.map_or_else(
            || self.effective_sidebar_width(),
            |(from, to, started, _)| {
                let progress = started.elapsed().as_secs_f32()
                    / ui::motion::MOTION_SLOW.as_secs_f32().max(f32::EPSILON);
                from + (to - from) * ui::motion::ease_out_quint(progress)
            },
        );
        let collapsing = !self.sidebar_collapsed;
        let target = if collapsing {
            SIDEBAR_COLLAPSED_WIDTH
        } else {
            self.sidebar_width
        };
        #[cfg(all(target_os = "linux", not(test)))]
        self.begin_browser_resize_drag(window, false);
        #[cfg(all(target_os = "linux", not(test)))]
        let final_sizes = self.browser_sizes_for_sidebar(window, target);
        self.sidebar_transition_serial = self.sidebar_transition_serial.wrapping_add(1);
        let serial = self.sidebar_transition_serial;
        self.sidebar_width_transition = Some((from, target, Instant::now(), serial));
        self.sidebar_collapsed = collapsing;
        self.save_state();

        #[cfg(all(target_os = "linux", not(test)))]
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(ui::motion::duration(ui::motion::MOTION_SLOW))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.sidebar_transition_serial == serial {
                    this.sidebar_width_transition = None;
                    this.finish_browser_resize_drag(final_sizes, "sidebar-toggle");
                    cx.notify();
                }
            });
        })
        .detach();

        cx.notify();
    }

    #[cfg(all(target_os = "linux", not(test)))]
    fn begin_browser_resize_drag(&mut self, window: &Window, split_divider: bool) {
        let pair = self.current_split_pair();
        let bounds = window.bounds();
        let width = f32::from(bounds.size.width);
        let height = (f32::from(bounds.size.height) - TOPBAR_HEIGHT - CONTENT_BEZEL_BOTTOM)
            .max(100.0) as i32;
        let sidebar_width = self.effective_sidebar_width();
        let content_width = (width - sidebar_width - CONTENT_BEZEL_X * 2.0).max(100.0);
        let fallback_widths = if pair.is_some() {
            let (first, second) = split_view::pane_widths(content_width, self.split_ratio);
            vec![first.max(1.0) as i32, second.max(1.0) as i32]
        } else if self.active_space().active.is_some() {
            vec![content_width.max(1.0) as i32]
        } else {
            Vec::new()
        };
        let tab_ids = pair
            .map(|pair| vec![pair.first, pair.second])
            .unwrap_or_else(|| self.active_space().active.clone().into_iter().collect());
        let fallback_surface = visual::space_theme(self.active_space().color).surface;
        let sizes = tab_ids
            .iter()
            .enumerate()
            .map(|(index, tab_id)| {
                let (current_width, current_height) = self
                    .engine
                    .browser_size(tab_id)
                    .unwrap_or((fallback_widths[index], height));
                split_view::BrowserSize {
                    tab_id: tab_id.clone(),
                    width: current_width,
                    height: current_height,
                }
            })
            .collect::<Vec<_>>();
        let edge_colors = tab_ids
            .iter()
            .map(|tab_id| {
                (
                    tab_id.clone(),
                    self.engine
                        .frame_edge_color(tab_id)
                        .unwrap_or(fallback_surface),
                )
            })
            .collect();
        self.resize_drag.start(sizes.clone(), edge_colors);
        for size in sizes {
            resize_debug(format_args!(
                "drag_start kind={} tab={} size={}x{}",
                if split_divider { "split" } else { "sidebar" },
                size.tab_id,
                size.width,
                size.height
            ));
        }
    }

    #[cfg(all(target_os = "linux", not(test)))]
    fn finish_browser_resize_drag(&mut self, sizes: Vec<split_view::BrowserSize>, kind: &str) {
        for size in self.resize_drag.finish(sizes) {
            resize_debug(format_args!(
                "drag_end kind={kind} tab={} size={}x{}",
                size.tab_id, size.width, size.height
            ));
            self.engine
                .flush_resize_browser(&size.tab_id, size.width, size.height);
        }
    }

    fn active_tab_id(&self) -> &str {
        self.active_space().active.as_deref().unwrap_or_default()
    }

    fn save_state(&self) {
        #[cfg(not(test))]
        persist::save_state(
            &self.spaces,
            &self.active_space,
            self.sidebar_width
                .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH),
            self.sidebar_collapsed,
        );
        #[cfg(test)]
        let _ = self;
    }

    fn navigate(&mut self, cx: &Context<Self>) {
        self.show_suggestions = false;
        self.selected_suggestion = None;
        self.address_editing = false;
        let input = self.address.read(cx).value().to_string();
        let space = self.active_space_mut();
        let Some((id, url, http_first)) = submit_address(space, &input) else {
            return;
        };
        self.remember_search(&input, &url);
        if http_first {
            self.engine.navigate_http_first(&id, &url);
        } else {
            self.engine.navigate(&id, &url);
        }
        self.save_state();
    }

    fn navigate_to(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_suggestions = false;
        self.selected_suggestion = None;
        self.address_editing = false;
        self.address.update(cx, |input, cx| {
            input.set_value(target, window, cx);
        });
        let space = self.active_space_mut();
        let Some((id, url, http_first)) = submit_address(space, target) else {
            return;
        };
        self.remember_search(target, &url);
        if http_first {
            self.engine.navigate_http_first(&id, &url);
        } else {
            self.engine.navigate(&id, &url);
        }
        self.save_state();
    }

    fn update_suggestions(&mut self, query: &str) {
        let list = compute_suggestions(query);
        self.show_suggestions = !list.is_empty();
        self.suggestions = list;
        if !query.trim().is_empty() {
            self.suggestions
                .extend(
                    history::search(query, 4)
                        .into_iter()
                        .map(|entry| SuggestionItem {
                            icon_type: "globe".to_string(),
                            title: if entry.title.is_empty() {
                                entry.url.clone()
                            } else {
                                entry.title
                            },
                            target: entry.url,
                        }),
                );
        }
        self.selected_suggestion = None;
        if query.trim().is_empty() {
            self.suggestions
                .extend(self.recent_searches.iter().map(|query| SuggestionItem {
                    icon_type: "search".to_string(),
                    title: format!("Recent search: {query}"),
                    target: query.clone(),
                }));
            self.suggestions.truncate(6);
            self.show_suggestions = !self.suggestions.is_empty();
        }
    }

    fn remember_search(&mut self, query: &str, url: &str) {
        let query = query.trim();
        if query.is_empty() || !url.starts_with("https://duckduckgo.com/?q=") {
            return;
        }
        self.recent_searches
            .retain(|recent| !recent.eq_ignore_ascii_case(query));
        self.recent_searches.insert(0, query.to_string());
        self.recent_searches.truncate(10);
        #[cfg(not(test))]
        history::save_recent_searches(&self.recent_searches);
    }

    fn move_suggestion_selection(&mut self, down: bool) {
        self.selected_suggestion =
            move_suggestion_selection(self.selected_suggestion, self.suggestions.len(), down);
    }

    fn command_items(&self, query: &str) -> Vec<PaletteItem> {
        let tabs: Vec<_> = self
            .spaces
            .iter()
            .flat_map(|space| {
                space.tabs.iter().map(move |tab| PaletteTab {
                    space_id: &space.id,
                    space_name: &space.name,
                    tab_id: &tab.id,
                    title: &tab.title,
                    url: &tab.url,
                })
            })
            .collect();
        let history = history::search(query, 20);
        command_palette::rank_items(query, &tabs, &history)
    }

    fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_suggestions = false;
        self.show_find = false;
        self.show_command_palette = true;
        self.selected_command = 0;
        self.browser_focused = false;
        self.engine.set_focus(self.active_tab_id(), false);
        self.command_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
    }

    fn close_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_command_palette = false;
        self.focus_browser(window, cx);
    }

    fn execute_palette_item(
        &mut self,
        item: PaletteItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_command_palette = false;
        match item.target {
            PaletteTarget::Tab { space_id, tab_id } => {
                let valid = if let Some(space) =
                    self.spaces.iter_mut().find(|space| space.id == space_id)
                {
                    if space.tabs.iter().any(|tab| tab.id == tab_id) {
                        space.active = Some(tab_id);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if valid {
                    self.activate_space(&space_id);
                }
            }
            PaletteTarget::History(url) => self.navigate_to(&url, window, cx),
            PaletteTarget::Action(action) => match action {
                PaletteAction::NewTab => self.add_tab(),
                PaletteAction::CloseTab => self.close_active_tab(),
                PaletteAction::ReopenClosedTab => self.reopen_closed_tab(),
                PaletteAction::Reload => self.reload_active(false),
                PaletteAction::FindInPage => {
                    self.show_find = true;
                    self.find_input
                        .update(cx, |input, cx| input.focus(window, cx));
                    self.engine.set_focus(self.active_tab_id(), false);
                }
                PaletteAction::ZoomIn => self.change_zoom(1.0),
                PaletteAction::ZoomOut => self.change_zoom(-1.0),
                PaletteAction::ZoomReset => self.change_zoom(0.0),
                PaletteAction::NewSpace => self.open_create_space_modal(window, cx),
            },
        }
        cx.notify();
    }

    fn move_command_selection(&mut self, down: bool, count: usize) {
        if count == 0 {
            self.selected_command = 0;
            return;
        }
        self.selected_command = if down {
            (self.selected_command + 1) % count
        } else {
            (self.selected_command + count - 1) % count
        };
    }

    pub(crate) fn focus_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browser_focused = true;
        self.show_suggestions = false;
        self.selected_suggestion = None;
        window.focus(&self.browser_focus_handle, cx);
        if let Some(pair) = self.current_split_pair() {
            self.engine.set_focus(&pair.first, false);
            self.engine.set_focus(&pair.second, false);
        }
        let tab_id = self.focused_tab_id().to_string();
        self.engine.set_focus(&tab_id, true);
    }

    fn focused_tab_id(&self) -> &str {
        let space = self.active_space();
        let active = space.active.as_deref().unwrap_or_default();
        let Some(pair) = space.split.as_ref().filter(|pair| {
            (active == pair.first || active == pair.second)
                && pair.first != pair.second
                && space.tabs.iter().any(|tab| tab.id == pair.first)
                && space.tabs.iter().any(|tab| tab.id == pair.second)
        }) else {
            return active;
        };
        split_view::tab_for_pane(&pair.first, &pair.second, self.focused_pane)
    }

    pub(crate) fn set_focused_pane(&mut self, pane_index: usize) {
        self.focused_pane = pane_index.min(1);
        if let Some(pair) = self.stored_split_pair() {
            self.active_space_mut().active = Some(
                split_view::tab_for_pane(&pair.first, &pair.second, self.focused_pane).to_owned(),
            );
        }
    }

    fn activate_split_group(&mut self) {
        if let Some(pair) = self.stored_split_pair() {
            self.focused_pane = 0;
            self.active_space_mut().active = Some(pair.first);
        }
    }

    fn current_split_pair(&self) -> Option<SplitPair> {
        let pair = self.stored_split_pair()?;
        let active = self.active_tab_id();
        (active == pair.first || active == pair.second).then_some(pair)
    }

    fn stored_split_pair(&self) -> Option<SplitPair> {
        let space = self.active_space();
        let pair = space.split.as_ref()?;
        let has_first = space.tabs.iter().any(|tab| tab.id == pair.first);
        let has_second = space.tabs.iter().any(|tab| tab.id == pair.second);
        (has_first && has_second && pair.first != pair.second).then(|| pair.clone())
    }

    fn split_active_with_next(&mut self) {
        let space = self.active_space_mut();
        let Some(first) = space.active.clone() else {
            return;
        };
        let Some(first_index) = space.tabs.iter().position(|tab| tab.id == first) else {
            return;
        };
        let second = if let Some(tab) = space.tabs.get(first_index + 1) {
            tab.id.clone()
        } else if space.tabs.len() > 1 {
            space.tabs[0].id.clone()
        } else {
            let id = next_tab_id(space);
            space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
            id
        };
        if first != second {
            space.split = Some(SplitPair { first, second });
            self.focused_pane = 0;
            self.split_ratio = 0.5;
            self.save_state();
        }
    }

    fn exit_split(&mut self) {
        if self.active_space().split.is_some() {
            self.active_space_mut().split = None;
            self.focused_pane = 0;
            self.save_state();
        }
    }

    fn active_space_mut(&mut self) -> &mut Space {
        self.spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist")
    }
}

pub fn compute_suggestions(query: &str) -> Vec<SuggestionItem> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }

    let mut list = Vec::new();

    // 1. DuckDuckGo search action avoids sending every free-form query to Google.
    list.push(SuggestionItem {
        icon_type: "search".to_string(),
        title: format!("Search DuckDuckGo for \"{q}\""),
        target: q.to_string(),
    });

    // 2. Direct domain navigation if not already a full URL
    if !q.contains("://") {
        let direct_domain = if q.contains('.') && !q.contains(' ') {
            format!("https://{q}")
        } else if !q.contains(' ') {
            format!("https://www.{q}.com")
        } else {
            String::new()
        };
        if !direct_domain.is_empty() {
            list.push(SuggestionItem {
                icon_type: "globe".to_string(),
                title: format!("Visit {direct_domain}"),
                target: direct_domain,
            });
        }
    }

    // 3. Matched quick sites and keywords
    let q_lower = q.to_lowercase();
    const POPULAR_SITES: &[(&str, &str, &str)] = &[
        ("youtube", "YouTube", "https://youtube.com"),
        ("github", "GitHub", "https://github.com"),
        ("google", "Google", "https://google.com"),
        ("gmail", "Gmail", "https://mail.google.com"),
        ("reddit", "Reddit", "https://reddit.com"),
        ("twitter", "Twitter / X", "https://x.com"),
        ("chatgpt", "ChatGPT", "https://chatgpt.com"),
        ("claude", "Claude AI", "https://claude.ai"),
        ("wikipedia", "Wikipedia", "https://wikipedia.org"),
        ("amazon", "Amazon", "https://amazon.com"),
        ("netflix", "Netflix", "https://netflix.com"),
        ("spotify", "Spotify", "https://open.spotify.com"),
        (
            "rust",
            "Rust Programming Language",
            "https://www.rust-lang.org",
        ),
        ("crates", "Crates.io", "https://crates.io"),
        (
            "stackoverflow",
            "Stack Overflow",
            "https://stackoverflow.com",
        ),
        ("discord", "Discord", "https://discord.com"),
        ("twitch", "Twitch", "https://twitch.tv"),
        ("notion", "Notion", "https://notion.so"),
        ("figma", "Figma", "https://figma.com"),
        ("news", "Google News", "https://news.google.com"),
        ("maps", "Google Maps", "https://maps.google.com"),
    ];

    for (slug, name, url) in POPULAR_SITES {
        if slug.starts_with(&q_lower) || name.to_lowercase().contains(&q_lower) {
            list.push(SuggestionItem {
                icon_type: "zap".to_string(),
                title: format!("{name} — {url}"),
                target: url.to_string(),
            });
        }
    }

    // 4. Common query completions
    if list.len() < 6 && !q.contains('.') {
        list.push(SuggestionItem {
            icon_type: "search".to_string(),
            title: format!("{q} official website"),
            target: format!("{q} official website"),
        });
        list.push(SuggestionItem {
            icon_type: "search".to_string(),
            title: format!("{q} login"),
            target: format!("{q} login"),
        });
        list.push(SuggestionItem {
            icon_type: "search".to_string(),
            title: format!("{q} documentation"),
            target: format!("{q} documentation"),
        });
    }

    list.truncate(6);
    list
}

fn encode_search_query(query: &str) -> String {
    let mut encoded = String::new();
    for byte in query.bytes() {
        match byte {
            b' ' => encoded.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn normalize_address(input: &str) -> String {
    let input = input.trim();
    if input.is_empty() {
        return String::new();
    }
    if input.starts_with("http://") {
        return crate::shield::upgrade_http_url(input).unwrap_or_else(|| input.to_owned());
    }
    if input.contains("://") || input.starts_with("about:") {
        return input.to_owned();
    }
    if input == "localhost" || input.starts_with("localhost:") || input.contains('.') {
        return format!("https://{input}");
    }

    let lower = input.to_lowercase();
    match lower.as_str() {
        "youtube" => "https://youtube.com".to_string(),
        "google" => "https://google.com".to_string(),
        "github" => "https://github.com".to_string(),
        "reddit" => "https://reddit.com".to_string(),
        "twitter" | "x" => "https://x.com".to_string(),
        "wikipedia" => "https://wikipedia.org".to_string(),
        "amazon" => "https://amazon.com".to_string(),
        "netflix" => "https://netflix.com".to_string(),
        "spotify" => "https://open.spotify.com".to_string(),
        "chatgpt" => "https://chatgpt.com".to_string(),
        "claude" => "https://claude.ai".to_string(),
        "twitch" => "https://twitch.tv".to_string(),
        "discord" => "https://discord.com".to_string(),
        "figma" => "https://figma.com".to_string(),
        "notion" => "https://notion.so".to_string(),
        "stackoverflow" => "https://stackoverflow.com".to_string(),
        _ => format!("https://duckduckgo.com/?q={}", encode_search_query(input)),
    }
}

fn submit_address(space: &mut Space, input: &str) -> Option<(String, String, bool)> {
    let http_first = input.trim().starts_with("http://")
        && crate::shield::upgrade_http_url(input.trim()).is_some();
    let url = normalize_address(input);
    let url = if url.is_empty() {
        "https://duckduckgo.com".to_owned()
    } else {
        url
    };

    if space.active.is_none() {
        let id = format!("tab-{}", space.tabs.len() + 1);
        space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
        space.active = Some(id);
    }
    let id = space.active.clone()?;
    let tab = space.tabs.iter_mut().find(|tab| tab.id == id)?;
    let title = if url.starts_with("https://duckduckgo.com/?q=") {
        format!("Search: {}", input.trim())
    } else if let Some(domain) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        let domain = domain.split('/').next().unwrap_or(domain);
        let domain = domain.strip_prefix("www.").unwrap_or(domain);
        domain.to_string()
    } else {
        url.clone()
    };
    tab.url = url.clone();
    tab.title = title;
    tab.state = TabState::Live;
    Some((id, url, http_first))
}

impl Render for Lumen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        _window.set_rem_size(px(16.0 * UI_SCALE));
        stats::record_ui_render();
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            eprintln!(
                "Lumen time-to-first-frame: {:.1} ms",
                self.started_at.elapsed().as_secs_f64() * 1000.0
            );
        }

        #[cfg(not(test))]
        if !self.cef_task_started {
            self.cef_task_started = true;
            let refresh_rate = engine::frame_rate_for_refresh(_window.refresh_interval(cx));
            if std::env::var_os("LUMEN_STATS").is_some_and(|value| value == "1") {
                eprintln!("Lumen OSR requested frame rate: {refresh_rate} fps");
            }
            self.engine.set_windowless_frame_rate(refresh_rate);
            // Synthetic input for repeatable OSR benchmarks on Wayland, where X11
            // window automation cannot target this GPUI window.
            if std::env::var_os("LUMEN_AUTOSCROLL").is_some_and(|value| value == "1") {
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(5))
                        .await;
                    let mut direction = -1;
                    let mut direction_started = Instant::now();
                    loop {
                        if direction_started.elapsed() >= std::time::Duration::from_secs(2) {
                            direction = -direction;
                            direction_started = Instant::now();
                        }
                        if this
                            .update(cx, |this, _| {
                                this.engine.send_mouse_wheel(
                                    this.active_tab_id(),
                                    300,
                                    300,
                                    0,
                                    50 * direction,
                                    &Default::default(),
                                );
                            })
                            .is_err()
                        {
                            break;
                        }
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(16))
                            .await;
                    }
                })
                .detach();
            }
            let ui_update_receiver = self.engine.ui_update_receiver();
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(180))
                    .await;
                loop {
                    while ui_update_receiver.try_recv().is_ok() {}
                    let updated = this.update(cx, |this, cx| {
                        let was_initialized = this.engine.is_initialized();
                        if let Err(error) = this.engine.initialize() {
                            eprintln!("Lumen CEF startup failed: {error}");
                            return None;
                        }
                        if this.engine.take_created_browsers() {
                            let active = this.active_tab_id().to_owned();
                            this.engine.set_visible(&active);
                            cx.notify();
                        }
                        let popups = this.engine.take_popups();
                        for (space_id, url) in &popups {
                            this.open_popup_tab(space_id, url.clone());
                        }
                        if !popups.is_empty() {
                            cx.notify();
                        }
                        let visible_ids = this.current_split_pair().map_or_else(
                            || vec![this.active_tab_id().to_string()],
                            |pair| vec![pair.first, pair.second],
                        );
                        let visible_refs =
                            visible_ids.iter().map(String::as_str).collect::<Vec<_>>();
                        let (has_updates, has_root_updates) =
                            this.engine.take_dirty_for(&visible_refs);
                        if has_root_updates {
                            stats::record_root_metadata_update();
                            for tab_id in &visible_ids {
                                if let Some((title, url)) = this.engine.take_updates(tab_id) {
                                    if let Some(tab) = this
                                        .active_space_mut()
                                        .tabs
                                        .iter_mut()
                                        .find(|tab| tab.id == *tab_id)
                                    {
                                        if let Some(title) = title.filter(|title| !title.is_empty())
                                        {
                                            tab.title = title;
                                        }
                                        if let Some(url) = url.filter(|url| !url.is_empty()) {
                                            tab.url = url;
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        }
                        if !was_initialized {
                            cx.notify();
                        } else if has_updates {
                            let retired_images = this.engine.take_retired_images();
                            let first_tab = visible_ids[0].clone();
                            let image = this.engine.get_image(&first_tab);
                            let cursor = this.engine.get_cursor(&first_tab);
                            let sidebar_width = this.effective_sidebar_width();
                            let frame_matches_target = this
                                .resize_drag
                                .target_size(&first_tab)
                                .is_some_and(|(w, h)| {
                                    this.engine.frame_matches_size(&first_tab, w, h)
                                });
                            let edge_color = this
                                .resize_drag
                                .edge_color_until_frame(&first_tab, frame_matches_target);
                            this.web_viewport.update(cx, |viewport, cx| {
                                viewport.set_frame(
                                    first_tab,
                                    sidebar_width,
                                    image,
                                    cursor,
                                    retired_images,
                                    0.0,
                                    0,
                                    visible_ids.len() > 1,
                                    edge_color,
                                );
                                cx.notify();
                            });
                            if let Some(second_tab) = visible_ids.get(1).cloned() {
                                let image = this.engine.get_image(&second_tab);
                                let cursor = this.engine.get_cursor(&second_tab);
                                this.second_web_viewport.update(cx, |viewport, cx| {
                                    viewport.set_content(second_tab, image, cursor, Vec::new());
                                    cx.notify();
                                });
                            }
                        }
                        Some(())
                    });
                    let Ok(Some(())) = updated else {
                        break;
                    };
                    futures_lite::future::or(
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(33)),
                        async {
                            let _ = ui_update_receiver.recv().await;
                        },
                    )
                    .await;
                }
            })
            .detach();
        }

        #[cfg(all(target_os = "linux", not(test)))]
        {
            let current_active_id = self.active_space().active.clone().unwrap_or_default();
            if !current_active_id.is_empty() {
                if let Some((new_title, new_url)) = self.engine.take_updates(&current_active_id) {
                    let space = self.active_space_mut();
                    if let Some(tab) = space.tabs.iter_mut().find(|t| t.id == current_active_id) {
                        if let Some(t) = new_title {
                            if !t.is_empty() {
                                tab.title = t;
                            }
                        }
                        if let Some(u) = new_url {
                            if !u.is_empty() {
                                tab.url = u;
                            }
                        }
                    }
                }
            }
        }

        let expired_tabs = self.engine.discard_inactive();
        if !expired_tabs.is_empty() {
            for space in &mut self.spaces {
                for tab in &mut space.tabs {
                    if expired_tabs.iter().any(|id| id == &tab.id) {
                        tab.state = TabState::Discarded;
                    }
                }
            }
        }

        for (space_id, url) in self.engine.take_popups() {
            self.open_popup_tab(&space_id, url);

            for retired in self.engine.take_retired_images() {
                let _ = _window.drop_image(retired);
            }
        }

        let active_address_tab = (self.active_space.clone(), self.active_tab_id().to_owned());
        if self.address_active_tab.as_ref() != Some(&active_address_tab) {
            let active_url = self
                .active_space()
                .tabs
                .iter()
                .find(|tab| tab.id == active_address_tab.1)
                .map(|tab| {
                    if tab.url == "about:blank" {
                        String::new()
                    } else {
                        tab.url.clone()
                    }
                })
                .unwrap_or_default();
            let address_focused =
                _window.focused(cx) == Some(self.address.read(cx).focus_handle(cx));
            let force_reset = self.reset_address_for_tab_change;
            if force_reset || !(self.address_editing && address_focused) {
                self.address.update(cx, |input, cx| {
                    input.set_value(&active_url, _window, cx);
                    if force_reset {
                        input.focus(_window, cx);
                    }
                });
                self.address_editing = false;
                self.show_suggestions = false;
                self.selected_suggestion = None;
            }
            self.address_active_tab = Some(active_address_tab);
            self.reset_address_for_tab_change = false;
        }

        let active_space_id = self.active_space.clone();
        let active_space = self.active_space().clone();
        let active_tab_id = active_space.active.clone();
        let selected_name = active_space.name.clone();
        let selected_color = self
            .space_modal
            .as_ref()
            .map_or(active_space.color, |modal| {
                let previews_active = match &modal.mode {
                    SpaceModalMode::Create => true,
                    SpaceModalMode::Edit { space_id } => space_id == &active_space_id,
                };
                if previews_active {
                    modal.selected_color
                } else {
                    active_space.color
                }
            });
        let target_theme = visual::space_theme(selected_color);
        let theme = if ui::motion::reduced_motion() {
            target_theme
        } else {
            self.theme_transition
                .map_or(target_theme, |(from, started, _)| {
                    let progress = started.elapsed().as_secs_f32()
                        / ui::motion::MOTION_SPACE_SWITCH
                            .as_secs_f32()
                            .max(f32::EPSILON);
                    visual::interpolate_theme(
                        from,
                        target_theme,
                        ui::motion::ease_out_quint(progress),
                    )
                })
        };
        let space_switch_transition =
            self.space_switch_transition
                .clone()
                .filter(|(_, _, started, _)| {
                    started.elapsed() < ui::motion::duration(ui::motion::MOTION_SPACE_SWITCH)
                });
        let tabs: Vec<(
            String,
            String,
            Option<std::sync::Arc<gpui_kit::RenderImage>>,
        )> = active_space
            .tabs
            .iter()
            .map(|tab| {
                (
                    tab.id.clone(),
                    tab.title.clone(),
                    self.engine.get_favicon_image(&tab.id),
                )
            })
            .collect();
        let active_tab = active_space
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == active_tab_id.as_ref());
        let _active_tab_title = active_tab
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| "New tab".into());
        let active_tab_id_string = active_tab_id.clone().unwrap_or_default();
        let active_tab_url = active_tab.map(|tab| tab.url.clone()).unwrap_or_default();
        let split_pair = self.current_split_pair();
        let stored_split_pair = self.stored_split_pair();

        #[cfg(all(target_os = "linux", not(test)))]
        let scale = _window.scale_factor();
        let sidebar_width_f32 = self.sidebar_width_transition.map_or_else(
            || self.effective_sidebar_width(),
            |(from, to, started, _)| {
                let progress = started.elapsed().as_secs_f32()
                    / ui::motion::MOTION_SLOW.as_secs_f32().max(f32::EPSILON);
                from + (to - from) * ui::motion::ease_out_quint(progress)
            },
        );
        #[cfg(all(target_os = "linux", not(test)))]
        let bounds = _window.bounds();
        #[cfg(all(target_os = "linux", not(test)))]
        let width = f32::from(bounds.size.width);
        #[cfg(all(target_os = "linux", not(test)))]
        let height = f32::from(bounds.size.height);
        #[cfg(all(target_os = "linux", not(test)))]
        let content_w = (width - sidebar_width_f32 - (CONTENT_BEZEL_X * 2.0)).max(100.0) as i32;
        #[cfg(all(target_os = "linux", not(test)))]
        let content_h = (height - TOPBAR_HEIGHT - CONTENT_BEZEL_BOTTOM).max(100.0) as i32;

        #[cfg(all(target_os = "linux", not(test)))]
        if self.engine.is_initialized()
            && !active_tab_id_string.is_empty()
            && (split_pair.is_some()
                || (active_tab_url != "lumen://welcome"
                    && active_tab_url != "about:blank"
                    && !active_tab_url.is_empty()))
        {
            let (left_width, right_width) =
                split_view::pane_widths(content_w as f32, self.split_ratio);
            let left_width_dip = left_width as i32;
            let right_width_dip = right_width as i32;
            let active_width = split_pair.as_ref().map_or(content_w, |pair| {
                if pair.first == active_tab_id_string {
                    left_width_dip
                } else if pair.second == active_tab_id_string {
                    right_width_dip
                } else {
                    content_w
                }
            });
            let active_width = self
                .resize_drag
                .frozen_size(&active_tab_id_string)
                .map_or(active_width, |(width, _)| width);
            if active_tab_url != "lumen://welcome"
                && active_tab_url != "about:blank"
                && !active_tab_url.is_empty()
            {
                if let Err(error) = self.engine.ensure_browser(
                    &active_space_id,
                    &active_tab_id_string,
                    &active_tab_url,
                    active_width.max(1),
                    content_h,
                    0xff000000 | theme.surface,
                    self.zoom_levels
                        .get(&url_host(&active_tab_url))
                        .copied()
                        .unwrap_or_else(visual::default_zoom_level),
                ) {
                    eprintln!("Lumen CEF browser creation failed: {error}");
                }
            }
            if let Some(pair) = &split_pair {
                if let Some(tab) = active_space.tabs.iter().find(|tab| tab.id == pair.first) {
                    if tab.url != "lumen://welcome"
                        && tab.url != "about:blank"
                        && !tab.url.is_empty()
                    {
                        if let Err(error) = self.engine.ensure_browser(
                            &active_space_id,
                            &pair.first,
                            &tab.url,
                            self.resize_drag
                                .frozen_size(&pair.first)
                                .map_or(left_width_dip.max(1), |(width, _)| width),
                            content_h,
                            0xff000000 | theme.surface,
                            self.zoom_levels
                                .get(&url_host(&tab.url))
                                .copied()
                                .unwrap_or_else(visual::default_zoom_level),
                        ) {
                            eprintln!("Lumen split browser creation failed: {error}");
                        }
                    }
                }
                if let Some(tab) = active_space.tabs.iter().find(|tab| tab.id == pair.second) {
                    if tab.url != "lumen://welcome"
                        && tab.url != "about:blank"
                        && !tab.url.is_empty()
                    {
                        if let Err(error) = self.engine.ensure_browser(
                            &active_space_id,
                            &pair.second,
                            &tab.url,
                            self.resize_drag
                                .frozen_size(&pair.second)
                                .map_or(right_width_dip.max(1), |(width, _)| width),
                            content_h,
                            0xff000000 | theme.surface,
                            self.zoom_levels
                                .get(&url_host(&tab.url))
                                .copied()
                                .unwrap_or_else(visual::default_zoom_level),
                        ) {
                            eprintln!("Lumen split browser creation failed: {error}");
                        }
                    }
                }
            }
            let host = url_host(&active_tab_url);
            let zoom_target = (active_tab_id_string.clone(), host.clone());
            if !host.is_empty() && self.active_zoom_target.as_ref() != Some(&zoom_target) {
                let level = visual::zoom_after_delta(self.zoom_levels.get(&host).copied(), 0.0);
                self.engine.set_zoom_level(&active_tab_id_string, level);
                self.active_zoom_target = Some(zoom_target);
            } else if host.is_empty() {
                self.active_zoom_target = None;
            }
            let visible_ids = split_pair.as_ref().map_or_else(
                || vec![active_tab_id_string.as_str()],
                |pair| vec![pair.first.as_str(), pair.second.as_str()],
            );
            self.engine.set_visible_tabs(&visible_ids);
            let display = _window.display(cx);
            let window_bounds = _window.bounds();
            let (screen_width, screen_height) = display
                .map(|display| {
                    let bounds = display.bounds();
                    (
                        f32::from(bounds.size.width) as i32,
                        f32::from(bounds.size.height) as i32,
                    )
                })
                .unwrap_or((content_w, content_h));
            for tab_id in &visible_ids {
                self.engine.set_screen_metrics(
                    tab_id,
                    screen_width,
                    screen_height,
                    scale,
                    f32::from(window_bounds.origin.x) as i32,
                    f32::from(window_bounds.origin.y) as i32,
                );
            }
            if let Some(tab) = self
                .active_space_mut()
                .tabs
                .iter_mut()
                .find(|tab| tab.id == active_tab_id_string)
            {
                tab.state = TabState::Live;
            }
        } else {
            self.engine.hide_all();
        }

        #[cfg(all(target_os = "linux", not(test)))]
        for retired in self.engine.take_retired_images() {
            let _ = _window.drop_image(retired);
        }

        let sidebar_width = px(sidebar_width_f32);
        let mut sidebar_content = div()
            .id("sidebar")
            .relative()
            .flex()
            .flex_col()
            .w(sidebar_width)
            .h_full()
            .px_3()
            .py_3()
            .gap_2()
            .bg(rgb(theme.base));

        if !self.sidebar_collapsed {
            let mut sidebar_row_index = 0usize;
            let mut active_row_index = None;
            if let Some(pair) = &stored_split_pair {
                if active_tab_id.as_deref() == Some(pair.first.as_str()) {
                    active_row_index =
                        Some(sidebar_row_index + usize::from(self.split_group_expanded));
                } else if active_tab_id.as_deref() == Some(pair.second.as_str()) {
                    active_row_index =
                        Some(sidebar_row_index + usize::from(self.split_group_expanded) * 2);
                }
                sidebar_row_index += 1 + usize::from(self.split_group_expanded) * 2;
            }
            for (id, _, _) in &tabs {
                if stored_split_pair
                    .as_ref()
                    .is_some_and(|pair| id == &pair.first || id == &pair.second)
                {
                    continue;
                }
                if active_tab_id.as_deref() == Some(id.as_str()) {
                    active_row_index = Some(sidebar_row_index);
                }
                sidebar_row_index += 1;
            }
            sidebar_content = sidebar_content
                .child(
                    div()
                        .id("sidebar-resize-handle")
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(-2.0 * UI_SCALE))
                        .w(px(6.0 * UI_SCALE))
                        .cursor_col_resize()
                        .on_mouse_down(
                            gpui_kit::MouseButton::Left,
                            cx.listener(|this, _, _window, cx| {
                                this.sidebar_resizing = true;
                                #[cfg(all(target_os = "linux", not(test)))]
                                this.begin_browser_resize_drag(_window, false);
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    div().flex().items_center().px_2().py_2().child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child(selected_name),
                    ),
                );

            let _ = active_row_index;

            if let Some(pair) = &stored_split_pair {
                let first = tabs.iter().find(|(id, _, _)| id == &pair.first);
                let second = tabs.iter().find(|(id, _, _)| id == &pair.second);
                if let (Some(first), Some(second)) = (first, second) {
                    let group_name = tab_list::group_label(&first.1, &second.1);
                    let group_key = format!("split-{}-{}", pair.first, pair.second);
                    let expanded = self.split_group_expanded;
                    crate::engine::log_favicon_row_rendered(&first.0, first.2.is_some());
                    crate::engine::log_favicon_row_rendered(&second.0, second.2.is_some());
                    let group_id = pair.first.clone();
                    let target_rotation = if expanded { std::f32::consts::PI } else { 0.0 };
                    let chevron = Icon::new(IconName::ChevronDown).size(px(14.0 * UI_SCALE));
                    let chevron = if ui::motion::reduced_motion() {
                        chevron
                            .rotate(gpui_kit::percentage(ui::motion::clamp_percentage(
                                target_rotation / (2.0 * std::f32::consts::PI),
                            )))
                            .into_any_element()
                    } else {
                        chevron
                            .with_spring(
                                "split-group-chevron-rotation",
                                SpringAnimation::new(SpringConfig::new(400.0, 32.0, 1.0))
                                    .to(target_rotation),
                                |icon, angle| {
                                    icon.rotate(gpui_kit::percentage(ui::motion::clamp_percentage(
                                        angle / (2.0 * std::f32::consts::PI),
                                    )))
                                },
                            )
                            .into_any_element()
                    };
                    let group_active = !expanded
                        && (active_tab_id.as_deref() == Some(pair.first.as_str())
                            || active_tab_id.as_deref() == Some(pair.second.as_str()));
                    let group_row = div()
                        .id("split-tab-group")
                        .w_full()
                        .h(px(visual::TAB_ROW_HEIGHT))
                        .cursor_pointer()
                        .active(|style| style.opacity(0.92))
                        .group(group_key.clone())
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .flex()
                                .items_center()
                                .gap(px(visual::SIDEBAR_ROW_GAP))
                                .px(px(visual::SIDEBAR_ROW_PADDING_X))
                                .rounded_lg()
                                .when(group_active, |row| row.bg(rgb(theme.surface)))
                                .hover(|row| row.bg(rgb(theme.surface_hover)))
                                .child(tab_list::split_favicon(first.2.clone(), second.2.clone()))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .text_color(rgb(theme.text))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(group_name),
                                )
                                .child(
                                    div()
                                        .id("split-group-toggle")
                                        .size(px(16.0 * UI_SCALE))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(rgb(theme.muted))
                                        .opacity(if expanded { 1.0 } else { 0.0 })
                                        .group_hover(group_key.clone(), |style| style.opacity(1.0))
                                        .child(chevron)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.split_group_expanded = !this.split_group_expanded;
                                            cx.stop_propagation();
                                            cx.notify();
                                        })),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate_split_group();
                            cx.notify();
                        }))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Right,
                            cx.listener(
                                move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                                    let (x, y, width, height) =
                                        tab_context_menu_position(event, window);
                                    this.tab_context_menu = Some(TabContextMenu {
                                        tab_id: group_id.clone(),
                                        showing_split_targets: false,
                                        x,
                                        y,
                                        width,
                                        height,
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                            ),
                        );
                    if expanded {
                        let mut member_rows = Vec::new();
                        for (pane_index, member) in [first, second].into_iter().enumerate() {
                            let member_id = member.0.clone();
                            let member_selected =
                                active_tab_id.as_deref() == Some(member_id.as_str());
                            let member_animation_id = member_id.clone();
                            let close_id = member_id.clone();
                            let pane = pane_index;
                            let stagger = std::time::Duration::from_millis(30 * pane_index as u64);
                            let total_duration = ui::motion::MOTION_BASE + stagger;
                            let base_seconds = ui::motion::MOTION_BASE.as_secs_f32();
                            let delay_seconds = stagger.as_secs_f32();
                            let total_seconds = total_duration.as_secs_f32();
                            crate::engine::log_favicon_row_rendered(&member.0, member.2.is_some());
                            let member_row = div()
                                .id(format!("split-member-{member_id}"))
                                .w_full()
                                .h(px(visual::TAB_ROW_HEIGHT))
                                .cursor_pointer()
                                .active(|style| style.opacity(0.92))
                                .group(format!("tab-row-{member_id}"))
                                .child(
                                    div()
                                        .w_full()
                                        .h_full()
                                        .flex()
                                        .items_center()
                                        .gap(px(visual::SIDEBAR_ROW_GAP))
                                        .pl(px(visual::SIDEBAR_ROW_PADDING_X
                                            + visual::SIDEBAR_ROW_ICON_SIZE
                                            + visual::SIDEBAR_ROW_GAP))
                                        .pr(px(visual::SIDEBAR_ROW_PADDING_X))
                                        .rounded_lg()
                                        .when(member_selected, |row| row.bg(rgb(theme.surface)))
                                        .hover(|row| row.bg(rgb(theme.surface_hover)))
                                        .child(tab_list::favicon(member.2.clone()))
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_sm()
                                                .text_color(rgb(theme.text))
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .child(member.1.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(format!("close-member-{member_id}"))
                                                .size(px(16.0 * UI_SCALE))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .opacity(0.0)
                                                .group_hover(format!("tab-row-{member_id}"), |s| {
                                                    s.opacity(1.0)
                                                })
                                                .text_color(rgb(theme.muted))
                                                .child(
                                                    Icon::new(IconName::X)
                                                        .size(px(14.0 * UI_SCALE)),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.close_tab_by_id(&close_id);
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_focused_pane(pane);
                                    cx.stop_propagation();
                                    cx.notify();
                                }))
                                .on_mouse_down(
                                    gpui_kit::MouseButton::Right,
                                    cx.listener(
                                        move |this,
                                              event: &gpui_kit::MouseDownEvent,
                                              window,
                                              cx| {
                                            let (x, y, width, height) =
                                                tab_context_menu_position(event, window);
                                            this.tab_context_menu = Some(TabContextMenu {
                                                tab_id: member_id.clone(),
                                                showing_split_targets: false,
                                                x,
                                                y,
                                                width,
                                                height,
                                            });
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    ),
                                )
                                .with_animation(
                                    format!("split-member-entry-{member_animation_id}"),
                                    Animation::new(ui::motion::duration(total_duration))
                                        .with_easing(move |progress| {
                                            ui::motion::spring_out(
                                                ((progress * total_seconds - delay_seconds)
                                                    / base_seconds)
                                                    .clamp(0.0, 1.0),
                                            )
                                        }),
                                    move |row, progress| {
                                        let progress = ui::motion::clamp_percentage(progress);
                                        row.h(px(visual::TAB_ROW_HEIGHT * progress))
                                            .opacity(progress)
                                    },
                                );
                            member_rows.push(member_row);
                        }
                        sidebar_content = sidebar_content.child(group_row);
                        for member_row in member_rows {
                            sidebar_content = sidebar_content.child(member_row);
                        }
                    } else {
                        sidebar_content = sidebar_content.child(group_row);
                    }
                }
            }

            for (id, title, favicon) in &tabs {
                if stored_split_pair
                    .as_ref()
                    .is_some_and(|pair| id == &pair.first || id == &pair.second)
                {
                    continue;
                }
                crate::engine::log_favicon_row_rendered(id, favicon.is_some());
                let tab_id = id.clone();
                let close_id = tab_id.clone();
                let current_space_id = active_space_id.clone();
                let selected = active_tab_id.as_ref() == Some(id);
                let row_group = format!("tab-row-{id}");
                let context_tab = id.clone();
                sidebar_content = sidebar_content.child(
                    div()
                        .id(format!("tab-{id}"))
                        .w_full()
                        .h(px(visual::TAB_ROW_HEIGHT))
                        .cursor_pointer()
                        .active(|style| style.opacity(0.92))
                        .group(row_group.clone())
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .flex()
                                .items_center()
                                .gap(px(visual::SIDEBAR_ROW_GAP))
                                .px(px(visual::SIDEBAR_ROW_PADDING_X))
                                .rounded_lg()
                                .when(selected, |row| row.bg(rgb(theme.surface)))
                                .hover(|row| row.bg(rgb(theme.surface_hover)))
                                .child(tab_list::favicon(favicon.clone()))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .text_color(rgb(if selected {
                                            theme.text
                                        } else {
                                            theme.muted
                                        }))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(title.clone()),
                                )
                                .child(
                                    div()
                                        .id(format!("close-tab-{id}"))
                                        .size(px(16.0 * UI_SCALE))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .opacity(0.0)
                                        .group_hover(row_group.clone(), |s| s.opacity(1.0))
                                        .text_color(rgb(theme.muted))
                                        .child(Icon::new(IconName::X).size(px(14.0 * UI_SCALE)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.close_tab_by_id(&close_id);
                                            cx.stop_propagation();
                                            cx.notify();
                                        })),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(space) = this
                                .spaces
                                .iter_mut()
                                .find(|space| space.id == current_space_id)
                            {
                                space.active = Some(tab_id.clone());
                            }
                            cx.notify();
                        }))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Right,
                            cx.listener(
                                move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                                    let (x, y, width, height) =
                                        tab_context_menu_position(event, window);
                                    this.tab_context_menu = Some(TabContextMenu {
                                        tab_id: context_tab.clone(),
                                        showing_split_targets: false,
                                        x,
                                        y,
                                        width,
                                        height,
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                            ),
                        )
                        .with_animation(
                            format!("tab-row-entry-{id}"),
                            Animation::new(ui::motion::duration(ui::motion::MOTION_BASE))
                                .with_easing(ui::motion::spring_out),
                            move |row, progress| {
                                let progress = ui::motion::clamp_percentage(progress);
                                row.h(px(visual::TAB_ROW_HEIGHT * progress))
                                    .opacity(progress)
                            },
                        ),
                );
            }

            if let Some((closing_space, closing_tab, closing_index, started, serial)) =
                &self.closing_tab
                && closing_space == &active_space_id
                && started.elapsed() < ui::motion::MOTION_FAST
            {
                let row_index = if let Some(pair) = &stored_split_pair {
                    let before = tabs
                        .iter()
                        .take(*closing_index)
                        .filter(|(id, _, _)| id != &pair.first && id != &pair.second)
                        .count();
                    1 + usize::from(self.split_group_expanded) * 2 + before
                } else {
                    *closing_index
                };
                let row_rect = visual::sidebar_row_rect(sidebar_width_f32, row_index);
                sidebar_content = sidebar_content.child(
                    div()
                        .id(format!("closing-tab-{}-{serial}", closing_tab.id))
                        .absolute()
                        .left(px(row_rect.left))
                        .top(px(row_rect.top))
                        .w(px(row_rect.width))
                        .h(px(row_rect.height))
                        .overflow_hidden()
                        .rounded_lg()
                        .bg(rgb(theme.surface))
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .flex()
                                .items_center()
                                .gap(px(visual::SIDEBAR_ROW_GAP))
                                .px(px(visual::SIDEBAR_ROW_PADDING_X))
                                .text_sm()
                                .text_color(rgb(theme.muted))
                                .child(tab_list::favicon(None))
                                .child(closing_tab.title.clone()),
                        )
                        .with_animation(
                            format!("tab-row-close-{}-{serial}", closing_tab.id),
                            Animation::new(ui::motion::duration(ui::motion::MOTION_FAST))
                                .with_easing(ui::motion::ease_out_quint),
                            move |row, progress| {
                                let remaining = ui::motion::clamp_percentage(1.0 - progress);
                                row.h(px(visual::TAB_ROW_HEIGHT * remaining))
                                    .opacity(remaining)
                            },
                        ),
                );
            }

            sidebar_content = sidebar_content.child(
                div()
                    .id("new-tab-action")
                    .w_full()
                    .h(px(visual::TAB_ROW_HEIGHT))
                    .cursor_pointer()
                    .active(|style| style.opacity(0.92))
                    .child(sidebar::new_tab_button(theme))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_tab();
                        this.address.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                            input.focus(window, cx);
                        });
                        cx.notify();
                    })),
            );

            // Spacer pushing the Arc-style dock to the bottom
            sidebar_content = sidebar_content.child(div().flex_1().min_h(px(8.0 * UI_SCALE)));

            // Bottom bar with Settings, Space Switcher (icons like Arc), and Downloads
            let active_space_index = self
                .spaces
                .iter()
                .position(|space| space.id == active_space_id)
                .unwrap_or(0);
            let mut spaces_dock = div()
                .id("spaces-dock")
                .relative()
                .flex()
                .items_center()
                .gap_1();
            let indicator_left = active_space_index as f32 * (28.0 * UI_SCALE + 4.0);
            let indicator = div()
                .id("space-dock-active-indicator")
                .absolute()
                .top_0()
                .left_0()
                .size(px(28.0 * UI_SCALE))
                .rounded_lg()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border_hover));
            let indicator = if ui::motion::reduced_motion() {
                indicator.left(px(indicator_left)).into_any_element()
            } else {
                indicator
                    .with_spring(
                        "space-dock-active-indicator-position",
                        SpringAnimation::new(SpringConfig::new(400.0, 32.0, 1.0))
                            .to(px(indicator_left)),
                        |indicator, left| indicator.left(left),
                    )
                    .into_any_element()
            };
            spaces_dock = spaces_dock.child(indicator);

            for space in &self.spaces {
                let id = space.id.clone();
                let is_active = space.id == active_space_id;
                let icon_btn = space_switcher::space_dock_icon_button(
                    &space.icon,
                    is_active,
                    visual::space_theme(space.color),
                );
                let id_right = id.clone();
                spaces_dock = spaces_dock.child(
                    div()
                        .id(format!("space-icon-{id}"))
                        .cursor_pointer()
                        .active(|style| style.opacity(0.92))
                        .child(icon_btn)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.space_context_menu = None;
                            this.activate_space(&id);
                            cx.notify();
                        }))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Right,
                            cx.listener(move |this, _, _, cx| {
                                this.space_context_menu = Some(SpaceContextMenu {
                                    space_id: id_right.clone(),
                                });
                                cx.notify();
                            }),
                        ),
                );
            }

            spaces_dock = spaces_dock.child(
                div()
                    .id("add-space-btn")
                    .cursor_pointer()
                    .active(|style| style.opacity(0.92))
                    .child(space_switcher::add_space_button(theme))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_create_space_modal(window, cx);
                        cx.notify();
                    })),
            );

            let bottom_bar = div()
                .id("sidebar-bottom-bar")
                .w_full()
                .pt_2()
                .border_t_1()
                .border_color(rgb(theme.border))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .id("settings-btn")
                        .child(space_switcher::settings_button(theme)),
                )
                .child(spaces_dock)
                .child(
                    div()
                        .id("downloads-btn")
                        .child(space_switcher::downloads_button(theme)),
                );

            sidebar_content = sidebar_content.child(bottom_bar);
        } else {
            let active_space_icon = self.active_space().icon.clone();
            sidebar_content = sidebar_content
                .child(
                    div()
                        .id("expand-sidebar")
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .text_color(rgb(theme.muted))
                        .hover(|style| style.bg(rgb(theme.surface_hover)))
                        .cursor_pointer()
                        .child("›")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.animate_sidebar_toggle(window, cx);
                        })),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id("collapsed-space-icon")
                        .flex()
                        .justify_center()
                        .w_full()
                        .child(space_switcher::space_icon_button(
                            &active_space_icon,
                            true,
                            theme,
                        )),
                );
        }

        let sidebar_content = if let Some((_, state, _, serial)) = &space_switch_transition {
            let start_alpha = state.incoming_opacity_start;
            let start_offset = state.incoming_offset_start;
            sidebar_content
                .with_animation(
                    format!("space-switch-sidebar-in-{serial}"),
                    Animation::new(ui::motion::duration(ui::motion::MOTION_SPACE_SWITCH))
                        .with_easing(ui::motion::ease_out_quint),
                    move |sidebar, progress| {
                        sidebar.left(px(start_offset * (1.0 - progress))).opacity(
                            ui::motion::clamp_percentage(
                                start_alpha + (1.0 - start_alpha) * progress,
                            ),
                        )
                    },
                )
                .into_any_element()
        } else if let Some((from, to, _, serial)) = self.sidebar_width_transition {
            sidebar_content
                .with_animation(
                    format!("sidebar-width-transition-{serial}"),
                    Animation::new(ui::motion::duration(ui::motion::MOTION_SLOW))
                        .with_easing(ui::motion::ease_out_quint),
                    move |sidebar, progress| {
                        sidebar.w(px(from + (to - from) * progress.clamp(0.0, 1.0)))
                    },
                )
                .into_any_element()
        } else {
            sidebar_content.into_any_element()
        };

        let outgoing_space_overlay =
            space_switch_transition
                .as_ref()
                .map(|(outgoing_space, state, _, serial)| {
                    let old_theme = visual::space_theme(outgoing_space.color);
                    let mut overlay = div()
                        .id(format!(
                            "space-switch-outgoing-{}-{serial}",
                            outgoing_space.id
                        ))
                        .absolute()
                        .top_0()
                        .left_0()
                        .w(px(sidebar_width_f32))
                        .h_full()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .px_3()
                        .py_3()
                        .gap_2()
                        .bg(rgb(old_theme.base))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .px_2()
                                .py_2()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(old_theme.text))
                                .child(outgoing_space.name.clone()),
                        );
                    let mut outgoing_rows = Vec::new();
                    let split_tabs = outgoing_space.split.as_ref().and_then(|pair| {
                        let first = outgoing_space
                            .tabs
                            .iter()
                            .find(|tab| tab.id == pair.first)?;
                        let second = outgoing_space
                            .tabs
                            .iter()
                            .find(|tab| tab.id == pair.second)?;
                        Some((pair, first, second))
                    });
                    if let Some((pair, first, second)) = split_tabs {
                        let active_is_member = outgoing_space.active.as_deref()
                            == Some(pair.first.as_str())
                            || outgoing_space.active.as_deref() == Some(pair.second.as_str());
                        outgoing_rows.push((
                            tab_list::group_label(&first.title, &second.title),
                            None,
                            active_is_member && !self.split_group_expanded,
                        ));
                        if self.split_group_expanded {
                            for member in [first, second] {
                                outgoing_rows.push((
                                    member.title.clone(),
                                    self.engine.get_favicon_image(&member.id),
                                    outgoing_space.active.as_deref() == Some(member.id.as_str()),
                                ));
                            }
                        }
                    }
                    for tab in &outgoing_space.tabs {
                        if outgoing_space
                            .split
                            .as_ref()
                            .is_some_and(|pair| tab.id == pair.first || tab.id == pair.second)
                        {
                            continue;
                        }
                        outgoing_rows.push((
                            tab.title.clone(),
                            self.engine.get_favicon_image(&tab.id),
                            outgoing_space.active.as_deref() == Some(tab.id.as_str()),
                        ));
                    }
                    let new_tab_index = outgoing_rows.len();
                    let active_row_index = outgoing_rows.iter().position(|(_, _, active)| *active);
                    for (row_index, (title, favicon, selected)) in
                        outgoing_rows.into_iter().enumerate()
                    {
                        let _ = row_index;
                        let row = div()
                            .flex_none()
                            .w_full()
                            .h(px(visual::TAB_ROW_HEIGHT))
                            .rounded_lg()
                            .when(selected, |row| row.bg(rgb(old_theme.surface)))
                            .flex()
                            .items_center()
                            .gap(px(visual::SIDEBAR_ROW_GAP))
                            .px(px(visual::SIDEBAR_ROW_PADDING_X))
                            .text_sm()
                            .text_color(rgb(if selected {
                                old_theme.text
                            } else {
                                old_theme.muted
                            }))
                            .overflow_hidden()
                            .child(tab_list::favicon(favicon))
                            .child(title);
                        overlay = overlay.child(row);
                    }
                    let _ = active_row_index;
                    let _ = new_tab_index;
                    overlay = overlay.child(
                        div()
                            .flex_none()
                            .w_full()
                            .h(px(visual::TAB_ROW_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(visual::SIDEBAR_ROW_GAP))
                            .px(px(visual::SIDEBAR_ROW_PADDING_X))
                            .text_sm()
                            .text_color(rgb(old_theme.muted))
                            .child(Icon::new(IconName::Plus).size(px(14.0 * UI_SCALE)))
                            .child("New tab"),
                    );
                    let initial_alpha = state.outgoing_opacity_start;
                    let initial_offset = state.outgoing_offset_start;
                    let outgoing_offset = state.outgoing_direction * 24.0;
                    overlay
                        .with_animation(
                            format!("space-switch-sidebar-out-{serial}"),
                            Animation::new(ui::motion::duration(ui::motion::MOTION_SPACE_SWITCH))
                                .with_easing(ui::motion::ease_out_quint),
                            move |overlay, progress| {
                                overlay
                                    .left(px(initial_offset * (1.0 - progress)
                                        + outgoing_offset * progress))
                                    .opacity(ui::motion::clamp_percentage(
                                        initial_alpha * (1.0 - progress),
                                    ))
                            },
                        )
                        .into_any_element()
                });

        let mut root = div()
            .relative()
            .flex()
            .size_full()
            .bg(rgb(theme.base))
            .text_color(rgb(theme.text))
            .on_mouse_move(
                cx.listener(|this, event: &gpui_kit::MouseMoveEvent, window, cx| {
                    if this.split_resizing {
                        let bounds = window.bounds();
                        let sidebar = this.effective_sidebar_width();
                        let card_left = sidebar + CONTENT_BEZEL_X;
                        let card_width =
                            f32::from(bounds.size.width) - sidebar - 2.0 * CONTENT_BEZEL_X;
                        this.split_ratio = split_view::ratio_at_pointer(
                            f32::from(event.position.x),
                            card_left,
                            card_width,
                        );
                        cx.notify();
                    }
                    if this.sidebar_resizing {
                        let new_width = f32::from(event.position.x);
                        this.sidebar_width = new_width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Left,
                cx.listener(|this, _, _window, cx| {
                    #[cfg(all(target_os = "linux", not(test)))]
                    let finish_split_resize = this.split_resizing;
                    #[cfg(all(target_os = "linux", not(test)))]
                    if this.sidebar_resizing || finish_split_resize {
                        let bounds = _window.bounds();
                        let sidebar = this.effective_sidebar_width();
                        let content_width =
                            (f32::from(bounds.size.width) - sidebar - 2.0 * CONTENT_BEZEL_X)
                                .max(100.0);
                        let height =
                            (f32::from(bounds.size.height) - TOPBAR_HEIGHT - CONTENT_BEZEL_BOTTOM)
                                .max(100.0) as i32;
                        let exact_sizes = if let Some(pair) = this.current_split_pair() {
                            let (left, right) =
                                split_view::pane_widths(content_width, this.split_ratio);
                            vec![
                                split_view::BrowserSize {
                                    tab_id: pair.first,
                                    width: left.max(1.0) as i32,
                                    height,
                                },
                                split_view::BrowserSize {
                                    tab_id: pair.second,
                                    width: right.max(1.0) as i32,
                                    height,
                                },
                            ]
                        } else {
                            this.active_space()
                                .active
                                .clone()
                                .map(|tab_id| {
                                    vec![split_view::BrowserSize {
                                        tab_id,
                                        width: content_width as i32,
                                        height,
                                    }]
                                })
                                .unwrap_or_default()
                        };
                        this.finish_browser_resize_drag(
                            exact_sizes,
                            if finish_split_resize {
                                "split"
                            } else {
                                "sidebar"
                            },
                        );
                    }
                    if this.sidebar_resizing || this.split_resizing {
                        this.sidebar_resizing = false;
                        this.split_resizing = false;
                        this.save_state();
                        cx.notify();
                    }
                }),
            )
            .child(sidebar_content)
            .children(outgoing_space_overlay)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(TOPBAR_HEIGHT))
                            .flex()
                            .items_center()
                            .px_4()
                            .child(
                                div()
                                    .id("address-bar-container")
                                    .flex_1()
                                    .h(px(ADDRESS_BAR_HEIGHT))
                                    .px_3()
                                    .rounded_lg()
                                    .bg(rgb(theme.surface))
                                    .border_1()
                                    .border_color(rgb(if self.browser_focused {
                                        theme.border
                                    } else {
                                        selected_color
                                    }))
                                    .hover(move |s| s.border_color(rgb(theme.border_hover)))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .cursor_text()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.browser_focused = false;
                                        let tab_id = this.active_tab_id().to_string();
                                        this.engine.set_focus(&tab_id, false);
                                        this.address.update(cx, |input, cx| {
                                            input.focus(window, cx);
                                        });
                                        let text = this.address.read(cx).value().to_string();
                                        this.update_suggestions(&text);
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_color(rgb(0x777b85))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(Icon::new(IconName::Search)),
                                    )
                                    .child(
                                        Input::new(&self.address)
                                            .id("address-bar")
                                            .w_full()
                                            .appearance(false),
                                    ),
                            ),
                    )
                    .child({
                        let viewport_sidebar = sidebar_width_f32;
                        let total_width = f32::from(_window.bounds().size.width)
                            - viewport_sidebar
                            - 2.0 * CONTENT_BEZEL_X;
                        let (left_width, right_width) =
                            split_view::pane_widths(total_width, self.split_ratio);
                        let left_tab_id = split_pair
                            .as_ref()
                            .map(|pair| pair.first.clone())
                            .unwrap_or_else(|| active_tab_id_string.clone());
                        let left_image = self.engine.get_image(&left_tab_id);
                        let left_cursor = self.engine.get_cursor(&left_tab_id);
                        #[cfg(all(target_os = "linux", not(test)))]
                        let left_frame_matches_target = self
                            .resize_drag
                            .target_size(&left_tab_id)
                            .is_some_and(|(w, h)| {
                                self.engine.frame_matches_size(&left_tab_id, w, h)
                            });
                        #[cfg(all(target_os = "linux", not(test)))]
                        let left_edge_color = self
                            .resize_drag
                            .edge_color_until_frame(&left_tab_id, left_frame_matches_target);
                        #[cfg(any(not(target_os = "linux"), test))]
                        let left_edge_color = None;
                        self.web_viewport.update(cx, |viewport, cx| {
                            viewport.set_theme(theme);
                            viewport.set_frame(
                                left_tab_id.clone(),
                                viewport_sidebar,
                                left_image,
                                left_cursor,
                                Vec::new(),
                                0.0,
                                0,
                                split_pair.is_some(),
                                left_edge_color,
                            );
                            cx.notify();
                        });
                        let right_tab_id = split_pair.as_ref().map(|pair| pair.second.clone());
                        if let Some(right_tab_id) = right_tab_id {
                            let right_image = self.engine.get_image(&right_tab_id);
                            let right_cursor = self.engine.get_cursor(&right_tab_id);
                            #[cfg(all(target_os = "linux", not(test)))]
                            let right_frame_matches_target =
                                self.resize_drag.target_size(&right_tab_id).is_some_and(
                                    |(w, h)| self.engine.frame_matches_size(&right_tab_id, w, h),
                                );
                            #[cfg(all(target_os = "linux", not(test)))]
                            let right_edge_color = self
                                .resize_drag
                                .edge_color_until_frame(&right_tab_id, right_frame_matches_target);
                            #[cfg(any(not(target_os = "linux"), test))]
                            let right_edge_color = None;
                            self.second_web_viewport.update(cx, |viewport, cx| {
                                viewport.set_theme(theme);
                                viewport.set_frame(
                                    right_tab_id.clone(),
                                    viewport_sidebar,
                                    right_image,
                                    right_cursor,
                                    Vec::new(),
                                    left_width + DIVIDER_WIDTH,
                                    1,
                                    true,
                                    right_edge_color,
                                );
                                cx.notify();
                            });
                        }
                        div()
                            .id("web-content-card")
                            .flex_1()
                            .mx(px(CONTENT_BEZEL_X))
                            .mb(px(CONTENT_BEZEL_BOTTOM))
                            .rounded(px(CARD_RADIUS))
                            .bg(rgb(theme.surface))
                            .overflow_hidden()
                            .child(if split_pair.is_some() {
                                div()
                                    .id("split-viewports")
                                    .size_full()
                                    .flex()
                                    .child(
                                        div()
                                            .h_full()
                                            .w(px(left_width))
                                            .with_animation(
                                                format!(
                                                    "split-left-enter-{}",
                                                    split_pair.as_ref().unwrap().first
                                                ),
                                                Animation::new(ui::motion::duration(
                                                    ui::motion::MOTION_SLOW,
                                                ))
                                                .with_easing(ui::motion::ease_out_quint),
                                                |pane, progress| {
                                                    pane.opacity(ui::motion::clamp_percentage(
                                                        progress,
                                                    ))
                                                },
                                            )
                                            .child(
                                                AnyView::from(self.web_viewport.clone())
                                                    .cached(StyleRefinement::default().size_full()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("split-divider")
                                            .w(px(DIVIDER_HIT_WIDTH))
                                            .mx(px((DIVIDER_WIDTH - DIVIDER_HIT_WIDTH) / 2.0))
                                            .h_full()
                                            .cursor_col_resize()
                                            .on_mouse_down(
                                                gpui_kit::MouseButton::Left,
                                                cx.listener(|this, _, _window, cx| {
                                                    this.split_resizing = true;
                                                    #[cfg(all(target_os = "linux", not(test)))]
                                                    this.begin_browser_resize_drag(_window, true);
                                                    cx.notify();
                                                }),
                                            )
                                            .with_animation(
                                                "split-divider-enter",
                                                Animation::new(ui::motion::duration(
                                                    ui::motion::MOTION_SLOW,
                                                ))
                                                .with_easing(ui::motion::ease_out_quint),
                                                |divider, progress| {
                                                    divider.opacity(ui::motion::clamp_percentage(
                                                        0.35 + 0.65 * progress,
                                                    ))
                                                },
                                            ),
                                    )
                                    .child(
                                        div()
                                            .h_full()
                                            .w(px(right_width))
                                            .with_animation(
                                                format!(
                                                    "split-right-enter-{}",
                                                    split_pair.as_ref().unwrap().second
                                                ),
                                                Animation::new(ui::motion::duration(
                                                    ui::motion::MOTION_SLOW,
                                                ))
                                                .with_easing(ui::motion::ease_out_quint),
                                                |pane, progress| {
                                                    pane.opacity(ui::motion::clamp_percentage(
                                                        progress,
                                                    ))
                                                },
                                            )
                                            .child(
                                                AnyView::from(self.second_web_viewport.clone())
                                                    .cached(StyleRefinement::default().size_full()),
                                            ),
                                    )
                            } else if active_tab_url == "lumen://welcome"
                                || active_tab_url == "about:blank"
                                || active_tab_url.is_empty()
                            {
                                div().id("empty-space").size_full()
                            } else {
                                div().id("web-viewport-entity").size_full().child(
                                    AnyView::from(self.web_viewport.clone())
                                        .cached(StyleRefinement::default().size_full()),
                                )
                            })
                    }),
            );

        let suggestions_open = self.show_suggestions && !self.suggestions.is_empty();
        if suggestions_open {
            if !self.suggestions_were_visible {
                self.suggestion_animation_serial = self.suggestion_animation_serial.wrapping_add(1);
                self.suggestions_were_visible = true;
                self.suggestion_closing = None;
            }
            self.suggestion_snapshot.clone_from(&self.suggestions);
        } else if self.suggestions_were_visible {
            self.suggestions_were_visible = false;
            self.suggestion_animation_serial = self.suggestion_animation_serial.wrapping_add(1);
            let serial = self.suggestion_animation_serial;
            self.suggestion_closing = Some((self.suggestion_snapshot.clone(), serial));
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(ui::motion::duration(ui::motion::MOTION_FAST))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this
                        .suggestion_closing
                        .as_ref()
                        .is_some_and(|(_, current)| *current == serial)
                    {
                        this.suggestion_closing = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        let suggestion_items = if suggestions_open {
            self.suggestions.clone()
        } else {
            self.suggestion_closing
                .as_ref()
                .map(|(items, _)| items.clone())
                .unwrap_or_default()
        };
        let closing_suggestions = self.suggestion_closing.as_ref().map(|(_, serial)| *serial);

        root = root.when(suggestions_open || closing_suggestions.is_some(), |this| {
            let mut list = div()
                .id("autocomplete-suggestions")
                .occlude()
                .absolute()
                .top(px(TOPBAR_HEIGHT + 4.0 * UI_SCALE))
                .left(px(sidebar_width_f32 + 16.0 * UI_SCALE))
                .w(px(560.0 * UI_SCALE))
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .rounded_xl()
                .p_1p5()
                .flex()
                .flex_col()
                .gap_1();

            for (idx, item) in suggestion_items.iter().enumerate() {
                let target = item.target.clone();
                let title = item.title.clone();
                let icon_type = item.icon_type.clone();
                list = list.child(
                    div()
                        .id(format!("suggestion-item-{idx}"))
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .gap_3()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme.surface_hover)))
                        .when(
                            closing_suggestions.is_none() && self.selected_suggestion == Some(idx),
                            |row| row.bg(rgb(theme.surface_hover)),
                        )
                        .child(
                            div()
                                .text_color(rgb(0x8b8f9a))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(16.0 * UI_SCALE))
                                .child(match icon_type.as_str() {
                                    "globe" => Icon::new(IconName::Globe),
                                    "zap" => Icon::new(IconName::Zap),
                                    _ => Icon::new(IconName::Search),
                                }),
                        )
                        .child(div().text_sm().text_color(rgb(theme.text)).child(title))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate_to(&target, window, cx);
                            cx.notify();
                        })),
                );
            }

            let list = if let Some(serial) = closing_suggestions {
                list.with_animation(
                    format!("autocomplete-suggestions-exit-{serial}"),
                    Animation::new(ui::motion::duration(ui::motion::MOTION_FAST))
                        .with_easing(ui::motion::ease_out_quint),
                    |panel, progress| {
                        let remaining = ui::motion::clamp_percentage(1.0 - progress);
                        panel
                            .w(px(560.0 * UI_SCALE * (0.96 + 0.04 * remaining)))
                            .opacity(remaining)
                    },
                )
                .into_any_element()
            } else {
                list.with_animation(
                    format!(
                        "autocomplete-suggestions-entrance-{}",
                        self.suggestion_animation_serial
                    ),
                    Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                        .with_easing(ui::motion::ease_out_quint),
                    |panel, progress| {
                        panel
                            .w(px(560.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                            .opacity(ui::motion::clamp_percentage(progress))
                    },
                )
                .into_any_element()
            };
            this.child(list)
        });

        root = root.when(self.show_find, |root| {
            root.child(
                div()
                    .id("find-in-page")
                    .absolute()
                    .top(px(8.0 * UI_SCALE))
                    .right(px(16.0 * UI_SCALE))
                    .w(px(300.0 * UI_SCALE))
                    .h(px(36.0 * UI_SCALE))
                    .px_3()
                    .rounded_lg()
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .child(
                        Input::new(&self.find_input)
                            .id("find-input")
                            .w_full()
                            .appearance(false),
                    ),
            )
        });

        if self.show_command_palette {
            let query = self.command_input.read(cx).value().to_string();
            let items = self.command_items(&query);
            let mut list = div().flex().flex_col().gap_1().p_1p5();
            for (index, item) in items.iter().enumerate() {
                let item = item.clone();
                list = list.child(
                    div()
                        .id(format!("command-palette-item-{index}"))
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .cursor_pointer()
                        .when(self.selected_command == index, |row| {
                            row.bg(rgb(theme.surface_hover))
                        })
                        .hover(|style| style.bg(rgb(theme.surface_hover)))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(theme.text))
                                        .child(item.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme.muted))
                                        .child(item.detail.clone()),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.execute_palette_item(item.clone(), window, cx);
                        })),
                );
            }
            let panel = div()
                .id("command-palette")
                .w(px(640.0 * UI_SCALE))
                .max_h(px(520.0 * UI_SCALE))
                .overflow_hidden()
                .rounded_xl()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .shadow_lg()
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(
                            Input::new(&self.command_input)
                                .id("command-palette-input")
                                .w_full()
                                .appearance(false),
                        ),
                )
                .child(list)
                .with_animation(
                    "command-palette-entrance",
                    Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                        .with_easing(ui::motion::ease_out_quint),
                    |panel, progress| {
                        panel
                            .w(px(640.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                            .opacity(ui::motion::clamp_percentage(progress))
                    },
                );
            root = root.child(
                div()
                    .absolute()
                    .top(px(88.0 * UI_SCALE))
                    .left(px(0.0))
                    .right(px(0.0))
                    .flex()
                    .justify_center()
                    .occlude()
                    .child(panel),
            );
        }

        root = root
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.to_lowercase();
                let modifiers = &event.keystroke.modifiers;
                let browser_modifier = modifiers.control || modifiers.platform;
                if !event.is_held
                    && matches!(key.as_str(), "escape" | "esc")
                    && this.tab_context_menu.is_some()
                {
                    this.tab_context_menu = None;
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if browser_modifier && key == "k" && !modifiers.shift && !event.is_held {
                    this.open_command_palette(window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !event.is_held && browser_modifier && modifiers.shift && key == "s" {
                    this.split_active_with_next();
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !event.is_held && browser_modifier && modifiers.shift && key == "w" {
                    this.exit_split();
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !event.is_held
                    && modifiers.alt
                    && browser_modifier
                    && this.current_split_pair().is_some()
                    && matches!(key.as_str(), "left" | "arrowleft" | "right" | "arrowright")
                {
                    let pane_index = if matches!(key.as_str(), "left" | "arrowleft") {
                        0
                    } else {
                        1
                    };
                    this.set_focused_pane(pane_index);
                    if let Some(pair) = this.current_split_pair() {
                        this.engine.set_focus(&pair.first, false);
                        this.engine.set_focus(&pair.second, false);
                    }
                    let focused = this.focused_tab_id().to_string();
                    this.engine.set_focus(&focused, true);
                    this.browser_focused = true;
                    window.focus(&this.browser_focus_handle, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.show_command_palette {
                    if event.is_held {
                        return;
                    }
                    let query = this.command_input.read(cx).value().to_string();
                    let items = this.command_items(&query);
                    match key.as_str() {
                        "escape" | "esc" => this.close_command_palette(window, cx),
                        "up" | "arrowup" => this.move_command_selection(false, items.len()),
                        "down" | "arrowdown" => this.move_command_selection(true, items.len()),
                        "enter" | "return" => {
                            if let Some(item) = items.get(this.selected_command).cloned() {
                                this.execute_palette_item(item, window, cx);
                            }
                        }
                        _ => return,
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.show_find && matches!(key.as_str(), "enter" | "return") {
                    this.find_in_page(!modifiers.shift, true, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if matches!(key.as_str(), "escape" | "esc") {
                    if this.show_find {
                        this.engine.stop_finding(this.focused_tab_id());
                        this.show_find = false;
                        this.focus_browser(window, cx);
                        window.prevent_default();
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    if this.engine.is_loading(this.focused_tab_id()) {
                        this.engine.stop_load(this.focused_tab_id());
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                }
                if browser_modifier && key == "f" && !modifiers.shift {
                    this.show_find = true;
                    this.browser_focused = false;
                    this.engine.set_focus(this.focused_tab_id(), false);
                    this.find_input
                        .update(cx, |input, cx| input.focus(window, cx));
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if modifiers.alt && matches!(key.as_str(), "left" | "arrowleft") {
                    this.engine.go_back(this.focused_tab_id());
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if modifiers.alt && matches!(key.as_str(), "right" | "arrowright") {
                    this.engine.go_forward(this.focused_tab_id());
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if browser_modifier && modifiers.alt && key == "r" && !modifiers.shift {
                    this.engine
                        .execute_script(this.active_tab_id(), &reader::toggle_script());
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                let suggestions_open = this.show_suggestions && !this.suggestions.is_empty();
                let is_suggestion_arrow = suggestions_open
                    && matches!(key.as_str(), "up" | "down" | "arrowup" | "arrowdown");
                if event.is_held && !is_suggestion_arrow {
                    return;
                }
                if is_suggestion_arrow {
                    this.move_suggestion_selection(matches!(key.as_str(), "down" | "arrowdown"));
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if !event.is_held
                    && suggestions_open
                    && matches!(key.as_str(), "enter" | "return")
                    && let Some(target) = this
                        .selected_suggestion
                        .and_then(|index| this.suggestions.get(index))
                        .map(|item| item.target.clone())
                {
                    this.navigate_to(&target, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if suggestions_open && matches!(key.as_str(), "escape" | "esc") {
                    this.show_suggestions = false;
                    this.selected_suggestion = None;
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if browser_modifier && key == "l" && !modifiers.shift {
                    let tab_id = this.focused_tab_id().to_string();
                    this.engine.set_focus(&tab_id, false);
                    this.address.update(cx, |input, cx| {
                        input.focus(window, cx);
                        input.select_all(window, cx);
                    });
                    this.browser_focused = false;
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if this.handle_shortcut(&key, browser_modifier, modifiers.shift) {
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, window, cx| {
                let modifiers = &event.keystroke.modifiers;
                let browser_modifier = modifiers.control || modifiers.platform;
                let key = event.keystroke.key.to_lowercase();
                if this.browser_focused
                    && !Self::is_app_shortcut(&key, browser_modifier, modifiers.shift)
                    && !(modifiers.alt
                        && matches!(key.as_str(), "left" | "arrowleft" | "right" | "arrowright"))
                {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }));

        if let Some(menu) = &self.space_context_menu {
            if let Some(target_space) = self.spaces.iter().find(|s| s.id == menu.space_id) {
                let target_id = target_space.id.clone();
                let target_id_edit = target_id.clone();
                let target_id_del = target_id.clone();
                let can_delete = self.spaces.len() > 1;

                let mut ctx_menu = div()
                    .id("space-context-menu")
                    .occlude()
                    .absolute()
                    .bottom(px(48.0 * UI_SCALE))
                    .left(px(8.0 * UI_SCALE))
                    .w(px(224.0 * UI_SCALE))
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .rounded_xl()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.muted))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(space_switcher::render_space_icon(&target_space.icon))
                            .child(target_space.name.clone()),
                    )
                    .child(
                        div()
                            .id("ctx-edit-space")
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .hover(move |style| style.bg(rgb(theme.surface_hover)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::Pencil))
                            .child("Rename & Change Icon")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_edit_space_modal(&target_id_edit, window, cx);
                                cx.notify();
                            })),
                    );

                if can_delete {
                    ctx_menu = ctx_menu.child(
                        div()
                            .id("ctx-delete-space")
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(0xff6b6b))
                            .hover(|style| style.bg(rgb(0x381e22)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::Trash))
                            .child("Delete Space")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.delete_space(&target_id_del);
                                cx.notify();
                            })),
                    );
                }

                root = root.child(
                    ctx_menu.with_animation(
                        format!("space-menu-entrance-{}", target_space.id),
                        Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                            .with_easing(ui::motion::ease_out_quint),
                        |menu, progress| {
                            menu.w(px(224.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                                .opacity(ui::motion::clamp_percentage(progress))
                        },
                    ),
                );
            }
        }

        if let Some(menu) = &self.tab_context_menu {
            let target_id = menu.tab_id.clone();
            let menu_width = menu.width;
            let target_tab = self
                .active_space()
                .tabs
                .iter()
                .find(|tab| tab.id == target_id);
            let is_split = self
                .active_space()
                .split
                .as_ref()
                .is_some_and(|pair| pair.first == target_id || pair.second == target_id);
            let menu_layer = div()
                .id("tab-context-dismiss-layer")
                .occlude()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.tab_context_menu = None;
                        cx.notify();
                    }),
                );
            let mut panel = div()
                .id("tab-context-menu")
                .occlude()
                .absolute()
                .top(px(menu.y))
                .left(px(menu.x))
                .w(px(menu.width))
                .max_h(px(menu.height))
                .overflow_y_scroll()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .rounded_xl()
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                );

            if menu.showing_split_targets {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.muted))
                        .child("Split with…"),
                );
                for (id, title, favicon) in &tabs {
                    if id == &target_id {
                        continue;
                    }
                    let other_id = id.clone();
                    panel = panel.child(
                        div()
                            .id(format!("split-target-{id}"))
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .hover(move |style| style.bg(rgb(theme.surface_hover)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(tab_list::favicon(favicon.clone()))
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(title.clone()),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.choose_split_target(&other_id);
                                cx.notify();
                            })),
                    );
                }
            } else {
                panel = panel.child(
                    div()
                        .id("ctx-split-with")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(rgb(theme.surface_hover)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Columns2))
                        .child("Split with…")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(menu) = &mut this.tab_context_menu {
                                menu.showing_split_targets = true;
                            }
                            cx.notify();
                        })),
                );
                if is_split {
                    panel = panel.child(
                        div()
                            .id("ctx-unsplit")
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.text))
                            .hover(move |style| style.bg(rgb(theme.surface_hover)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::Columns2))
                            .child("Unsplit")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.exit_split();
                                this.tab_context_menu = None;
                                cx.notify();
                            })),
                    );
                }
                let reload_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-reload")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(rgb(theme.surface_hover)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::RotateCw))
                        .child("Reload")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.engine.reload(&reload_id, false);
                            this.tab_context_menu = None;
                            cx.notify();
                        })),
                );
                let duplicate_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-duplicate")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(rgb(theme.surface_hover)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::CopyPlus))
                        .child("Duplicate tab")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.duplicate_tab(&duplicate_id);
                            cx.notify();
                        })),
                );
                let copy_url = target_tab.map(|tab| tab.url.clone()).unwrap_or_default();
                panel = panel.child(
                    div()
                        .id("ctx-copy-link")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(rgb(theme.surface_hover)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Copy))
                        .child("Copy link")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                copy_url.clone(),
                            ));
                            this.tab_context_menu = None;
                            cx.notify();
                        })),
                );
                let close_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-close-tab")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(rgb(theme.surface_hover)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::X))
                        .child("Close tab")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_tab_by_id(&close_id);
                            this.tab_context_menu = None;
                            cx.notify();
                        })),
                );
                let other_ids: Vec<String> = self
                    .active_space()
                    .tabs
                    .iter()
                    .filter(|tab| tab.id != target_id)
                    .map(|tab| tab.id.clone())
                    .collect();
                let keep_id = target_id.clone();
                panel = panel.child(
                    div()
                        .id("ctx-close-others")
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .hover(move |style| style.bg(rgb(theme.surface_hover)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::X))
                        .child("Close other tabs")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            for id in &other_ids {
                                this.close_tab_by_id(id);
                            }
                            this.active_space_mut().active = Some(keep_id.clone());
                            this.active_space_mut().split = None;
                            this.focused_pane = 0;
                            this.tab_context_menu = None;
                            this.save_state();
                            cx.notify();
                        })),
                );
            }
            let panel = panel.with_animation(
                format!("tab-menu-entrance-{}", menu.tab_id),
                Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                    .with_easing(ui::motion::ease_out_quint),
                move |panel, progress| {
                    panel
                        .w(px(menu_width * (0.96 + 0.04 * progress)))
                        .opacity(ui::motion::clamp_percentage(progress))
                },
            );
            root = root.child(menu_layer.child(panel));
        }

        if let Some(modal) = &self.space_modal {
            let is_edit = matches!(modal.mode, SpaceModalMode::Edit { .. });
            let title = if is_edit { "Edit Space" } else { "New Space" };
            let action_label = if is_edit { "Save" } else { "Create" };
            let current_selected_icon = modal.selected_icon.clone();
            let current_selected_color = modal.selected_color;
            let modal_theme = visual::space_theme(current_selected_color);

            let popover = div()
                .id("space-popover-card")
                .occlude()
                .absolute()
                .bottom(px(52.0 * UI_SCALE))
                .left(px(8.0 * UI_SCALE))
                .w(px(224.0 * UI_SCALE))
                .bg(rgb(modal_theme.surface))
                .border_1()
                .border_color(rgb(modal_theme.border))
                .rounded_xl()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(modal_theme.text))
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(modal_theme.muted))
                                .child("Name"),
                        )
                        .child(
                            div()
                                .id("space-name-input-container")
                                .w_full()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(modal_theme.border))
                                .bg(rgb(modal_theme.surface))
                                .cursor_text()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(modal) = &this.space_modal {
                                        modal.name_input.update(cx, |input, cx| {
                                            input.focus(window, cx);
                                        });
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    Input::new(&modal.name_input)
                                        .id("space-name-input")
                                        .w_full()
                                        .appearance(false),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(modal_theme.muted))
                                .child("Icon"),
                        )
                        .child({
                            let mut icons_grid = div()
                                .id("svg-picker-grid")
                                .h(px(100.0 * UI_SCALE))
                                .overflow_y_scroll()
                                .p_1()
                                .rounded_md()
                                .bg(rgb(modal_theme.surface))
                                .border_1()
                                .border_color(rgb(modal_theme.border))
                                .flex()
                                .flex_wrap()
                                .gap_1();
                            for (i, &icon) in SPACE_ICONS.iter().enumerate() {
                                let is_selected = icon == current_selected_icon;
                                let icon_str = icon.to_string();
                                icons_grid = icons_grid.child(
                                    div()
                                        .id(format!("space-icon-choice-{i}"))
                                        .size(px(26.0 * UI_SCALE))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(rgb(if is_selected {
                                            modal_theme.surface_hover
                                        } else {
                                            modal_theme.surface
                                        }))
                                        .border_1()
                                        .border_color(rgb(if is_selected {
                                            modal_theme.accent
                                        } else {
                                            modal_theme.border
                                        }))
                                        .text_color(rgb(if is_selected {
                                            0xffffff
                                        } else {
                                            0x8b8f9a
                                        }))
                                        .hover(|s| {
                                            s.bg(rgb(modal_theme.surface_hover))
                                                .text_color(rgb(modal_theme.text))
                                        })
                                        .cursor_pointer()
                                        .child(space_switcher::render_space_icon(icon))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(modal) = &mut this.space_modal {
                                                modal.selected_icon = icon_str.clone();
                                            }
                                            cx.notify();
                                        })),
                                );
                            }
                            icons_grid
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(modal_theme.muted))
                                .child("Color"),
                        )
                        .child({
                            let mut colors_row = div().flex().items_center().justify_between();
                            for (i, &color) in SPACE_COLORS.iter().enumerate() {
                                let is_selected = color == current_selected_color;
                                colors_row = colors_row.child(
                                    div()
                                        .id(format!("space-color-choice-{i}"))
                                        .size(px(18.0 * UI_SCALE))
                                        .rounded_full()
                                        .bg(rgb(color))
                                        .border_2()
                                        .border_color(rgb(if is_selected {
                                            0xffffff
                                        } else {
                                            modal_theme.surface
                                        }))
                                        .cursor_pointer()
                                        .hover(|s| s.border_color(rgb(0xffffff)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(modal) = &mut this.space_modal {
                                                modal.selected_color = color;
                                            }
                                            cx.notify();
                                        })),
                                );
                            }
                            colors_row
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .pt_1()
                        .child(
                            div()
                                .id("popover-cancel-btn")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .text_xs()
                                .text_color(rgb(0xa1a4af))
                                .hover(move |s| {
                                    s.bg(rgb(modal_theme.surface_hover))
                                        .text_color(rgb(modal_theme.text))
                                })
                                .cursor_pointer()
                                .child("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.space_modal = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("popover-commit-btn")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(rgb(modal_theme.accent))
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(modal_theme.base))
                                .hover(|s| s.bg(rgb(0xb4bcff)))
                                .cursor_pointer()
                                .child(action_label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.commit_space_modal(cx);
                                    cx.notify();
                                })),
                        ),
                );

            root = root.child(
                popover.with_animation(
                    "space-popover-entrance",
                    Animation::new(ui::motion::duration(ui::motion::MOTION_POPOVER))
                        .with_easing(ui::motion::ease_out_quint),
                    |popover, progress| {
                        popover
                            .w(px(224.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                            .opacity(ui::motion::clamp_percentage(progress))
                    },
                ),
            );
        }

        let root = if let Some((from_theme, _, transition_id)) = self.theme_transition {
            let to_theme = target_theme;
            root.with_animation(
                format!("space-theme-transition-{transition_id}"),
                Animation::new(ui::motion::duration(ui::motion::MOTION_SPACE_SWITCH))
                    .with_easing(ui::motion::ease_out_quint),
                move |root, progress| {
                    let theme = visual::interpolate_theme(from_theme, to_theme, progress);
                    root.bg(rgb(theme.base)).text_color(rgb(theme.text))
                },
            )
            .into_any_element()
        } else {
            root.into_any_element()
        };

        root
    }
}

fn main() {
    if handle_import_cli() {
        return;
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some() {
        // GPUI prefers Wayland when both display variables are set. CEF's
        // windowed Linux child browser needs an X11/XWayland parent instead.
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::remove_var("WAYLAND_SOCKET");
        }
    }
    if let Some(code) = engine::execute_subprocess() {
        std::process::exit(code);
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            gpui_kit::component::Theme::update(cx, |theme| {
                theme.caret = gpui_kit::rgb(0xffffff).into();
            });
            let bounds = Bounds::centered(None, size(px(1200.0), px(760.0)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Lumen::new(window, cx)),
            )
            .expect("open Lumen window");
        });
}

fn handle_import_cli() -> bool {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(index) = args.iter().position(|arg| arg == "--import") else {
        return false;
    };
    let Some(browser) = args.get(index + 1) else {
        eprintln!("usage: lumen --import chrome|chromium|brave|edge|all");
        std::process::exit(2);
    };
    match import::import(browser) {
        Ok(summary) => {
            println!(
                "Import complete: {} profiles, {} bookmarks, {} visits",
                summary.profiles, summary.bookmarks, summary.visits
            );
            println!(
                "Bookmarks: {}",
                paths::data_dir().join("bookmarks.json").display()
            );
            println!(
                "History: {}",
                paths::data_dir().join("history.sqlite").display()
            );
            for warning in summary.warnings {
                eprintln!("Import warning: {warning}");
            }
        }
        Err(error) => {
            eprintln!("Import failed: {error}");
            std::process::exit(2);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::Focusable;

    fn test_app(window: &mut Window, cx: &mut Context<Lumen>) -> Lumen {
        let address = cx.new(|cx| InputState::new(window, cx));
        address.update(cx, |input, cx| input.focus(window, cx));
        let address_subscription =
            cx.subscribe(&address, |app: &mut Lumen, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    app.address_editing = true;
                    let text = app.address.read(cx).value().to_string();
                    app.update_suggestions(&text);
                    app.selected_suggestion = None;
                    cx.notify();
                }
            });
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in page"));
        let command_input = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search tabs, history, and actions"));
        let command_subscription =
            cx.subscribe(&command_input, |this: &mut Lumen, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.selected_command = 0;
                    cx.notify();
                }
            });
        let (spaces, active_space) = create_default_space();
        Lumen {
            spaces,
            active_space,
            sidebar_collapsed: false,
            sidebar_width: 240.0 * UI_SCALE,
            sidebar_resizing: false,
            sidebar_width_transition: None,
            sidebar_transition_serial: 0,
            theme_transition: None,
            theme_transition_id: 0,
            space_switch_transition: None,
            space_switch_transition_id: 0,
            #[cfg(all(target_os = "linux", not(test)))]
            resize_drag: split_view::ResizeDrag::default(),
            started_at: Instant::now(),
            first_frame_logged: true,
            address,
            address_active_tab: None,
            address_editing: false,
            reset_address_for_tab_change: false,
            find_input,
            command_input,
            browser_focus_handle: cx.focus_handle(),
            web_viewport: {
                let root_entity = cx.weak_entity();
                let focus_handle = cx.focus_handle();
                cx.new(|_| WebViewport::new(root_entity, focus_handle))
            },
            second_web_viewport: {
                let root_entity = cx.weak_entity();
                let focus_handle = cx.focus_handle();
                cx.new(|_| WebViewport::new(root_entity, focus_handle))
            },
            focused_pane: 0,
            split_ratio: 0.5,
            split_resizing: false,
            _address_subscription: address_subscription,
            _command_subscription: command_subscription,
            show_find: false,
            show_command_palette: false,
            selected_command: 0,
            suggestions: Vec::new(),
            suggestion_snapshot: Vec::new(),
            suggestions_were_visible: false,
            suggestion_closing: None,
            suggestion_animation_serial: 0,
            selected_suggestion: None,
            recent_searches: Vec::new(),
            zoom_levels: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", not(test)))]
            active_zoom_target: None,
            show_suggestions: false,
            engine: engine::Engine::new(),
            space_modal: None,
            space_context_menu: None,
            tab_context_menu: None,
            split_group_expanded: false,
            recently_closed_tabs: Vec::new(),
            closing_tab: None,
            closing_tab_serial: 0,
            browser_focused: false,
            #[cfg(not(test))]
            cef_task_started: false,
        }
    }

    fn dispatch_test_key(
        cx: &mut gpui::TestAppContext,
        handle: gpui_kit::AnyWindowHandle,
        key: &'static str,
    ) {
        cx.update_window(handle, |_, window, cx| {
            window.draw(cx).clear(cx);
            window.dispatch_keystroke(gpui_kit::Keystroke::parse(key).unwrap(), cx);
            window.draw(cx).clear(cx);
        })
        .unwrap();
    }

    #[gpui::test]
    fn browser_tab_shortcuts_work_from_focused_address_bar(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        dispatch_test_key(cx, handle, "ctrl-t");
        view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-2"));
        dispatch_test_key(cx, handle, "ctrl-w");
        view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-1"));
        dispatch_test_key(cx, handle, "ctrl-shift-t");
        view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-2"));
    }

    #[gpui::test]
    fn split_shortcuts_create_focus_and_exit_the_pane_pair(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        dispatch_test_key(cx, handle, "ctrl-shift-s");
        view.read_with(cx, |app, _| {
            assert_eq!(
                app.current_split_pair(),
                Some(SplitPair {
                    first: "tab-1".into(),
                    second: "tab-2".into(),
                })
            );
        });
        dispatch_test_key(cx, handle, "ctrl-shift-w");
        view.read_with(cx, |app, _| assert!(app.current_split_pair().is_none()));
        dispatch_test_key(cx, handle, "ctrl-shift-s");
        dispatch_test_key(cx, handle, "ctrl-alt-right");
        view.read_with(cx, |app, _| {
            assert_eq!(app.focused_pane, 1);
            assert_eq!(app.focused_tab_id(), "tab-2");
        });
        view.update(cx, |app, _| app.close_active_tab());
        view.read_with(cx, |app, _| {
            assert!(app.current_split_pair().is_none());
            assert_eq!(app.active_space().tabs.len(), 1);
            assert_eq!(app.active_tab_id(), "tab-1");
        });
        view.read_with(cx, |app, _| {
            assert_eq!(app.focused_pane, 0);
            assert!(app.current_split_pair().is_none());
        });
    }

    #[gpui::test]
    fn closing_a_non_active_tab_by_id_preserves_the_active_tab(cx: &mut gpui::TestAppContext) {
        let (_, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        view.update(cx, |app, _| {
            app.add_tab();
            app.active_space_mut().active = Some("tab-1".into());
            assert!(app.close_tab_by_id("tab-2"));
        });
        view.read_with(cx, |app, _| {
            assert_eq!(app.active_tab_id(), "tab-1");
            assert_eq!(app.active_space().tabs.len(), 1);
        });
    }

    #[gpui::test]
    fn closing_a_split_member_exits_split_and_selects_its_partner(cx: &mut gpui::TestAppContext) {
        let (_, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        view.update(cx, |app, _| {
            app.add_tab();
            assert!(app.split_tabs("tab-1", "tab-2"));
            assert!(app.close_tab_by_id("tab-1"));
        });
        view.read_with(cx, |app, _| {
            assert!(app.current_split_pair().is_none());
            assert_eq!(app.active_tab_id(), "tab-2");
        });
    }

    #[gpui::test]
    fn split_with_context_flow_uses_the_clicked_tab_and_selected_target(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        view.update(cx, |app, _| {
            app.add_tab();
            app.tab_context_menu = Some(TabContextMenu {
                tab_id: "tab-1".into(),
                showing_split_targets: true,
                x: 8.0 * UI_SCALE,
                y: 8.0 * UI_SCALE,
                width: 240.0 * UI_SCALE,
                height: 520.0 * UI_SCALE,
            });
            assert!(app.choose_split_target("tab-2"));
            assert!(app.tab_context_menu.is_none());
        });
        view.read_with(cx, |app, _| {
            assert_eq!(
                app.current_split_pair(),
                Some(SplitPair {
                    first: "tab-1".into(),
                    second: "tab-2".into(),
                })
            );
        });
    }

    #[gpui::test]
    fn find_shortcut_opens_and_escape_closes_find(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        dispatch_test_key(cx, handle, "ctrl-f");
        view.read_with(cx, |app, _| assert!(app.show_find));
        dispatch_test_key(cx, handle, "escape");
        view.read_with(cx, |app, _| assert!(!app.show_find));
    }

    #[gpui::test]
    fn command_palette_opens_with_ctrl_k_and_closes_with_escape(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        dispatch_test_key(cx, handle, "ctrl-k");
        view.read_with(cx, |app, _| assert!(app.show_command_palette));
        dispatch_test_key(cx, handle, "down");
        view.read_with(cx, |app, _| assert_eq!(app.selected_command, 1));
        dispatch_test_key(cx, handle, "up");
        view.read_with(cx, |app, _| assert_eq!(app.selected_command, 0));
        dispatch_test_key(cx, handle, "escape");
        view.read_with(cx, |app, _| assert!(!app.show_command_palette));
    }

    #[gpui::test]
    fn command_palette_enter_runs_the_matching_action(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        dispatch_test_key(cx, handle, "ctrl-k");
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.command_input
                    .update(cx, |input, cx| input.set_value("New tab", window, cx));
            });
        })
        .unwrap();
        dispatch_test_key(cx, handle, "enter");
        view.read_with(cx, |app, _| {
            assert_eq!(app.active_tab_id(), "tab-2");
            assert!(!app.show_command_palette);
        });
    }

    #[gpui::test]
    fn browser_tab_cycle_and_number_shortcuts_select_the_expected_tab(
        cx: &mut gpui::TestAppContext,
    ) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = test_app(window, cx);
                    app.add_tab();
                    app.add_tab();
                    app
                })
            })
            .expect("open test window")
        });
        dispatch_test_key(cx, handle, "ctrl-1");
        view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-1"));
        dispatch_test_key(cx, handle, "ctrl-tab");
        view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-2"));
        dispatch_test_key(cx, handle, "ctrl-shift-tab");
        view.read_with(cx, |app, _| assert_eq!(app.active_tab_id(), "tab-1"));
    }

    #[gpui::test]
    fn new_tab_clears_and_focuses_the_address_bar(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        for key in ["g", "i", "t"] {
            dispatch_test_key(cx, handle, key);
        }
        dispatch_test_key(cx, handle, "ctrl-t");
        view.read_with(cx, |app, cx| {
            assert_eq!(app.active_tab_id(), "tab-2");
            assert_eq!(app.address.read(cx).value().to_string(), "");
            assert!(!app.show_suggestions);
        });
        cx.update_window(handle, |_, window, cx| {
            view.read_with(cx, |app, cx| {
                assert_eq!(
                    window.focused(cx),
                    Some(app.address.read(cx).focus_handle(cx))
                );
            });
        })
        .unwrap();
    }

    #[gpui::test]
    fn tab_switch_syncs_address_to_the_active_tab(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = test_app(window, cx);
                    app.add_tab();
                    app.add_tab();
                    app.active_space_mut().tabs[0].url = "https://one.example".into();
                    app.active_space_mut().tabs[1].url = "https://two.example".into();
                    app.active_space_mut().active = Some("tab-1".into());
                    app
                })
            })
            .expect("open test window")
        });
        dispatch_test_key(cx, handle, "ctrl-2");
        view.read_with(cx, |app, cx| {
            assert_eq!(app.active_tab_id(), "tab-2");
            assert_eq!(
                app.address.read(cx).value().to_string(),
                "https://two.example"
            );
        });
        dispatch_test_key(cx, handle, "ctrl-1");
        view.read_with(cx, |app, cx| {
            assert_eq!(
                app.address.read(cx).value().to_string(),
                "https://one.example"
            );
        });
        view.update(cx, |app, cx| {
            let mut other = Space::new("other", "Other", "code", 0x123456);
            other
                .tabs
                .push(Tab::new("tab-1", "Other page", "https://other.example"));
            other.active = Some("tab-1".into());
            app.spaces.push(other);
            app.activate_space("other");
            cx.notify();
        });
        view.read_with(cx, |app, cx| {
            assert_eq!(
                app.address.read(cx).value().to_string(),
                "https://other.example"
            );
        });
    }

    #[gpui::test]
    fn new_tab_temporarily_leaves_a_split_and_address_targets_it(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = test_app(window, cx);
                    app.add_tab();
                    app.active_space_mut().tabs[0].url = "https://a.example".into();
                    app.active_space_mut().tabs[1].url = "https://b.example".into();
                    assert!(app.split_tabs("tab-1", "tab-2"));
                    app
                })
            })
            .expect("open test window")
        });

        dispatch_test_key(cx, handle, "ctrl-t");
        view.read_with(cx, |app, _| {
            assert_eq!(app.active_tab_id(), "tab-3");
            assert!(app.current_split_pair().is_none());
            assert_eq!(
                app.stored_split_pair()
                    .map(|pair| (pair.first, pair.second)),
                Some(("tab-1".into(), "tab-2".into()))
            );
        });
        dispatch_test_key(cx, handle, "x");
        dispatch_test_key(cx, handle, "enter");
        view.read_with(cx, |app, _| {
            assert_eq!(app.active_tab_id(), "tab-3");
            assert_eq!(app.active_space().tabs[2].url, "https://x.com");
            assert!(app.current_split_pair().is_none());
            assert!(app.stored_split_pair().is_some());
        });

        view.update(cx, |app, cx| {
            app.activate_split_group();
            cx.notify();
        });
        view.read_with(cx, |app, _| {
            assert_eq!(app.active_tab_id(), "tab-1");
            assert!(app.current_split_pair().is_some());
            assert!(app.stored_split_pair().is_some());
        });
        view.update(cx, |app, cx| {
            app.active_space_mut().active = Some("tab-3".into());
            app.set_focused_pane(1);
            cx.notify();
        });
        view.read_with(cx, |app, _| {
            assert_eq!(app.active_tab_id(), "tab-2");
            assert!(app.current_split_pair().is_some());
        });
    }

    #[gpui::test]
    fn autocomplete_arrows_select_a_suggestion_and_enter_opens_it(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        view.update(cx, |app, cx| {
            app.update_suggestions("example.com");
            cx.notify();
        });

        dispatch_test_key(cx, handle, "down");
        view.read_with(cx, |app, _| assert_eq!(app.selected_suggestion, Some(0)));
        dispatch_test_key(cx, handle, "down");
        view.read_with(cx, |app, _| assert_eq!(app.selected_suggestion, Some(1)));
        dispatch_test_key(cx, handle, "up");
        view.read_with(cx, |app, _| assert_eq!(app.selected_suggestion, Some(0)));
        dispatch_test_key(cx, handle, "down");
        dispatch_test_key(cx, handle, "enter");
        view.read_with(cx, |app, _| {
            assert_eq!(app.active_space().tabs[0].url, "https://example.com");
            assert!(!app.show_suggestions);
        });
    }

    #[gpui::test]
    fn typing_in_the_address_bar_opens_autocomplete(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });

        for key in ["y", "o", "u"] {
            dispatch_test_key(cx, handle, key);
        }

        view.read_with(cx, |app, cx| {
            assert_eq!(app.address.read(cx).value().to_string(), "you");
            assert!(app.show_suggestions);
            assert!(app.suggestions.iter().any(|suggestion| {
                suggestion.title.contains("YouTube") && suggestion.target == "https://youtube.com"
            }));
        });
    }

    #[gpui::test]
    fn focusing_web_content_moves_focus_off_the_address_bar(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        view.update(cx, |app, cx| {
            app.update_suggestions("youtube");
            cx.notify();
        });

        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |app, cx| app.focus_browser(window, cx));
            view.read_with(cx, |app, cx| {
                assert!(app.browser_focused);
                assert!(!app.show_suggestions);
                assert_eq!(window.focused(cx), Some(app.browser_focus_handle.clone()));
                assert_ne!(
                    window.focused(cx),
                    Some(app.address.read(cx).focus_handle(cx))
                );
            });
        })
        .unwrap();
    }

    #[gpui::test]
    fn recent_searches_appear_when_the_address_bar_is_empty(cx: &mut gpui::TestAppContext) {
        let (handle, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| test_app(window, cx))
            })
            .expect("open test window")
        });
        view.update(cx, |app, cx| {
            app.recent_searches = vec!["lumen browser shortcuts".to_string()];
            app.update_suggestions("");
            cx.notify();
        });
        view.read_with(cx, |app, _| {
            assert_eq!(app.suggestions.len(), 1);
            assert_eq!(
                app.suggestions[0].title,
                "Recent search: lumen browser shortcuts"
            );
        });
        dispatch_test_key(cx, handle, "down");
        dispatch_test_key(cx, handle, "enter");
        view.read_with(cx, |app, _| {
            assert_eq!(
                app.active_space().tabs[0].url,
                "https://duckduckgo.com/?q=lumen+browser+shortcuts"
            );
        });
    }

    #[test]
    fn test_all_assets_has_all_space_icons() {
        for &name in SPACE_ICONS {
            let icon_name = match name {
                "globe" => IconName::Globe,
                "compass" => IconName::Compass,
                "code" => IconName::Code,
                "terminal" => IconName::Terminal,
                "folder" => IconName::Folder,
                "book" => IconName::BookOpen,
                "briefcase" => IconName::Briefcase,
                "layers" => IconName::Layers,
                "box" => IconName::Box,
                "sparkles" => IconName::Sparkles,
                "star" => IconName::Star,
                "heart" => IconName::Heart,
                "bookmark" => IconName::Bookmark,
                "music" => IconName::Music,
                "gamepad" => IconName::Gamepad2,
                "cpu" => IconName::Cpu,
                "zap" => IconName::Zap,
                "hash" => IconName::Hash,
                "shield" => IconName::Shield,
                "palette" => IconName::Palette,
                "rocket" => IconName::Rocket,
                "bell" => IconName::Bell,
                _ => IconName::Compass,
            };
            let path = icon_name.path();
            assert!(
                gpui_kit::assets::AllAssets::get(&path).is_some(),
                "Icon {name} with path {path} not found in AllAssets"
            );
        }
    }

    #[test]
    fn address_bar_routes_urls_and_searches_to_the_active_tab() {
        let mut space = Space::new("test", "Test", "compass", 0x123456);
        let tab = Tab::new("existing", "Welcome", "lumen://welcome");
        space.active = Some(tab.id.clone());
        space.tabs.push(tab);

        // Smart domain navigation for top keywords
        let (id, url, _) = submit_address(&mut space, "youtube").unwrap();
        assert_eq!(id, "existing");
        assert_eq!(url, "https://youtube.com");
        assert_eq!(space.tabs[0].url, url);
        assert_eq!(space.tabs[0].title, "youtube.com");
        assert_eq!(space.tabs[0].state, TabState::Live);

        // General search query routes to Google search with clean title
        let (id, url, _) = submit_address(&mut space, "rust compiler docs").unwrap();
        assert_eq!(id, "existing");
        assert_eq!(url, "https://duckduckgo.com/?q=rust+compiler+docs");
        assert_eq!(space.tabs[0].title, "Search: rust compiler docs");
    }

    #[test]
    fn empty_address_opens_search_home_and_creates_a_tab_if_needed() {
        let mut space = Space::new("test", "Test", "compass", 0x123456);
        let (id, url, _) = submit_address(&mut space, "").unwrap();
        assert_eq!(id, "tab-1");
        assert_eq!(url, "https://duckduckgo.com");
        assert_eq!(space.active.as_deref(), Some("tab-1"));
        assert_eq!(space.tabs[0].url, url);
    }

    #[test]
    fn space_has_icon_and_isolated_identity() {
        let space1 = Space::new("s1", "Personal", "compass", 0x9aa6ff);
        let space2 = Space::new("s2", "Work", "briefcase", 0x76d7b2);
        let space3 = Space::new("s3", "Fun", "gamepad", 0xf6ad55);

        assert_eq!(space1.icon, "compass");
        assert_eq!(space2.icon, "briefcase");
        assert_eq!(space3.icon, "gamepad");
    }

    #[test]
    fn default_space_has_no_hardcoded_secondary_spaces() {
        let (spaces, active) = create_default_space();
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].id, "personal");
        assert_eq!(spaces[0].icon, "compass");
        assert_eq!(spaces[0].tabs[0].url, "about:blank");
        assert_eq!(spaces[0].tabs[0].title, "New tab");
        assert_eq!(active, "personal");
        assert!(!spaces.iter().any(|s| s.id == "work"));
    }

    #[test]
    fn svg_icon_palette_has_plenty_of_distinct_choices() {
        assert!(SPACE_ICONS.len() >= 20);
        let mut set = std::collections::HashSet::new();
        for &icon in SPACE_ICONS {
            assert!(set.insert(icon), "Duplicate SVG icon found: {}", icon);
        }
    }

    #[test]
    fn address_normalization_handles_urls_domains_and_search_queries() {
        assert_eq!(normalize_address(""), "");
        assert_eq!(
            normalize_address("https://news.ycombinator.com"),
            "https://news.ycombinator.com"
        );
        assert_eq!(normalize_address("about:blank"), "about:blank");
        assert_eq!(normalize_address("github.com"), "https://github.com");
        assert_eq!(
            normalize_address("http://example.com/x"),
            "https://example.com/x"
        );
        assert_eq!(
            normalize_address("http://localhost:8080/x"),
            "http://localhost:8080/x"
        );
        assert_eq!(
            normalize_address("localhost:8080"),
            "https://localhost:8080"
        );
        assert_eq!(
            normalize_address("what is the airspeed velocity of a swallow"),
            "https://duckduckgo.com/?q=what+is+the+airspeed+velocity+of+a+swallow"
        );
        assert_eq!(
            normalize_address("cats & dogs"),
            "https://duckduckgo.com/?q=cats+%26+dogs"
        );
    }

    #[test]
    fn space_creation_and_deletion_invariants() {
        let (mut spaces, mut active) = create_default_space();
        assert_eq!(spaces.len(), 1);
        assert_eq!(active, "personal");

        // Add second space
        let mut dev_space = Space::new("dev", "Dev Workspace", "code", 0x63b3ed);
        dev_space
            .tabs
            .push(Tab::new("tab-dev-1", "GitHub", "https://github.com"));
        dev_space.active = Some("tab-dev-1".into());
        spaces.push(dev_space);
        active = "dev".into();
        assert_eq!(spaces.len(), 2);
        assert_eq!(active, "dev");

        // Delete active space: active should fallback to remaining space
        let delete_idx = spaces.iter().position(|s| s.id == "dev").unwrap();
        spaces.remove(delete_idx);
        active = spaces[0].id.clone();
        assert_eq!(spaces.len(), 1);
        assert_eq!(active, "personal");

        // Single space cannot be deleted
        assert_eq!(spaces.len(), 1);
    }

    #[test]
    fn sidebar_width_has_minimum_and_maximum_clamp() {
        let requested_shrink = 50.0_f32;
        let clamped = requested_shrink.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
        assert_eq!(clamped, SIDEBAR_MIN_WIDTH);

        let requested_grow = 500.0_f32;
        let clamped_grow = requested_grow.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
        assert_eq!(clamped_grow, SIDEBAR_MAX_WIDTH);

        let mid = 240.0_f32;
        assert_eq!(mid.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH), 240.0);
    }

    #[test]
    fn sidebar_width_depends_only_on_manual_collapse_state() {
        assert_eq!(sidebar_width_for(false, 240.0), 240.0);
        assert_eq!(sidebar_width_for(true, 240.0), SIDEBAR_COLLAPSED_WIDTH);
    }

    #[test]
    fn zoom_is_keyed_by_site_host() {
        assert_eq!(url_host("https://Example.com/path?q=1"), "example.com");
        assert_eq!(url_host("http://example.com:8080/path"), "example.com");
        assert_eq!(
            url_host("https://user:pass@example.com/path"),
            "example.com"
        );
        assert_eq!(url_host("about:blank"), "");
        assert_eq!(url_host("lumen://welcome"), "");
    }

    #[test]
    fn autocomplete_suggestions_produces_smart_search_and_completions() {
        // Empty query gives empty list
        assert!(compute_suggestions("").is_empty());
        assert!(compute_suggestions("   ").is_empty());

        // Typing "you" provides search, domain completion, and YouTube quick site
        let you_sugs = compute_suggestions("you");
        assert!(!you_sugs.is_empty());
        assert_eq!(you_sugs[0].title, "Search DuckDuckGo for \"you\"");
        assert!(
            you_sugs
                .iter()
                .any(|s| s.title.contains("https://www.you.com"))
        );
        assert!(
            you_sugs
                .iter()
                .any(|s| s.title.contains("YouTube") && s.target == "https://youtube.com")
        );

        // Typing "git" provides search and GitHub
        let git_sugs = compute_suggestions("git");
        assert!(
            git_sugs
                .iter()
                .any(|s| s.title.contains("GitHub") && s.target == "https://github.com")
        );

        // Direct domain query produces clean domain navigation
        let domain_sugs = compute_suggestions("example.com");
        assert!(
            domain_sugs
                .iter()
                .any(|s| s.target == "https://example.com")
        );
    }
}
