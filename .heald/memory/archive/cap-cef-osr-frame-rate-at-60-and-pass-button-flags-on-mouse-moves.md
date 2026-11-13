---
type: decision
title: "Cap CEF OSR frame rate at 60 and pass button flags on mouse moves"
timestamp: 2026-11-11T16:05:03.503406509+00:00
---
OSR paints cost a full-window BGRA memcpy plus a new RenderImage/atlas upload per frame, so tracking display refresh (144/240Hz) saturated memory bandwidth and stalled CEF compositing on heavy pages. frame_rate_for_refresh now clamps to OSR_MAX_FRAME_RATE=60 (ANARC_OSR_FPS overrides for benchmarks); it doubles as the UI update loop fallback cadence. Separately, CEF mouse-move events must carry the held-button bits (EVENTFLAG_LEFT_MOUSE_BUTTON=1/MIDDLE=16/RIGHT=32) in MouseEvent.modifiers or Blink sees Button::kNoButton and never starts drag selection, which is why text could not be selected/copied by click-drag; Engine::send_mouse_move takes gpui MouseMoveEvent.pressed_button and send_mouse_click passes the button on press only.
