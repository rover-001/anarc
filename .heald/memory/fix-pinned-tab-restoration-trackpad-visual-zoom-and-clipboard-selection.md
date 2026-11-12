---
type: decision
title: "Fix pinned tab restoration, trackpad visual zoom, and clipboard selection"
timestamp: 2026-11-12T04:59:31.331962034+00:00
---
1. Resolved pinned/favorite tab URL persistence: added pinned_title field to Tab, ensured unload_kept_tab restores tab.url and tab.title from pinned destination on unload/close, startup resets kept tabs, and sidebar/context-menu close actions use close_section_tab. 2. Trackpad pinch zoom now sends Ctrl+mouse_wheel events via pinch_wheel_delta to emulate Chromium visual viewport zoom rather than changing layout page zoom level. 3. Fixed compilation error CefStringUserfreeUtf16 by converting through CefStringUtf16::from, wired right-click context menu copy/paste, and auto-synced selected text to Wayland/X11 primary clipboard on mouseup. 4. Successfully built and installed release binary via ./install.sh.
