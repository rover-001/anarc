---
type: decision
title: "Implement file upload dialog handler and drag-and-drop file ingestion"
timestamp: 2026-11-11T13:39:58.169516981+00:00
---
Implemented CEF DialogHandler in LumenDialogHandler spawning background native zenity/kdialog file choosers with filter parsing and callback resolution; hooked GPUI ExternalPaths drop events on WebViewport into CEF BrowserHost drag target APIs (drag_enter, drag_over, drop, ended) with DragData.
