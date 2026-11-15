---
type: decision
title: "Web view container enforces size_full layout and suppresses trailing pinch moves"
timestamp: 2026-11-14T06:10:21.139603057+00:00
---
Root cause of the floating 50% card shrinkage on trackpad pinch-in and release: WebViewport render set explicit dimensions w(width * pinch_factor).h(height * pinch_factor) and top(pinch_offset.1).left(pinch_offset.0) on the inner container with CARD_RADIUS. When returning to natural scale (or unzoomed), any factor <= 1.0 or trailing TouchPhase::Moved event recreated the preview with factor < 1.0, shrinking the viewport into a 50% floating card with rounded corners exposing window background. Fix: 1) Made current container strictly size_full() whenever pinch_factor <= 1.0, and clamped offsets with min(0.0) so content never pulls away from viewport edges; 2) Added pinch_active tracking to ignore trailing TouchPhase::Moved events that arrive after Ended/Cancelled; 3) Made transition and crossfade containers size_full().
