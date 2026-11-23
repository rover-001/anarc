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
    /// Remote peer IPs for live cursor and tab sync.
    #[serde(default)]
    pub peer_ips: Vec<String>,
}

/// Up to four tabs sharing one content area. Two tabs fill a single row; when
/// `third`/`fourth` are present the panes form a 2x2 grid. The optional fields
/// keep saves from the two-pane era loadable without a migration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitPair {
    pub first: String,
    pub second: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub third: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fourth: Option<String>,
}

impl SplitPair {
    pub fn new(first: String, second: String) -> Self {
        Self {
            first,
            second,
            third: None,
            fourth: None,
        }
    }

    pub fn is_quad(&self) -> bool {
        self.third.is_some() && self.fourth.is_some()
    }

    /// Tab ids in pane order: top-left, top-right, bottom-left, bottom-right.
    pub fn panes(&self) -> Vec<&str> {
        let mut panes = vec![self.first.as_str(), self.second.as_str()];
        if let Some(third) = &self.third {
            panes.push(third);
        }
        if let Some(fourth) = &self.fourth {
            panes.push(fourth);
        }
        panes
    }

    pub fn pane(&self, index: usize) -> Option<&str> {
        self.panes().get(index).copied()
    }

    pub fn pane_count(&self) -> usize {
        self.panes().len()
    }

    /// Pane ids with `tab_id` removed, preserving row-major order.
    pub fn panes_without(&self, tab_id: &str) -> Vec<String> {
        self.panes()
            .into_iter()
            .filter(|id| *id != tab_id)
            .map(str::to_owned)
            .collect()
    }

    /// Rebuilds a pair from ordered pane ids. Returns `None` when fewer than
    /// two panes remain, i.e. the split should be exited.
    pub fn from_panes(panes: &[String]) -> Option<Self> {
        let mut panes = panes.iter();
        let first = panes.next()?.clone();
        let second = panes.next()?.clone();
        Some(Self {
            first,
            second,
            third: panes.next().cloned(),
            fourth: panes.next().cloned(),
        })
    }
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
            peer_ips: Vec::new(),
        }
    }
}

fn default_theme_intensity() -> f32 {
    0.24
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
        space.split = Some(SplitPair::new("tab-1".into(), "tab-2".into()));
        let decoded: Space = serde_json::from_str(&serde_json::to_string(&space).unwrap()).unwrap();
        assert_eq!(decoded.split, space.split);
        assert_eq!(decoded.split.as_ref().unwrap().pane_count(), 2);

        let mut quad = SplitPair::new("tab-1".into(), "tab-2".into());
        quad.third = Some("tab-3".into());
        quad.fourth = Some("tab-4".into());
        assert!(quad.is_quad());
        assert_eq!(quad.panes(), ["tab-1", "tab-2", "tab-3", "tab-4"]);
        space.split = Some(quad);
        let decoded: Space = serde_json::from_str(&serde_json::to_string(&space).unwrap()).unwrap();
        assert_eq!(decoded.split, space.split);
    }

    #[test]
    fn closing_one_pane_compacts_the_remaining_panes() {
        let quad = SplitPair {
            first: "tab-1".into(),
            second: "tab-2".into(),
            third: Some("tab-3".into()),
            fourth: Some("tab-4".into()),
        };

        let after_first = SplitPair::from_panes(&quad.panes_without("tab-1")).unwrap();
        assert_eq!(after_first.panes(), ["tab-2", "tab-3", "tab-4"]);
        assert!(!after_first.is_quad());
        assert_eq!(after_first.third.as_deref(), Some("tab-4"));

        let after_fourth = SplitPair::from_panes(&quad.panes_without("tab-4")).unwrap();
        assert_eq!(after_fourth.panes(), ["tab-1", "tab-2", "tab-3"]);

        let triple = after_fourth;
        let after_second = SplitPair::from_panes(&triple.panes_without("tab-2")).unwrap();
        assert_eq!(after_second.panes(), ["tab-1", "tab-3"]);

        let pair = SplitPair::new("tab-1".into(), "tab-2".into());
        assert!(SplitPair::from_panes(&pair.panes_without("tab-1")).is_none());
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
