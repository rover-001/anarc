//! One-shot imports from Chromium-family browser profiles.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Bookmark {
    title: String,
    url: String,
    folder: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ImportedVisit {
    title: String,
    url: String,
    /// Unix time in milliseconds.
    visit_time: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub profiles: usize,
    pub bookmarks: usize,
    pub visits: usize,
    pub warnings: Vec<String>,
}

const CHROMIUM_EPOCH_MICROS: i64 = 11_644_473_600_000_000;

/// Imports profile bookmarks and visit records into the Lumen data directory.
pub fn import(browser: &str) -> Result<ImportSummary, String> {
    let roots = browser_roots(browser)?;
    let profiles = roots.into_iter().flat_map(profile_dirs).collect::<Vec<_>>();
    let mut summary = ImportSummary::default();
    let mut bookmarks = Vec::new();
    let mut visits = Vec::new();

    for (browser_name, profile) in profiles {
        summary.profiles += 1;
        let profile_name = profile
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("profile");
        match read_bookmarks(&profile.join("Bookmarks")) {
            Ok(found) => bookmarks.extend(found),
            Err(error) => summary
                .warnings
                .push(format!("{browser_name}/{profile_name} bookmarks: {error}")),
        }
        match read_history(&profile.join("History")) {
            Ok(found) => visits.extend(found),
            Err(error) => summary
                .warnings
                .push(format!("{browser_name}/{profile_name} history: {error}")),
        }
    }

    let data_dir = crate::paths::data_dir();
    fs::create_dir_all(&data_dir)
        .map_err(|error| format!("cannot create {}: {error}", data_dir.display()))?;
    summary.bookmarks = merge_json(&data_dir.join("bookmarks.json"), bookmarks)?;
    summary.visits = crate::history::import_visits(
        visits
            .into_iter()
            .map(|visit| (visit.url, visit.title, visit.visit_time)),
    )
    .map_err(|error| format!("cannot import history into SQLite: {error}"))?;
    Ok(summary)
}

fn browser_roots(browser: &str) -> Result<Vec<(String, PathBuf)>, String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    let candidates = [
        ("chromium", config.join("chromium")),
        ("chrome", config.join("google-chrome")),
        ("brave", config.join("BraveSoftware/Brave-Browser")),
        ("edge", config.join("microsoft-edge")),
    ];
    let names: &[&str] = match browser.to_ascii_lowercase().as_str() {
        "chrome" => &["chrome"],
        "chromium" => &["chromium"],
        "brave" => &["brave"],
        "edge" => &["edge"],
        "all" => &["chromium", "chrome", "brave", "edge"],
        _ => {
            return Err(format!(
                "unsupported browser '{browser}' (use chrome, chromium, brave, edge, or all)"
            ));
        }
    };
    Ok(candidates
        .into_iter()
        .filter(|(name, _)| names.contains(name))
        .map(|(name, path)| (name.to_owned(), path))
        .collect())
}

fn profile_dirs((browser, root): (String, PathBuf)) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut dirs = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_profile = name == "Default"
                || name.strip_prefix("Profile ").is_some_and(|number| {
                    !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
                });
            (is_profile && entry.path().is_dir()).then_some(entry.path())
        })
        .collect::<Vec<_>>();
    dirs.sort();
    dirs.into_iter()
        .map(|path| (browser.clone(), path))
        .collect()
}

fn read_bookmarks(path: &Path) -> Result<Vec<Bookmark>, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let root: Value = serde_json::from_str(&content).map_err(|error| error.to_string())?;
    let roots = root
        .get("roots")
        .and_then(Value::as_object)
        .ok_or_else(|| "missing roots object".to_owned())?;
    let mut bookmarks = Vec::new();
    for node in roots.values() {
        collect_bookmarks(node, Vec::new(), &mut bookmarks);
    }
    Ok(bookmarks)
}

fn collect_bookmarks(node: &Value, folders: Vec<String>, output: &mut Vec<Bookmark>) {
    let Some(kind) = node.get("type").and_then(Value::as_str) else {
        return;
    };
    if kind == "url" {
        if let (Some(title), Some(url)) = (
            node.get("name").and_then(Value::as_str),
            node.get("url").and_then(Value::as_str),
        ) {
            output.push(Bookmark {
                title: title.to_owned(),
                url: url.to_owned(),
                folder: folders.join("/"),
            });
        }
        return;
    }
    if kind != "folder" {
        return;
    }
    let mut child_folders = folders;
    if let Some(name) = node.get("name").and_then(Value::as_str)
        && !name.is_empty()
    {
        child_folders.push(name.to_owned());
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            collect_bookmarks(child, child_folders.clone(), output);
        }
    }
}

fn read_history(path: &Path) -> Result<Vec<ImportedVisit>, String> {
    if !path.is_file() {
        return Err("History database not found".to_owned());
    }
    let temp = std::env::temp_dir().join(format!(
        "lumen-history-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::copy(path, &temp).map_err(|error| format!("cannot copy locked database: {error}"))?;
    let source_wal = PathBuf::from(format!("{}-wal", path.display()));
    if source_wal.is_file() {
        let target_wal = PathBuf::from(format!("{}-wal", temp.display()));
        fs::copy(source_wal, target_wal)
            .map_err(|error| format!("cannot copy history WAL: {error}"))?;
    }
    let result = query_history(&temp);
    let _ = fs::remove_file(&temp);
    let _ = fs::remove_file(format!("{}-wal", temp.display()));
    result
}

fn query_history(path: &Path) -> Result<Vec<ImportedVisit>, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT urls.title, urls.url, visits.visit_time
             FROM visits JOIN urls ON urls.id = visits.url
             WHERE urls.url != '' ORDER BY visits.visit_time DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let chromium_time: i64 = row.get(2)?;
            Ok(ImportedVisit {
                title: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                url: row.get(1)?,
                visit_time: (chromium_time - CHROMIUM_EPOCH_MICROS) / 1_000,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn merge_json<T>(path: &Path, incoming: Vec<T>) -> Result<usize, String>
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq,
{
    let mut existing = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Vec<T>>(&bytes)
            .map_err(|error| format!("cannot parse {}: {error}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let mut imported = 0;
    for item in incoming {
        if !existing.contains(&item) {
            existing.push(item);
            imported += 1;
        }
    }
    let bytes = serde_json::to_vec_pretty(&existing).map_err(|error| error.to_string())?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temp, path).map_err(|error| error.to_string())?;
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_chromium_bookmark_folders() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chromium/Default/Bookmarks");
        let output = read_bookmarks(&fixture).unwrap();
        assert_eq!(output[0].folder, "Bookmarks bar/Docs");
        assert_eq!(output[0].url, "https://rust-lang.org");
    }

    #[test]
    fn imports_visits_from_a_copied_sqlite_fixture() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chromium/Default/History");
        let base = std::env::temp_dir().join(format!("lumen-import-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let database = base.join("History");
        fs::copy(fixture, &database).unwrap();
        let visits = read_history(&database).unwrap();
        assert_eq!(visits[0].url, "https://example.test/");
        assert_eq!(visits[0].visit_time, 1_000);
        let _ = fs::remove_dir_all(base);
    }
}
