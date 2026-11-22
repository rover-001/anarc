---
type: decision
title: "Focal pinch-to-zoom, non-GPU software fallback, and idle CPU reduction"
timestamp: 2026-11-12T12:32:13.360231325+00:00
---
1. Routed pinch zoom through CEF SendMouseWheelEvent with EVENTFLAG_CONTROL_DOWN and EVENTFLAG_PRECISION_SCROLLING_DELTA at cursor position (x, y), ensuring Blink anchors visual viewport scaling directly around user fingers rather than (0, 0). 2. Forwarded EVENTFLAG_PRECISION_SCROLLING_DELTA on pixel-delta scroll events for 1:1 smooth trackpad panning. 3. Added has_hardware_gpu() detection via /dev/dri and environment overrides (ANARC_NO_GPU, LIBGL_ALWAYS_SOFTWARE) to dynamically disable GPU rasterization/compositing and clamp OSR frame rates to 60 fps on machines without dedicated GPU. 4. Fixed idle CPU burn in the main UI loop by switching from unconditional 144Hz 7ms timer polling to an adaptive burst model that sleeps for 250ms when idle and wakes immediately on paint events, dropping idle CPU from ~20% to 0.0%.
