---
type: decision
title: "Restore visual viewport page scaling with cursor anchor and optimize benchmark throughput"
timestamp: 2026-11-12T12:47:38.269816283+00:00
---
1. Restored set_page_scale(next) in pinch_zoom using Emulation.setPageScaleFactor to fix non-functional pinch-zoom, and paired it with high-precision scroll wheel deltas (-dx, -dy) to anchor the visual viewport directly at the cursor coordinates (x, y). 2. Capped OSR_MAX_FRAME_RATE back to 60 fps (customizable via ANARC_OSR_FPS), eliminating the 144Hz frame copy memory saturation (1.2 GB/s) that throttled compositor and caused the low Speedometer score (21.7). 3. Added fast-path bypass in ShieldState::should_block for non-HTTP/HTTPS schemes (such as blob:, data:, about:) to prevent thousands of synchronous IPC channel queries and context switches during JS benchmarks and web app execution.
