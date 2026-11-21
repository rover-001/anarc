---
name: anarc-agent
description: >-
  Anarc Agent Control: five compact tools (see, do, read, find, shot) for
  browser automation with 5x fewer tokens than full accessibility snapshots.
triggers:
  - "browser control"
  - "anarc agent"
  - "anarc-ctl"
  - "automate browser"
---

# Anarc Agent Control

Anarc exposes five high-density, low-token tools over a local socket: `see`, `do`, `read`, `find`, and `shot`.
Control is available via stdio MCP (`anarc-ctl mcp`) or direct CLI (`anarc-ctl <cmd>`).

Never read Anarc source. On any error run `doctor` (CLI: `anarc-ctl doctor`) first. Errors say what to do next.

## Five Core Tools

### 1. `see`
Compact index of what can be acted on.
- **Args**: `scope` = `view` (default) | `all` | `main` | `<ref>` | `+` (next page)
- **Output format**:
  ```text
  g4 github.com/login "Sign in to GitHub" ↓0
  tabs: 1* github.com, 2 about:blank
  i1 "Username or email"
  i2 "Password" secret
  c1 "Remember me" off
  b1 "Sign in" primary
  l1 "Forgot password?"
  !banner b2 "Accept cookies"
  ```
- **Ref types**: `i` input · `b` button · `l` link · `c` checkbox/radio · `s` select · `o` option · `f` file · `x` clickable · `m` media.

### 2. `do`
Execute batch actions using the `do` DSL.
- **Args**: `ops` (semicolon or newline separated)
- **Syntax**:
  - `b1` : Click element `b1`
  - `i1="text"` : Focus, clear, and type into input `i1`
  - `i1+"text"` : Append text without clearing
  - `i2=@secret:github` : Inject secret from local vault (value never enters context)
  - `b:"Sign in"` : Semantic selector resolved at runtime
  - `key Enter` / `key ctrl+a` : Dispatch keypress or keyboard chord
  - `go https://example.com` | `back` | `fwd` | `reload`
  - `scroll down` | `scroll up` | `scroll top` | `scroll end`
  - `hover b1` | `focus i1` | `drag b1 b2` | `xy X Y` (canvas fallback)
  - `dialog ok` | `dialog cancel` | `tab new` | `tab N` | `tab close`
  - `submit` : Press Enter or click the nearest send button
  - `wait idle` | `wait 500` | `wait "Results"` | `wait stable [ms]` at the end of a batch
- **Diff replies**:
  If generation didn't change, returns minimal diff:
  ```text
  ok 3 · g4
  +b5 "Confirm" -b1 ~i2 err "Required"
  ```
  If generation changed (navigation/SPA update), automatically appends new `see` output.

### 3. `read`
Extract page content in structured, budgeted modes.
- **Args**: `target` (ref, `page`, or `last`), `mode` (`text` | `table` | `links` | `outline`), `max` (character limit, default: 4000)

### 4. `find`
Locate up to 5 elements matching text or keyword query.
- **Args**: `q` (search query)

### 5. `shot`
Capture a viewport screenshot. CLI prints a PNG path. MCP returns an image, with `mcp --shot-as path` available for path output.
- **Args**: `target` (ref or `view`), `scale` (0.1 to 1.0)
