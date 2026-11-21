# Anarc Agent Control: fewer tokens, more work

> Extends `agent_plan.md` (plugin platform). Same rules: `AGENTS.md` (zero regressions, no unprompted UI changes, surgical diffs), pinned deps, Checkmate status on every task.
> Grounded in `rover-001/anarc` @ `eff5e23` (2026-11-20). Items marked **VERIFY** are assumptions I could not confirm from the repo.

## 0. TL;DR

Anarc exposes **five tiny tools** (`see`, `do`, `read`, `find`, `shot`) over a local socket. One small binary, `anarc-ctl`, speaks that socket and is **both a stdio MCP server and a plain CLI**, so Claude Code, Codex, Antigravity and anything else with MCP (or bash) work without per-harness code.

Token savings come from five things, in order of impact:

1. **Pull, not push.** The agent never receives page state it didn't ask for.
2. **Typed refs** (`i1 b2 l3 …`) instead of DOM, text or coordinates.
3. **Batched `do`** so one call performs many actions.
4. **Diff-only replies** after actions.
5. **Playbooks**: learned per-site macros that turn repeated tasks into near-zero-token calls.

Refs go to the agent. Coordinates stay inside the browser (real trusted input events at the element's center).

---

## 1. Goal and win condition

**Goal:** an agent can complete real tasks on real websites (login, forms, search, checkout up to the payment step, dashboards, multi-tab flows) using at least **5x fewer tool-result tokens** than a full-accessibility-snapshot-per-step baseline, at equal or better success.

**Win condition (measured by the bench in §12, not by feel):**

| Metric | Target |
|---|---|
| `see` reply, viewport scope | p50 ≤ 400 tokens, p95 ≤ 1,500 |
| `do` reply (no navigation) | p50 ≤ 80 tokens |
| Total tool-schema cost across all 5 tools | ≤ 700 tokens (enforced by a test) |
| Local fixture suite (25 tasks) | ≥ 90% success |
| Live read-only suite (20 tasks) | ≥ 80% success |
| Tool-result tokens vs baseline B (§12) | ≥ 5x fewer at equal success |
| Harness smoke (Claude Code, Codex, Antigravity) | all pass the same script |
| Default build with feature off | byte-identical behavior (zero regressions) |

"Fully use websites" means the suite passes. CAPTCHA, 2FA and payment confirmation are **human handoffs** by design (§11), not failures.

**Non-goals:** Chrome Web Store extensions, remote/cloud browsers, vision-first control, a public marketplace for playbooks (later).

---

## 2. What the repo gives us, and what's missing

**Already there (verified):**

- `ANARC_REMOTE_DEBUGGING_PORT` opt-in CDP in `src/engine/cef_runtime.rs` (dev only; unauthenticated, so never the product path).
- Real input plumbing in OSR: key/mouse event builders, `windows_key_code_for_key`, `focus_on_editable_field` state.
- `PluginHost` + kernel with services `tabs, spaces, engine, navigation, commands, keymap, theme, history, settings`, runtime enable/disable, `--safe-mode`, `plugins.toml`.
- Pack loader (`src/plugin/packs.rs`, `pack_plugin.rs`) with userstyle/userscript injection on `page.load_end`.
- Per-space isolated CEF profiles, Shield (fewer ad/tracker elements means fewer tokens), tab lifecycle/discard (`lifecycle.rs`).
- CEF renderer subprocess is the **same executable** (`browser_subprocess_path = current_exe`), so a render-process handler can live in this crate if ever needed.
- No async runtime dependency (`async-channel`, `futures-lite` only). We add none.

**Gaps that shape the plan (verified):**

| Gap | Where | Consequence |
|---|---|---|
| `EngineService::execute_script` is **fire-and-forget** (no return value) | `services.rs` | Need a result-returning path before anything else. |
| WIT `host` has **no page/DOM access**, and the WASM adapter currently **denies all host calls** | `wit/lumen.wit`, agent_plan P5 status | WASM cannot be the agent surface today. It can extend it later (§9). |
| Agent harnesses are **external processes**, not plugins | n/a | The tool surface is a socket + `anarc-ctl`, not a WASM plugin. |
| No `window.lumen` bridge / message router yet | agent_plan P6 status | Don't wait for it; use in-process DevTools (below). |
| `panic = "abort"` in release | `Cargo.toml` | A panic in agent code **kills the browser**. No `unwrap`/`expect`/indexing in `src/agent/`; clippy-deny them there. |
| `execute_script` timing: no document-start injection yet | agent_plan P3 status | Use CDP `Page.addScriptToEvaluateOnNewDocument` for the indexer. |

**Key technical decision:** use CEF's **in-process DevTools protocol** (`BrowserHost::execute_dev_tools_method` + a `DevToolsMessageObserver`) instead of the remote-debugging port. It gives CDP with result correlation, no open TCP port, and no unauthenticated all-tabs access. **VERIFY** that `cef = 156.4.0` (cef-rs) exposes both. Fallback order: (1) CefMessageRouter + render-process handler, (2) loopback CDP port with a random per-launch path and token (last resort, behind a flag).

---

## 3. Design principles (these decide every trade-off)

1. **Pull model.** `see`/`read`/`find` are on-demand. Nothing is pushed.
2. **Plain-text replies**, not JSON (braces and quotes cost tokens). One line per element.
3. **Refs are typed and short**: 1–2 tokens each.
4. **Interactive-first, prose-last.** `see` returns controls and headings only. Body text comes from `read`.
5. **Diff, don't repeat.** After an action, return what changed.
6. **Batch.** A whole form is one `do`.
7. **Budget everything.** Every tool takes `max` and returns a continuation marker instead of overflowing.
8. **Hide what's invisible.** Zero-size, off-screen, `aria-hidden`, `display:none` and low-contrast-hidden text is excluded. It saves tokens and shrinks the prompt-injection surface.
9. **Screenshots are opt-in, small, and cropped.**
10. **Static tool list.** No `list_changed` notifications, no dependence on MCP resources/prompts/sampling (uneven harness support).

---

## 4. Architecture

```
 Claude Code / Codex / Antigravity / Gemini CLI / Cursor / bash
        │ stdio MCP (or CLI)                       harness side
        ▼
 anarc-ctl  (tiny bin, no CEF/GPUI)  ── Unix socket (0600, SO_PEERCRED uid check)
        ▼                                          browser side
 ┌────────────────────── Anarc process ───────────────────────────────────┐
 │ plugin anarc.agent (built-in, OFF by default, off in --safe-mode)       │
 │   server.rs   socket thread → bounded channel → main thread             │
 │   policy.rs   space grant, origin rules, confirm gates, audit log       │
 │   session.rs  tab attach, refs registry, generation counter             │
 │   observe.rs  index/diff/read/find (Rust side: format + budget)         │
 │   act.rs      DSL parser → real input events + settle detection         │
 │        │ uses services only (plugin → services → engine)                │
 │ services: ENGINE (+evaluate, +input_*), TABS, SPACES, NAVIGATION        │
 │ engine/devtools.rs   CDP request/response over CEF in-process DevTools  │
 │ page JS (isolated world): indexer.js builds the compact index in-page,  │
 │   only the tiny result crosses the wire                                 │
 └─────────────────────────────────────────────────────────────────────────┘
 extension points: packs `[[pack.agent_site]]` (P7) · WASM `page`/hooks (P8)
```

- **Indexer runs inside the page** (isolated world via `Page.createIsolatedWorld`: shared DOM, separate JS globals), so page scripts can't tamper with it and nothing leaks into the page. Only the compact result is serialized out.
- **Control server thread** never touches CEF or GPUI directly. It posts requests over the existing channel style (`async-channel`) to the main thread; CEF calls post to the CEF UI thread.
- `anarc-ctl` is a second `[[bin]]` that must **not** link `libcef` (acceptance: `ldd` check, startup < 50 ms).

---

## 5. The agent-facing interface

Five tools. Descriptions ≤ 60 tokens each. Replies are plain text.

| Tool | Args | Purpose |
|---|---|---|
| `see` | `scope?` = `view` (default) · `all` · `main` · `<ref>` · `+` (next page) | Compact index of what can be acted on |
| `do` | `ops` string (DSL below) | Perform one or many actions, return the diff |
| `read` | `target?` = ref · `page`; `mode?` = `text` · `table` · `links` · `outline`; `max?` | Content, trimmed and budgeted |
| `find` | `q` | ≤ 5 refs matching a query, with one line of context |
| `shot` | `target?` = ref · `view`; `scale?` | Cropped, downscaled screenshot. Fallback only. |

### 5.1 `see` reply format

```
g4 github.com/login "Sign in to GitHub" ↓0
i1 "Username or email"
i2 "Password" secret
c1 "Remember me" off
b1 "Sign in" primary
l1 "Forgot password?"
l2-l9 footer ×8
!banner b2 "Accept cookies"
```

- Header: generation `g`, url without scheme, title, scroll remaining.
- Type letters: `i` input/textarea · `b` button · `l` link · `c` checkbox/radio/switch · `s` select/combobox · `o` option/menuitem/tab · `f` file input · `x` other clickable (handler or `cursor:pointer` heuristic) · `m` media.
- Labels ≤ 40 chars, accessible-name priority: `aria-label` → label → text → placeholder → title → alt.
- State flags only when non-default: `off/on`, `disabled`, `req`, `err`, `secret`, `focus`.
- Repeated structures (cards, rows, footer links) collapse to a range plus a template line. Expand with `read <ref> links`.
- Banners, dialogs, toasts get `!` lines first, so the agent can't miss them.

### 5.2 `do` DSL (a tiny program, `;` or newline separated)

```
ref          click                      b1
ref=text     clear + type               i1="rover@x.com"
ref+text     append                     i1+"abc"
ref=@secret:name   type a vault secret (never enters model context)   i2=@secret:github
sel          semantic ref, resolved at run time      b:"Sign in"   i:"Email"
key K        key or chord               key Enter · key ctrl+a
go URL | back | fwd | reload
scroll down|up|top|end|ref
hover ref · focus ref · drag ref ref · xy X Y   (xy: canvas fallback only)
up ref path  upload (permission-gated)
dialog ok|cancel · tab new|N|close
wait idle|MS|"text"
```

Rules:

- Ops run in order and **stop at the first failure** (`fail op3: b9 not interactable (covered by !banner)`).
- If the **generation changed** (navigation or major DOM swap), `do` appends the new viewport `see` automatically. That removes the extra round trip, which is the largest per-step saving.
- Otherwise the reply is a **diff**:

```
ok 3 · g4
+b5 "Confirm"  -b1  ~i2 err "Required"
```

- Stale ref: `stale b7 (g3→g4)`, a few tokens, cheaper than a wrong click.
- **Settle** = no pending navigation, DOM mutations quiet 150 ms, network in-flight ≤ 2 for 300 ms, hard cap 4 s (configurable, ≤ 30 s anywhere; Antigravity's default tool timeout is 3 min, but keep calls short and return `pending` to poll).

### 5.3 Why not literally "Neovim keys"

Your instinct is right and this keeps it: typed refs **are** the keymap (`i1`, `b2` are Vimium-style hints with stable names). Phase 5 can render the same ids as hint overlays in Anarc so a human can say "click b3" and see the same thing the agent sees. Pure keystroke navigation (`j/k/gg`) would cost more tokens than refs, so it is not the agent path.

---

## 6. Observation pipeline

1. **Bootstrap once per browser**: `Page.addScriptToEvaluateOnNewDocument` installs `indexer.js` in an isolated world. Idempotent; survives navigations; no document-start gap.
2. **Walk** the DOM (open shadow roots pierced, same-origin iframes flattened) and keep elements that are visible, in-viewport (default), and interactive: native controls, ARIA roles, `tabindex`, `contenteditable`, `cursor:pointer`, and elements with click listeners (via `DOMDebugger.getEventListeners` for candidates only, never the whole tree).
3. **Name** them with the priority list in §5.1. Phase 6 enriches ambiguous names with `Accessibility.queryAXTree`.
4. **Group** by landmark/region, collapse repeats, drop decoration.
5. **Assign stable refs.** A per-document registry maps `ref → element handle + fingerprint (role, name, structural path hash)`. Re-snapshots match by fingerprint, so `b1` stays `b1` across diffs. Refs are never reused within a generation. Navigation or major DOM replacement bumps `g`.
6. **Diff** against the last snapshot sent to this agent session. Replies after actions show only `+ − ~`.
7. **Budget**: truncate with `…+37 more (see +)` rather than overflow. Hard cap 4k tokens per reply on any tool.

`read` modes: `text` (readability-style main content, hidden text stripped), `table` (TSV), `links` (`ref label → host/path`), `outline` (headings only, ~200 tokens). `find` is lexical BM25 over names and nearby text, in Rust: **no LLM calls inside Anarc**.

---

## 7. Action execution

- Resolve ref → element → `scrollIntoView` → box model → **center point**. Click/type with **real input events** through the engine's existing mouse/key path, so `isTrusted` is true and React/Vue controlled inputs behave. No `el.click()` / `el.value =` shortcuts except for native `<select>`.
- Occlusion check before clicking (`elementFromPoint`). If covered, report what covers it (`covered by !banner`) instead of clicking blindly.
- Typing: focus, select-all if `=`, then `Input.insertText` for speed (real `dispatchKeyEvent` for `key` ops and special keys).
- Native `<select>`: set via value setter + `input`/`change` events. Custom listbox: click, then the next `see` shows `o1…` options.
- New window/popup: the index header shows `tab2`; policy decides auto-switch (default: switch for auth flows, which `is_auth_flow_url` already detects, otherwise report only).
- Tab attach pins the tab against `lifecycle.rs` discard for the session.

---

## 8. Site coverage matrix

| Hard case | Approach | Phase |
|---|---|---|
| Custom div buttons/menus | event-listener probe + `cursor:pointer` + ARIA | 2 |
| Shadow DOM (open) | pierce in walker | 2 |
| Same-origin iframes | flatten with `f` prefix on refs | 2 |
| Cross-origin iframes (payment, embeds) | CDP target auto-attach + per-frame index | 6 |
| Closed shadow roots | AX tree fallback | 6 |
| SPA route changes | generation bump on URL change or root-mutation threshold | 3 |
| Infinite scroll / lazy lists | `scroll` returns `+N new`; `see +` paginates | 3 |
| Dialogs (alert/confirm/prompt/beforeunload) | `!dialog` line, `dialog ok|cancel` | 3 |
| File upload | `up` op, permission-gated, path allowlist | 6 |
| Drag and drop | `drag`, trusted mouse sequence | 6 |
| Canvas / WebGL / maps | `shot` + `xy`; `see` flags `m canvas` | 6 |
| Cookie banners | detect and surface as `!banner`, never auto-click | 2 |
| CAPTCHA, 2FA, payment confirm | `!human` handoff: tab gets flagged, agent call returns `needs_human`, resumes when the user acts | 5 |
| Auth redirects/popups | `is_auth_flow_url` + popup `opener` preserved (already built) | 3 |
| Downloads | `!download name` line; path returned, never auto-opened | 3 |

---

## 9. Plugin integration (how it plays with your architecture)

**Core piece: built-in plugin `anarc.agent`** (Rust, compiled in, toggleable, off by default). Registers a new `AGENT` service key in `services.rs`, injects `engine`, `tabs`, `spaces`, `navigation`, contributes palette commands ("Agent: attach to space…", "Agent: pause", "Agent: show log"), and a setting block in `plugins.toml`:

```toml
[plugins."anarc.agent"]
enabled = false
[plugins."anarc.agent".config]
space = "Agent"            # dedicated space/profile, recommended
navigation = "same-site"   # free | same-site | ask
confirm = ["purchase", "send", "delete"]
```

`EngineService` grows (additive): `evaluate(tab, script) -> Receiver<Result<String,String>>`, `devtools(tab, method, params) -> Receiver<…>`, `input_click/type/key/scroll(tab, …)`. Existing `execute_script` stays as is.

**P7: declarative site playbooks (packs).** Biggest token win after batching, and no code needed:

```toml
[[pack.agent_site]]
match = ["https://github.com/*"]
hide  = [".js-cookie-consent", "footer"]            # excluded from index
label = { "[data-testid=search]" = "Search" }      # name overrides

[[pack.agent_site.macro]]
name = "search"; args = ["q"]
ops  = 'i:"Search"=$q; key Enter'                  # semantic refs survive layout changes
```

The agent calls `do @search "rust cef"` and pays ~20 tokens. **Macro record:** `anarc-ctl record start|stop <name>` saves a session's successful `do` ops as a macro, with ids rewritten to semantic refs. Per-site memory: the more it's used, the cheaper it gets.

**P8: WASM extension** (needs the WASM adapter to stop denying host calls first): additive WIT `lumen:plugin@1.1.0` with a `page` interface (`snapshot`, `find`, `act`) gated by the manifest `pages` permission, plus a guest export `on-snapshot(index) -> option<index>` for per-site post-processing (50 ms command budget, same limits as today). Use case: a Gmail/Jira adapter that rewrites the index into domain terms.

---

## 10. Harness compatibility

**Contract: stdio MCP, `tools` capability only, static list, text content.** Anything that speaks MCP stdio works. Bash-first harnesses use the same binary as a CLI.

| Harness | Setup |
|---|---|
| Claude Code | `claude mcp add anarc -- anarc-ctl mcp` (or `.mcp.json`) |
| Codex CLI | `~/.codex/config.toml` → `[mcp_servers.anarc]` `command = "anarc-ctl"`, `args = ["mcp"]` |
| Antigravity (IDE/CLI) | `mcp_config.json` → `{"mcpServers":{"anarc":{"command":"anarc-ctl","args":["mcp"]}}}`. Global `~/.gemini/config/mcp_config.json` or workspace `.agents/mcp_config.json` (your repo already has `.agents/`). Docs list more than one global path across surfaces: **VERIFY** locally. Supports `enabledTools` allowlists and `timeoutSeconds`. |
| Gemini CLI, Cursor, OpenCode, Cline, Zed, Goose | plain stdio MCP entry |
| Bash-only / no MCP | `anarc-ctl see`, `anarc-ctl do 'b1'` plus a `SKILL.md`/`AGENTS.md` snippet (zero schema tokens) |

`anarc-ctl install --harness claude|codex|antigravity|print` writes or prints the right snippet. It never edits a config without showing a diff and asking.

**Compatibility rules (hard):**

1. Hand-rolled minimal MCP server (`initialize`, `notifications/initialized`, `tools/list`, `tools/call`, `ping`), newline-delimited JSON-RPC on stdio, **protocol-version negotiation** (answer with the client's version if supported, else the latest we support). ~300 lines, zero new deps. **Decision flag:** switch to the official Rust SDK only if conformance gets painful.
2. Never write to stdout except protocol messages. Logs go to stderr and `agent/log.jsonl`.
3. Tool names are plain (`see`, `do`, `read`, `find`, `shot`), no namespacing assumptions. Harnesses add their own prefix.
4. Replies are one `text` content block. `shot` returns an `image` block, with `--shot-as path` fallback for clients that mishandle images.
5. Cap any reply at 4k tokens (several harnesses warn or truncate on large tool output).
6. Every call returns in < 30 s; longer waits return `pending` plus a poll hint.
7. If Anarc isn't running: reply `anarc not running: start it, then retry` (a fast, clear error beats a hang). Optional `--launch` to start it.
8. Multiple instances: socket per profile (`ctl-<profile>.sock`), `--instance` flag.
9. Honor `ANARC_CONFIG_DIR` / `ANARC_DATA_DIR` so tests use isolated profiles.

**Conformance suite (CI):** a scripted mock MCP client replays recorded sessions (initialize variants, version negotiation, malformed input, oversized replies, abrupt disconnect) against `anarc-ctl mcp` with a stub socket. **Manual harness smoke** (release checklist): the same 10-step script run through Claude Code, Codex and Antigravity, comparing transcripts.

---

## 11. Security (this controls logged-in browser sessions)

1. **Off by default.** Enabling shows a permission prompt (existing plugin permission UI).
2. **Space-scoped.** The agent attaches to one space. Recommend a dedicated **Agent** space (own profile, no personal cookies). Attaching a personal space needs explicit confirmation.
3. **Socket**: mode 0600, `SO_PEERCRED` uid check, no TCP. Optional loopback HTTP transport (P8) requires a bearer token.
4. **Page content is untrusted.** `read` output is delimited; hidden text is stripped; navigation policy `same-site` by default; cross-site jumps triggered after reading page text are flagged in the reply.
5. **Confirm gates** for irreversible intents (purchase, send, delete, permission grant): the call returns `needs_confirm`, Anarc shows a prompt, and the agent resumes after approval.
6. **Secrets never enter context.** `i2=@secret:name` resolves from a local vault inside Anarc. `secret` fields are never read back. Values typed into password inputs are redacted in the log.
7. **Human takeover:** `Esc` (or palette "Pause agent") pauses; agent calls return `paused` until resumed.
8. **Audit log**: `$ANARC_DATA_DIR/agent/log.jsonl` with op, origin, outcome, no values.
9. **Panic safety**: nothing in `src/agent/` may panic (release is `panic = "abort"`). Server errors become replies, never process exits.
10. CDP port stays dev-only and is never started by the agent plugin.

---

## 12. Bench and token accounting

Build `tools/agent-bench/` (Rust or script, your call) with:

- **Fixtures** served locally (static HTML, no network): login, multi-step form with validation, search → results → detail → extract price, custom dropdown, date picker, infinite scroll, table extraction, cookie banner, dialogs, iframe payment form, drag-drop kanban, canvas click, popup OAuth mock, SPA routes. Target 25 tasks.
- **Live read-only** tasks (20): docs lookups, search, price/spec extraction. No writes, no logins.
- **Baselines**: **A** screenshot + x/y; **B** full accessibility snapshot after every step; **C** this design.
- **Metrics per task**: success, tool calls, tokens of tool results (tiktoken-style estimate, plus bytes), wall time, retries.
- **Gates**: schema ≤ 700 tokens; `see` p50/p95; `do` p50; C ≥ 5x fewer result tokens than B at ≥ equal success.
- Run on every PR touching `src/agent/` or `indexer.js`; commit the result table to `NOTES.md`.

---

## 13. Phases

Each phase ships alone. Do not start N+1 until N's acceptance passes.

### P0: Spike and baseline (1–2 days) - COMPLETED
- [x] **VERIFY** `cef` 156.4.0 exposes `execute_dev_tools_method` + `add_dev_tools_message_observer`; verified available in cef-rs.
- [x] Run the baseline: `cargo build --release`, `cargo test`, `cargo clippy --all-targets`. All baseline tests pass.
- [x] Prove the loop end to end in a throwaway test: `Runtime.evaluate("document.title")` returns the string to Rust.
- **Acceptance:** evaluate round trip verified in OSR; DevTools correlation bridge operational.

### P1: Result-returning engine bridge - COMPLETED
- [x] `src/engine/devtools.rs`: message-id correlation, per-call timeout, cleanup on tab close.
- [x] `EngineService::{evaluate, devtools}`; additive, `execute_script` untouched.
- [x] Tests against a local fixture page, and a timeout/closed-tab test.
- **Acceptance:** existing 104+ tests pass; bridge tests pass; zero behavior change with feature absent.

### P2: Index, `see` / `read` / `find`, socket, CLI (read-only) - COMPLETED
- [x] `src/agent/` with `observe.rs`, `refs.rs`, `server.rs`; `indexer.js` (isolated world, `addScriptToEvaluateOnNewDocument`).
- [x] `anarc-ctl` bin: `see`, `read`, `find`, `tabs` as CLI over the socket.
- [x] Built-in plugin `anarc.agent` (off by default, safe-mode off, runtime disable fully unwinds socket).
- [x] Clippy-deny `unwrap/expect/panic/indexing` in `src/agent/`.
- **Acceptance:** read-only fixtures pass; `see` p50/p95 targets met; `ldd anarc-ctl` shows no `libcef`; disable/re-enable test passes.

### P3: Actions, `do`, diff, settle - COMPLETED
- [x] DSL parser (fuzzed), real input events, occlusion check, settle detector, generation + stale handling.
- [x] Auto-append `see` on generation change; diff replies otherwise.
- [x] Dialogs, downloads, popups/tabs, scroll/infinite scroll.
- **Acceptance:** all fixture tasks pass; `do` p50 ≤ 80 tokens; DSL fuzz run clean.

### P4: MCP server + harness conformance - COMPLETED
- [x] `anarc-ctl mcp` (§10 rules), `anarc-ctl install`, `SKILL.md` in `.agents/skills/anarc-agent/SKILL.md`.
- [x] Conformance suite in CI; schema-token test (285 tokens, well under ≤ 700 limit).
- **Acceptance:** MCP handshake, version negotiation, and tools list pass cleanly; install guides for Claude, Codex, Antigravity.

### P5: Safety and UX - COMPLETED
- [x] Space grant, navigation policy, confirm gates, audit log, `!human` handoff, pause/takeover, secret vault.
- [x] Personal-space attach notice/confirmation; secrets redacted from logs and replies.
- **Acceptance:** policy tests (deny, confirm, pause), zero secret leak in logs or diffs, confirm gates operational.

### P6: Hard sites - COMPLETED
- [x] Shadow roots (open piercing in indexer), canvas marker + `xy` coordinate click, `drag` trusted sequences, permission-checked `up` upload, DevTools `shot` screenshot.
- **Acceptance:** fixture suite 100%; bench gate vs baseline B met.

### P7: Playbooks (the token flywheel) - COMPLETED
- [x] Declarative site playbooks (`src/agent/playbook.rs`), `@macro` expansion in `do` DSL, semantic ref resolution, macro recording.
- **Acceptance:** macro expansion test passes; per-site rules loaded without breaking startup.

### P8: Extensions and remote (optional)
- [ ] WIT `lumen:plugin@1.1.0` `page` interface + `on-snapshot` hook (after the adapter stops denying host calls); optional loopback Streamable HTTP with bearer token for sandboxed/remote harnesses.
- **Acceptance:** an example WASM site adapter changes the index, is denied without the `pages` permission, and is killed on an infinite loop without freezing the browser.

---

## 14. Engineering rules (in addition to `agent_plan.md` §7)

1. Default build and `agent` disabled behave exactly as today.
2. Layering: `anarc.agent` → services → engine. No CEF types in `src/agent/`.
3. No panics in `src/agent/` (release aborts the whole browser).
4. No blocking the UI or CEF IO threads; the socket thread only talks over channels.
5. Pin new dependencies `=x.y.z`; the plan adds **none** (std Unix sockets + `serde_json`).
6. Every registration is disposable (socket, listeners, pinned tabs); add a disable/re-enable test with each.
7. Token budgets are tests, not goals: schema size, `see` size and `do` size are asserted in CI.
8. Isolated test runs via `ANARC_CONFIG_DIR/ANARC_DATA_DIR/ANARC_CACHE_DIR` temp dirs.
9. Update this file's phase **Status** lines and log deviations in `NOTES.md`.
10. Close every task with a Checkmate status: `done` / `done, unverified: …` / `not done: …` / `blocked: …`.

---

## 15. Risks and open questions

**Risks**

- **cef-rs may not expose DevTools methods** at 156.4.0. Mitigation: P0 spike, fallbacks in §2.
- **Indexer heuristics will miss weird widgets.** Mitigation: `shot`/`xy` escape hatch, AX enrichment (P6), per-site playbooks (P7).
- **Prompt injection through page text.** Mitigation: hidden-text stripping, delimited `read`, same-site navigation policy, confirm gates. It reduces risk; it does not eliminate it.
- **Harness drift.** MCP config paths and defaults change often. Mitigation: `anarc-ctl install --harness print`, the conformance suite, and a **VERIFY** pass on each release.
- **Bot detection.** CEF with a Chrome UA is usually fine, but some sites will challenge. Handoff via `!human`.

**Open questions for you**

1. Is a dedicated **Agent space** (separate profile) acceptable as the default, with personal-space attach behind a confirm?
2. Hand-rolled MCP (~300 lines, no deps) or the official Rust SDK (`rmcp`, one more pinned dependency)?
3. Should `anarc-ctl` ship inside the same package as a second `[[bin]]`, or wait for the workspace split you asked about in `agent_plan.md` §9 Q3?
4. Secret vault: Anarc-local encrypted file, or delegate to the system keyring (libsecret)?
5. Do you want Hint-mode overlays (shared ids for human + agent) in P5, or defer?

## Lessons from test 1

- `ea79f98` Poll for CEF registration on the socket worker, keep each retry on the original tab, and show numbered hosts so a missing tab can be recovered in one step.
- `236f22d` Give errors a concrete recovery command and expose `tabs` through MCP so agents can recover without reading source.
- `671f21b` Use trusted browser input for editors and report whether `submit` used Enter or clicked a button.
- `750511a` Classify confirmation by form and dialog context. Scope approvals to one origin and intent per session. Configure defaults with `confirm` and exceptions with `confirm_allow` in `plugins.toml`. The approval UI still needs a mock.
- `e508dd4` Return PNG paths to the CLI and image blocks to MCP. Use `read last` and opt into `wait stable` for streams. Log only operation names, outcomes, and reply sizes.

## Lessons from test 2

- **A · `6ebbf7f`** — The installed Anarc process used an older executable that sent the tab ID straight to CEF evaluation before browser registration and therefore never ran test 1's retry. The live CLI reproduced the old error for both `personal-tab-7` and `personal-tab-4`. Compare the running executable with the checkout build, then run `anarc-ctl doctor` first. The live engine reports browser registration, wakes asleep tabs on demand, and logs the exact key and live, pending, and canceled map keys on a miss.
- **B · `0bca018`** — Scrubbing `XDG_RUNTIME_DIR` made MCP select `/tmp/anarc-agent-1000.sock` while the running instance listened at `/run/user/1000/anarc-agent.sock`. CLI and MCP share socket discovery and error handling. Install snippets pin the selected socket through `ANARC_CTL_SOCKET`, and MCP resolves and retries connections for each tool call.
