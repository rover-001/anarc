---
type: decision
title: "Eliminate heavy site scroll and network stutter on hianime"
timestamp: 2026-11-10T12:44:10.367551835+00:00
---
Diagnosed lag on heavy media/ad-heavy sites (such as hianime.at/home). Fixed 5 compounding bottlenecks: 1) Added a fast 8K-entry hash cache to ShieldState dropping adblock query overhead from 1110µs to 5.8µs (189x speedup) and preventing CEF network thread serialization on subresources; 2) Enabled Chromium hardware GPU rasterization, ignore-gpu-blocklist, zero-copy, and OOP canvas rasterization while re-enabling GPU shader disk cache; 3) Throttled OSR median edge color calculation from 144Hz to 1Hz with stride sampling; 4) Implemented subpixel scroll delta accumulator in WebViewport to prevent fractional scroll ticks from dropping to zero; 5) Removed redundant UI loop wakeups on mouse wheel dispatch.
