use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub url: String,
    /// Destination restored when a pinned tab is unloaded and opened again.
    #[serde(default)]
    pub pinned_url: Option<String>,
    #[serde(default)]
    pub pinned_title: Option<String>,
    #[serde(default)]
    pub favicon_path: Option<String>,
    /// Pinned/favorite state. Both flags are aliases of one kept-tab concept;
    /// `is_pinned`/`set_pinned` are the only accessors new code should use.
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub favorite: bool,
    /// Folder that owns this tab in the sidebar, if any.
    #[serde(default)]
    pub folder: Option<String>,
    /// Kept tabs with the same group share a row of favicon boxes. `None`
    /// means the tab owns a full-width single-tab row instead.
    #[serde(default)]
    pub kept_group: Option<String>,
    #[serde(default)]
    pub last_active: u64,
    #[serde(default)]
    pub scroll_y: f64,
    #[serde(default)]
    pub state: TabState,
}

impl Tab {
    pub fn new(id: impl Into<String>, title: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            url: url.into(),
            pinned_url: None,
            pinned_title: None,
            favicon_path: None,
            pinned: false,
            favorite: false,
            folder: None,
            kept_group: None,
            last_active: 0,
            scroll_y: 0.0,
            state: TabState::Discarded,
        }
    }

    /// Favorites and pinned tabs are the same section. Reads use `||` so old
    /// saved tabs that only set one flag keep working.
    pub fn is_pinned(&self) -> bool {
        self.pinned || self.favorite
    }

    /// Writes both flags so the alias never diverges in new saves.
    pub fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned;
        self.favorite = pinned;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabState {
    Live,
    Frozen,
    #[default]
    Discarded,
}
