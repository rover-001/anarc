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

## Session 2026-11-09T17:44:50.748556542+00:00

Fixed stale/non-live favicons in the CEF engine: get_favicon_image now treats engine navigation state (current_host/current_url) as authoritative instead of the sidebar tab model URL, which lags for background tabs and was resurrecting old disk-cached icons; added favicon_preferred provenance so fresh network favicon downloads replace stale cached icons while page-declared icons still outrank the low-res origin /favicon.ico fallback; added same_favicon_host/should_store_favicon helpers and unit tests. cargo check clean, all 15 engine tests pass; 2 unrelated pre-existing test failures confirmed on HEAD.

## Session 2026-11-09T17:54:50.492249168+00:00

Fixed favicon colour corruption: GPUI RenderImage frames are uploaded as BGRA, but favicon paths (CEF download callback, data-URL favicons, disk cache loader) built RenderImage from RGBA without channel swap, so red favicons rendered blue/violet (YouTube #FF0033 -> #3300FF), looking like an old/wrong icon. Added rgba_to_bgra_in_place and applied it at all three favicon RenderImage construction sites while keeping disk PNGs in correct RGBA; OSR paint path untouched (already BGRA). Added 2 unit tests (channel swap + disk load byte order). cargo check/build clean, 17 engine tests pass.

## Session 2026-11-10T05:51:20.357831972+00:00

Researched DeepSeek Harness/Cordis, CEF Alloy extension status, WASM options; wrote agent_plan.md: 8-phase plan to make Lumen plugin-based.

## Session 2026-11-10T06:39:36.755130673+00:00

Implemented the Phase 0 plugin kernel scaffold and unit coverage. Release build, 97 tests, clippy, formatting and diff checks pass. Recorded baseline repair and X11 manual smoke limitation; Phase 1 remains gated.

## Session 2026-11-10T07:05:11.621553090+00:00

Theme picker: equal horizontal Intensity/Grain tracks, white handles with preserved grab offsets, explicit rounded backdrop corners. Replaced repeated gradient strips and grain quads across space surfaces with native gradients and bounded cached grain textures to reduce redraw cost. All 103 tests pass; release build in progress. Live frame pacing not measured.

## Session 2026-11-10T07:20:31.049411425+00:00

Started Phase 1 of the Lumen plugin platform: promoted the kernel into production app construction, mounted a core plugin for palette and keymap registrations, added plugins.toml load/save with flattened unknown-field preservation, and updated agent_plan.md. Phase 1 remains incomplete: service facades, event call sites, tests, and smoke verification are outstanding. No build or tests were run.

## Session 2026-11-10T07:35:47.775614908+00:00

Fixed command palette scrolling using a bounded GPUI uniform_list and scroll-to-selected behavior; corrected GPUI API usage, suppressed only staged plugin scaffold dead-code warnings, and cargo check now passes without warnings. Added initial address.submit and navigation.before waterfall dispatch. UI was not manually relaunched; user should validate palette arrow scrolling and Ctrl+K reopen. Phase 1 service facades and remaining event hooks are still outstanding.

## Session 2026-11-10T07:38:10.198974749+00:00

Fixed command palette row highlight width by setting uniform-list rows to full width. cargo check passes with no warnings. The palette now renders, scrolls selected rows, and retains the existing Ctrl+K handler. Phase 1 plugin service facades remain the next incomplete plan item.

## Session 2026-11-10T07:44:50.546449588+00:00

Committed the current project tree as cf8e9a9 (excluding .heald session memory), including the command palette scroll/selection-width fixes. Continued to Phase 1 after the commit by adding and wiring core history, settings, and navigation service facades; cargo check passes without warnings. Tabs, spaces, engine, commands, keymap, and theme facades plus remaining events/tests/smoke checks are still outstanding.

## Session 2026-11-10T07:49:07.609715900+00:00

Continued Phase 1 without committing, as requested. Added typed core service facades for tabs, spaces, navigation, engine, settings, history, commands, keymap, and theme; registered them through the core plugin and updated agent_plan.md. cargo fmt and cargo check passed, with no warnings. Remaining work: other event call sites, palette/shortcut snapshot coverage, and manual smoke verification.

## Session 2026-11-10T08:02:16.197245378+00:00

Completed Phase 1 kernel integration: promoted the kernel into the app, added core service facades and plugin config, wired app and queued CEF event notifications, and added palette/shortcut regression snapshots. Verified with 104 passing tests, cargo check, release build, clippy review, and an isolated X11 first-frame smoke run. CEF policy decisions remain observational pending the typed deadline-aware bridge; no commit created.

## Session 2026-11-10T09:14:40.419133635+00:00

Paused plugin platform work for handoff. Fixed Plugins settings render panic by passing state into view; added plugins to Extensions popover, approved package permission review, signed ZIP distribution, panel-open integration, palette commands, pack runtime/theme work. Full suite passed 135/135 before latest in-flight theme/palette/panel changes; targeted render regression passed; default and wasm feature cargo checks passed before latest changes, Phase6 agent reports cargo check passed with two unused-import warnings. No commit.

## Session 2026-11-10T09:34:25.018092619+00:00

Wired active_layout_preset and active theme tokens into app shell; resolved cargo check warnings; verified 140 base tests and 148 wasm-plugins tests pass cleanly; updated agent_plan.md and NOTES.md

## Session 2026-11-10T10:03:20.686645198+00:00

Rebranded browser to Anarc; integrated uploaded logo into welcome page, sidebar headers, and settings; fixed favorite tiles and plugin manager buttons layout/responsiveness; verified all 148 tests pass

## Session 2026-11-10T10:24:12.781018091+00:00

Removed plugins from Extensions popover (only Ad blocker extension is displayed) and fixed selected tab hover to maintain clean surface color without murky browny tint.

## Session 2026-11-10T10:35:32.649780770+00:00

Removed new-tab welcome element; removed logo from sidebar space header and collapsed sidebar; removed solid background from logo asset producing clean transparent RGBA PNG.

## Session 2026-11-10T10:45:00.459526582+00:00

Audited codebase for secrets, hardened .gitignore, updated README.md with comprehensive installation and CEF setup guides for Linux distributions, created private GitHub repository rover-001/anarc, and pushed all code to origin/main.

## Session 2026-11-10T11:55:20.496892931+00:00

Audited all 60+ files across engine, UI render, plugins, shield, persistence, navigation, and build configurations. Generated exhaustive performance.md categorized into P0-P3 priorities with concrete drop-in code fixes.

## Session 2026-11-10T12:05:55.549119705+00:00

Configured Cargo.toml release profile (opt-level 3, thin LTO, 1 codegen-unit, panic abort, strip symbols), pruned image crate default features, added .cargo/config.toml with native target cpu rustflags, and verified 140/140 tests pass.

## Session 2026-11-10T12:16:40.958615486+00:00

Renamed crate and binary to anarc across Cargo.toml, install.sh, and CLI. Fixed Chromium subprocess launch bug by setting browser_subprocess_path, resources_dir_path, and locales_dir_path in CEF Settings and adding runtime symlinks. Fixed stale page flash on tab close and navigation by cleaning transition frames on blank/unloaded tabs.

## Session 2026-11-10T12:20:51.642219001+00:00

Committed and pushed fixes for Chromium launch on startup, stale page flash on tab close/search, Anarc renaming, and performance optimizations; updated install.sh with dual anarc/lumen symlinks

## Session 2026-11-10T12:44:15.778247557+00:00

Diagnosed and resolved heavy website and scroll lag on hianime.at/home across 5 subsystems: 189x faster adblock shield matching via query cache, GPU rasterization and zero-copy flags enabled in CEF, throttled 1Hz median edge color OSR calculation, smooth subpixel scroll accumulator, and redundant UI wake elimination. Tested release build and verified all test suites.

## Session 2026-11-10T13:17:14.822147743+00:00

Fixed heavy-site scroll/network lag (adblock query cache 189x speedup, CEF GPU rasterization & zero copy flags, frame edge color 1Hz throttling, subpixel scroll accumulator); fixed ghost closing tab animation bug on blank tabs and anchored exit animations to root sidebar coordinates; enabled sidebar right-click context menu across empty canvas, tabs zone, and root container; eliminated all compiler & clippy warnings; added performance.md to .gitignore, purged it from git commit history, and pushed cleanly to origin/main.

## Session 2026-11-10T13:17:29.425297069+00:00

Pushed finalized memory logs to remote main.

## Session 2026-11-10T13:41:51.366347179+00:00

Fixed browser downloads (CEF can_download and on_before_download set to 1 with ui_update_wake dispatch), added top-right download active indicator badge and live progress notification card. Pre-installed uBlock Origin with YouTube ad tracking filters and client-side SPA ad-stripping scriptlet for complete YouTube video & banner ad elimination; added Manage Extensions navigation to Settings -> Plugins.
