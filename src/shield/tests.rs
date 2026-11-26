use super::*;

fn enable_shield_plugin() -> std::sync::MutexGuard<'static, ()> {
    // Hold the shared feature-flag lock for the whole test: plugin-lifecycle
    // tests toggle these process-wide switches off while running in parallel.
    let guard = crate::plugins::GLOBAL_FEATURE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    set_plugin_enabled(true);
    guard
}

#[test]
fn https_first_upgrades_public_http_hosts_only() {
    let _global_guard = enable_shield_plugin();
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
        "Anarc shield match cost: {:.3} µs/request",
        started.elapsed().as_secs_f64() * 1_000_000.0 / 10_000.0
    );
}

#[test]
fn bundled_rules_block_and_increment_per_tab_counts() {
    let _global_guard = enable_shield_plugin();
    let state = ShieldState::start();
    let url = "https://doubleclick.net/ad.js";
    let source = "https://publisher.test/";
    assert!(state.should_block(url, source, "script", "GET", "tab-a",));
    let started = std::time::Instant::now();
    for _ in 0..500 {
        assert!(state.should_block(url, source, "script", "GET", "tab-a"));
    }
    eprintln!(
        "Anarc shield request match (worker handoff): {:.3} µs/request",
        started.elapsed().as_secs_f64() * 1_000_000.0 / 500.0
    );
    assert_eq!(state.blocked_counts().get("tab-a"), Some(&501));
}

#[test]
fn bundled_easylist_blocks_ad_scripts_beyond_the_starter_rules() {
    let engine = adblock::Engine::new_with_list_text(
        include_str!("../../assets/filters/easylist.txt").to_owned(),
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
fn bundled_lists_mirror_the_ubol_rulesets() {
    let engine = build_filter_engine(&[], &[]);
    for url in [
        "https://www.google-analytics.com/analytics.js",
        "https://cdn.taboola.com/libtrc/publisher/loader.js",
        "https://ads.pubmatic.com/AdServer/js/showad.js",
        "https://static.criteo.net/js/ld/publishertag.js",
    ] {
        let request =
            adblock::request::Request::new(url, "https://publisher.test/", "script", "GET")
                .unwrap();
        assert!(
            engine.check_network_request(&request).should_block(),
            "{url}"
        );
    }
}

#[test]
fn cosmetic_rules_hide_inline_ad_containers() {
    let engine = build_filter_engine(&["example.com##.sponsored-slot".to_owned()], &[]);
    let injection = cosmetic_injection_for(&engine, "https://example.com/article")
        .expect("cosmetic rules for example.com");
    assert!(
        injection.css.contains(".sponsored-slot"),
        "{}",
        injection.css
    );
    assert!(injection.css.ends_with("{display:none !important}"));
    let script = injection.to_script();
    assert!(script.contains("anarc-shield-cosmetic"));
    assert!(script.contains(".sponsored-slot"));
}

#[test]
fn cosmetic_script_is_cached_per_host_and_respects_global_toggle() {
    let _global_guard = enable_shield_plugin();
    let state = ShieldState::start();
    let url = "https://www.youtube.com/watch?v=123";
    for _ in 0..500 {
        if state.engine_ready.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(state.cosmetic_script(url).is_some());
    assert!(state.cosmetic_script(url).is_some());
    state.set_enabled(false);
    assert!(state.cosmetic_script(url).is_none());
    state.set_enabled(true);
    assert!(state.cosmetic_script(url).is_some());
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
    let _global_guard = enable_shield_plugin();
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
    assert!(script.contains("v.readyState>=2"));
    assert!(script.contains("__anarc_ytInAd"));
}

#[test]
fn youtube_ad_shield_does_not_click_generic_dialog_buttons() {
    let script = youtube_ad_shield_script();
    // Generic dialog auto-clicking loops with MutationObserver and hangs
    // user flows such as YouTube's "Create a channel" modal.
    assert!(!script.contains("document.querySelectorAll('tp-yt-paper-dialog,"));
    assert!(script.contains("__anarc_ytDismissed"));
    assert!(script.contains("pendingSkip"));
}

#[test]
fn ad_blocker_defaults_on_and_global_toggle_controls_filtering() {
    let _global_guard = enable_shield_plugin();
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
