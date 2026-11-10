# Lumen plugin platform: agent execution plan

> Audience: coding agents (and humans) building Lumen's plugin system.
> Read this whole file before starting a phase, and update the **Status** line of a phase when you finish it.
> Repo rules in `AGENTS.md` (zero regressions, no unprompted UI changes, surgical diffs, verify before you call something done) apply to every step here.

---

## 1. Goal

Turn Lumen into a **highly customisable browser where everything is a plugin**. Users install, enable, disable and configure plugins at runtime to make the browser their own: how it looks, how it behaves, what it blocks, which commands and keys exist, and which panels it shows.

This follows the **DeepSeek Harness / Cordis** model, where the core is a small kernel with no product features and every capability (model, tools, sessions, UI) is a plugin that registers services on a shared context and can be mounted or unmounted without a restart. In Lumen, the "capabilities" are browser features: ad blocking, history, reader mode, the command palette, themes, search engines, keymaps, the sidebar and so on.

### Win condition

1. A kernel (`src/plugin/`) mounts and unmounts plugins at runtime, resolves service dependencies, and tears down every registration a plugin made.
2. Every existing Lumen feature that can be optional is shipped as a **built-in plugin** that can be disabled, and **the default build looks and behaves exactly as it does today**.
3. Users can add third-party plugins of three kinds: **declarative packs** (themes, userstyles, userscripts, keymaps, search engines, filter lists), **sandboxed WASM plugins**, and **web panels**.
4. A **Plugins** settings page lists, toggles, configures and removes plugins, and a `--safe-mode` flag starts Lumen with only core plugins.

### Non-goals (for now)

- **Chrome Web Store / WebExtensions compatibility.** Lumen runs CEF's **Alloy** runtime (see `NOTES.md`: the Chrome runtime crashed in `TabInterface`). CEF deprecated the Alloy extension API around M127, and extensions are only supported under the Chrome runtime. Userscripts, userstyles and WASM plugins cover the common cases. Revisit only if Lumen moves to the Chrome runtime.
- Native `.so`/`dlopen` third-party plugins. They have no sandbox, Rust has no stable ABI, and a crash takes the browser down with it.
- A public plugin registry or marketplace, which is phase 7 and optional.

---

## 2. Research summary (why this design)

| Source | What we take |
|---|---|
| **DeepSeek Harness (dsh)**: "everything is a plugin" on the Cordis kernel. Models, tools, sessions, agent loop and UI are all plugins, swapped through config. | The kernel holds no features. Capabilities are services looked up by key (`ctx.tools`, `ctx.llm`). Built-in "modes" are just preset plugin sets. |
| **Cordis** (Koishi's kernel, which dsh uses; a Rust port `cordis-rs` exists) | **Context** = service registry. **inject** = declared dependencies: a plugin activates only when they exist, deactivates if one disappears, and reactivates when it returns. **effect / dispose** = every registration returns a disposer, and teardown runs them in reverse. **Fiber states** `Pending → Loading → Active → Disposed`. **Event modes**: `emit` (fire-and-forget), `parallel`, `serial`, `waterfall` (middleware with `next`), `bail` (first non-empty result wins). Rule: *events for interception and policy, service methods for direct calls.* Loader config chooses which plugins mount, and a `disabled` flag decides each mount. |
| **CEF / Alloy** | Hook points already exist in `src/engine/callbacks.rs`: `on_before_browse`, `on_before_resource_load`, `on_load_end`, `on_address_change`, `on_title_change`, `on_before_popup`, `on_before_download`, context-menu and permission handlers. JS injection already works (`src/reader.rs` → Readability). Extensions are not available (see Non-goals). |
| **wasmtime component model + WIT** | Typed, versioned host↔plugin interfaces, a capability-based sandbox, fuel/epoch interruption for time budgets, and plugins written in any language. Preferred over Extism because WIT gives us a typed API contract we can version. |
| **Browser customisation prior art**: qutebrowser (userscripts, config-as-code), Vivaldi/Zen mods (CSS mods), Arc Boosts (per-site CSS/JS), Greasemonkey (userscripts with `@match`) | The cheapest and most loved customisations are **declarative**: per-site CSS/JS, themes and keymaps. Ship these first, before the WASM runtime. |

---

## 3. Current architecture (baseline, verified 2026-11-10)

- Rust 2024, `gpui-kit = 0.9.0` UI, `cef = 156.4.0` in OSR mode, X11 child path on Linux.
- `src/app/state.rs`: the `Lumen` god-struct (spaces, address bar, palette, menus, engine, preferences).
- `src/app/actions/*`: `impl Lumen` blocks for tabs, spaces, folders, navigation, shortcuts, palette and resize.
- `src/app/render*.rs`: GPUI composition (shell, sidebar, context menus, dialogs).
- `src/engine/`: CEF boundary. `callbacks.rs` holds every CEF handler, and engine events go out through `EngineEvents`.
- Services: `shield.rs` (adblock 0.13.5 on a worker thread, HTTPS-first), `history.rs` (SQLite), `reader.rs`, `lifecycle.rs` (discard), `import.rs`, `persist.rs`, `settings.rs` (`BrowserPreferences`, `SearchEngine` enum, zoom), `stats.rs`.
- UI tokens: `src/ui/visual.rs` (`UI_SCALE`, geometry consts, `SpaceTheme`), and `src/ui/motion.rs` (springs).
- Palette actions are a hard-coded `PaletteAction` enum plus an `ACTIONS` table (`src/ui/command_palette.rs`).
- Shortcuts are hard-coded in `src/app/actions/shortcuts.rs`.
- Layering rules in `refactor.md` (UI doesn't call CEF, the engine doesn't mutate app state). **Keep them.** The plugin kernel sits beside `app`, and plugins reach the engine only through services.

Before phase 1, record the baseline: `cargo build --release`, `cargo test`, `cargo clippy --all-targets`. Write the pass/fail counts in the phase 0 status line.

---

## 4. Target architecture

```text
            ┌──────────────────────────── Lumen process ─────────────────────────────┐
            │  main.rs → app::Lumen (GPUI)                                           │
            │      │  emits/handles                                                  │
            │      ▼                                                                 │
            │  plugin::Kernel ── Context (service registry + event bus + effects)    │
            │      │                                                                 │
            │      ├── core plugins (always on): engine, sessions, ui-shell, settings│
            │      ├── built-in plugins (toggleable): shield, history, reader,       │
            │      │     palette, search-engines, lifecycle, import, stats, ...      │
            │      ├── pack loader   → themes / userstyles / userscripts / keymaps / │
            │      │                   search engines / filter lists (TOML/CSS/JS)   │
            │      ├── wasm host     → sandboxed third-party plugins (wasmtime, WIT) │
            │      └── panel host    → web panels in CEF views (lumen://plugin/...)  │
            └────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Kernel concepts (Rust port of Cordis, kept small)

```rust
// src/plugin/mod.rs (sketch, not final API)
pub trait Plugin: 'static {
    fn manifest(&self) -> &Manifest;                 // id, version, inject, provides
    fn apply(&mut self, ctx: &mut PluginCtx) -> Result<(), PluginError>;
    // No explicit dispose: everything registered via ctx is auto-disposed.
}

pub struct PluginCtx<'k> { /* scoped to one plugin fiber */ }
impl PluginCtx<'_> {
    pub fn provide<S: Service>(&mut self, key: ServiceKey<S>, svc: S);   // ctx.provide
    pub fn get<S: Service>(&self, key: ServiceKey<S>) -> Option<&S>;     // injected services
    pub fn on<E: Event>(&mut self, handler: E::Handler, order: Order);   // emit/serial/...
    pub fn waterfall<E: Waterfall>(&mut self, mw: E::Middleware, order: Order);
    pub fn contribute(&mut self, c: Contribution);   // command, keybinding, slot item, setting
    pub fn effect(&mut self, setup: impl FnOnce() -> Disposer);          // generic cleanup
    pub fn config<T: DeserializeOwned>(&self) -> T;  // validated against manifest schema
}
```

- **Fiber state machine**: `Pending` (waiting on `inject`) → `Loading` → `Active` → `Disposed`, plus `Failed { reason }`. An unmet required inject keeps the fiber `Pending` without failing. A missing optional inject still activates.
- **Disposal**: every `provide/on/waterfall/contribute/effect` call pushes a disposer onto the fiber's stack. Unmount pops them in reverse. Removing a service cascades: dependents go back to `Pending`.
- **Event modes** match Cordis (`emit`, `parallel`, `serial`, `waterfall`, `bail`). Each event declares exactly one mode in its type.
- **Threading**: the kernel lives on the GPUI main thread. Events raised on CEF IO/UI threads (e.g. `on_before_resource_load`) never call plugin code synchronously on those threads. Instead they use **precompiled matchers** that plugins register (like `shield.rs`'s worker today) or a bounded async channel with a deadline. See §4.5.
- **Determinism**: ordering is `Order::Before(id) / After(id) / Priority(i32)`, and ties break by plugin id. Never rely on load order.

### 4.2 Plugin kinds

| Kind | Code? | Sandbox | Examples | Phase |
|---|---|---|---|---|
| **Core** | Rust, compiled in | n/a | engine, sessions/persist, ui-shell, settings, plugin-manager | 1 |
| **Built-in** | Rust, compiled in, toggleable | n/a | shield, history, reader, palette, search-engines, lifecycle, import, stats, split-view, downloads | 1–2 |
| **Pack** (declarative) | none (TOML + CSS/JS assets) | page JS runs in page context, scoped by `match` | themes, userstyles, userscripts, keymaps, search engines, filter lists, layout/motion presets | 3 |
| **WASM** | any language → `wasm32-wasip2` component | wasmtime, capability permissions, fuel/epoch limits | tab sorter, custom new-tab logic, URL rewriters, auto-tab-groupers, AI sidebar | 5 |
| **Web panel** | HTML/JS served from plugin dir | CEF view, separate request context, `lumen.*` JS bridge limited by permissions | notes panel, RSS, music controls, dashboards | 6 |

### 4.3 Manifest: `lumen-plugin.toml`

```toml
[plugin]
id          = "dev.example.tidy-tabs"   # reverse-DNS, unique, immutable
name        = "Tidy Tabs"
version     = "0.3.1"                    # semver
api         = "1"                        # Lumen plugin API major version
kind        = "wasm"                     # core | builtin | pack | wasm | panel
entry       = "tidy_tabs.wasm"           # wasm/panel only
description = "Groups stale tabs into a folder."
authors     = ["..."]
license     = "MIT"

[inject]                                  # services this plugin needs
required = ["tabs", "spaces"]
optional = ["history"]

[provides]                                # services this plugin registers
services = []

[permissions]                             # shown to user on install; enforced by host
tabs     = "read-write"                  # none | read | read-write
history  = "read"
network  = []                             # allowed fetch hosts for wasm/panel
pages    = ["https://*.example.com/*"]   # where it may inject/observe
storage  = true                           # private KV store
clipboard = false

[[contributes.commands]]
id = "tidy-tabs.run"; title = "Tidy tabs now"; palette = true

[[contributes.keybindings]]
command = "tidy-tabs.run"; key = "ctrl-shift-t"; when = "!input"

[[contributes.settings]]
key = "stale_minutes"; type = "integer"; default = 60; min = 5; max = 1440
title = "Minutes before a tab counts as stale"
```

Packs use the same file with `kind = "pack"` and a `[pack]` table (see §6).

### 4.4 Services and events (the plugin API surface)

Core services, which phase 1 extracts from existing code. Names are stable API keys:

| Key | Backing code today | Main methods |
|---|---|---|
| `tabs` | `app/actions/tabs.rs` | list, open, close, activate, move, pin, favorite, split |
| `spaces` | `app/actions/spaces.rs` | list, create, switch, update theme |
| `folders` | `app/actions/folders.rs` | create, rename, move, collapse |
| `navigation` | `navigation/`, `app/actions/navigation.rs` | navigate, back/forward, reload, normalize address |
| `engine` | `engine/` | execute_js(tab, script), zoom, find, devtools (no raw CEF types leak) |
| `history` | `history.rs` | query, record, delete |
| `settings` | `settings.rs` | get/set typed keys, register schema |
| `theme` | `ui/visual.rs` | current tokens, register theme, set per-space theme |
| `commands` | `ui/command_palette.rs` + shortcuts | register command, execute, list |
| `keymap` | `app/actions/shortcuts.rs` | bind, unbind, resolve(key, context) |
| `ui.slots` | render modules | contribute items to named slots (§4.6) |
| `storage` | new | per-plugin KV in `$ANARC_DATA_DIR/plugins/<id>/kv.sqlite` |
| `notify` | new | toast/status messages (styled with existing tokens) |

Events, which map onto existing CEF callbacks and app actions:

| Event | Mode | Raised from |
|---|---|---|
| `navigation.before` (url, tab, is_user) → allow / redirect / cancel | waterfall | `on_before_browse`, address submit |
| `address.submit` (text) → URL | waterfall | `navigation::submit_address*` (search engines, bangs, aliases) |
| `address.suggest` (text) → suggestions | parallel + merge | `navigation/suggestions.rs` |
| `request.filter` | **matcher registration only** (§4.5) | `on_before_resource_load` |
| `page.load_end` (tab, url) | emit | `on_load_end` → userscripts run here |
| `page.title` / `page.address` / `page.favicon` | emit | display handler |
| `tab.created/activated/closed/discarded` | emit | tab actions, `lifecycle.rs` |
| `space.switched` | emit | space actions |
| `popup.before` (url) → tab / window / block | bail | `on_before_popup` |
| `download.before` (name, url) → path / cancel | waterfall | `on_before_download` |
| `context_menu.build` (target) → items | serial | `on_before_context_menu` + GPUI menus |
| `permission.request` (origin, kind) → grant/deny/ask | bail | permission handler |
| `app.ready` / `app.quit` | parallel | bootstrap / shutdown |

### 4.5 Hot paths and threading (do not skip)

- `on_before_resource_load` runs per request on CEF's IO thread. Plugins **never** run arbitrary code there. They register **declarative rules** (adblock filter text, host allow/deny lists, header rules), which the shield worker compiles into its matcher. This extends the existing `shield.rs` design (see heald decision "Use Brave adblock matcher on an isolated worker").
- `navigation.before` and `download.before` cross to the main thread over the existing engine event channel. WASM handlers get a **5 ms epoch deadline**. On timeout the event falls through with the default result, and the plugin is flagged; three strikes auto-disable it with a notification.
- Rendering never waits on a plugin. Slot contributions are cached data that the plugin updates, and `render()` only reads the cache.

### 4.6 UI customisation without breaking the design system

`AGENTS.md` forbids unprompted visual changes, so plugins **do not draw arbitrary pixels in native chrome**. They customise through:

1. **Theme tokens.** Lift the constants in `ui/visual.rs` / `ui/motion.rs` (colours, `UI_SCALE`, radii, row heights, spring configs, fonts) into a `ThemeTokens` struct whose **defaults are exactly today's values**. A theme pack overrides tokens, and per-space themes keep working through `SpaceTheme`.
2. **Slots.** Named mount points rendered with existing components only: `sidebar.top`, `sidebar.bottom`, `sidebar.section`, `topbar.left`, `topbar.right`, `address.actions`, `statusbar`, `newtab`, `context_menu.tab/folder/space/page`, `settings.section`, `palette`. A slot item is data (`{icon, label, badge, tooltip, command}`) that the host styles.
3. **Layout presets** (a pack option): sidebar left/right, collapsed by default, top bar hidden or compact, density. Each is a token or flag with today's behaviour as the default.
4. **Web panels** (phase 6) for rich custom UI, inside a sidebar or split pane, isolated in their own CEF view.
5. **Page-level customisation** through userstyles and userscripts (phase 3), which are unrestricted in look because they only touch web content.

Adding a slot to an existing view is a surgical integration. The slot renders nothing when empty, so default screenshots stay identical.

### 4.7 On-disk layout and config

```text
$ANARC_CONFIG_DIR/plugins.toml          # enabled set + per-plugin config (user-editable)
$ANARC_DATA_DIR/plugins/<id>/           # installed third-party plugin (manifest + assets)
$ANARC_DATA_DIR/plugins/<id>/kv.sqlite  # plugin private storage
```

```toml
# plugins.toml
profile = "default"            # optional preset: minimal | default | power (like dsh modes)

[plugins."lumen.shield"]
enabled = true
[plugins."lumen.shield".config]
https_first = true

[plugins."dev.example.tidy-tabs"]
enabled = false
```

- A missing `plugins.toml` means today's behaviour: every built-in that is on today stays enabled.
- Unknown keys are preserved on rewrite, so the file round-trips without losing user data.
- **Profiles** are named plugin sets (`minimal` = core plus shield; `default` = today; `power` = default plus extra built-ins), like dsh's Standard/Code/Minimal modes.
- `lumen --safe-mode` loads core plugins only and never writes `plugins.toml`.
- `lumen --plugin-dev <path>` loads one unpacked plugin from a path and hot-reloads it when files change.

---

## 5. Phased execution plan

Each phase is shippable on its own. **Do not start phase N+1 until phase N's acceptance criteria pass.** Never mix file moves with behaviour changes in one commit (rule from `refactor.md`).

### Phase 0: Baseline and scaffolding
- [x] Record the automated baseline: `cargo build --release` passed; `cargo test` passed (93 tests after repairing the existing test fixture); `cargo clippy --all-targets` passed with five existing dead-code warnings.
- [x] Manual baseline smoke: owner confirms the existing tab/search/split/space/reader/ad-block interactions work; no repeat smoke was needed for the Phase 0 kernel scaffold.
- [x] Add `src/plugin/{mod.rs, kernel.rs, fiber.rs, context.rs, event.rs, manifest.rs, registry.rs}` with no production callers (compiled only for unit tests in this phase).
- [x] Unit-test the kernel in isolation: dependency activation and transitive cascade, pending→active on service arrival, reverse-order disposal, every event mode, ordering ties, and panics in `apply` → `Failed` using `catch_unwind`.
- **Acceptance**: tests pass, the app binary is unchanged in behaviour, and the kernel is under ~800 lines.
- **Status**: complete — scaffold and automated checks pass; production kernel modules total 584 lines; existing baseline interactions were owner-confirmed.

### Phase 1: Kernel in the app, core services extracted
- [x] Construct the plugin host/kernel in `Lumen::new` and store it on `Lumen`.
- [x] Wrap existing functionality as **service facades** (`tabs`, `spaces`, `navigation`, `engine`, `settings`, `history`, `commands`, `keymap`, `theme`) that delegate to the current `impl Lumen` methods. Do not rewrite them.
- [x] Raise the events in §4.4 from the existing call sites. With no listeners, behaviour is identical.
- [x] Replace the hard-coded `PaletteAction`/`ACTIONS` and shortcut table with **registrations made by a core plugin** that produce the same items, labels, order and keys. Keep `PaletteAction` as the internal command id so existing tests still compile.
- [x] Add `plugins.toml` load/save (§4.7), with defaults equal to today.
- **Acceptance**: all existing tests still pass, plus new tests that palette items and shortcuts match a snapshot taken before the change, and a manual smoke run matches the baseline.
- **Status**: complete — kernel/services/config and core registrations are in the app; event notifications are wired from app actions and queued CEF callbacks; palette/shortcut snapshot and 104-test suite pass; release build and isolated X11 first-frame smoke pass. Policy-changing CEF hooks remain observational until the typed, deadline-aware decision bridge described in §4.5 is implemented (see `NOTES.md`).

### Phase 2: Built-ins become toggleable plugins
Port one at a time, one PR each, in this order (lowest risk first): `stats` → `reader` → `import` → `lifecycle` → `history` → `search-engines` → `shield` → `downloads` page → `split-view`.
- [ ] Each built-in is a `Plugin` impl in `src/plugins/<name>.rs` that `inject`s what it needs and registers its commands, settings, events and slots.
- [ ] `search-engines`: turn the `SearchEngine` enum into a registry. The Google/Bing/DuckDuckGo entries ship as built-in registrations, and `browser-preferences.json` keeps deserialising (keep the enum's serde names as ids).
- [ ] `shield`: expose `request.filter` rule registration, and the existing EasyList + `$ANARC_DATA_DIR/filters/*.txt` load becomes the plugin's default config.
- [ ] Disabling a plugin at runtime must fully unwind: commands vanish from the palette, keys unbind, hooks stop, and slot items disappear with no restart.
- **Acceptance**: for each plugin, a test that enabled = baseline behaviour, a test that disabled = feature cleanly absent, and a test that re-enable works. `--safe-mode` boots.
- **Status**: in progress — all nine optional built-ins register through `PluginHost`, have runtime gates, default-on config behavior, and safe-mode handling. Runtime disable/re-enable and legacy search-engine serde are covered. Remaining: formal `request.filter` registration and fuller per-plugin behavior/config tests.

### Phase 3: Declarative packs (the biggest user-visible win)
- [ ] Pack loader: scan `$ANARC_DATA_DIR/plugins/*/lumen-plugin.toml` with `kind = "pack"` and validate it, rejecting bad packs with a clear error and never crashing.
- [ ] `[pack.theme]`: `ThemeTokens` overrides (dark/light variants, per-space accent support).
- [ ] `[[pack.userstyle]]`: `{ match = ["https://*.youtube.com/*"], css = "yt.css", run_at = "document_start" }`, injected via `engine.execute_js` at load. Re-apply on SPA navigations (`page.address`).
- [ ] `[[pack.userscript]]`: Greasemonkey-style `match/exclude/run_at`, in the page main world. Show a clear install-time warning that userscripts can read the pages they match.
- [ ] `[pack.keymap]`, `[[pack.search_engine]]` (name, url template with `%s`, keyword/bang), `[[pack.filter_list]]` (local file or URL with refresh interval), and `[pack.layout]` (§4.6 presets).
- [ ] Hot reload: editing a pack's files re-applies it (file watcher only in `--plugin-dev`; otherwise on toggle).
- **Acceptance**: sample packs in `examples/plugins/` (a theme, a YouTube userstyle, a vim-like keymap, a search pack with bangs) work end to end, and a broken pack shows an error without breaking startup.
- **Status**: in progress — safe manifest scanning, keymap/search-engine registration, local filter-list application, matched userstyle/userscript injection on `page.load_end`, theme token overrides (`border_hover`, colors, card radius), shell layout presets (`sidebar_side`, `sidebar_collapsed`, `topbar`), and four examples are wired. Remaining: remote filter fetching, SPA/document-start timing, and dev hot reload.

### Phase 4: Plugin manager UI
- [ ] Add `SettingsSection::Plugins` (additive enum variant) and render it in `src/ui/app_pages.rs` **using the existing settings-page components and tokens only**.
- [ ] Per plugin: name, version, kind, description, enable toggle, permissions summary, config form generated from `[[contributes.settings]]`, remove button, and error state (`Failed`/`Pending` with the reason).
- [ ] Install from a local folder or `.lumenplugin` (zip) file, showing a permission prompt before enabling.
- [ ] Palette commands: "Enable plugin…", "Disable plugin…", "Reload plugins", "Open plugin folder".
- [ ] **Ask the user before finalising any new visual element** (AGENTS.md §2): present a text mock in the PR or chat first.
- **Acceptance**: every toggle applies live, survives a restart, and the existing settings sections look pixel-identical.
- **Status**: in progress — approved Plugins settings page, status/error display, live toggles, local folder and signed package review/install, remove, and plugin rows in the Extensions popover are implemented. Remaining: generated config forms, richer enable/disable palette selection and reload, and a manual desktop UI check.

### Phase 5: WASM plugin runtime
- [ ] Add `wasmtime` (component model) behind a cargo feature `wasm-plugins` (on by default once stable). Pin exact versions like the rest of `Cargo.toml`.
- [ ] Define `wit/lumen.wit`, package `lumen:plugin@1.0.0`, with host imports per service (`tabs`, `spaces`, `navigation`, `history`, `storage`, `notify`, `commands`, `ui-slots`, `http` limited to allowed hosts) and guest exports (`apply`, `on-event`, `run-command`).
- [ ] Gate every import call on the manifest `[permissions]`. Denied calls return an error and are never silently allowed.
- [ ] Limits: epoch interruption (5 ms per event handler, 50 ms per command), fuel cap, 64 MB memory cap, no filesystem except the plugin's own data dir (WASI preopen). A trap marks the fiber `Failed`, and the browser keeps running.
- [ ] Run WASM on a dedicated worker thread; the main thread exchanges messages only.
- [ ] Add a `lumen-plugin-sdk` crate (Rust guest bindings plus macros) and one example plugin in `examples/plugins/tidy-tabs/`.
- **Acceptance**: the example builds with `cargo component build`, installs, runs, is denied a permission it didn't request, and is killed on an infinite loop without freezing the UI.
- **Status**: in progress — pinned Wasmtime/WASI dependencies, WIT, guest SDK/example, bounded worker, permission checks, resource/fuel/deadline limits, package scan, and lifecycle cleanup are implemented behind `wasm-plugins`. The adapter currently denies all host service calls and async worker failures are not reflected in manager status; `cargo component` install/run acceptance remains unverified.

### Phase 6: Web panels
- [ ] Add `kind = "panel"`: a plugin ships `index.html` + assets served from a custom scheme `lumen-plugin://<id>/` (CEF scheme handler) with its own request context.
- [ ] Mount panels in the slots `sidebar.panel` and the split pane. Panel chrome (header, border, radius) comes from host tokens.
- [ ] JS bridge: `window.lumen.*` through `CefMessageRouter`, mirroring the WIT API and checked against the same permissions.
- [ ] Apply a CSP by default, and allow network access only to the hosts in the manifest.
- **Acceptance**: an example "notes" panel persists text through `storage`, and closing or disabling it destroys its browser and frees memory (check with `ANARC_STATS=1`).
- **Status**: in progress — secure `lumen-plugin://` CEF scheme, confined package resources, CSP, HTTPS host restrictions, and plugin mount/dispose registration are implemented. The panel still lacks a dedicated sidebar/split view and `window.lumen` message bridge; no notes example or storage persistence acceptance yet.

### Phase 7 (optional): Distribution
- `.lumenplugin` signing (ed25519, publisher key in the manifest), update checks, a git-repo-based index (like the dsh `dsh-plugin` GitHub topic), and import/export of a whole setup (`plugins.toml` plus packs) so people can share their "Lumen config".
- **Status**: in progress — bounded ZIP inspection, Ed25519 signing/verification, explicit signer review, package install, and export APIs are implemented. Persistent publisher trust, update checks, registry/index, and whole-profile import/export are not implemented.

---

## 6. Pack manifest reference (phase 3)

```toml
[plugin]
id = "me.rover.midnight"; name = "Midnight"; version = "1.0.0"; api = "1"; kind = "pack"

[pack.theme]
mode = "dark"
[pack.theme.tokens]
base = "#0f1014"; surface = "#17181d"; accent = "#9aa6ff"; text = "#e6e7eb"; muted = "#8a8d98"
card_radius = 16; ui_scale = 0.85; font_family = "Inter"
[pack.theme.motion]
spring_stiffness = 380; spring_damping = 32       # matches ui/motion.rs defaults by name

[[pack.userstyle]]
match = ["https://www.youtube.com/*"]; css = "styles/youtube.css"; run_at = "document_start"

[[pack.userscript]]
match = ["https://github.com/*"]; js = "scripts/gh.js"; run_at = "document_idle"

[[pack.search_engine]]
id = "gh"; name = "GitHub"; url = "https://github.com/search?q=%s"; keyword = "!gh"

[pack.keymap]
"ctrl-j" = "tabs.next"; "ctrl-k" = "tabs.prev"; "ctrl-shift-r" = "reader.toggle"

[pack.layout]
sidebar_side = "left"; sidebar_collapsed = false; topbar = "default"
```

Token names must exactly match `ThemeTokens` fields. Unknown tokens produce a warning, not an error.

---

## 7. Engineering rules for agents

1. **Zero regressions.** With an empty or missing `plugins.toml`, Lumen behaves and looks exactly as it does today. Prove it with tests and smoke runs at every phase.
2. **Additive first.** Add facades, events and slots around existing code, and move logic only in dedicated move-only commits.
3. **No visual changes without approval.** New UI (the plugin manager page, permission prompt, error toasts) uses existing tokens and components, and is shown to the user as a text mock before you merge.
4. **Never block the UI or IO threads on plugin code** (§4.5).
5. **Every registration is disposable.** If you add a new kind of contribution, add its disposer and a disable/re-enable test in the same PR.
6. **Permissions are enforced by the host**, never by trusting the plugin. Denied by default.
7. **Pin dependencies** with `=x.y.z` like the existing `Cargo.toml`.
8. **Keep `refactor.md` layering**: plugins → services → app/engine, never plugin → CEF types directly.
9. **Isolate test runs** with `ANARC_CONFIG_DIR/ANARC_DATA_DIR/ANARC_CACHE_DIR` temp dirs (see README) so no real profile is touched.
10. **Update this file**: tick checkboxes, set the phase **Status**, and record deviations in `NOTES.md`.
11. Close every task with a Checkmate status: `done` / `done, unverified: …` / `not done: …` / `blocked: …`.

## 8. Verification checklist (run every phase)

```sh
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings   # if baseline is clean; else no new warnings
```

Manual smoke (isolated profile): new tab → search → open link → split view → switch space (theme transition) → reader toggle → ad blocked on a test page → command palette → shortcuts → restart restores session. Then repeat with `--safe-mode`, and again with every plugin disabled.

## 9. Open questions for the owner

1. Should `power` profile built-ins (e.g. vertical/horizontal tab layouts beyond today's) be designed now or deferred?
2. Should userscripts be allowed by default, or require a global "allow userscripts" toggle?
3. Is moving to a cargo workspace (`lumen-plugin-api`, `lumen-plugin-sdk`) acceptable in phase 5, or should everything stay in one crate?
4. Is a public registry (phase 7) wanted, or only file/git-based sharing?

## 10. References

- DeepSeek Harness overview: https://betterstack.com/community/guides/ai/deepseek-harness-plugin/
- "Everything really is a plugin": https://webkul.com/blog/deepseek-harness-agent-framework/amp/
- Cordis primer (dsh docs): https://cdn.jsdelivr.net/npm/dsh-plugin-guide@0.3.33/references/official-docs/docs/cordis-primer.md
- Cordis core: https://github.com/cordiverse/cordis · Rust port: https://docs.rs/crate/cordis-rs/0.2.2
- CEF Alloy extension deprecation: https://groups.google.com/g/cef-announce/c/s1WaovAopFo
- wasmtime component model: https://docs.wasmtime.dev/ · WIT: https://component-model.bytecodealliance.org/
