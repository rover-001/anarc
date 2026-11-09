use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub favicon_path: Option<String>,
    pub pinned: bool,
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
            favicon_path: None,
            pinned: false,
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
