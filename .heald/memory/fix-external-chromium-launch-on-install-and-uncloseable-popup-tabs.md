---
type: decision
title: "Fix external Chromium launch on install and uncloseable popup tabs"
timestamp: 2026-11-11T12:56:39.187229503+00:00
---
1. Updated install.sh to export PATH with bin/CEF dirs, link chrome/chromium fallbacks to anarc, use absolute desktop Exec path, and configured CEF with Alloy runtime style and disable-chrome-runtime switch to avoid launching system Chromium. 2. Implemented do_close in LumenLifeSpanHandler, updated tab close logic to search across all spaces, deduplicated popup triggers, and rejected blank popup URLs.
