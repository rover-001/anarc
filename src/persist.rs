//! Session persistence for Anarc spaces and tabs.

use crate::engine::DownloadRecord;
use crate::model::Space;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Keep the Downloads page bounded; the same cap applies to live records.
pub const MAX_DOWNLOAD_RECORDS: usize = 128;

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

/// Downloads survive a restart so completed downloads still appear on the page.
pub fn downloads_path() -> PathBuf {
    crate::paths::config_dir().join("downloads.json")
}

pub fn load_downloads() -> Vec<DownloadRecord> {
    let path = downloads_path();
    let Ok(data) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<DownloadRecord>>(&data) {
        Ok(mut records) => {
            if records.len() > MAX_DOWNLOAD_RECORDS {
                records.drain(..records.len() - MAX_DOWNLOAD_RECORDS);
            }
            records
        }
        Err(error) => {
            eprintln!("[Persistence] Failed to parse {}: {}", path.display(), error);
            Vec::new()
        }
    }
}

fn trim_downloads(records: &[DownloadRecord]) -> &[DownloadRecord] {
    if records.len() > MAX_DOWNLOAD_RECORDS {
        &records[records.len() - MAX_DOWNLOAD_RECORDS..]
    } else {
        records
    }
}

pub fn save_downloads(records: &[DownloadRecord]) {
    let path = downloads_path();
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(trim_downloads(records)) else {
        return;
    };
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(tmp, path);
    }
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
    fn download_record_serde_round_trip() {
        let record = DownloadRecord {
            id: 42,
            url: "https://example.com/file.bin".into(),
            path: std::path::PathBuf::from("/home/rover/Downloads/file.bin"),
            received_bytes: 512,
            total_bytes: 1024,
            complete: false,
            canceled: false,
        };
        let json = serde_json::to_string(&record).unwrap();
        let decoded: DownloadRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, record);
    }

    #[test]
    fn trimming_keeps_newest_downloads() {
        let records: Vec<DownloadRecord> = (0..MAX_DOWNLOAD_RECORDS + 5)
            .map(|id| DownloadRecord {
                id: id as u32,
                url: format!("https://example.com/{id}"),
                path: std::path::PathBuf::from(format!("/tmp/{id}")),
                received_bytes: 0,
                total_bytes: 0,
                complete: true,
                canceled: false,
            })
            .collect();
        let trimmed = trim_downloads(&records);
        assert_eq!(trimmed.len(), MAX_DOWNLOAD_RECORDS);
        assert_eq!(trimmed.first().unwrap().id, 5);
        assert_eq!(trimmed.last().unwrap().id, MAX_DOWNLOAD_RECORDS as u32 + 4);
    }

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
