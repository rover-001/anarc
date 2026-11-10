use serde::{Deserialize, Serialize};

use super::Tab;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// Accent color as a 24-bit RGB value.
    pub color: u32,
    /// Additional colors used by the space's multi-color theme.
    #[serde(default)]
    pub theme_colors: Vec<u32>,
    #[serde(default)]
    pub theme_appearance: u8,
    #[serde(skip)]
    pub theme_auto_light: bool,
    #[serde(default = "default_theme_intensity")]
    pub theme_intensity: f32,
    #[serde(default)]
    pub theme_grain: f32,
    pub tabs: Vec<Tab>,
    pub active: Option<String>,
    #[serde(default)]
    pub split: Option<SplitPair>,
    /// Sidebar folders that group tabs below the pinned section.
    #[serde(default)]
    pub folders: Vec<Folder>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitPair {
    pub first: String,
    pub second: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub expanded: bool,
}

impl Folder {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            expanded: true,
        }
    }
}

impl Space {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        icon: impl Into<String>,
        color: u32,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            color,
            theme_colors: Vec::new(),
            theme_appearance: 0,
            theme_auto_light: false,
            theme_intensity: 0.24,
            theme_grain: 0.0,
            tabs: Vec::new(),
            active: None,
            split: None,
            folders: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_space_json_loads_without_a_split_pair_and_split_round_trips() {
        let old_json = r#"{"id":"personal","name":"Personal","icon":"compass","color":1010101,"tabs":[],"active":null}"#;
        let mut space: Space = serde_json::from_str(old_json).unwrap();
        assert!(space.split.is_none());
        assert!(space.folders.is_empty());
        space.split = Some(SplitPair {
            first: "tab-1".into(),
            second: "tab-2".into(),
        });
        let decoded: Space = serde_json::from_str(&serde_json::to_string(&space).unwrap()).unwrap();
        assert_eq!(decoded.split, space.split);
    }

    #[test]
    fn folders_round_trip_with_their_tabs() {
        let mut space = Space::new("personal", "Personal", "compass", 0x9aa6ff);
        space.folders.push(Folder::new("folder-1", "Work"));
        let mut tab = Tab::new("tab-1", "Docs", "https://example.com");
        tab.folder = Some("folder-1".into());
        space.tabs.push(tab);

        let decoded: Space = serde_json::from_str(&serde_json::to_string(&space).unwrap()).unwrap();
        assert_eq!(decoded.folders.len(), 1);
        assert_eq!(decoded.folders[0].name, "Work");
        assert_eq!(decoded.tabs[0].folder.as_deref(), Some("folder-1"));
    }
}

fn default_theme_intensity() -> f32 {
    0.24
}
