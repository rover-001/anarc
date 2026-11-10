---
type: decision
title: "Sidebar right-click context menu and blank tab close animation fix"
timestamp: 2026-11-10T13:13:48.030941935+00:00
---
Fixed sidebar canvas right-click context menu by attaching handler to root sidebar content, drop zone, and empty canvas. Prevented ghost closing tab flash when closing empty/blank tabs by skipping closing_tab state, and anchored closing tab exit animation to root sidebar coordinates instead of nested regular_tabs container.
