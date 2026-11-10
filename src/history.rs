//! Persistent recent-search history.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Transaction, params};

const MAX_RECENT_SEARCHES: usize = 10;
static PLUGIN_ENABLED: AtomicBool = AtomicBool::new(true);

pub(crate) fn set_plugin_enabled(enabled: bool) {
    PLUGIN_ENABLED.store(enabled, Ordering::Relaxed);
}

pub(crate) fn plugin_enabled() -> bool {
    PLUGIN_ENABLED.load(Ordering::Relaxed)
}

fn history_path() -> PathBuf {
    crate::paths::data_dir().join("recent-searches.json")
}

#[cfg(not(test))]
fn database_path() -> PathBuf {
    crate::paths::data_dir().join("history.sqlite")
}

fn open_database() -> rusqlite::Result<Connection> {
    #[cfg(test)]
    let connection = Connection::open_in_memory()?;
    #[cfg(not(test))]
    let connection = {
        let path = database_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Connection::open(path)?
    };
    initialize_database(&connection)?;
    Ok(connection)
}

fn initialize_database(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS history (
            url TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            visit_time INTEGER NOT NULL,
            visit_count INTEGER NOT NULL DEFAULT 1
        );
        CREATE INDEX IF NOT EXISTS history_visit_time ON history(visit_time DESC);
        CREATE TABLE IF NOT EXISTS imported_visits (
            url TEXT NOT NULL,
            title TEXT NOT NULL,
            visit_time_ms INTEGER NOT NULL,
            PRIMARY KEY(url, title, visit_time_ms)
        );",
    )
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visit_time: i64,
    pub visit_count: i64,
}

/// Record each committed main-frame URL once, increasing its visit count.
pub fn record_visit(url: &str, title: &str) {
    if !plugin_enabled() {
        return;
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    let Ok(connection) = open_database() else {
        return;
    };
    let _ = record_visit_in(&connection, url, title);
}

/// Merge imported browser visits into the same SQLite history used by search.
/// The source identity table makes importing the same profile idempotent.
pub fn import_visits<I>(visits: I) -> rusqlite::Result<usize>
where
    I: IntoIterator<Item = (String, String, i64)>,
{
    if !plugin_enabled() {
        return Ok(0);
    }
    let mut connection = open_database()?;
    let transaction = connection.transaction()?;
    let imported = import_visits_in(&transaction, visits)?;
    transaction.commit()?;
    Ok(imported)
}

fn import_visits_in<I>(transaction: &Transaction<'_>, visits: I) -> rusqlite::Result<usize>
where
    I: IntoIterator<Item = (String, String, i64)>,
{
    let mut imported = 0;
    for (url, title, visit_time_ms) in visits {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO imported_visits(url, title, visit_time_ms)
             VALUES (?1, ?2, ?3)",
            params![url, title, visit_time_ms],
        )?;
        if inserted == 0 {
            continue;
        }
        transaction.execute(
            "INSERT INTO history(url, title, visit_time, visit_count) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(url) DO UPDATE SET
                title = CASE WHEN excluded.title != '' THEN excluded.title ELSE history.title END,
                visit_time = MAX(history.visit_time, excluded.visit_time),
                visit_count = history.visit_count + 1",
            params![url, title, visit_time_ms / 1_000],
        )?;
        imported += 1;
    }
    Ok(imported)
}

fn record_visit_in(connection: &Connection, url: &str, title: &str) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO history(url, title, visit_time, visit_count) VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(url) DO UPDATE SET
            title = CASE WHEN excluded.title != '' THEN excluded.title ELSE history.title END,
            visit_time = excluded.visit_time,
            visit_count = history.visit_count + 1",
        params![url, title, now()],
    )?;
    Ok(())
}

/// Refresh the title for a URL already recorded in history.
pub fn update_title(url: &str, title: &str) {
    if !plugin_enabled() {
        return;
    }
    if url.is_empty() || title.is_empty() {
        return;
    }
    let Ok(connection) = open_database() else {
        return;
    };
    let _ = update_title_in(&connection, url, title);
}

fn update_title_in(connection: &Connection, url: &str, title: &str) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE history SET title = ?1 WHERE url = ?2",
        params![title, url],
    )?;
    Ok(())
}

/// Return URL/title matches ordered by a frecency score (frequency and recency).
pub fn search(query: &str, limit: usize) -> Vec<HistoryEntry> {
    if !plugin_enabled() || query.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    let Ok(connection) = open_database() else {
        return Vec::new();
    };
    search_in(&connection, query, limit)
}

fn search_in(connection: &Connection, query: &str, limit: usize) -> Vec<HistoryEntry> {
    if query.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    let pattern = format!("%{}%", query.trim().replace('%', "\\%").replace('_', "\\_"));
    let mut statement = match connection.prepare(
        "SELECT url, title, visit_time, visit_count FROM history
         WHERE url LIKE ?1 ESCAPE '\\' COLLATE NOCASE OR title LIKE ?1 ESCAPE '\\' COLLATE NOCASE
         ORDER BY (visit_count * 1000000 + visit_time) DESC LIMIT ?2",
    ) {
        Ok(statement) => statement,
        Err(_) => return Vec::new(),
    };
    statement
        .query_map(params![pattern, limit as i64], |row| {
            Ok(HistoryEntry {
                url: row.get(0)?,
                title: row.get(1)?,
                visit_time: row.get(2)?,
                visit_count: row.get(3)?,
            })
        })
        .and_then(Iterator::collect)
        .unwrap_or_default()
}

pub fn load_recent_searches() -> Vec<String> {
    if !plugin_enabled() {
        return Vec::new();
    }
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
    if !plugin_enabled() {
        return;
    }
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

pub fn clear_browsing_history() {
    if !plugin_enabled() {
        return;
    }
    if let Ok(connection) = open_database() {
        let _ = connection.execute_batch("DELETE FROM history; DELETE FROM imported_visits;");
    }
    let _ = fs::remove_file(history_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_history_tracks_counts_titles_and_searches_by_url_or_title() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        record_visit_in(&connection, "https://example.com/one", "Example One").unwrap();
        record_visit_in(&connection, "https://example.com/one", "Example One").unwrap();
        update_title_in(
            &connection,
            "https://example.com/one",
            "Example One Updated",
        )
        .unwrap();
        record_visit_in(&connection, "https://other.test/page", "Other Page").unwrap();
        connection
            .execute(
                "UPDATE history SET visit_time = 10 WHERE url = ?1",
                ["https://example.com/one"],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE history SET visit_time = 20 WHERE url = ?1",
                ["https://other.test/page"],
            )
            .unwrap();

        let by_title = search_in(&connection, "updated", 10);
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].visit_count, 2);
        let by_url = search_in(&connection, "example.com", 10);
        assert_eq!(by_url[0].url, "https://example.com/one");
        assert_eq!(by_url[0].title, "Example One Updated");
        let ranked = search_in(&connection, "https://", 10);
        assert_eq!(ranked[0].url, "https://example.com/one");
    }

    #[test]
    fn imported_visits_merge_into_search_history_idempotently() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let visits = vec![
            (
                "https://example.com/".to_owned(),
                "Example".to_owned(),
                1_000,
            ),
            (
                "https://example.com/".to_owned(),
                "Example".to_owned(),
                1_000,
            ),
            (
                "https://example.com/".to_owned(),
                "Example".to_owned(),
                2_000,
            ),
        ];
        let transaction = connection.transaction().unwrap();
        assert_eq!(import_visits_in(&transaction, visits.clone()).unwrap(), 2);
        transaction.commit().unwrap();

        let transaction = connection.transaction().unwrap();
        assert_eq!(import_visits_in(&transaction, visits).unwrap(), 0);
        transaction.commit().unwrap();

        let found = search_in(&connection, "example.com", 10);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].visit_count, 2);
        assert_eq!(found[0].visit_time, 2);
    }
}
