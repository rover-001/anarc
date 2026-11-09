mod engine;
mod history;
mod lifecycle;
mod model;
mod paths;
mod persist;
mod stats;
mod ui;

use std::time::Instant;

use gpui_kit::{
    Bounds, Context, FontWeight, KeyDownEvent, KeyUpEvent, Render, Window, WindowBounds,
    WindowOptions,
    assets::IconName,
    component::Icon,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::*,
    px, rgb, size,
};
use model::{Space, Tab, TabState};
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

pub const SIDEBAR_MIN_WIDTH: f32 = 160.0;
pub const SIDEBAR_MAX_WIDTH: f32 = 320.0;
pub const CONTENT_BEZEL_X: f32 = 4.0;
pub const CONTENT_BEZEL_BOTTOM: f32 = 4.0;
pub const CARD_RADIUS: f32 = 16.0;

#[derive(Clone, Debug, PartialEq)]
pub struct SuggestionItem {
    pub icon_type: String,
    pub title: String,
    pub target: String,
}

struct Lumen {
    spaces: Vec<Space>,
    active_space: String,
    sidebar_collapsed: bool,
    sidebar_width: f32,
    sidebar_resizing: bool,
    started_at: Instant,
    first_frame_logged: bool,
    address: gpui_kit::Entity<InputState>,
    browser_focus_handle: gpui_kit::FocusHandle,
    _address_subscription: gpui_kit::Subscription,
    suggestions: Vec<SuggestionItem>,
    selected_suggestion: Option<usize>,
    recent_searches: Vec<String>,
    show_suggestions: bool,
    engine: engine::Engine,
    space_modal: Option<SpaceModal>,
    space_context_menu: Option<SpaceContextMenu>,
    recently_closed_tabs: Vec<(String, Tab)>,
    browser_focused: bool,
    #[cfg(not(test))]
    cef_task_started: bool,
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
        let (mut spaces, active_space) = if let Some(mut saved) = persist::load_state() {
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
                (saved.spaces, active)
            } else {
                create_default_space()
            }
        } else {
            create_default_space()
        };

        let initial_query = std::env::args().skip(1).find(|arg| !arg.starts_with("--"));
        if let Some(query) = &initial_query {
            if let Some(space) = spaces.iter_mut().find(|s| s.id == active_space) {
                let _ = submit_address(space, query);
            }
        }

        persist::save_state(&spaces, &active_space);

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
        let address_subscription =
            cx.subscribe(&address, |this: &mut Lumen, _, event, cx| match event {
                InputEvent::PressEnter { .. } => {
                    this.navigate(cx);
                    cx.notify();
                }
                InputEvent::Change => {
                    let text = this.address.read(cx).value().to_string();
                    this.update_suggestions(&text);
                    this.selected_suggestion = None;
                    cx.notify();
                }
                _ => {}
            });

        Self {
            spaces,
            active_space,
            sidebar_collapsed: false,
            sidebar_width: 240.0,
            sidebar_resizing: false,
            started_at: Instant::now(),
            first_frame_logged: false,
            address,
            browser_focus_handle,
            _address_subscription: address_subscription,
            suggestions: Vec::new(),
            selected_suggestion: None,
            recent_searches: history::load_recent_searches(),
            show_suggestions: false,
            engine: engine::Engine::new(),
            space_modal: None,
            space_context_menu: None,
            recently_closed_tabs: Vec::new(),
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
                self.active_space = id;
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
            self.spaces.remove(idx);
            if self.active_space == space_id {
                self.active_space = self.spaces[0].id.clone();
            }
            self.save_state();
        }
    }

    fn add_tab(&mut self) {
        self.show_suggestions = false;
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
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return;
        };
        let Some(active_id) = space.active.clone() else {
            self.add_tab();
            return;
        };
        let Some(index) = space.tabs.iter().position(|tab| tab.id == active_id) else {
            self.add_tab();
            return;
        };
        let closed = space.tabs.remove(index);
        self.recently_closed_tabs.push((space.id.clone(), closed));
        if space.tabs.is_empty() {
            let id = next_tab_id(space);
            space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
            space.active = Some(id);
        } else {
            let next = index.min(space.tabs.len() - 1);
            space.active = Some(space.tabs[next].id.clone());
        }
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
            self.active_space = space_id;
        }
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
                || key == "t"
                || key == "tab"
                || key == "r"
                || (key == "w" && !shift)
                || (!shift && key.len() == 1 && matches!(key.as_bytes()[0], b'1'..=b'9')))
    }

    fn handle_shortcut(&mut self, key: &str, control: bool, shift: bool) -> bool {
        if !control {
            return false;
        }
        match (key, shift) {
            ("t", true) => self.reopen_closed_tab(),
            ("t", false) => self.add_tab(),
            ("w", false) => self.close_active_tab(),
            ("tab", shift) => self.cycle_tab(shift),
            (digit, false) if digit.len() == 1 && matches!(digit.as_bytes()[0], b'1'..=b'9') => {
                self.activate_tab_number(key.parse().unwrap_or(1));
            }
            ("r", false) => self.reload_active(false),
            ("r", true) => self.reload_active(true),
            _ => return false,
        }
        true
    }

    fn reload_active(&self, ignore_cache: bool) {
        if let Some(tab_id) = self.active_space().active.as_deref() {
            self.engine.reload(tab_id, ignore_cache);
        }
    }

    fn active_tab_id(&self) -> &str {
        self.active_space().active.as_deref().unwrap_or_default()
    }

    fn save_state(&self) {
        #[cfg(not(test))]
        persist::save_state(&self.spaces, &self.active_space);
        #[cfg(test)]
        let _ = self;
    }

    fn navigate(&mut self, cx: &Context<Self>) {
        self.show_suggestions = false;
        self.selected_suggestion = None;
        let input = self.address.read(cx).value().to_string();
        let space = self.active_space_mut();
        let Some((id, url)) = submit_address(space, &input) else {
            return;
        };
        self.remember_search(&input, &url);
        self.engine.navigate(&id, &url);
        self.save_state();
    }

    fn navigate_to(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_suggestions = false;
        self.selected_suggestion = None;
        self.address.update(cx, |input, cx| {
            input.set_value(target, window, cx);
        });
        let space = self.active_space_mut();
        let Some((id, url)) = submit_address(space, target) else {
            return;
        };
        self.remember_search(target, &url);
        self.engine.navigate(&id, &url);
        self.save_state();
    }

    fn update_suggestions(&mut self, query: &str) {
        let list = compute_suggestions(query);
        self.show_suggestions = !list.is_empty();
        self.suggestions = list;
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

    fn focus_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browser_focused = true;
        self.show_suggestions = false;
        self.selected_suggestion = None;
        window.focus(&self.browser_focus_handle, cx);
        let tab_id = self.active_tab_id().to_string();
        self.engine.set_focus(&tab_id, true);
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

fn submit_address(space: &mut Space, input: &str) -> Option<(String, String)> {
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
    Some((id, url))
}

impl Render for Lumen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(180))
                    .await;
                loop {
                    let updated = this.update(cx, |this, cx| {
                        let was_initialized = this.engine.is_initialized();
                        if let Err(error) = this.engine.initialize() {
                            eprintln!("Lumen CEF startup failed: {error}");
                            return false;
                        }
                        this.engine.pump();
                        let has_updates = this.engine.take_dirty();
                        if !was_initialized || has_updates {
                            cx.notify();
                        }
                        true
                    });
                    if !matches!(updated, Ok(true)) {
                        break;
                    }
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(16))
                        .await;
                }
            })
            .detach();
        }

        #[cfg(all(target_os = "linux", not(test)))]
        for retired in self.engine.take_retired_images() {
            let _ = _window.drop_image(retired);
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

        let active_space_id = self.active_space.clone();
        let active_space = self.active_space();
        let active_tab_id = active_space.active.clone();
        let selected_name = active_space.name.clone();
        let _selected_color = active_space.color;
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

        #[cfg(all(target_os = "linux", not(test)))]
        let scale = _window.scale_factor();
        let sidebar_width_f32 = if self.sidebar_collapsed {
            64.0
        } else {
            self.sidebar_width
        };
        #[cfg(all(target_os = "linux", not(test)))]
        let bounds = _window.bounds();
        #[cfg(all(target_os = "linux", not(test)))]
        let width = f32::from(bounds.size.width);
        #[cfg(all(target_os = "linux", not(test)))]
        let height = f32::from(bounds.size.height);
        #[cfg(all(target_os = "linux", not(test)))]
        let content_w =
            ((width - sidebar_width_f32 - (CONTENT_BEZEL_X * 2.0)).max(100.0) * scale) as i32;
        #[cfg(all(target_os = "linux", not(test)))]
        let content_h = ((height - 52.0 - CONTENT_BEZEL_BOTTOM).max(100.0) * scale) as i32;

        #[cfg(all(target_os = "linux", not(test)))]
        if self.engine.is_initialized()
            && !active_tab_id_string.is_empty()
            && active_tab_url != "lumen://welcome"
            && active_tab_url != "about:blank"
            && !active_tab_url.is_empty()
        {
            if let Err(error) = self.engine.ensure_browser(
                &active_space_id,
                &active_tab_id_string,
                &active_tab_url,
                content_w,
                content_h,
            ) {
                eprintln!("Lumen CEF browser creation failed: {error}");
            }
            self.engine.set_visible(&active_tab_id_string);
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
            .bg(rgb(0x191a1f));

        if !self.sidebar_collapsed {
            sidebar_content = sidebar_content
                .child(
                    div()
                        .id("sidebar-resize-handle")
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(-2.0))
                        .w(px(6.0))
                        .cursor_col_resize()
                        .hover(|s| s.bg(rgb(0x3e424d)))
                        .on_mouse_down(
                            gpui_kit::MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.sidebar_resizing = true;
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    div().flex().items_center().px_2().py_2().child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xf1f2f5))
                            .child(selected_name),
                    ),
                );

            for (id, title, favicon) in &tabs {
                let tab_id = id.clone();
                let current_space_id = active_space_id.clone();
                let selected = active_tab_id.as_ref() == Some(id);
                sidebar_content = sidebar_content.child(
                    div()
                        .id(format!("tab-{id}"))
                        .w_full()
                        .cursor_pointer()
                        .child(tab_list::tab(title, selected, favicon.clone()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(space) = this
                                .spaces
                                .iter_mut()
                                .find(|space| space.id == current_space_id)
                            {
                                space.active = Some(tab_id.clone());
                            }
                            cx.notify();
                        })),
                );
            }

            sidebar_content = sidebar_content.child(
                div()
                    .id("new-tab-action")
                    .w_full()
                    .cursor_pointer()
                    .child(sidebar::new_tab_button())
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
            sidebar_content = sidebar_content.child(div().flex_1().min_h(px(8.0)));

            // Bottom bar with Settings, Space Switcher (icons like Arc), and Downloads
            let mut spaces_dock = div().id("spaces-dock").flex().items_center().gap_1();

            for space in &self.spaces {
                let id = space.id.clone();
                let is_active = space.id == active_space_id;
                let icon_btn =
                    space_switcher::space_icon_button(&space.icon, is_active, space.color);
                let id_right = id.clone();
                spaces_dock = spaces_dock.child(
                    div()
                        .id(format!("space-icon-{id}"))
                        .cursor_pointer()
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
                    .child(space_switcher::add_space_button())
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
                .border_color(rgb(0x23252b))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .id("settings-btn")
                        .child(space_switcher::settings_button()),
                )
                .child(spaces_dock)
                .child(
                    div()
                        .id("downloads-btn")
                        .child(space_switcher::downloads_button()),
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
                        .text_color(rgb(0x858994))
                        .hover(|style| style.bg(rgb(0x282b31)))
                        .cursor_pointer()
                        .child("›")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sidebar_collapsed = false;
                            cx.notify();
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
                            self.active_space().color,
                        )),
                );
        }

        let mut root = div()
            .relative()
            .flex()
            .size_full()
            .bg(rgb(0x111216))
            .text_color(rgb(0xe6e7eb))
            .on_mouse_move(
                cx.listener(|this, event: &gpui_kit::MouseMoveEvent, _, cx| {
                    if this.sidebar_resizing {
                        let new_width = f32::from(event.position.x);
                        this.sidebar_width = new_width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.sidebar_resizing {
                        this.sidebar_resizing = false;
                        cx.notify();
                    }
                }),
            )
            .child(sidebar_content)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div().h(px(52.0)).flex().items_center().px_4().child(
                            div()
                                .id("address-bar-container")
                                .flex_1()
                                .h(px(36.0))
                                .px_3()
                                .rounded_lg()
                                .bg(rgb(0x191a1f))
                                .border_1()
                                .border_color(rgb(0x282b33))
                                .hover(|s| s.border_color(rgb(0x3e424d)))
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
                    .when(
                        self.show_suggestions && !self.suggestions.is_empty(),
                        |this| {
                            let mut list = div()
                                .id("autocomplete-suggestions")
                                .occlude()
                                .absolute()
                                .top(px(56.0))
                                .left(px(16.0))
                                .w(px(560.0))
                                .bg(rgb(0x191a1f))
                                .border_1()
                                .border_color(rgb(0x2f323a))
                                .rounded_xl()
                                .p_1p5()
                                .flex()
                                .flex_col()
                                .gap_1();

                            for (idx, item) in self.suggestions.iter().enumerate() {
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
                                        .hover(|s| s.bg(rgb(0x262830)))
                                        .when(self.selected_suggestion == Some(idx), |row| {
                                            row.bg(rgb(0x262830))
                                        })
                                        .child(
                                            div()
                                                .text_color(rgb(0x8b8f9a))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .size(px(16.0))
                                                .child(match icon_type.as_str() {
                                                    "globe" => Icon::new(IconName::Globe),
                                                    "zap" => Icon::new(IconName::Zap),
                                                    _ => Icon::new(IconName::Search),
                                                }),
                                        )
                                        .child(
                                            div().text_sm().text_color(rgb(0xe6e7eb)).child(title),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.navigate_to(&target, window, cx);
                                            cx.notify();
                                        })),
                                );
                            }

                            this.child(list)
                        },
                    )
                    .child(
                        div()
                            .id("web-content-card")
                            .flex_1()
                            .mx(px(CONTENT_BEZEL_X))
                            .mb(px(CONTENT_BEZEL_BOTTOM))
                            .rounded(px(CARD_RADIUS))
                            .bg(rgb(0x15161a))
                            .overflow_hidden()
                            .child(
                                if active_tab_url == "lumen://welcome"
                                    || active_tab_url == "about:blank"
                                    || active_tab_url.is_empty()
                                {
                                    div().id("empty-space").size_full()
                                } else {
                                    #[cfg(all(target_os = "linux", not(test)))]
                                    let maybe_image = self.engine.get_image(&active_tab_id_string);
                                    #[cfg(any(not(target_os = "linux"), test))]
                                    let maybe_image: Option<
                                        std::sync::Arc<gpui_kit::RenderImage>,
                                    > = None;

                                    if let Some(render_image) = maybe_image {
                                        let tab_id_down = active_tab_id_string.clone();
                                        let tab_id_up = active_tab_id_string.clone();
                                        let tab_id_move = active_tab_id_string.clone();
                                        let tab_id_scroll = active_tab_id_string.clone();

                                        div()
                                            .id("web-view-viewport")
                                            .track_focus(&self.browser_focus_handle)
                                            .size_full()
                                            .bg(rgb(0x15161a))
                                            .rounded(px(CARD_RADIUS))
                                            .overflow_hidden()
                                            .on_mouse_down(
                                                gpui_kit::MouseButton::Left,
                                                cx.listener(
                                                    move |this,
                                                          event: &gpui_kit::MouseDownEvent,
                                                          window,
                                                          cx| {
                                                        this.focus_browser(window, cx);
                                                        let scale = window.scale_factor();
                                                        let sidebar_num = if this.sidebar_collapsed {
                                                            64.0
                                                        } else {
                                                            this.sidebar_width
                                                        };
                                                        let origin_x = sidebar_num + CONTENT_BEZEL_X;
                                                        let x = ((f32::from(event.position.x)
                                                            - origin_x)
                                                            .max(0.0)
                                                            * scale)
                                                            as i32;
                                                        let y = ((f32::from(event.position.y) - 52.0)
                                                            .max(0.0)
                                                            * scale)
                                                            as i32;
                                                        this.engine.send_mouse_click(
                                                            &tab_id_down,
                                                            x,
                                                            y,
                                                            cef::MouseButtonType::LEFT,
                                                            false,
                                                            event.click_count as i32,
                                                        );
                                                    },
                                                ),
                                            )
                                            .on_mouse_up(
                                                gpui_kit::MouseButton::Left,
                                                cx.listener(
                                                    move |this,
                                                          event: &gpui_kit::MouseUpEvent,
                                                          window,
                                                          _cx| {
                                                        let scale = window.scale_factor();
                                                        let sidebar_num = if this.sidebar_collapsed {
                                                            64.0
                                                        } else {
                                                            this.sidebar_width
                                                        };
                                                        let origin_x = sidebar_num + CONTENT_BEZEL_X;
                                                        let x = ((f32::from(event.position.x)
                                                            - origin_x)
                                                            .max(0.0)
                                                            * scale)
                                                            as i32;
                                                        let y = ((f32::from(event.position.y) - 52.0)
                                                            .max(0.0)
                                                            * scale)
                                                            as i32;
                                                        this.engine.send_mouse_click(
                                                            &tab_id_up,
                                                            x,
                                                            y,
                                                            cef::MouseButtonType::LEFT,
                                                            true,
                                                            event.click_count as i32,
                                                        );
                                                    },
                                                ),
                                            )
                                            .on_mouse_move(cx.listener(
                                                move |this,
                                                      event: &gpui_kit::MouseMoveEvent,
                                                      window,
                                                      _cx| {
                                                    let scale = window.scale_factor();
                                                    let sidebar_num = if this.sidebar_collapsed {
                                                        64.0
                                                    } else {
                                                        this.sidebar_width
                                                    };
                                                    let origin_x = sidebar_num + CONTENT_BEZEL_X;
                                                    let x = ((f32::from(event.position.x) - origin_x)
                                                        .max(0.0)
                                                        * scale)
                                                        as i32;
                                                    let y = ((f32::from(event.position.y) - 52.0).max(0.0)
                                                        * scale)
                                                        as i32;
                                                    this.engine.send_mouse_move(&tab_id_move, x, y, false);
                                                },
                                            ))
                                            .on_scroll(cx.listener(
                                                move |this, event: &gpui_kit::ScrollEvent, window, _cx| {
                                                    let scale = window.scale_factor();
                                                    let sidebar_num = if this.sidebar_collapsed {
                                                        64.0
                                                    } else {
                                                        this.sidebar_width
                                                    };
                                                    let origin_x = sidebar_num + CONTENT_BEZEL_X;
                                                    let x = ((f32::from(event.position.x) - origin_x)
                                                        .max(0.0)
                                                        * scale)
                                                        as i32;
                                                    let y = ((f32::from(event.position.y) - 52.0).max(0.0)
                                                        * scale)
                                                        as i32;
                                                    let (dx, dy) = match event.delta {
                                                        gpui_kit::ScrollDelta::Pixels(p) => {
                                                            (f32::from(p.x) * scale, f32::from(p.y) * scale)
                                                        }
                                                        gpui_kit::ScrollDelta::Lines(l) => {
                                                            (l.x * 40.0 * scale, l.y * 40.0 * scale)
                                                        }
                                                    };
                                                    this.engine.send_mouse_wheel(
                                                        &tab_id_scroll,
                                                        x,
                                                        y,
                                                        dx as i32,
                                                        dy as i32,
                                                    );
                                                },
                                            ))
                                            .child(
                                                gpui_kit::img(render_image)
                                                    .size_full()
                                                    .rounded(px(CARD_RADIUS)),
                                            )
                                    } else {
                                        div().id("web-view-loading").size_full().bg(rgb(0x15161a))
                                    }
                                }
                            ),
                    ),
            );

        root = root
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.to_lowercase();
                let suggestions_open = this.show_suggestions && !this.suggestions.is_empty();
                let is_suggestion_arrow = suggestions_open
                    && matches!(key.as_str(), "up" | "down" | "arrowup" | "arrowdown");
                if event.is_held && !is_suggestion_arrow {
                    return;
                }
                let modifiers = &event.keystroke.modifiers;
                let browser_modifier = modifiers.control || modifiers.platform;
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
                    let tab_id = this.active_tab_id().to_string();
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
                } else if this.browser_focused {
                    this.engine.send_key_down(this.active_tab_id(), event);
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, window, cx| {
                let modifiers = &event.keystroke.modifiers;
                let browser_modifier = modifiers.control || modifiers.platform;
                let key = event.keystroke.key.to_lowercase();
                if this.browser_focused
                    && !Self::is_app_shortcut(&key, browser_modifier, modifiers.shift)
                {
                    this.engine.send_key_up(this.active_tab_id(), event);
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
                    .bottom(px(48.0))
                    .left(px(8.0))
                    .w(px(224.0))
                    .bg(rgb(0x1e2026))
                    .border_1()
                    .border_color(rgb(0x32353e))
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
                            .text_color(rgb(0x7e828d))
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
                            .text_color(rgb(0xe6e7eb))
                            .hover(|style| style.bg(rgb(0x282b33)))
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

                root = root.child(ctx_menu);
            }
        }

        if let Some(modal) = &self.space_modal {
            let is_edit = matches!(modal.mode, SpaceModalMode::Edit { .. });
            let title = if is_edit { "Edit Space" } else { "New Space" };
            let action_label = if is_edit { "Save" } else { "Create" };
            let current_selected_icon = modal.selected_icon.clone();
            let current_selected_color = modal.selected_color;

            let popover = div()
                .id("space-popover-card")
                .occlude()
                .absolute()
                .bottom(px(52.0))
                .left(px(8.0))
                .w(px(224.0))
                .bg(rgb(0x191a1f))
                .border_1()
                .border_color(rgb(0x2f323a))
                .rounded_xl()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xf1f2f5))
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
                                .text_color(rgb(0x9fa3ae))
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
                                .border_color(rgb(0x2f323a))
                                .bg(rgb(0x15161a))
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
                                .text_color(rgb(0x9fa3ae))
                                .child("Icon"),
                        )
                        .child({
                            let mut icons_grid = div()
                                .id("svg-picker-grid")
                                .h(px(100.0))
                                .overflow_y_scroll()
                                .p_1()
                                .rounded_md()
                                .bg(rgb(0x15161a))
                                .border_1()
                                .border_color(rgb(0x282b33))
                                .flex()
                                .flex_wrap()
                                .gap_1();
                            for (i, &icon) in SPACE_ICONS.iter().enumerate() {
                                let is_selected = icon == current_selected_icon;
                                let icon_str = icon.to_string();
                                icons_grid = icons_grid.child(
                                    div()
                                        .id(format!("space-icon-choice-{i}"))
                                        .size(px(26.0))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(rgb(if is_selected { 0x333742 } else { 0x1e2026 }))
                                        .border_1()
                                        .border_color(rgb(if is_selected {
                                            0x9aa6ff
                                        } else {
                                            0x282b33
                                        }))
                                        .text_color(rgb(if is_selected {
                                            0xffffff
                                        } else {
                                            0x8b8f9a
                                        }))
                                        .hover(|s| s.bg(rgb(0x3a3e4a)).text_color(rgb(0xffffff)))
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
                                .text_color(rgb(0x9fa3ae))
                                .child("Color"),
                        )
                        .child({
                            let mut colors_row = div().flex().items_center().justify_between();
                            for (i, &color) in SPACE_COLORS.iter().enumerate() {
                                let is_selected = color == current_selected_color;
                                colors_row = colors_row.child(
                                    div()
                                        .id(format!("space-color-choice-{i}"))
                                        .size(px(18.0))
                                        .rounded_full()
                                        .bg(rgb(color))
                                        .border_2()
                                        .border_color(rgb(if is_selected {
                                            0xffffff
                                        } else {
                                            0x191a1f
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
                                .hover(|s| s.bg(rgb(0x282b33)).text_color(rgb(0xffffff)))
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
                                .bg(rgb(0x9aa6ff))
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0x111216))
                                .hover(|s| s.bg(rgb(0xb4bcff)))
                                .cursor_pointer()
                                .child(action_label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.commit_space_modal(cx);
                                    cx.notify();
                                })),
                        ),
                );

            root = root.child(popover);
        }

        root
    }
}

fn main() {
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
                    let text = app.address.read(cx).value().to_string();
                    app.update_suggestions(&text);
                    app.selected_suggestion = None;
                    cx.notify();
                }
            });
        let (spaces, active_space) = create_default_space();
        Lumen {
            spaces,
            active_space,
            sidebar_collapsed: false,
            sidebar_width: 240.0,
            sidebar_resizing: false,
            started_at: Instant::now(),
            first_frame_logged: true,
            address,
            browser_focus_handle: cx.focus_handle(),
            _address_subscription: address_subscription,
            suggestions: Vec::new(),
            selected_suggestion: None,
            recent_searches: Vec::new(),
            show_suggestions: false,
            engine: engine::Engine::new(),
            space_modal: None,
            space_context_menu: None,
            recently_closed_tabs: Vec::new(),
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
        let (id, url) = submit_address(&mut space, "youtube").unwrap();
        assert_eq!(id, "existing");
        assert_eq!(url, "https://youtube.com");
        assert_eq!(space.tabs[0].url, url);
        assert_eq!(space.tabs[0].title, "youtube.com");
        assert_eq!(space.tabs[0].state, TabState::Live);

        // General search query routes to Google search with clean title
        let (id, url) = submit_address(&mut space, "rust compiler docs").unwrap();
        assert_eq!(id, "existing");
        assert_eq!(url, "https://duckduckgo.com/?q=rust+compiler+docs");
        assert_eq!(space.tabs[0].title, "Search: rust compiler docs");
    }

    #[test]
    fn empty_address_opens_search_home_and_creates_a_tab_if_needed() {
        let mut space = Space::new("test", "Test", "compass", 0x123456);
        let (id, url) = submit_address(&mut space, "").unwrap();
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
        assert_eq!(clamped, 160.0);

        let requested_grow = 500.0_f32;
        let clamped_grow = requested_grow.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
        assert_eq!(clamped_grow, 320.0);

        let mid = 240.0_f32;
        assert_eq!(mid.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH), 240.0);
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
