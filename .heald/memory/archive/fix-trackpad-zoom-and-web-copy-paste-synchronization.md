---
type: decision
title: "Fix trackpad zoom and web copy-paste synchronization"
timestamp: 2026-11-12T02:54:01.518632794+00:00
---
Resolved trackpad zooming by tuning pinch sensitivity from 0.1 to 0.02, handling pinch start/end resets, routing zoom changes per target tab across all spaces, and invalidating CEF OSR view on zoom changes to guarantee immediate repaints. Fixed web copy/paste (including ChatGPT and web apps) by enabling javascript_access_clipboard and javascript_dom_paste in CEF BrowserSettings, auto-accepting CEF_PERMISSION_TYPE_CLIPBOARD in PermissionHandler, preventing CHAR key event emissions on Control shortcuts to eliminate text input collisions in Blink, adding frame-level copy/paste/cut/select_all commands, injecting bidirectional clipboard synchronization scripts, and bridging to Wayland/X11 system clipboard via wl-copy/wl-paste.
