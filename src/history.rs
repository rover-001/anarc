//! Persistent recent-search history.

use std::fs;
use std::path::PathBuf;

const MAX_RECENT_SEARCHES: usize = 10;

fn history_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join(".local/share/lumen/recent-searches.json")
}

pub fn load_recent_searches() -> Vec<String> {
    fs::read_to_string(history_path())
        .ok()
        .and_then(|data| serde_json::from_str::<Vec<String>>(&data).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|query| !query.trim().is_empty())
        .take(MAX_RECENT_SEARCHES)
        .collect()
}

#[cfg(not(test))]
pub fn save_recent_searches(searches: &[String]) {
    let path = history_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let searches: Vec<_> = searches.iter().take(MAX_RECENT_SEARCHES).collect();
    if let Ok(data) = serde_json::to_vec_pretty(&searches) {
        let tmp_path = path.with_extension("tmp");
        if fs::write(&tmp_path, data).is_ok() {
            let _ = fs::rename(tmp_path, path);
        }
    }
}
