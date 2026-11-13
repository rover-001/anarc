---
type: decision
title: "Shield mirrors uBlock Origin Lite; no WebExtensions in Alloy"
timestamp: 2026-11-11T13:31:46.989804745+00:00
---
The built-in shield compiles the same filter sets the user has enabled in Chromium uBlock Origin Lite (EasyList, EasyPrivacy, uBlock filters, uBlock badware, Peter Lowe) plus Lumen starter rules (assets/filters, see LICENSES.md), and now runs a cosmetic stage: url_cosmetic_resources -> one injected <style> of hide selectors, executed on page.load_end via Engine::shield_cosmetic_script. urlhaus-full is intentionally not bundled (12 MB, malware not ads). Literal uBOL cannot load because CEF runs RuntimeStyle::ALLOY (no extension support); do not plan extension loading without switching to the Chrome runtime.
