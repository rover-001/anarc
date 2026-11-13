---
type: decision
title: "Wayland clipboard synchronization and scroll/cursor responsiveness"
timestamp: 2026-11-12T15:29:11.835516660+00:00
---
Fixed web text copying across all methods by removing unregistered script URL schemes from execute_java_script, registering DOM copy and mouseup handlers in the capturing phase on window and document, proxying navigator.clipboard.writeText directly to wl-copy, forwarding keydown events during Ctrl+C/Ctrl+X, adding get_selected_text for context-menu Copy, falling back to system clipboard in the address bar, and waking the UI loop on cursor changes with round-based scroll delta accumulation.
