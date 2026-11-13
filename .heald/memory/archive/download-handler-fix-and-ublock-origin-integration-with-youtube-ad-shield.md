---
type: decision
title: "Download handler fix and uBlock Origin integration with YouTube ad shield"
timestamp: 2026-11-10T13:41:32.310305500+00:00
---
Implemented can_download=1 and on_before_download=1 in LumenDownloadHandler with UI loop wakes to enable file downloads; added active download button indicator badge and top-right download progress toast. Pre-installed uBlock Origin with YouTube ad tracking filters and client-side SPA ad-stripping scriptlet for in-stream and banner ad elimination.
