use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppPage {
    Settings,
    Downloads,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    #[default]
    General,
    Appearance,
    Shortcuts,
    Plugins,
}

pub(super) struct PendingPluginReview {
    pub(super) review: crate::ui::plugin_manager::PluginReview,
    pub(super) source_path: std::path::PathBuf,
    pub(super) signer_public_key: Option<Vec<u8>>,
    pub(super) manifest_toml: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SpaceModalMode {
    Create,
    Edit { space_id: String },
}

pub(super) struct SpaceModal {
    pub(super) mode: SpaceModalMode,
    pub(super) name_input: gpui_kit::Entity<InputState>,
    pub(super) _name_sub: gpui_kit::Subscription,
    pub(super) selected_icon: String,
    pub(super) theme_draft: Space,
}

pub(super) struct SpaceContextMenu {
    pub(super) space_id: String,
}

pub(super) struct SidebarContextMenu {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

pub(super) struct ThemeEditor {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) selected_color: usize,
    pub(super) dragging: bool,
    pub(super) control: usize,
    pub(super) target_modal: bool,
    pub(super) grab_offset: (f32, f32),
    pub(super) pointer_position: Option<(usize, f32, f32)>,
    pub(super) panel_bounds: std::rc::Rc<std::cell::Cell<gpui_kit::Bounds<gpui_kit::Pixels>>>,
    pub(super) bounds: std::rc::Rc<std::cell::Cell<[gpui_kit::Bounds<gpui_kit::Pixels>; 3]>>,
}

pub(super) struct TabContextMenu {
    pub(super) tab_id: String,
    pub(super) showing_split_targets: bool,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

#[derive(Clone, Debug)]
pub(super) struct WebContextMenu {
    pub(super) tab_id: String,
    pub(super) x: f32,
    pub(super) y: f32,
}

/// Arc-style peek: a link that would open a new tab is previewed in a floating
/// overlay instead. `id` doubles as the CEF browser key so promoting the preview
/// to a real tab keeps the live page (including its history and scroll state).
#[derive(Clone, Debug)]
pub(super) struct PeekState {
    pub(super) id: String,
    pub(super) space_id: String,
    pub(super) title: String,
    pub(super) url: String,
}

pub(super) struct FolderContextMenu {
    pub(super) folder_id: String,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum FolderModalMode {
    Create { target_tab: Option<String> },
    Rename { folder_id: String },
}

pub(super) struct FolderModal {
    pub(super) mode: FolderModalMode,
    pub(super) name_input: gpui_kit::Entity<InputState>,
    pub(super) _name_sub: gpui_kit::Subscription,
    pub(super) anchor_x: f32,
    pub(super) anchor_y: f32,
}

pub(crate) struct Anarc {
    pub(super) spaces: Vec<Space>,
    pub(super) active_space: String,
    pub(super) sidebar_collapsed: bool,
    pub(super) sidebar_hovered: bool,
    pub(crate) kept_drag_hovered_tab: Option<String>,
    pub(crate) kept_drag_hover_after: bool,
    pub(crate) kept_drag_source_id: Option<String>,
    pub(crate) kept_section_drag_hovered: bool,
    pub(super) sidebar_width: f32,
    pub(super) sidebar_resizing: bool,
    pub(super) sidebar_width_transition: Option<(f32, f32, Instant, u64)>,
    pub(super) sidebar_transition_serial: u64,
    pub(super) theme_transition: Option<(visual::SpaceTheme, Instant, u64)>,
    pub(super) theme_transition_id: u64,
    pub(super) space_switch_transition:
        Option<(Space, space_switcher::SpaceSwitchState, Instant, u64)>,
    pub(super) space_switch_transition_id: u64,
    #[cfg(not(test))]
    pub(super) resize_drag: split_view::ResizeDrag,
    pub(super) address: gpui_kit::Entity<InputState>,
    pub(super) address_active_tab: Option<(String, String, String)>,
    pub(super) address_editing: bool,
    pub(super) reset_address_for_tab_change: bool,
    pub(super) find_input: gpui_kit::Entity<InputState>,
    pub(super) command_input: gpui_kit::Entity<InputState>,
    pub(super) command_palette_scroll: gpui_kit::UniformListScrollHandle,
    pub(crate) browser_focus_handle: gpui_kit::FocusHandle,
    pub(super) _address_subscription: gpui_kit::Subscription,
    pub(super) _command_subscription: gpui_kit::Subscription,
    pub(super) show_find: bool,
    pub(super) show_command_palette: bool,
    pub(super) selected_command: usize,
    pub(super) suggestions: Vec<SuggestionItem>,
    pub(super) suggestion_snapshot: Vec<SuggestionItem>,
    pub(super) suggestions_were_visible: bool,
    pub(super) suggestion_closing: Option<(Vec<SuggestionItem>, u64)>,
    pub(super) suggestion_animation_serial: u64,
    pub(super) selected_suggestion: Option<usize>,
    pub(super) recent_searches: Vec<String>,
    pub(super) zoom_levels: std::collections::HashMap<String, f64>,
    #[cfg(not(test))]
    pub(super) active_zoom_target: Option<(String, String)>,
    pub(super) show_suggestions: bool,
    pub(crate) engine: crate::engine::Engine,
    pub(super) web_viewport: gpui_kit::Entity<WebViewport>,
    pub(super) second_web_viewport: gpui_kit::Entity<WebViewport>,
    pub(super) third_web_viewport: gpui_kit::Entity<WebViewport>,
    pub(super) fourth_web_viewport: gpui_kit::Entity<WebViewport>,
    pub(super) peek_web_viewport: gpui_kit::Entity<WebViewport>,
    pub(super) peek: Option<PeekState>,
    pub(super) peek_serial: u64,
    pub(crate) peek_focused: bool,
    pub(crate) focused_pane: usize,
    pub(super) split_ratio: f32,
    pub(super) split_ratio_y: f32,
    pub(super) split_resizing: bool,
    pub(super) split_resizing_horizontal: bool,
    pub(super) space_modal: Option<SpaceModal>,
    pub(super) space_context_menu: Option<SpaceContextMenu>,
    pub(super) sidebar_context_menu: Option<SidebarContextMenu>,
    pub(super) theme_editor: Option<ThemeEditor>,
    pub(super) tab_context_menu: Option<TabContextMenu>,
    pub(super) web_context_menu: Option<WebContextMenu>,
    pub(super) folder_context_menu: Option<FolderContextMenu>,
    pub(super) folder_modal: Option<FolderModal>,
    pub(super) split_group_expanded: bool,
    /// (space id, tab id, when) of the tab just created via add_tab; only it plays the row entrance.
    pub(super) added_tab: Option<(String, String, Instant)>,
    /// When the split group was last expanded; member rows only animate right after that.
    pub(super) split_expanded_at: Option<Instant>,
    pub(super) recently_closed_tabs: Vec<(String, Tab)>,
    pub(super) closing_tab: Option<(String, Tab, usize, Instant, u64)>,
    pub(super) closing_tab_serial: u64,
    pub(crate) browser_focused: bool,
    pub(super) fullscreen_tab: Option<String>,
    pub(super) fullscreen_window_was_fullscreen: Option<bool>,
    pub(super) app_page: Option<AppPage>,
    pub(super) plugins_menu_open: bool,
    pub(super) dismissed_download_toast_id: Option<u32>,
    pub(crate) settings_section: SettingsSection,
    #[allow(dead_code)]
    pub(crate) last_spaces_mtime: std::sync::atomic::AtomicU64,
    pub(crate) last_shared_cookies_mtime: std::sync::atomic::AtomicU64,
    pub(crate) last_cookie_sync: std::time::Instant,
    // Live Share session state mirrored from the daemon's live-share-session.json.
    pub(crate) last_live_share_session_mtime: std::sync::atomic::AtomicU64,
    pub(crate) live_share_session_generation: u64,
    pub(crate) live_share_is_host: bool,
    pub(crate) live_share_space_id: String,
    pub(crate) pending_auth_recovery: std::collections::HashMap<String, (String, std::time::Instant)>,
    pub(super) preferences: crate::settings::BrowserPreferences,
    pub(super) plugins: crate::plugin::PluginHost,
    pub(super) panel_registry: crate::plugin::PanelRegistryHandle,
    pub(super) pending_plugin_review: Option<PendingPluginReview>,
    pub(super) plugin_action_error: Option<String>,
    /// Plugin ids whose description/details accordion is expanded.
    pub(crate) expanded_plugins: std::collections::HashSet<String>,
    #[cfg(not(test))]
    pub(super) cef_task_started: bool,
}

impl Drop for Anarc {
    fn drop(&mut self) {
        self.plugins.emit(
            "app.quit",
            crate::plugin::EventMode::Parallel,
            crate::plugin::EventPayload::Empty,
        );
    }
}
