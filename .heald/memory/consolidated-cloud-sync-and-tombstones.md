---
type: decision
title: "Consolidated: Cloud Sync & Tombstones"
timestamp: 2026-11-21T02:46:41.838783209+00:00
tags: [consolidated, "cloud-sync-&-tombstones"]
---

# Consolidated: Cloud Sync & Tombstones

Consolidated architectural decisions, system invariants, and implementation patterns.

## Core System Decisions & Invariants

- **Push Live Share plugin and separate Agent Control branch**: Separated working tree into two clean, self-contained branches without touching main: 1) live-share: committed and pushed Live Share plugin, live cursor broadcasting, peer cursor overlays, space sync, and DevTools cookie synchronization to origin/live-share (803e85c); 2) feat/agent-control: branched from live-share, committed and pushed full Agent Control pipeline (src/agent/, anarc-ctl CLI/MCP server, indexer.js, typed refs, act/observe, skill docs, and benchmarks) to origin/feat/agent-control (dc296c1). main remained untouched.
- **Frame sync via external begin frame driven by GPUI vsync**: Set external_begin_frame_enabled = 1 and aligned host.send_external_begin_frame() with GPUI per-frame vsync callbacks. Eliminated timer phase drift and reduced p99 frame latency from 25.6ms to 20.9ms on heavy pages.
- **Wayland clipboard synchronization and scroll/cursor responsiveness**: Fixed web text copying across all methods by removing unregistered script URL schemes from execute_java_script, registering DOM copy and mouseup handlers in the capturing phase on window and document, proxying navigator.clipboard.writeText directly to wl-copy, forwarding keydown events during Ctrl+C/Ctrl+X, adding get_selected_text for context-menu Copy, falling back to system clipboard in the address bar, and waking the UI loop on cursor changes with round-based scroll delta accumulation.
- **Fix pinch-to-zoom cursor anchoring and hianime ad shields**: 1. Derived the exact mathematical formula for visual viewport drift: (x + scroll_x) * (next / previous - 1.0) and clamped within allowable viewport boundaries to keep the cursor focal point stationary across all zoom scales and pans. 2. Preserved subpixel fractions across frames with step_pinch_accum and removed artificial smooth-scrolling momentum easing so scale and wheel input apply on the identical frame. 3. Tuned pinch coalescing to 6ms to sync with 144Hz displays and default windowless frame rate to monitor refresh rate. 4. Added hianime and anime streaming rotator ad networks, popup defusers, and cosmetic banner blockers to bundled shield filters.
- **Restore visual viewport page scaling with cursor anchor and optimize benchmark throughput**: 1. Restored set_page_scale(next) in pinch_zoom using Emulation.setPageScaleFactor to fix non-functional pinch-zoom, and paired it with high-precision scroll wheel deltas (-dx, -dy) to anchor the visual viewport directly at the cursor coordinates (x, y). 2. Capped OSR_MAX_FRAME_RATE back to 60 fps (customizable via ANARC_OSR_FPS), eliminating the 144Hz frame copy memory saturation (1.2 GB/s) that throttled compositor and caused the low Speedometer score (21.7). 3. Added fast-path bypass in ShieldState::should_block for non-HTTP/HTTPS schemes (such as blob:, data:, about:) to prevent thousands of synchronous IPC channel queries and context switches during JS benchmarks and web app execution.
- **Fix trackpad zoom and web copy-paste synchronization**: Resolved trackpad zooming by tuning pinch sensitivity from 0.1 to 0.02, handling pinch start/end resets, routing zoom changes per target tab across all spaces, and invalidating CEF OSR view on zoom changes to guarantee immediate repaints. Fixed web copy/paste (including ChatGPT and web apps) by enabling javascript_access_clipboard and javascript_dom_paste in CEF BrowserSettings, auto-accepting CEF_PERMISSION_TYPE_CLIPBOARD in PermissionHandler, preventing CHAR key event emissions on Control shortcuts to eliminate text input collisions in Blink, adding frame-level copy/paste/cut/select_all commands, injecting bidirectional clipboard synchronization scripts, and bridging to Wayland/X11 system clipboard via wl-copy/wl-paste.

## Impacted Files & Subsystems

- `indexer.js`

## Consolidated Evolution History (6 items archived)

- 2026-11-21 — Push Live Share plugin and separate Agent Control branch (`push-live-share-plugin-and-separate-agent-control-branch.md`)
- 2026-11-19 — Frame sync via external begin frame driven by GPUI vsync (`frame-sync-via-external-begin-frame-driven-by-gpui-vsync.md`)
- 2026-11-12 — Wayland clipboard synchronization and scroll/cursor responsiveness (`wayland-clipboard-synchronization-and-scrollcursor-responsiveness.md`)
- 2026-11-12 — Fix pinch-to-zoom cursor anchoring and hianime ad shields (`fix-pinch-to-zoom-cursor-anchoring-and-hianime-ad-shields.md`)
- 2026-11-12 — Restore visual viewport page scaling with cursor anchor and optimize benchmark throughput (`restore-visual-viewport-page-scaling-with-cursor-anchor-and-optimize-benchmark-throughput.md`)
- 2026-11-12 — Fix trackpad zoom and web copy-paste synchronization (`fix-trackpad-zoom-and-web-copy-paste-synchronization.md`)
