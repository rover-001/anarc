---
type: decision
title: "Pinch preview enforces floor factor of 1.0 to prevent card shrinkage"
timestamp: 2026-11-14T05:30:01.793106004+00:00
---
Render for WebViewport previously calculated factor = scale / base_scale, which produced factor < 1.0 (e.g. 0.5) when pinching in or returning from high zoom levels. This caused GPUI to shrink the container to width*factor and height*factor, rendering a small floating card inside the viewport and exposing window background. Clamped factor to 1.0 (pinch_factor = 1.0, pinch_offset = (0,0) whenever factor <= 1.0 + 1e-4) so the webpage container can never shrink below full viewport size.
