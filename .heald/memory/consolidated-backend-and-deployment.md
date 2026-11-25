---
type: decision
title: "Consolidated: Backend & Deployment"
timestamp: 2026-11-25T16:14:01.145833620+00:00
tags: [consolidated, "backend-&-deployment"]
---

# Consolidated: Backend & Deployment

Consolidated architectural decisions, system invariants, and implementation patterns.

## Core System Decisions & Invariants

- **Modularized src/agent/act and src/agent/plugin with zero clippy errors/warnings**: Split src/agent/act.rs (3588 lines) into src/agent/act/{mod.rs,types.rs,parse.rs,exec.rs,dom.rs} and src/agent/plugin.rs (1039 lines) into src/agent/plugin/{mod.rs,telemetry.rs,channel.rs,dispatch.rs}. Fixed all clippy::indexing_slicing, clippy::unwrap_used, as_chunks, and unused-item warnings across src/agent/.
- **Pinch-in releases clean preview state and restores zero horizontal drift**: When returning to natural scale (scale <= 1.0005) or releasing pinch-in, WebViewport immediately terminates the pinch preview via cancel_pinch() rather than retaining awaiting_frame=true with an old base_scale. This prevents the frame from shrinking to half-size (causing the page to appear as if adapting) and prevents subsequent pinch-out gestures from inheriting corrupted scale factors and jumping. In Engine::pinch_zoom, returning to <= 1.0005 resets state.page_scale to 1.0, resets pinch_view to default, clamps horizontal document scroll to 0.0 to eliminate layout shifts, and restores the vertical pre-zoom offset.
- **Focal pinch-to-zoom, non-GPU software fallback, and idle CPU reduction**: 1. Routed pinch zoom through CEF SendMouseWheelEvent with EVENTFLAG_CONTROL_DOWN and EVENTFLAG_PRECISION_SCROLLING_DELTA at cursor position (x, y), ensuring Blink anchors visual viewport scaling directly around user fingers rather than (0, 0). 2. Forwarded EVENTFLAG_PRECISION_SCROLLING_DELTA on pixel-delta scroll events for 1:1 smooth trackpad panning. 3. Added has_hardware_gpu() detection via /dev/dri and environment overrides (ANARC_NO_GPU, LIBGL_ALWAYS_SOFTWARE) to dynamically disable GPU rasterization/compositing and clamp OSR frame rates to 60 fps on machines without dedicated GPU. 4. Fixed idle CPU burn in the main UI loop by switching from unconditional 144Hz 7ms timer polling to an adaptive burst model that sleeps for 250ms when idle and wakes immediately on paint events, dropping idle CPU from ~20% to 0.0%.
- **Page popups are CEF-native and render in the peek overlay**: window.open/sign-in popups are no longer cancelled and re-created by the app. on_before_popup returns 0 with a windowless WindowInfo + full client so CEF creates the browser with a live window.opener, and the app only binds the peek overlay to that browser id (PopupRequest.browser_id). Pending state is queued in EngineEvents.pending_popup_states so ensure_browser cannot duplicate the id. window.close() routes through close_peek_from_page -> Engine::forget_closed_browser (no double close). Narrative: auth popup navigated the tab destroyed window.opener, so the callback postMessage failed and window.close() closed the main tab.
- **Plugin platform architecture (agent_plan.md)**: Cordis-style Rust kernel in src/plugin (services by key, inject, auto-disposed effects, emit/parallel/serial/waterfall/bail events). Plugin kinds: core, builtin (toggleable), declarative packs (theme tokens/userstyles/userscripts/keymaps/search/filters), WASM via wasmtime component model + WIT with host-enforced permissions, web panels via lumen-plugin:// scheme. No WebExtensions (Alloy). Hot paths use declarative matchers only. Defaults must equal current behaviour.

## Impacted Files & Subsystems

- `src/agent/act.rs`
- `src/agent/plugin.rs`

## Consolidated Evolution History (5 items archived)

- 2026-11-25 — Modularized src/agent/act and src/agent/plugin with zero clippy errors/warnings (`modularized-srcagentact-and-srcagentplugin-with-zero-clippy-errorswarnings.md`)
- 2026-11-14 — Pinch-in releases clean preview state and restores zero horizontal drift (`pinch-in-releases-clean-preview-state-and-restores-zero-horizontal-drift.md`)
- 2026-11-12 — Focal pinch-to-zoom, non-GPU software fallback, and idle CPU reduction (`focal-pinch-to-zoom-non-gpu-software-fallback-and-idle-cpu-reduction.md`)
- 2026-11-11 — Page popups are CEF-native and render in the peek overlay (`page-popups-are-cef-native-and-render-in-the-peek-overlay.md`)
- 2026-11-10 — Plugin platform architecture (agent_plan.md) (`plugin-platform-architecture-agentplanmd.md`)
