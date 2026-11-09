---
type: log
---
# Memory Log

## Session 2026-11-09T11:56:23.409753060+00:00

Implemented centralized test-safe Lumen paths and opt-in once-per-second stats. cargo test --offline passed 20 tests; debug and release builds passed. Safe live run was blocked by inaccessible X server; commit was blocked by read-only shared Git metadata. Wrote infra report.

## Session 2026-11-09T12:11:36.749048586+00:00

Resolved ux merge conflicts in src/main.rs and src/engine/mod.rs, preserving infra/engine and UX handlers and state updates. Fixed moved download path compile error. Offline tests passed (24), build and formatting checks passed, no src conflict markers. Report: /home/rover/Projects/kreon/briefs/integrate-report.md. Live CEF behavior unverified.

## Session 2026-11-09T12:17:38.128542876+00:00

Round 2 integrated perf WebViewport and on_schedule_message_pump_work with infra/engine/ux. Preserved stats, paths, screen metrics, all CEF handlers, sidebar effective sizing, mouse back/forward, keyboard shortcuts, set_visible, and retired image eviction. Popup events drain in pump. Offline tests passed (28), debug and release builds passed, src has no conflict markers, formatting check passed. Appended Round 2 to /home/rover/Projects/kreon/briefs/integrate-report.md. Live CEF interactions unverified.

## Session 2026-11-09T12:20:58.240823179+00:00

Round 3 fixed CEF cache nesting: root_cache_path uses LUMEN_DATA_DIR/cef and per-space RequestContext cache paths use root/profiles/<space_id>. Updated path tests and README Testing guidance; LUMEN_CACHE_DIR remains for non-profile caches. cargo test --offline passed (29), cargo build --release --offline passed, fmt check passed. Round 3 appended to integrate-report.md. Live cache persistence remains unverified.

## Session 2026-11-09T12:58:15.089199794+00:00

Implemented Phase 2 shield: adblock-backed CEF request filtering with persisted per-site exceptions and per-tab stats; HTTPS-first GET navigation with one-time per-host fallback. Verified 34 tests, release build, benchmark, and isolated live blocking run. Parent owns commit; live HTTPS failure fallback remains unverified.

## Session 2026-11-09T13:46:58.935580177+00:00

Implemented grouped split tabs, favicon rendering, per-tab hover close, and tab context actions in the tabs worktree; tests/build had completed before the parent stop instruction, and the final occlude adjustment remains unverified.

## Session 2026-11-09T14:01:38.311588559+00:00

Completed favicon pipeline instrumentation and explicit root/sidebar wake, split group chevron hover and expanded state, symmetric fixed icon slots for group/member/single rows, and favicon origin URL unit coverage. Offline cargo checks passed; live browser check not run per brief. Wrote briefs/tabfix-report.md.

## Session 2026-11-09T14:06:04.066522303+00:00

Aligned sidebar tab, split, and new-tab rows with shared padding, icon, gap, and vertical-centering metrics. Offline checks passed; no live visual run.

## Session 2026-11-09T14:10:40.539612811+00:00

Resolved merge conflicts in src/main.rs and src/ui/sidebar.rs, preserving aligned sidebar row metrics and applying SpaceTheme colors. Offline cargo checks, rustfmt, diff check, and conflict marker scan passed. Wrote merge4 report; no commit.

## Session 2026-11-09T14:12:37.214071957+00:00

Resolved merge5 conflicts in engine, main UI, and web viewport; preserved SpaceTheme rendering and splitfix sizing/resizing behavior. No conflict markers remain. Formatting and diff checks passed; compile/live verification not run per task instructions.

## Session 2026-11-09T14:26:47.927505400+00:00

Fixed CEF DIP sizing and input coordinates, 8 ms split resizing, clipped rounded web frames, split member row flow, and persisted sidebar width/collapse preferences with a serde round-trip test. Cargo checks and diff formatting passed; live verification not run per brief.

## Session 2026-11-09T14:44:46.483401763+00:00

Fixed autocomplete overlay stacking, synced the address bar on tab and space changes, reset and focused it for new tabs, made split rendering depend on active pair membership while retaining sidebar grouping, and set new and reset page zoom to 90%. Added GPUI regression and zoom tests. cargo check --offline and cargo check --tests --offline passed; tests and live UI were not run per brief. Wrote briefs/omni-report.md.

## Session 2026-11-09T15:27:12.590124432+00:00

Resolved the main.rs merge import conflict retaining omni Focusable and motion Animation imports; web_view corner rings and frame transitions are preserved. Implemented spring-driven active row highlight and chevron, 30ms split-member stagger, closing-row collapse, autocomplete enter/exit, and sidebar width transition with CEF frozen to max frame size then resized once at completion. cargo check --offline and --tests --offline passed; no tests/live run. Updated briefs/motion-report.md. hc send t20 was attempted but Core unavailable due read-only filesystem.

## Session 2026-11-09T15:32:15.720233677+00:00

Fixed animated percentage/opacity clamping with NaN handling and added a unit test; moved per-space CEF profile directories to sanitized direct children of root cache, updated README and paths tests. cargo check --offline and cargo check --tests --offline passed; runtime/tests not run per instruction.

## Session 2026-11-09T15:58:15.177162292+00:00

Implemented resize frame crossfade handoff, corrected sidebar highlight geometry, and added coordinated space-switch transitions. Offline cargo checks and diff check pass; runtime behavior and tests remain unverified.

## Session 2026-11-09T16:16:08.213404544+00:00

Removed border from search bar and added back and forward navigation icon buttons to the left of the search bar

## Session 2026-11-09T16:24:18.238805967+00:00

Centered sidebar resize handle directly over the visual cut at sidebar_width + CONTENT_BEZEL_X on root overlay to eliminate hit target occlusion and offset

## Session 2026-11-09T16:34:15.770435114+00:00

Ensured address bar syncs to current tab URL upon back/forward navigation and auto-hides autocomplete suggestions when focus moves out of the address bar or outside clicked

## Session 2026-11-09T16:55:50.260279347+00:00

Fixed new tab 90% zoom retention across creation/load, fixed web view width adaptation and horizontal scrollbars by syncing CEF layout bounds and screen info notification, and unlocked high-refresh display FPS (up to 240Hz, auto-detecting 144Hz via Hyprland/xrandr) with tuned pump timer.

## Session 2026-11-09T17:15:42.350968289+00:00

Fixed input typing for period (.) and all OEM punctuation/symbol keys by implementing windows_key_code_for_key with VK_OEM codes instead of ASCII clashes (e.g. 0x2e VK_DELETE), with fallback character encoding; and fixed website favicons by resolving relative/absolute/data URLs, supporting cached/304 HTTP responses, adding raw bitmap extraction fallback, disk caching, and immediate tab row favicon hydration.

## Session 2026-11-09T17:20:12.670969535+00:00

Updated default tab zoom factor to 80% (0.8) in src/ui/visual.rs and switched default search engine from DuckDuckGo to Google in src/main.rs (including search suggestions, address bar normalization, empty query search home, and title handling)

## Session 2026-11-09T17:36:18.224713013+00:00

Fixed favicon site switching and stale icon issues: (1) Added host-tracking to BrowserSharedState (favicon_host, current_url, current_host) to invalidate and reload site icons immediately when navigating or switching sites; (2) Added favicon_url_score to rank modern high-res PNG/SVG/touch icons over legacy 16x16 ICOs; (3) Fixed LumenFaviconCallback to save favicons under the tab's page host rather than third-party CDN hosts (like gstatic or webflow); (4) Made save_favicon_to_disk and load_favicon_from_disk handle www and bare domains symmetrically.
