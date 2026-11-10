---
type: decision
title: "Fix Chromium subprocess launch and transition frame flash, complete Anarc rename"
timestamp: 2026-11-10T12:16:31.095389300+00:00
---
Renamed package to anarc in Cargo.toml and install scripts. Configured browser_subprocess_path, resources_dir_path, and locales_dir_path in CEF Settings to prevent CEF from searching PATH and launching system /usr/bin/chromium. Fixed web_view transition logic so closing a tab and navigating to a new search never flashes the stale closed page.
