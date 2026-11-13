---
type: decision
title: "Plugin platform architecture (agent_plan.md)"
timestamp: 2026-11-10T05:51:20.351346727+00:00
---
Cordis-style Rust kernel in src/plugin (services by key, inject, auto-disposed effects, emit/parallel/serial/waterfall/bail events). Plugin kinds: core, builtin (toggleable), declarative packs (theme tokens/userstyles/userscripts/keymaps/search/filters), WASM via wasmtime component model + WIT with host-enforced permissions, web panels via lumen-plugin:// scheme. No WebExtensions (Alloy). Hot paths use declarative matchers only. Defaults must equal current behaviour.
