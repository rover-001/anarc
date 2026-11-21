//! Session persistence for Lumen spaces and tabs.

use crate::model::Space;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    pub spaces: Vec<Space>,
    pub active_space: String,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    #[serde(default)]
    pub sidebar_collapsed: bool,
}

fn default_sidebar_width() -> f32 {
    240.0 * crate::ui::visual::UI_SCALE
}

pub fn spaces_path() -> PathBuf {
    crate::paths::config_dir().join("spaces.json")
}

pub fn spaces_mtime() -> Option<std::time::SystemTime> {
    let path = spaces_path();
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn load_state() -> Option<AppState> {
    let path = spaces_path();
    let data = fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&data) {
        Ok(state) => Some(state),
        Err(err) => {
            eprintln!("[Persistence] Failed to parse {}: {}", path.display(), err);
            let bak = path.with_extension("json.bak");
            let _ = fs::write(bak, &data);
            None
        }
    }
}

pub fn save_state(
    spaces: &[Space],
    active_space: &str,
    sidebar_width: f32,
    sidebar_collapsed: bool,
) {
    let path = spaces_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let state = AppState {
        spaces: spaces.to_vec(),
        active_space: active_space.to_string(),
        sidebar_width,
        sidebar_collapsed,
    };
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_sidebar_preferences_round_trip() {
        let state = AppState {
            spaces: Vec::new(),
            active_space: "personal".into(),
            sidebar_width: 271.5,
            sidebar_collapsed: true,
        };
        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: AppState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.sidebar_width, 271.5);
        assert!(decoded.sidebar_collapsed);
    }
}
