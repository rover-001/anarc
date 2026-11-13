---
type: decision
title: "Eliminate panning, zooming, and scrolling lag on high refresh displays"
timestamp: 2026-11-12T12:03:51.730808989+00:00
---
1. Raised OSR_MAX_FRAME_RATE from 60 to 240 fps, allowing CEF offscreen rendering and GPUI fallback pacing to match native monitor refresh rates (such as 144Hz) without judder or 60Hz throttling. 2. Fixed pinch-to-zoom by adding immediate OSR view invalidation and UI loop wake dispatch in pinch_zoom, setting instant scroll behavior in DevTools scrollBy to bypass CSS smooth-scroll fighting, lowering PINCH_EMIT_INTERVAL to 4ms for 144Hz gesture pacing, and expanding MIN_PAGE_SCALE to 0.25 to allow zooming out. 3. Increased line scroll step multiplier from 40px to 100px to match standard Chromium mouse-wheel travel. 4. Resolved collapsible-if and negated-comparison clippy lints across tabs, browser, and clipboard modules.
