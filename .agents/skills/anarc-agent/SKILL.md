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
  - `b1` / `click b1` / `click css=button` / `click aria="Send"` : Click element
  - `clickjs b1` / `domclick b1` : Direct DOM `.click()` via CDP for React/SPA links that swallow coordinate clicks
  - `i1="text"` / `fill i1 text` / `fill css=input text` : Focus, clear, and type into input
  - `i1+"text"` / `type append i1 text` / `type i1 text append=true` : Append text without wiping rich tokens (Slate.js/Draft.js)
  - `type i1 text` : Type into input/contenteditable emitting standard `insertText` events
  - `i2=@secret:github` : Inject secret from local vault (value never enters context)
  - `b:"Sign in"` : Semantic selector resolved at runtime
  - `key Enter` / `key "Ctrl+v"` / `key Shift+Tab` : Dispatch keypress or keyboard chord
  - `keydown Shift; key ArrowLeft; keyup Shift` : Hold and release modifiers
  - `navigate https://example.com` / `go https://example.com` | `back` | `fwd` | `reload`
  - `scroll down` | `scroll up` | `scroll top` | `scroll end`
  - `hover b1` | `focus i1` | `drag b1 b2` | `xy X Y` (canvas fallback)
  - `upload [target] /path/file` : Upload file to target or page `<input type=file>`
  - `dialog ok` | `dialog cancel` | `tab new [url]` | `tab select <id|N>` | `tab close`
  - `submit` : Press Enter or click the nearest send button
  - `wait idle` | `wait 500` | `wait "Results"` | `wait stable [ms]` at the end of a batch
- **Diff replies**:
  If generation didn't change, returns minimal diff:
  ```text
  ok 3 · g4
  +b5 "Confirm" -b1 ~i2 err "Required"
  ```
  If generation changed (navigation/SPA update), automatically appends new `see` output.

- **Reliability notes:**
  - `tab select <id>` wakes asleep tabs and waits for `document.readyState != "loading"`; the DOM is indexed implicitly, so no extra `reload` or `go` is needed.
  - `fill` / `<ref>="text"` scrolls the field into view, focuses it, then types. Failures name the reason (`no-field`, `disabled`, `readonly`, `not-editable`, `off-screen`) instead of a generic error.
  - `click <ref>` rejects sticky/fixed overlay occlusion. `xy X Y` is a raw-coordinate fallback with no element identity — prefer typed refs. Use `clickjs <ref>` when an SPA swallows coordinate clicks.
  - **Direct URL routing for complex SPAs** saves interaction turns. Prefer one `navigate <url>` / `go <url>` (or `tab new <url>` for a fresh tab) over multi-step modal clicking:
    - LinkedIn people: `navigate https://www.linkedin.com/search/results/people/?keywords=Parth%20Sharma`
    - LinkedIn compose: `navigate https://www.linkedin.com/messaging/compose/?recipient=<encoded-name>`
    - Gmail compose: `navigate https://mail.google.com/mail/u/0/?view=cm&fs=1&to=<email>&su=<subject>&body=<body>`
    - GitHub: navigate straight to `/issues`, `/pulls`, or `/repos/<owner>/<repo>`.
    - Query strings containing `;` must be quoted: `navigate "https://example.com/?a=1;b=2"`.
  - Cold start: `anarc-ctl start` launches Anarc detached and waits for its socket. MCP/CLI calls also attempt auto-spawn when the socket is missing.

### 3. `read`
Extract page content in structured, budgeted modes.
- **Args**: `target` (ref, `page`, or `last`), `mode` (`text` | `table` | `links` | `outline`), `max` (character limit, default: 4000)

### 4. `find`
Locate up to 5 elements matching text or keyword query.
- **Args**: `q` (search query)

### 5. `shot`
Capture a viewport screenshot. CLI prints a PNG path. MCP returns an image, with `mcp --shot-as path` available for path output.
- **Args**: `target` (ref or `view`), `scale` (0.1 to 1.0)

### 6. `upload`
Upload local files to target element or page file input without native OS dialog friction.
- **Args**: `files` (array of file paths), `target` (optional ref e.g. `i1` or CSS selector e.g. `css=input[type=file]`), `tab_id` (optional)
