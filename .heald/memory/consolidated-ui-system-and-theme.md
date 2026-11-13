---
type: decision
title: "Consolidated: UI System & Theme"
timestamp: 2026-11-12T12:03:51.730808989+00:00
tags: [consolidated, "ui-system-&-theme"]
---

# Consolidated: UI System & Theme

Consolidated architectural decisions, system invariants, and implementation patterns.

## Core System Decisions & Invariants

- **Eliminate panning, zooming, and scrolling lag on high refresh displays**: 1. Raised OSR_MAX_FRAME_RATE from 60 to 240 fps, allowing CEF offscreen rendering and GPUI fallback pacing to match native monitor refresh rates (such as 144Hz) without judder or 60Hz throttling. 2. Fixed pinch-to-zoom by adding immediate OSR view invalidation and UI loop wake dispatch in pinch_zoom, setting instant scroll behavior in DevTools scrollBy to bypass CSS smooth-scroll fighting, lowering PINCH_EMIT_INTERVAL to 4ms for 144Hz gesture pacing, and expanding MIN_PAGE_SCALE to 0.25 to allow zooming out. 3. Increased line scroll step multiplier from 40px to 100px to match standard Chromium mouse-wheel travel. 4. Resolved collapsible-if and negated-comparison clippy lints across tabs, browser, and clipboard modules.
- **Fix pinned tab restoration, trackpad visual zoom, and clipboard selection**: 1. Resolved pinned/favorite tab URL persistence: added pinned_title field to Tab, ensured unload_kept_tab restores tab.url and tab.title from pinned destination on unload/close, startup resets kept tabs, and sidebar/context-menu close actions use close_section_tab. 2. Trackpad pinch zoom now sends Ctrl+mouse_wheel events via pinch_wheel_delta to emulate Chromium visual viewport zoom rather than changing layout page zoom level. 3. Fixed compilation error CefStringUserfreeUtf16 by converting through CefStringUtf16::from, wired right-click context menu copy/paste, and auto-synced selected text to Wayland/X11 primary clipboard on mouseup. 4. Successfully built and installed release binary via ./install.sh.
- **Cap CEF OSR frame rate at 60 and pass button flags on mouse moves**: OSR paints cost a full-window BGRA memcpy plus a new RenderImage/atlas upload per frame, so tracking display refresh (144/240Hz) saturated memory bandwidth and stalled CEF compositing on heavy pages. frame_rate_for_refresh now clamps to OSR_MAX_FRAME_RATE=60 (ANARC_OSR_FPS overrides for benchmarks); it doubles as the UI update loop fallback cadence. Separately, CEF mouse-move events must carry the held-button bits (EVENTFLAG_LEFT_MOUSE_BUTTON=1/MIDDLE=16/RIGHT=32) in MouseEvent.modifiers or Blink sees Button::kNoButton and never starts drag selection, which is why text could not be selected/copied by click-drag; Engine::send_mouse_move takes gpui MouseMoveEvent.pressed_button and send_mouse_click passes the button on press only.
- **Shield mirrors uBlock Origin Lite; no WebExtensions in Alloy**: The built-in shield compiles the same filter sets the user has enabled in Chromium uBlock Origin Lite (EasyList, EasyPrivacy, uBlock filters, uBlock badware, Peter Lowe) plus Lumen starter rules (assets/filters, see LICENSES.md), and now runs a cosmetic stage: url_cosmetic_resources -> one injected <style> of hide selectors, executed on page.load_end via Engine::shield_cosmetic_script. urlhaus-full is intentionally not bundled (12 MB, malware not ads). Literal uBOL cannot load because CEF runs RuntimeStyle::ALLOY (no extension support); do not plan extension loading without switching to the Chrome runtime.
- **Sidebar right-click context menu and blank tab close animation fix**: Fixed sidebar canvas right-click context menu by attaching handler to root sidebar content, drop zone, and empty canvas. Prevented ghost closing tab flash when closing empty/blank tabs by skipping closing_tab state, and anchored closing tab exit animation to root sidebar coordinates instead of nested regular_tabs container.
- **Eliminate heavy site scroll and network stutter on hianime**: Diagnosed lag on heavy media/ad-heavy sites (such as hianime.at/home). Fixed 5 compounding bottlenecks: 1) Added a fast 8K-entry hash cache to ShieldState dropping adblock query overhead from 1110µs to 5.8µs (189x speedup) and preventing CEF network thread serialization on subresources; 2) Enabled Chromium hardware GPU rasterization, ignore-gpu-blocklist, zero-copy, and OOP canvas rasterization while re-enabling GPU shader disk cache; 3) Throttled OSR median edge color calculation from 144Hz to 1Hz with stride sampling; 4) Implemented subpixel scroll delta accumulator in WebViewport to prevent fractional scroll ticks from dropping to zero; 5) Removed redundant UI loop wakeups on mouse wheel dispatch.
- **Create private repo anarc and publish installation guide**: Created private GitHub repository rover-001/anarc, hardened .gitignore against secrets and caches, scanned staged code for credentials (clean), updated README.md with comprehensive installation instructions for Linux and CEF setup, and pushed main branch.
- **Rebrand browser to Anarc and polish button responsiveness**: Renamed browser from Lumen to Anarc across window title, settings, and core plugin descriptor; embedded the uploaded Anarc logo as compile-time asset in welcome view, sidebar header, and settings; fixed favorite tiles to size responsively across sidebar width with properly scaled 20px favicons; grouped plugin manager install action buttons responsively.

## Impacted Files & Subsystems

- `install.sh`

## Consolidated Evolution History (8 items archived)

- 2026-11-12 — Eliminate panning, zooming, and scrolling lag on high refresh displays (`eliminate-panning-zooming-and-scrolling-lag-on-high-refresh-displays.md`)
- 2026-11-12 — Fix pinned tab restoration, trackpad visual zoom, and clipboard selection (`fix-pinned-tab-restoration-trackpad-visual-zoom-and-clipboard-selection.md`)
- 2026-11-11 — Cap CEF OSR frame rate at 60 and pass button flags on mouse moves (`cap-cef-osr-frame-rate-at-60-and-pass-button-flags-on-mouse-moves.md`)
- 2026-11-11 — Shield mirrors uBlock Origin Lite; no WebExtensions in Alloy (`shield-mirrors-ublock-origin-lite-no-webextensions-in-alloy.md`)
- 2026-11-10 — Sidebar right-click context menu and blank tab close animation fix (`sidebar-right-click-context-menu-and-blank-tab-close-animation-fix.md`)
- 2026-11-10 — Eliminate heavy site scroll and network stutter on hianime (`eliminate-heavy-site-scroll-and-network-stutter-on-hianime.md`)
- 2026-11-10 — Create private repo anarc and publish installation guide (`create-private-repo-anarc-and-publish-installation-guide.md`)
- 2026-11-10 — Rebrand browser to Anarc and polish button responsiveness (`rebrand-browser-to-anarc-and-polish-button-responsiveness.md`)
