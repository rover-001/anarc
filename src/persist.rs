//! Session persistence for Lumen spaces and tabs.

use crate::model::Space;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    pub spaces: Vec<Space>,
    pub active_space: String,
}

pub fn spaces_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".config/lumen/spaces.json")
}

pub fn load_state() -> Option<AppState> {
    let path = spaces_path();
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_state(spaces: &[Space], active_space: &str) {
    let path = spaces_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let state = AppState {
        spaces: spaces.to_vec(),
        active_space: active_space.to_string(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(tmp, path);
        }
    }
}
