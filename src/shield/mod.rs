//! Network blocking and HTTPS-first navigation policy.

mod https;
#[cfg(test)]
mod tests;
mod youtube;

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use adblock::lists::{FilterSet, ParseOptions};

pub use https::{upgrade_http_url, url_host};
pub use youtube::{is_youtube_url, youtube_ad_shield_script};

static PLUGIN_ENABLED: AtomicBool = AtomicBool::new(true);

pub(crate) fn set_plugin_enabled(enabled: bool) {
    PLUGIN_ENABLED.store(enabled, Ordering::Relaxed);
}

pub struct ShieldState {
    matcher: SyncSender<MatcherMessage>,
    allowlist: Mutex<HashSet<String>>,
    blocked_by_tab: Mutex<HashMap<String, u64>>,
    cache: Mutex<HashMap<u64, bool>>,
    cosmetic_cache: Mutex<HashMap<String, Option<Arc<CosmeticInjection>>>>,
    engine_ready: Arc<AtomicBool>,
    enabled: AtomicBool,
}

struct MatchRequest {
    url: String,
    source_url: String,
    resource_type: String,
    method: String,
    reply: SyncSender<bool>,
}

struct CosmeticRequest {
    url: String,
    reply: SyncSender<Option<CosmeticInjection>>,
}

/// Element-hiding rules for one page, mirroring uBlock Origin's cosmetic
/// filtering stage: a stylesheet of selectors plus optional scriptlets.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CosmeticInjection {
    pub css: String,
    pub script: String,
}

impl CosmeticInjection {
    fn is_empty(&self) -> bool {
        self.css.is_empty() && self.script.is_empty()
    }

    /// Self-contained snippet that installs or refreshes the page stylesheet
    /// and runs any scriptlet payload once per document.
    fn to_script(&self) -> String {
        let css = serde_json::to_string(&self.css).unwrap_or_else(|_| "\"\"".to_owned());
        let mut script = format!(
            "(()=>{{const css={css};if(!css)return;const id='anarc-shield-cosmetic';\
let s=document.getElementById(id);if(!s){{s=document.createElement('style');s.id=id;\
(document.head||document.documentElement).appendChild(s);}}s.textContent=css;}})();"
        );
        if !self.script.is_empty() {
            script.push_str(&self.script);
        }
        script
    }
}

enum MatcherMessage {
    Check(MatchRequest),
    Cosmetic(CosmeticRequest),
    ReplacePackLists(Vec<String>),
}

static ACTIVE_PACK_FILTER_LISTS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static SHIELD_INSTANCES: OnceLock<Mutex<Vec<Weak<ShieldState>>>> = OnceLock::new();

fn active_pack_filter_lists() -> &'static Mutex<Vec<String>> {
    ACTIVE_PACK_FILTER_LISTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn shield_instances() -> &'static Mutex<Vec<Weak<ShieldState>>> {
    SHIELD_INSTANCES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Replace the complete set of active pack filter rules and fan the snapshot
/// out to live shield workers. No user-owned or bundled lists are removed.
pub(crate) fn replace_active_pack_filter_lists(lists: Vec<String>) {
    if let Ok(mut active) = active_pack_filter_lists().lock() {
        *active = lists.clone();
    }
    let instances = shield_instances().lock().map(|mut instances| {
        let mut active = Vec::new();
        instances.retain(|weak| {
            if let Some(instance) = weak.upgrade() {
                active.push(instance);
                true
            } else {
                false
            }
        });
        active
    });
    if let Ok(instances) = instances {
        for instance in instances {
            instance.replace_pack_filter_lists(lists.clone());
        }
    }
}

impl ShieldState {
    pub fn start() -> Arc<Self> {
        let allowlist = read_allowlist();
        let (matcher, requests) = mpsc::sync_channel::<MatcherMessage>(256);
        let initial_pack_lists = active_pack_filter_lists()
            .lock()
            .map(|lists| lists.clone())
            .unwrap_or_default();
        let engine_ready = Arc::new(AtomicBool::new(false));
        let worker_ready = Arc::clone(&engine_ready);
        let state = Arc::new(Self {
            matcher,
            allowlist: Mutex::new(allowlist),
            blocked_by_tab: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::with_capacity(4096)),
            cosmetic_cache: Mutex::new(HashMap::new()),
            engine_ready,
            enabled: AtomicBool::new(true),
        });
        if let Err(error) = std::thread::Builder::new()
            .name("lumen-filter-engine".into())
            .spawn(move || run_filter_worker(requests, initial_pack_lists, worker_ready))
        {
            eprintln!("Anarc filter engine thread failed to start: {error}");
        }
        if let Ok(mut instances) = shield_instances().lock() {
            instances.push(Arc::downgrade(&state));
        }
        state
    }

    fn is_active(&self) -> bool {
        PLUGIN_ENABLED.load(Ordering::Relaxed) && self.enabled.load(Ordering::Relaxed)
    }

    fn is_allowlisted(&self, host: &str) -> bool {
        self.allowlist
            .lock()
            .is_ok_and(|allowlist| allowlist.contains(host))
    }

    fn clear_caches(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
        if let Ok(mut cache) = self.cosmetic_cache.lock() {
            cache.clear();
        }
    }

    fn record_blocked(&self, tab_id: &str) {
        if let Ok(mut counts) = self.blocked_by_tab.lock() {
            if counts.len() >= 4096 {
                counts.clear();
            }
            *counts.entry(tab_id.to_owned()).or_default() += 1;
        }
    }

    /// Replace only plugin-provided rules in this worker. Bundled EasyList,
    /// Anarc's starter list and user filter files are rebuilt alongside them.
    pub(crate) fn replace_pack_filter_lists(&self, lists: Vec<String>) {
        self.clear_caches();
        if let Err(error) = self
            .matcher
            .try_send(MatcherMessage::ReplacePackLists(lists))
        {
            eprintln!("Anarc could not queue pack filter update: {error}");
        }
    }

    /// Element-hiding rules for a page. The result is cached per hostname so
    /// repeat loads and SPA navigations do not pay for the worker round-trip.
    pub fn cosmetic_injection(&self, url: &str) -> Option<CosmeticInjection> {
        if !self.is_active() {
            return None;
        }
        // A page that loads while the worker is still parsing the bundled lists
        // is skipped instead of freezing the UI; a later load retries.
        if !self.engine_ready.load(Ordering::Relaxed) {
            return None;
        }
        let host = url_host(url)?;
        if self.is_allowlisted(&host) {
            return None;
        }
        if let Ok(cache) = self.cosmetic_cache.lock()
            && let Some(cached) = cache.get(&host)
        {
            return cached.as_ref().map(|injection| (**injection).clone());
        }
        let (reply, result) = mpsc::sync_channel(1);
        let query = CosmeticRequest {
            url: url.to_owned(),
            reply,
        };
        // Called from the UI thread while a page finishes loading, so never
        // block on a worker that has stopped draining its queue.
        let injection = if self.matcher.send(MatcherMessage::Cosmetic(query)).is_ok() {
            result
                .recv_timeout(std::time::Duration::from_millis(250))
                .ok()
                .flatten()
        } else {
            None
        }
        .filter(|injection| !injection.is_empty());
        if let Ok(mut cache) = self.cosmetic_cache.lock() {
            if cache.len() >= 2048 {
                cache.clear();
            }
            cache.insert(host, injection.clone().map(Arc::new));
        }
        injection
    }

    /// Ready-to-execute snippet that hides ad containers on the page.
    pub fn cosmetic_script(&self, url: &str) -> Option<String> {
        self.cosmetic_injection(url)
            .map(|injection| injection.to_script())
    }

    pub fn should_block(
        &self,
        url: &str,
        source_url: &str,
        resource_type: &str,
        method: &str,
        tab_id: &str,
    ) -> bool {
        if !self.is_active() {
            return false;
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return false;
        }
        if let Some(host) = url_host(url)
            && self.is_allowlisted(&host)
        {
            return false;
        }
        let hash = request_hash(url, source_url, resource_type, method);
        if let Ok(cache) = self.cache.lock()
            && let Some(&blocked) = cache.get(&hash)
        {
            if blocked {
                self.record_blocked(tab_id);
            }
            return blocked;
        }
        let blocked = self.query_worker_block(url, source_url, resource_type, method);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= 8192 {
                cache.clear();
            }
            cache.insert(hash, blocked);
        }
        if blocked {
            self.record_blocked(tab_id);
        }
        blocked
    }

    fn query_worker_block(
        &self,
        url: &str,
        source_url: &str,
        resource_type: &str,
        method: &str,
    ) -> bool {
        let (reply, result) = mpsc::sync_channel(1);
        let query = MatchRequest {
            url: url.to_owned(),
            source_url: source_url.to_owned(),
            resource_type: resource_type.to_owned(),
            method: method.to_owned(),
            reply,
        };
        self.matcher.send(MatcherMessage::Check(query)).is_ok() && result.recv().unwrap_or(false)
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
        self.clear_caches();
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn toggle_site(&self, host: &str) -> bool {
        self.clear_caches();
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
            eprintln!("Anarc cannot save shield settings: {error}");
        }
        enabled
    }

    pub fn site_protection_enabled(&self, host: &str) -> bool {
        !self.is_allowlisted(&host.to_ascii_lowercase())
    }
}

fn run_filter_worker(
    requests: Receiver<MatcherMessage>,
    initial_pack_lists: Vec<String>,
    worker_ready: Arc<AtomicBool>,
) {
    let user_lists = read_user_filter_lists();
    let mut pack_lists = initial_pack_lists;
    let mut engine = build_filter_engine(&user_lists, &pack_lists);
    worker_ready.store(true, Ordering::Relaxed);
    while let Ok(message) = requests.recv() {
        match message {
            MatcherMessage::Check(request) => {
                let blocked = adblock::request::Request::new(
                    &request.url,
                    &request.source_url,
                    &request.resource_type,
                    &request.method,
                )
                .ok()
                .is_some_and(|req| engine.check_network_request(&req).should_block());
                let _ = request.reply.send(blocked);
            }
            MatcherMessage::Cosmetic(request) => {
                let _ = request
                    .reply
                    .send(cosmetic_injection_for(&engine, &request.url));
            }
            MatcherMessage::ReplacePackLists(lists) => {
                worker_ready.store(false, Ordering::Relaxed);
                pack_lists = lists;
                engine = build_filter_engine(&user_lists, &pack_lists);
                worker_ready.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn request_hash(url: &str, source_url: &str, resource_type: &str, method: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    source_url.hash(&mut hasher);
    resource_type.hash(&mut hasher);
    method.hash(&mut hasher);
    hasher.finish()
}

fn read_user_filter_lists() -> Vec<String> {
    let user_lists = crate::paths::data_dir().join("filters");
    let Ok(entries) = std::fs::read_dir(&user_lists) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect::<Vec<PathBuf>>();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(error) => {
                eprintln!("Anarc cannot read filter list {}: {error}", path.display());
                None
            }
        })
        .collect()
}

/// Bundled lists. These mirror the filter sets enabled in the user's
/// uBlock Origin Lite install: EasyList, EasyPrivacy, uBlock filters, uBlock
/// badware risks and Peter Lowe's list. uBlock Origin Lite itself cannot run
/// here because this build uses CEF's Alloy runtime, which has no extension
/// support, so the equivalent lists are compiled into the built-in shield.
const BUNDLED_FILTER_LISTS: &[&str] = &[
    include_str!("../../assets/filters/easylist.txt"),
    include_str!("../../assets/filters/easyprivacy.txt"),
    include_str!("../../assets/filters/ublock-filters.txt"),
    include_str!("../../assets/filters/ublock-badware.txt"),
    include_str!("../../assets/filters/pgl.txt"),
    include_str!("../../assets/filters/anarc.txt"),
];

pub(super) fn build_filter_engine(user_lists: &[String], pack_lists: &[String]) -> adblock::Engine {
    let mut filters = FilterSet::new(false);
    for text in BUNDLED_FILTER_LISTS {
        filters.add_filter_list((*text).to_owned(), ParseOptions::default());
    }
    for text in user_lists.iter().chain(pack_lists) {
        filters.add_filter_list(text.clone(), ParseOptions::default());
    }
    adblock::Engine::new_with_filter_set(filters)
}

/// Mirrors uBlock's cosmetic stage: collect the hide selectors that apply to
/// this page and turn them into one stylesheet plus any scriptlet payload.
pub(super) fn cosmetic_injection_for(
    engine: &adblock::Engine,
    url: &str,
) -> Option<CosmeticInjection> {
    let resources = engine.url_cosmetic_resources(url);
    let mut injection = CosmeticInjection::default();
    if !resources.hide_selectors.is_empty() {
        let mut selectors = resources
            .hide_selectors
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        selectors.sort_unstable();
        injection.css = format!("{}{{display:none !important}}", selectors.join(","));
    }
    injection.script = resources.injected_script;
    (!injection.is_empty()).then_some(injection)
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
