---
type: decision
title: "Fix pinch-to-zoom cursor anchoring and hianime ad shields"
timestamp: 2026-11-12T14:12:25.312534990+00:00
---
1. Derived the exact mathematical formula for visual viewport drift: (x + scroll_x) * (next / previous - 1.0) and clamped within allowable viewport boundaries to keep the cursor focal point stationary across all zoom scales and pans. 2. Preserved subpixel fractions across frames with step_pinch_accum and removed artificial smooth-scrolling momentum easing so scale and wheel input apply on the identical frame. 3. Tuned pinch coalescing to 6ms to sync with 144Hz displays and default windowless frame rate to monitor refresh rate. 4. Added hianime and anime streaming rotator ad networks, popup defusers, and cosmetic banner blockers to bundled shield filters.
