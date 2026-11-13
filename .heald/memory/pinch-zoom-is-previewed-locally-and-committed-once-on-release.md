---
type: decision
title: "Pinch zoom is previewed locally and committed once on release"
timestamp: 2026-11-13T16:21:49.747290705+00:00
---
Blink re-rasterizes on every page-scale change (~20 ms/frame in OSR), so forwarding each trackpad step capped gestures at ~45 fps. WebViewport now keeps a PinchPreview (scale, base_scale, page_scale, anchor, awaiting_frame, rebase, settle) that scales and re-anchors the last OSR frame in GPUI at display rate; Engine::pinch_zoom is called once on release. Scale factors multiply against page_scale (Blink target) not the on-screen frame, so a gesture started before the previous commit repaints still commits the right factor; on_frame_arrived() either ends the preview or re-bases it when rebase is set. Preview holds until a fresh frame arrives, bounded by PINCH_FRAME_TIMEOUT (700 ms); sub-1.05 scales snap back with motion::pinch_settle().
