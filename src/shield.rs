//! Network blocking and HTTPS-first navigation policy.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};

use adblock::lists::{FilterSet, ParseOptions};

pub struct ShieldState {
    matcher: SyncSender<MatchRequest>,
    allowlist: Mutex<HashSet<String>>,
    blocked_by_tab: Mutex<HashMap<String, u64>>,
    enabled: AtomicBool,
}

struct MatchRequest {
    url: String,
    source_url: String,
    resource_type: String,
    method: String,
    reply: SyncSender<bool>,
}

impl ShieldState {
    pub fn start() -> Arc<Self> {
        let allowlist = read_allowlist();
        let (matcher, requests) = mpsc::sync_channel::<MatchRequest>(256);
        let state = Arc::new(Self {
            matcher,
            allowlist: Mutex::new(allowlist),
            blocked_by_tab: Mutex::new(HashMap::new()),
            enabled: AtomicBool::new(true),
        });
        if let Err(error) = std::thread::Builder::new()
            .name("lumen-filter-engine".into())
            .spawn(move || {
                let mut filters = FilterSet::new(false);
                filters.add_filter_list(
                    include_str!("../assets/filters/easylist.txt").to_owned(),
                    ParseOptions::default(),
                );
                filters.add_filter_list(
                    include_str!("../assets/filters/lumen.txt").to_owned(),
                    ParseOptions::default(),
                );
                let user_lists = crate::paths::data_dir().join("filters");
                if let Ok(entries) = std::fs::read_dir(user_lists) {
                    let mut paths: Vec<PathBuf> = entries
                        .filter_map(Result::ok)
                        .map(|entry| entry.path())
                        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
                        .collect();
                    paths.sort();
                    for path in paths {
                        match std::fs::read_to_string(&path) {
                            Ok(text) => {
                                filters.add_filter_list(text, ParseOptions::default());
                            }
                            Err(error) => eprintln!(
                                "Lumen cannot read filter list {}: {error}",
                                path.display()
                            ),
                        }
                    }
                }
                let engine = adblock::Engine::new_with_filter_set(filters);
                while let Ok(request) = requests.recv() {
                    let blocked = adblock::request::Request::new(
                        &request.url,
                        &request.source_url,
                        &request.resource_type,
                        &request.method,
                    )
                    .ok()
                    .is_some_and(|request| engine.check_network_request(&request).should_block());
                    let _ = request.reply.send(blocked);
                }
            })
        {
            eprintln!("Lumen filter engine thread failed to start: {error}");
        }
        state
    }

    pub fn should_block(
        &self,
        url: &str,
        source_url: &str,
        resource_type: &str,
        method: &str,
        tab_id: &str,
    ) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }
        if let Some(host) = url_host(url)
            && self
                .allowlist
                .lock()
                .is_ok_and(|allowlist| allowlist.contains(&host))
        {
            return false;
        }
        let (reply, result) = mpsc::sync_channel(1);
        let query = MatchRequest {
            url: url.to_owned(),
            source_url: source_url.to_owned(),
            resource_type: resource_type.to_owned(),
            method: method.to_owned(),
            reply,
        };
        let blocked = self.matcher.send(query).is_ok() && result.recv().unwrap_or(false);
        if blocked {
            if let Ok(mut counts) = self.blocked_by_tab.lock() {
                *counts.entry(tab_id.to_owned()).or_default() += 1;
            }
            crate::stats::record_blocked_request(tab_id);
        }
        blocked
    }

    pub fn blocked_counts(&self) -> HashMap<String, u64> {
        self.blocked_by_tab
            .lock()
            .map(|counts| counts.clone())
            .unwrap_or_default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn toggle_site(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        let Ok(mut allowlist) = self.allowlist.lock() else {
            return false;
        };
        let enabled = if allowlist.remove(&host) {
            false
        } else {
            allowlist.insert(host);
            true
        };
        if let Err(error) = persist_allowlist(&allowlist) {
            eprintln!("Lumen cannot save shield settings: {error}");
        }
        enabled
    }

    pub fn site_protection_enabled(&self, host: &str) -> bool {
        self.allowlist
            .lock()
            .is_ok_and(|allowlist| !allowlist.contains(&host.to_ascii_lowercase()))
    }
}

fn allowlist_path() -> PathBuf {
    crate::paths::data_dir().join("shield.json")
}

fn read_allowlist() -> HashSet<String> {
    std::fs::read(allowlist_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<HashSet<String>>(&bytes).ok())
        .unwrap_or_default()
}

fn persist_allowlist(allowlist: &HashSet<String>) -> std::io::Result<()> {
    let path = allowlist_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(allowlist).map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}

pub fn url_host(url: &str) -> Option<String> {
    let (_, remainder) = url.split_once("://")?;
    let authority = remainder.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if authority.starts_with('[') {
        authority.split(']').next()?.trim_start_matches('[')
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

pub fn eligible_for_https(url: &str) -> bool {
    let Some(host) = url_host(url) else {
        return false;
    };
    if !url.starts_with("http://") {
        return false;
    }
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return false;
    }
    host.parse::<std::net::IpAddr>().is_err()
}

pub fn upgrade_http_url(url: &str) -> Option<String> {
    eligible_for_https(url).then(|| format!("https{}", &url[4..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_first_upgrades_public_http_hosts_only() {
        assert_eq!(
            upgrade_http_url("http://example.com/path?q=1"),
            Some("https://example.com/path?q=1".into())
        );
        for url in [
            "https://example.com",
            "http://localhost:8080/a",
            "http://127.0.0.1/a",
            "http://[::1]/",
            "http://router.local/",
        ] {
            assert_eq!(upgrade_http_url(url), None, "{url}");
        }
    }

    #[test]
    fn extracts_hosts_with_ports_and_userinfo() {
        assert_eq!(
            url_host("http://user:pass@Example.com:8080/path").as_deref(),
            Some("example.com")
        );
        assert_eq!(url_host("http://[::1]:8080/").as_deref(), Some("::1"));
    }

    #[test]
    fn filter_match_cost_is_reported_in_microseconds() {
        let engine = adblock::Engine::new_with_list_text("||ads.example^\n".to_owned());
        let request = adblock::request::Request::new(
            "https://ads.example/banner.js",
            "https://publisher.test",
            "script",
            "GET",
        )
        .unwrap();
        let started = std::time::Instant::now();
        for _ in 0..10_000 {
            assert!(engine.check_network_request(&request).should_block());
        }
        eprintln!(
            "Lumen shield match cost: {:.3} µs/request",
            started.elapsed().as_secs_f64() * 1_000_000.0 / 10_000.0
        );
    }

    #[test]
    fn bundled_rules_block_and_increment_per_tab_counts() {
        let state = ShieldState::start();
        let url = "https://doubleclick.net/ad.js";
        let source = "https://publisher.test/";
        assert!(state.should_block(url, source, "script", "GET", "tab-a",));
        let started = std::time::Instant::now();
        for _ in 0..500 {
            assert!(state.should_block(url, source, "script", "GET", "tab-a"));
        }
        eprintln!(
            "Lumen shield request match (worker handoff): {:.3} µs/request",
            started.elapsed().as_secs_f64() * 1_000_000.0 / 500.0
        );
        assert_eq!(state.blocked_counts().get("tab-a"), Some(&501));
    }

    #[test]
    fn bundled_easylist_blocks_ad_scripts_beyond_the_starter_rules() {
        let engine = adblock::Engine::new_with_list_text(
            include_str!("../assets/filters/easylist.txt").to_owned(),
        );
        let request = adblock::request::Request::new(
            "https://ads.google.com/google_adsense-banner.js",
            "https://publisher.test/",
            "script",
            "GET",
        )
        .unwrap();
        assert!(engine.check_network_request(&request).should_block());
    }

    #[test]
    fn ad_blocker_defaults_on_and_global_toggle_controls_filtering() {
        let state = ShieldState::start();
        let url = "https://doubleclick.net/ad.js";
        let source = "https://publisher.test/";
        assert!(state.enabled());
        assert!(state.should_block(url, source, "script", "GET", "tab-a"));

        state.set_enabled(false);
        assert!(!state.should_block(url, source, "script", "GET", "tab-a"));
        state.set_enabled(true);
        assert!(state.should_block(url, source, "script", "GET", "tab-a"));
    }
}
