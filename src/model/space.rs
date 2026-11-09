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
        }
    }
}
