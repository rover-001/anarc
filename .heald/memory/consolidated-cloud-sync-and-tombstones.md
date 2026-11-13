---
type: decision
title: "Consolidated: Cloud Sync & Tombstones"
timestamp: 2026-11-12T12:47:38.269816283+00:00
tags: [consolidated, "cloud-sync-&-tombstones"]
---

# Consolidated: Cloud Sync & Tombstones

Consolidated architectural decisions, system invariants, and implementation patterns.

## Core System Decisions & Invariants

- **Restore visual viewport page scaling with cursor anchor and optimize benchmark throughput**: 1. Restored set_page_scale(next) in pinch_zoom using Emulation.setPageScaleFactor to fix non-functional pinch-zoom, and paired it with high-precision scroll wheel deltas (-dx, -dy) to anchor the visual viewport directly at the cursor coordinates (x, y). 2. Capped OSR_MAX_FRAME_RATE back to 60 fps (customizable via ANARC_OSR_FPS), eliminating the 144Hz frame copy memory saturation (1.2 GB/s) that throttled compositor and caused the low Speedometer score (21.7). 3. Added fast-path bypass in ShieldState::should_block for non-HTTP/HTTPS schemes (such as blob:, data:, about:) to prevent thousands of synchronous IPC channel queries and context switches during JS benchmarks and web app execution.
- **Fix trackpad zoom and web copy-paste synchronization**: Resolved trackpad zooming by tuning pinch sensitivity from 0.1 to 0.02, handling pinch start/end resets, routing zoom changes per target tab across all spaces, and invalidating CEF OSR view on zoom changes to guarantee immediate repaints. Fixed web copy/paste (including ChatGPT and web apps) by enabling javascript_access_clipboard and javascript_dom_paste in CEF BrowserSettings, auto-accepting CEF_PERMISSION_TYPE_CLIPBOARD in PermissionHandler, preventing CHAR key event emissions on Control shortcuts to eliminate text input collisions in Blink, adding frame-level copy/paste/cut/select_all commands, injecting bidirectional clipboard synchronization scripts, and bridging to Wayland/X11 system clipboard via wl-copy/wl-paste.

## Impacted Files & Subsystems

*(Files referenced in historical session logs)*

## Consolidated Evolution History (2 items archived)

- 2026-11-12 — Restore visual viewport page scaling with cursor anchor and optimize benchmark throughput (`restore-visual-viewport-page-scaling-with-cursor-anchor-and-optimize-benchmark-throughput.md`)
- 2026-11-12 — Fix trackpad zoom and web copy-paste synchronization (`fix-trackpad-zoom-and-web-copy-paste-synchronization.md`)
