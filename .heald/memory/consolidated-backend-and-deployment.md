---
type: decision
title: "Consolidated: Backend & Deployment"
timestamp: 2026-11-12T12:32:13.360231325+00:00
tags: [consolidated, "backend-&-deployment"]
---

# Consolidated: Backend & Deployment

Consolidated architectural decisions, system invariants, and implementation patterns.

## Core System Decisions & Invariants

- **Focal pinch-to-zoom, non-GPU software fallback, and idle CPU reduction**: 1. Routed pinch zoom through CEF SendMouseWheelEvent with EVENTFLAG_CONTROL_DOWN and EVENTFLAG_PRECISION_SCROLLING_DELTA at cursor position (x, y), ensuring Blink anchors visual viewport scaling directly around user fingers rather than (0, 0). 2. Forwarded EVENTFLAG_PRECISION_SCROLLING_DELTA on pixel-delta scroll events for 1:1 smooth trackpad panning. 3. Added has_hardware_gpu() detection via /dev/dri and environment overrides (ANARC_NO_GPU, LIBGL_ALWAYS_SOFTWARE) to dynamically disable GPU rasterization/compositing and clamp OSR frame rates to 60 fps on machines without dedicated GPU. 4. Fixed idle CPU burn in the main UI loop by switching from unconditional 144Hz 7ms timer polling to an adaptive burst model that sleeps for 250ms when idle and wakes immediately on paint events, dropping idle CPU from ~20% to 0.0%.
- **Page popups are CEF-native and render in the peek overlay**: window.open/sign-in popups are no longer cancelled and re-created by the app. on_before_popup returns 0 with a windowless WindowInfo + full client so CEF creates the browser with a live window.opener, and the app only binds the peek overlay to that browser id (PopupRequest.browser_id). Pending state is queued in EngineEvents.pending_popup_states so ensure_browser cannot duplicate the id. window.close() routes through close_peek_from_page -> Engine::forget_closed_browser (no double close). Narrative: auth popup navigated the tab destroyed window.opener, so the callback postMessage failed and window.close() closed the main tab.
- **Plugin platform architecture (agent_plan.md)**: Cordis-style Rust kernel in src/plugin (services by key, inject, auto-disposed effects, emit/parallel/serial/waterfall/bail events). Plugin kinds: core, builtin (toggleable), declarative packs (theme tokens/userstyles/userscripts/keymaps/search/filters), WASM via wasmtime component model + WIT with host-enforced permissions, web panels via lumen-plugin:// scheme. No WebExtensions (Alloy). Hot paths use declarative matchers only. Defaults must equal current behaviour.

## Impacted Files & Subsystems

*(Files referenced in historical session logs)*

## Consolidated Evolution History (3 items archived)

- 2026-11-12 — Focal pinch-to-zoom, non-GPU software fallback, and idle CPU reduction (`focal-pinch-to-zoom-non-gpu-software-fallback-and-idle-cpu-reduction.md`)
- 2026-11-11 — Page popups are CEF-native and render in the peek overlay (`page-popups-are-cef-native-and-render-in-the-peek-overlay.md`)
- 2026-11-10 — Plugin platform architecture (agent_plan.md) (`plugin-platform-architecture-agentplanmd.md`)
