use serde::{Deserialize, Serialize};

use super::Tab;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// Accent color as a 24-bit RGB value.
    pub color: u32,
    pub tabs: Vec<Tab>,
    pub active: Option<String>,
    #[serde(default)]
    pub split: Option<SplitPair>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitPair {
    pub first: String,
    pub second: String,
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
            tabs: Vec::new(),
            active: None,
            split: None,
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
        space.split = Some(SplitPair {
            first: "tab-1".into(),
            second: "tab-2".into(),
        });
        let decoded: Space = serde_json::from_str(&serde_json::to_string(&space).unwrap()).unwrap();
        assert_eq!(decoded.split, space.split);
    }
}
