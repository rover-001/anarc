---
type: decision
title: "Pinch-in releases clean preview state and restores zero horizontal drift"
timestamp: 2026-11-14T05:13:20.677249218+00:00
---
When returning to natural scale (scale <= 1.0005) or releasing pinch-in, WebViewport immediately terminates the pinch preview via cancel_pinch() rather than retaining awaiting_frame=true with an old base_scale. This prevents the frame from shrinking to half-size (causing the page to appear as if adapting) and prevents subsequent pinch-out gestures from inheriting corrupted scale factors and jumping. In Engine::pinch_zoom, returning to <= 1.0005 resets state.page_scale to 1.0, resets pinch_view to default, clamps horizontal document scroll to 0.0 to eliminate layout shifts, and restores the vertical pre-zoom offset.
