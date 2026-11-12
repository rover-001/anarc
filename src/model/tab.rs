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
    pub pinned: bool,
    /// Favorite tabs live in the Arc-style grid at the top of the sidebar.
    #[serde(default)]
    pub favorite: bool,
    /// Folder that owns this tab in the sidebar, if any.
    #[serde(default)]
    pub folder: Option<String>,
    pub last_active: u64,
    pub scroll_y: f64,
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
            last_active: 0,
            scroll_y: 0.0,
            state: TabState::Discarded,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabState {
    Live,
    Frozen,
    #[default]
    Discarded,
}
