---
type: decision
title: "Window disables GPUI inactive frame interval so background pages keep full cadence"
timestamp: 2026-11-13T16:21:55.031058004+00:00
---
GPUI caps unfocused/inactive windows at ~30 fps via WindowOptions::inactive_frame_interval (33_333 us, only bypassed by sustained high-rate input) and gpui-linux drops the periodic refresh loop once the X11 window is not visible. A browser that plays video or animates in the background looked laggy next to the focused window. src/main.rs now opens the window with inactive_frame_interval: None. Rendering stays demand-driven (dirty frames only), so idle cost is unchanged; release cadence for an animating page went from 39-100 to 77-119 root UI renders/s at OSR paints/s=144.
