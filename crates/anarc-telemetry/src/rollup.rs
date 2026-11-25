//! Local daily rollup file. Written before any send so "Show what would be
//! sent" and the network body come from the same canonical serialization.

use crate::schema::{Ping, ping_json};
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RollupStore {
    dir: PathBuf,
}

impl RollupStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Returns `true` only when a new file for this day was created. A second
    /// flush on the same day is a no-op (one ping per day).
    pub fn write(&self, day: u64, ping: &Ping) -> Result<bool, String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| format!("{}: {error}", self.dir.display()))?;
        let path = self.dir.join(format!("rollup-{day}.json"));
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        let body = ping_json(ping)?;
        file.write_all(body.as_bytes())
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(true)
    }

    pub fn path_for_day(&self, day: u64) -> PathBuf {
        self.dir.join(format!("rollup-{day}.json"))
    }

    /// Used by Settings > Privacy: renders the exact bytes of the next ping.
    pub fn render_pending(ping: &Ping) -> Result<String, String> {
        ping_json(ping)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AgentStats, Arch, Distro};
    use uuid::Uuid;

    fn ping_for_test() -> Ping {
        Ping {
            schema_version: 1,
            app_version: "0.1.0".into(),
            distro: Distro::Arch,
            arch: Arch::X86_64,
            install_id: Uuid::from_u128(7),
            agent: Some(AgentStats {
                enabled: true,
                harness: Vec::new(),
                sessions: crate::schema::Bucket::new(0),
                calls_by_action: Default::default(),
                snapshot_token_ratio: crate::schema::Bucket::new(0),
                snapshot_ms: crate::schema::Bucket::new(0),
                element_count: crate::schema::Bucket::new(0),
                ref_resolve_success: crate::schema::Bucket::new(0),
                errors_by_code: Default::default(),
                plugins_installed: crate::schema::Bucket::new(0),
            }),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "anarc-telemetry-rollup-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn writes_once_per_day_with_canonical_bytes() {
        let dir = temp_dir("once");
        let store = RollupStore::new(dir.clone());
        let ping = ping_for_test();
        let expected = RollupStore::render_pending(&ping).expect("render");
        let day = 20_000;
        assert!(store.write(day, &ping).expect("first write"));
        assert!(!store.write(day, &ping).expect("second write"));
        assert!(store.write(day + 1, &ping).expect("next day"));
        let written = std::fs::read_to_string(store.path_for_day(day)).expect("read");
        assert_eq!(written, expected);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
