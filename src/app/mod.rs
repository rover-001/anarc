use std::time::Instant;

use crate::model::{Folder, Space, SplitPair, Tab, TabState};
pub(crate) use crate::navigation::{
    SuggestionItem, compute_suggestions, move_suggestion_selection,
};
#[cfg(not(test))]
use crate::settings::url_host;
use crate::ui::command_palette::{self, PaletteAction, PaletteItem, PaletteTab, PaletteTarget};
use crate::ui::sidebar::rows::{TabDrag, TabDragGhost, section_tab_row, tab_context_menu_position};
use crate::ui::split_view::{self, DIVIDER_HIT_WIDTH, DIVIDER_WIDTH};
use crate::ui::visual::{self, ADDRESS_BAR_HEIGHT, NAV_BUTTON_SIZE, TOPBAR_HEIGHT, UI_SCALE};
use crate::ui::web_view::WebViewport;
use crate::ui::{space_switcher, tab_list};
use crate::{engine, history, persist, reader, stats, ui};
use gpui_kit::{
    Animation, AnimationExt as _, AnyView, Context, Focusable, FontWeight, KeyDownEvent,
    KeyUpEvent, Render, SpringAnimation, SpringConfig, StyleRefinement, Window,
    assets::IconName,
    component::Icon,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::*,
    px, rgb,
};

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

pub const SIDEBAR_MIN_WIDTH: f32 = 160.0 * UI_SCALE;
pub const SIDEBAR_MAX_WIDTH: f32 = 320.0 * UI_SCALE;
pub const SIDEBAR_COLLAPSED_WIDTH: f32 = 64.0 * UI_SCALE;
pub(crate) const CONTENT_BEZEL_X: f32 = 4.0 * UI_SCALE;
pub const CONTENT_BEZEL_BOTTOM: f32 = 4.0 * UI_SCALE;
pub(crate) const CARD_RADIUS: f32 = 16.0 * UI_SCALE;

pub(crate) use crate::model::sidebar::closing_tab_row_index;
pub(crate) use state::{AppPage, Lumen, SettingsSection};
use state::{
    FolderContextMenu, FolderModal, FolderModalMode, SidebarContextMenu, SpaceContextMenu,
    SpaceModal, SpaceModalMode, TabContextMenu, ThemeEditor,
};

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
    let space_prefix = format!("{}-tab-", space.id);
    let next = space
        .tabs
        .iter()
        .filter_map(|tab| {
            tab.id
                .strip_prefix(&space_prefix)
                .or_else(|| tab.id.strip_prefix("tab-"))?
                .parse::<usize>()
                .ok()
        })
        .max()
        .unwrap_or(0)
        + 1;
    #[cfg(not(test))]
    {
        format!("{}-tab-{next}", space.id)
    }
    #[cfg(test)]
    {
        let _ = space;
        format!("tab-{next}")
    }
}

/// Browser instances are indexed by tab ID, so IDs must be unique across
/// spaces even though older persisted sessions could reuse `tab-1` in each.
fn make_tab_ids_unique(spaces: &mut [Space]) {
    let mut seen = std::collections::HashSet::new();
    for space in spaces {
        let mut replacements = std::collections::HashMap::new();
        for tab in &mut space.tabs {
            if seen.insert(tab.id.clone()) {
                continue;
            }
            let original = tab.id.clone();
            let mut replacement = format!("{}-{original}", space.id);
            let mut suffix = 2;
            while seen.contains(&replacement) {
                replacement = format!("{}-{original}-{suffix}", space.id);
                suffix += 1;
            }
            seen.insert(replacement.clone());
            replacements.insert(original, replacement.clone());
            tab.id = replacement;
        }
        if let Some(active) = &mut space.active
            && let Some(replacement) = replacements.get(active)
        {
            *active = replacement.clone();
        }
        if let Some(split) = &mut space.split {
            if let Some(replacement) = replacements.get(&split.first) {
                split.first = replacement.clone();
            }
            if let Some(replacement) = replacements.get(&split.second) {
                split.second = replacement.clone();
            }
        }
    }
}

fn sidebar_width_for(collapsed: bool, width: f32) -> f32 {
    if collapsed {
        SIDEBAR_COLLAPSED_WIDTH
    } else {
        width
    }
}

/// Number of favorite tiles per sidebar row.
const FAVORITE_TILES_PER_ROW: usize = 4;

#[cfg(all(target_os = "linux", not(test)))]
fn resize_debug(message: std::fmt::Arguments<'_>) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var("ANARC_DEBUG_RESIZE").as_deref() == Ok("1")) {
        eprintln!("[resize-debug] {message}");
    }
}

mod actions;
mod render;
mod state;
#[cfg(test)]
mod tests;
