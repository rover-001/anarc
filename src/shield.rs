//! Network blocking and HTTPS-first navigation policy.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use adblock::lists::{FilterSet, ParseOptions};

static PLUGIN_ENABLED: AtomicBool = AtomicBool::new(true);

pub(crate) fn set_plugin_enabled(enabled: bool) {
    PLUGIN_ENABLED.store(enabled, Ordering::Relaxed);
}

pub struct ShieldState {
    matcher: SyncSender<MatcherMessage>,
    allowlist: Mutex<HashSet<String>>,
    blocked_by_tab: Mutex<HashMap<String, u64>>,
    cache: Mutex<HashMap<u64, bool>>,
    enabled: AtomicBool,
}

struct MatchRequest {
    url: String,
    source_url: String,
    resource_type: String,
    method: String,
    reply: SyncSender<bool>,
}

enum MatcherMessage {
    Check(MatchRequest),
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
        let state = Arc::new(Self {
            matcher,
            allowlist: Mutex::new(allowlist),
            blocked_by_tab: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::with_capacity(4096)),
            enabled: AtomicBool::new(true),
        });
        if let Err(error) = std::thread::Builder::new()
            .name("lumen-filter-engine".into())
            .spawn(move || {
                let user_lists = read_user_filter_lists();
                let mut pack_lists = initial_pack_lists;
                let mut engine = build_filter_engine(&user_lists, &pack_lists);
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
                            .is_some_and(|request| {
                                engine.check_network_request(&request).should_block()
                            });
                            let _ = request.reply.send(blocked);
                        }
                        MatcherMessage::ReplacePackLists(lists) => {
                            pack_lists = lists;
                            engine = build_filter_engine(&user_lists, &pack_lists);
                        }
                    }
                }
            })
        {
            eprintln!("Lumen filter engine thread failed to start: {error}");
        }
        if let Ok(mut instances) = shield_instances().lock() {
            instances.push(Arc::downgrade(&state));
        }
        state
    }

    /// Replace only plugin-provided rules in this worker. Bundled EasyList,
    /// Lumen's starter list and user filter files are rebuilt alongside them.
    pub(crate) fn replace_pack_filter_lists(&self, lists: Vec<String>) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
        if let Err(error) = self
            .matcher
            .try_send(MatcherMessage::ReplacePackLists(lists))
        {
            eprintln!("Lumen could not queue pack filter update: {error}");
        }
    }

    pub fn should_block(
        &self,
        url: &str,
        source_url: &str,
        resource_type: &str,
        method: &str,
        tab_id: &str,
    ) -> bool {
        if !PLUGIN_ENABLED.load(Ordering::Relaxed) || !self.enabled.load(Ordering::Relaxed) {
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
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            url.hash(&mut hasher);
            source_url.hash(&mut hasher);
            resource_type.hash(&mut hasher);
            method.hash(&mut hasher);
            hasher.finish()
        };
        if let Ok(cache) = self.cache.lock()
            && let Some(&blocked) = cache.get(&hash) {
                if blocked {
                    if let Ok(mut counts) = self.blocked_by_tab.lock() {
                        *counts.entry(tab_id.to_owned()).or_default() += 1;
                    }
                    crate::stats::record_blocked_request(tab_id);
                }
                return blocked;
            }
        let (reply, result) = mpsc::sync_channel(1);
        let query = MatchRequest {
            url: url.to_owned(),
            source_url: source_url.to_owned(),
            resource_type: resource_type.to_owned(),
            method: method.to_owned(),
            reply,
        };
        let blocked = self.matcher.send(MatcherMessage::Check(query)).is_ok()
            && result.recv().unwrap_or(false);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= 8192 {
                cache.clear();
            }
            cache.insert(hash, blocked);
        }
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
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn toggle_site(&self, host: &str) -> bool {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
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
                eprintln!("Lumen cannot read filter list {}: {error}", path.display());
                None
            }
        })
        .collect()
}

fn build_filter_engine(user_lists: &[String], pack_lists: &[String]) -> adblock::Engine {
    let mut filters = FilterSet::new(false);
    filters.add_filter_list(
        include_str!("../assets/filters/easylist.txt").to_owned(),
        ParseOptions::default(),
    );
    filters.add_filter_list(
        include_str!("../assets/filters/lumen.txt").to_owned(),
        ParseOptions::default(),
    );
    for text in user_lists.iter().chain(pack_lists) {
        filters.add_filter_list(text.clone(), ParseOptions::default());
    }
    adblock::Engine::new_with_filter_set(filters)
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
    (PLUGIN_ENABLED.load(Ordering::Relaxed) && eligible_for_https(url))
        .then(|| format!("https{}", &url[4..]))
}

pub fn is_youtube_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("youtube.com") || lower.contains("youtu.be")
}

pub fn youtube_ad_shield_script() -> &'static str {
    r#"(()=>{
if(window.__anarc_ublock_yt)return;
window.__anarc_ublock_yt=true;
try{
const s=document.createElement('style');
s.id='anarc-ublock-yt';
s.textContent=`
.video-ads, .ytp-ad-module, .ytp-ad-overlay-container, .ytp-ad-player-overlay,
.ytp-ad-action-interstitial, .ytp-ad-skip-button-slot, .ytp-ad-preview-container,
ytd-ad-slot-renderer, #masthead-ad, ytd-banner-promo-renderer,
ytd-in-feed-ad-layout-renderer, ytd-promoted-video-renderer,
ytd-display-ad-renderer, #player-ads, ytd-statement-banner-renderer,
ytd-popup-container:has(ytd-enforcement-message-view-model),
tp-yt-paper-dialog:has(#feedback), .ytd-merch-shelf-renderer,
#companion, .ad-showing{display:none!important}
`;
(document.head||document.documentElement).appendChild(s);

function scrub(obj){
if(!obj||typeof obj!=='object')return;
delete obj.adPlacements;
delete obj.playerAds;
delete obj.adSlots;
if(obj.playbackTracking)delete obj.playbackTracking.adPlacements;
}
if(window.ytInitialPlayerResponse)scrub(window.ytInitialPlayerResponse);

function skipAds(){
const mp=document.getElementById('movie_player')||document.querySelector('.html5-video-player');
const isAd=mp&&(mp.classList.contains('ad-showing')||mp.classList.contains('ad-interrupting'));
const v=document.querySelector('video.html5-main-video')||document.querySelector('video');
if(isAd||document.querySelector('.ytp-ad-module:not(:empty)')){
if(v&&!isNaN(v.duration)&&isFinite(v.duration)&&v.duration>0){
v.muted=true;
v.playbackRate=16.0;
v.currentTime=v.duration;
}
const btns=document.querySelectorAll('.ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-skip-ad-button, .ytp-ad-skip-button-container button');
btns.forEach(b=>b&&b.click());
}
const dialogs=document.querySelectorAll('tp-yt-paper-dialog, ytd-enforcement-message-view-model');
dialogs.forEach(d=>{
const b=d.querySelector('yt-button-shape button, #dismiss-button');
if(b)b.click();
});
}
setInterval(skipAds,200);
const ob=new MutationObserver(skipAds);
ob.observe(document.body||document.documentElement,{childList:true,subtree:true});
}catch(e){}
})();"#
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enable_shield_plugin() {
        set_plugin_enabled(true);
    }

    #[test]
    fn https_first_upgrades_public_http_hosts_only() {
        enable_shield_plugin();
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
        enable_shield_plugin();
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
    fn replacing_pack_filters_preserves_bundled_and_user_lists() {
        let user = vec!["||user-filter.example^".to_owned()];
        let with_pack = build_filter_engine(&user, &["||pack-filter.example^".to_owned()]);
        for url in [
            "https://doubleclick.net/ad.js",
            "https://user-filter.example/ad.js",
            "https://pack-filter.example/ad.js",
        ] {
            let request =
                adblock::request::Request::new(url, "https://publisher.test/", "script", "GET")
                    .unwrap();
            assert!(
                with_pack.check_network_request(&request).should_block(),
                "{url}"
            );
        }

        let after_pack_unmount = build_filter_engine(&user, &[]);
        for url in [
            "https://doubleclick.net/ad.js",
            "https://user-filter.example/ad.js",
        ] {
            let request =
                adblock::request::Request::new(url, "https://publisher.test/", "script", "GET")
                    .unwrap();
            assert!(
                after_pack_unmount
                    .check_network_request(&request)
                    .should_block(),
                "{url}"
            );
        }
        let removed = adblock::request::Request::new(
            "https://pack-filter.example/ad.js",
            "https://publisher.test/",
            "script",
            "GET",
        )
        .unwrap();
        assert!(
            !after_pack_unmount
                .check_network_request(&removed)
                .should_block()
        );
    }

    #[test]
    fn worker_replaces_pack_rules_without_disabling_builtins() {
        enable_shield_plugin();
        let state = ShieldState::start();
        state.replace_pack_filter_lists(vec!["||worker-pack-filter.example^".into()]);
        assert!(state.should_block(
            "https://worker-pack-filter.example/ad.js",
            "https://publisher.test/",
            "script",
            "GET",
            "pack-test"
        ));
        assert!(state.should_block(
            "https://doubleclick.net/ad.js",
            "https://publisher.test/",
            "script",
            "GET",
            "pack-test"
        ));

        state.replace_pack_filter_lists(Vec::new());
        assert!(!state.should_block(
            "https://worker-pack-filter.example/ad.js",
            "https://publisher.test/",
            "script",
            "GET",
            "pack-test"
        ));
        assert!(state.should_block(
            "https://doubleclick.net/ad.js",
            "https://publisher.test/",
            "script",
            "GET",
            "pack-test"
        ));
    }

    #[test]
    fn youtube_url_detection_matches_domains() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=123"));
        assert!(is_youtube_url("https://youtu.be/123"));
        assert!(!is_youtube_url("https://example.com/watch?v=123"));
    }

    #[test]
    fn youtube_ad_shield_script_contains_ad_classes_and_skip_logic() {
        let script = youtube_ad_shield_script();
        assert!(script.contains(".ytp-ad-module"));
        assert!(script.contains("skipAds"));
        assert!(script.contains("playbackRate=16.0"));
    }

    #[test]
    fn ad_blocker_defaults_on_and_global_toggle_controls_filtering() {
        enable_shield_plugin();
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
