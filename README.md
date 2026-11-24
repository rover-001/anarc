<div align="center">

<img src="assets/anarc_logo.png" alt="Anarc logo" width="128">

<a href="https://anarc.pages.dev/#workspace"><img src="assets/anarc-demo.gif" alt="Anarc browser demo showing the Personal space and sidebar"></a>

<h1>Anarc</h1>

<h3>The keyboard-first browser for Linux and Windows, built for you and your AI agents.</h3>

<p>Spaces. Split views. Built-in ad blocking. A native Rust interface.<br>
And a local control layer that lets agents drive the real, visible browser.</p>

<p>
<a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-native-b7410e?style=for-the-badge&logo=rust&logoColor=white" alt="Rust"></a>
<a href="#development"><img src="https://img.shields.io/badge/UI-GPUI%20Kit-4a7090?style=for-the-badge" alt="GPUI Kit"></a>
<a href="#development"><img src="https://img.shields.io/badge/Engine-Chromium%20(CEF)-61a09a?style=for-the-badge&logo=googlechrome&logoColor=white" alt="Chromium CEF"></a>
<a href="#installation"><img src="https://img.shields.io/badge/Linux%20%7C%20Windows-supported-e2c490?style=for-the-badge&labelColor=333" alt="Platforms"></a>
<a href="#agent-control--mcp"><img src="https://img.shields.io/badge/Agents-MCP%20ready-6f42c1?style=for-the-badge" alt="MCP"></a>
</p>

<p>
<a href="https://anarc.pages.dev"><b>Website</b></a> ·
<a href="#installation"><b>Install</b></a> ·
<a href="#agent-control--mcp"><b>Agent Control</b></a> ·
<a href="#keyboard-shortcuts"><b>Shortcuts</b></a> ·
<a href="https://github.com/rover-001/anarc/issues"><b>Report a bug</b></a>
</p>

</div>


## Why Anarc?

Most Linux browsers give you tabs and a toolbar. Anarc gives you a **workspace**.

- **Organized by default.** Group tabs into spaces, each with its own color, folders, and isolated profile.
- **Fast and native.** A Rust interface on GPUI Kit, with 240 fps hardware rendering and spring-based motion.
- **Quiet.** Ads, trackers, and YouTube ad overlays are blocked out of the box.
- **Agent-ready.** A local MCP server and `anarc-ctl` CLI let Claude, Codex, and other harnesses control the same browser you are looking at.


## Features

### Browsing and interface

| | |
| --- | --- |
| 🗂️ **Spaces** | Their own colors, a unified pinned/favorites section, folders, and isolated browser profiles. |
| 🪟 **Split views** | Up to four tabs in a 2×2 grid with draggable dividers. Drag a sidebar tab onto a pane to split, extend, or replace it. |
| ⌨️ **Command palette** | Switch tabs, search history, and run browser commands without leaving the keyboard. |
| 🛡️ **Shield** | Blocks ads and trackers with uBlock Origin–compatible filter lists and cosmetic element hiding, strips YouTube ad overlays, and upgrades eligible HTTP URLs to HTTPS. Toggle globally or per tab with `Ctrl+Shift+B`. |
| 📖 **Reader mode** | Clean, distraction-free reading with `Ctrl+Alt+R`. |
| 🔖 **Bookmark import** | Bring your bookmarks from Chrome, Chromium, Brave, and Edge. |
| ⬇️ **Downloads** | A Chromium-style recent-download bubble from the toolbar icon (rows can be dragged onto a page to upload them), a full downloads page, an active-download badge, and progress toasts. |
| ⚙️ **Settings** | General, Appearance, Shortcuts (with a full in-app reference), and Plugins. |
| 🧩 **Plugin packs** | Themes, site styles, search engines, and extensions. Sandboxed WebAssembly plugins with the optional `wasm-plugins` build feature, and Ed25519-signed, verifiable packages. |

Also included: trackpad pinch-to-zoom and right-click menus for tabs, spaces, folders, and pages.

### Performance and platform

- **Spring-based motion** with reduced-motion support.
- **240 fps hardware OSR rendering**, vsync-aligned frame dispatch, and full-rate rendering in background windows.
- **Windows support** with native minimize, maximize, and close controls in the top bar.


## Agent control

Anarc is built so an AI agent can use the same browser you do, visibly, and under a policy you control.

```sh
anarc-ctl see                    # compact index of interactable elements
anarc-ctl do 'click @b3; key Enter'
anarc-ctl read page text
```

| Capability | What it gives you |
| --- | --- |
| **Local MCP server** | `anarc-ctl mcp` exposes `see`, `do`, `read`, `find`, `shot`, `tabs`, `tab`, `upload`, `doctor`, `status`, `pause`, `resume`, `update`. |
| **Live CDP bridge** | Typed element refs, coordinate clicks, DOM and file-input actions, diff-only action output, and semantic ref recovery when the page changes. |
| **Action DSL** | Key chords and modifiers, `@file:` text injection, multi-file and glob uploads, drag-and-drop ingestion, and tab actions (`pin`, `fav`, `close`, `new`, `select`). |
| **Policy gate** | Asks for confirmation on sensitive intents such as sending a message, plus navigation restrictions. |
| **Resilient runtime** | Socket activation via `anarc.socket`, on-demand start with `anarc-ctl start`, stale-socket recovery, `doctor` diagnostics, and up to 9 isolated instances. |
| **Plugin MCP server** | `anarc_plugin_*` tools (with `lumen_*` aliases) to scaffold, validate, build, install, toggle, and package plugins, and to query spaces and history. |

Everything stays on your machine. [Jump to the full setup guide](#agent-control--mcp).


## Installation

> [!NOTE]
> There are no prebuilt binaries yet. Anarc builds from source on Linux and Windows.

### Linux

Requirements: a current stable Rust toolchain, a C/C++ compiler, CMake, `pkg-config`, and the X11 and keyboard development libraries. On a Wayland desktop, install **XWayland** so Anarc can use its X11 backend.

<details>
<summary><b>Install build dependencies</b></summary>


**Arch Linux**

```sh
sudo pacman -S --needed base-devel cmake pkgconf libx11 libxcursor libxrandr libxi libxkbcommon libxkbcommon-x11
```

**Debian and Ubuntu**

```sh
sudo apt install build-essential cmake pkg-config libx11-dev libxcursor-dev \
  libxrandr-dev libxi-dev libxkbcommon-dev libxkbcommon-x11-dev
```

**Fedora**

```sh
sudo dnf install gcc gcc-c++ make cmake pkgconf-pkg-config libX11-devel libXcursor-devel \
  libXrandr-devel libXi-devel libxkbcommon-devel libxkbcommon-x11-devel
```

</details>

Then clone and install:

```sh
git clone https://github.com/rover-001/anarc.git
cd anarc
./install.sh
```

The installer builds Anarc, installs the CEF runtime, and adds a desktop launcher. Open **Anarc** from your app launcher or run `~/.local/bin/anarc`.

- The first build downloads the CEF version pinned in [`Cargo.lock`](Cargo.lock).
- Already have the matching CEF distribution? Set `CEF_PATH` to the extracted directory containing `libcef.so` and the CEF archive metadata, then run the installer.
- Run `./install.sh --help` for all installer options.

To run directly from the checkout:

```sh
cargo run --locked --release
# with WebAssembly plugin support
cargo run --locked --release --features wasm-plugins
```

#### H.264 / AAC video playback (Linux)

The CEF download used by `cef-dll-sys` is the Spotify CDN *minimal* build, which has proprietary codecs compiled out. Video editors and other web apps then fail to preview ordinary H.264 MP4 screen recordings (`canPlayType('video/mp4; codecs="avc1…"')` is empty). `install.sh` therefore installs a checksum-pinned CEF build with H.264/AAC enabled by default; pass `--no-codecs` to keep the stock minimal runtime. For an existing installation, run the swap directly:

```sh
tools/install-cef-codecs.sh
```

Restart Anarc afterwards. The script backs up the original runtime to `~/.local/share/anarc-cef-minimal-backup`. If you build CEF yourself, any compatible build with `proprietary_codecs=true ffmpeg_branding=Chrome` works through `CEF_PATH`. Review the patent-licensing obligations for H.264/AAC before redistributing a codec-enabled build.

#### Updates (Linux)

After the first install you never need to clone the repo or run `./install.sh` again. `install.sh` installs `anarc-update` and, when systemd is available, a daily timer that downloads the newest GitHub release, verifies its published SHA-256 digest, and installs it. Updates are applied only while Anarc is closed (swapping the in-use CEF runtime would be unsafe); dev/source builds are left alone unless you force an update.

```sh
anarc-ctl update --check       # report only; exit 10 means a newer release exists
anarc-ctl update               # install the latest signed release
anarc-ctl update --force       # replace even a dev build / update while running
anarc-update --rollback        # restore binaries from before the last update
anarc-update --disable-auto    # stop scheduled updates
anarc-update --enable-auto     # resume them
```

On installations made before the updater existed, bootstrap it once without cloning:

```sh
curl -fsSL https://raw.githubusercontent.com/rover-001/anarc/main/tools/anarc-update.sh -o /tmp/anarc-update.sh
sh /tmp/anarc-update.sh
```

Pass `ANARC_UPDATE_API` to check a mirrored release endpoint. Pass `--no-auto-update` to `./install.sh` to skip the systemd timer (you can still run `anarc-ctl update` manually).

### Windows

There is no Windows installer yet. Build from source with the MSVC toolchain.

Requirements: stable Rust with the `x86_64-pc-windows-msvc` target, Visual Studio Build Tools (C++ workload and Windows SDK), CMake, and Ninja. In PowerShell:

```powershell
git clone https://github.com/rover-001/anarc.git
cd anarc
cargo build --locked --release
```

Run `target\release\anarc.exe`. The CEF runtime files (`libcef.dll`, `*.pak`, `locales\`, etc.) are copied next to the executable by the CEF build script, so ship them together in any archive. `anarc-ctl.exe` works on Windows through a loopback endpoint in the per-user data directory.

<details>
<summary><b>Cross-building Windows from Linux (experimental)</b></summary>


CEF's build script supports Linux → Windows MSVC cross-builds through [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin). The Windows GNU target is not accepted, so use the MSVC target. On Arch Linux:

```sh
rustup target add x86_64-pc-windows-msvc
sudo pacman -S --needed llvm lld ninja
cargo install --locked cargo-xwin --version 0.24.1
export XWIN_ARCH=x86_64
```

Two source changes are required before building:

1. **`build.rs`**: the `embed_resource::compile("assets/anarc.rc", …)` call is wrapped in a host-side `#[cfg(target_os = "windows")]`, which is false on a Linux host. Remove that `#[cfg]` (keep the existing `CARGO_CFG_TARGET_OS == "windows"` guard) so the icon and version metadata are embedded with `llvm-rc`. Skipping this only affects file metadata.
2. **`vendor/gpui-pre-windows/build.rs`**: release builds include an `OUT_DIR/shaders_bytes.rs` generated by the Windows-only `fxc.exe`. Generate it once on a Windows machine with `cargo build --release -p gpui-pre-windows`, copy the file to `vendor/gpui-pre-windows/src/shaders_bytes.rs`, and add a Linux-host fallback in that build script that copies it into `OUT_DIR` when `CARGO_CFG_TARGET_OS=windows`. Without this the release build cannot compile.

Then build and package:

```sh
export RUSTFLAGS="-C target-feature=+crt-static"
export XWIN_ARCH=x86_64
cargo xwin build --release --target x86_64-pc-windows-msvc --bins

cd target/x86_64-pc-windows-msvc/release
zip -r ../../../anarc-windows-x86_64.zip \
  anarc.exe anarc-ctl.exe *.dll *.pak *.bin *.json locales/ plugins/
```

The first build downloads the MSVC CRT/SDK through xwin and the Windows CEF distribution, so it is slow. Cross-building proves compilation only. GPU initialization, CEF sandboxing, fonts, notifications, and plugin loading still need a real Windows smoke test. The zero-patch alternative is building on a native Windows machine.

</details>

### macOS

> [!WARNING]
> Anarc is **not natively supported on macOS yet.** `install.sh` refuses to run on macOS, the installer expects a Linux CEF runtime containing `libcef.so`, and there is no macOS app bundle or CEF helper apps. Cross-compiling macOS from Linux is not possible without an Apple SDK, which cannot be redistributed.

The supported option is an x86_64 Linux VM or dual boot, then follow the [Linux instructions](#linux). Building directly on a macOS host is experimental and unsupported: do not run `install.sh`, and expect manual CEF work.


## Agent Control & MCP

Anarc ships two binaries: the browser (`anarc`) and a control CLI (`anarc-ctl`). The CLI talks to a running browser over a local Unix socket on Linux, or a loopback TCP endpoint guarded by a per-user token on Windows. **Nothing is exposed outside the machine.**

### 1. Check the CLI

On Linux the installer puts `anarc-ctl` on your `PATH`. On Windows keep `anarc.exe` and `anarc-ctl.exe` in the same folder (or add that folder to `PATH`).

```sh
anarc-ctl version
anarc-ctl doctor
anarc-ctl update --check
```

`doctor` reports the control endpoint, server version, CEF state, and open tabs. It is the first command to run when a call fails. `update` checks/installs signed releases without a source checkout; see [Updates](#updates-linux).

### 2. Control the browser

```sh
anarc-ctl tabs                          # list tabs and spaces
anarc-ctl see [view|all]                # compact index of interactable elements
anarc-ctl read page text                # read page content
anarc-ctl find "submit button"          # locate elements
anarc-ctl do 'click @b3; key Enter'     # run the action DSL
anarc-ctl shot                          # save a screenshot
anarc-ctl status | pause | resume
anarc-ctl start                         # launch Anarc and wait for the endpoint
anarc-ctl update                        # self-update from GitHub releases
```

Target a tab or instance and shape the output with `--tab <id>`, `--instance <1-9>`, `--socket <path>` (the endpoint file on Windows), `--mode`, `--max`, and `--shot-as path`. Run `anarc-ctl --help` for the full command list.

### 3. Connect your agent

**Browser control** is a stdio MCP server: `anarc-ctl mcp`. Register it with your harness:

```sh
anarc-ctl install --harness claude     # also: codex | antigravity | print
```

This prints the config snippet with `ANARC_CTL_SOCKET` pointed at the active endpoint. The `systemd` harness target is Linux-only; on Windows, start Anarc at logon with Task Scheduler. The `update` tool (`{ "check": true }` to check only, `{ "force": true }` to replace a dev build) mirrors `anarc-ctl update` and works without a running browser.

**Plugin authoring** lives in [`tools/anarc-mcp`](tools/anarc-mcp) (the legacy [`tools/lumen-mcp`](tools/lumen-mcp) is still available). It provides `anarc_plugin_*` tools, with `lumen_*` aliases, for scaffolding, validating, building, installing, toggling, and packaging plugins, plus querying spaces and history. Setup details are in [`tools/anarc-mcp/README.md`](tools/anarc-mcp/README.md).

Both MCP surfaces expose the same tool names and JSON schemas on Linux and Windows.


## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+L` | Focus the address bar |
| `Ctrl+K` | Open the command palette |
| `Ctrl+T` / `Ctrl+W` | Open or close a tab |
| `Ctrl+Shift+T` | Reopen a closed tab |
| `Ctrl+1` … `Ctrl+9` | Jump to tab (`9` = last) |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Cycle through tabs |
| `Ctrl+Shift+S` / `Ctrl+Shift+W` | Split view / add pane pair, or exit split view (max 4) |
| `Ctrl+Alt+←` / `→` | Focus a split pane |
| `Alt+←` / `→` | Back / forward |
| `Ctrl+R` / `Ctrl+Shift+R` | Reload / hard reload |
| `Ctrl+F` | Find on the current page |
| `Ctrl+S` | Toggle the sidebar |
| `Ctrl+Alt+R` | Reader mode |
| `Ctrl+=` / `-` / `0` | Zoom in / out / reset |
| `Ctrl+Shift+B` | Toggle Shield for the active tab |


## Configuration

| Environment variable | Default | Contents |
| --- | --- | --- |
| `ANARC_CONFIG_DIR` | `~/.config/anarc` | Spaces and plugin configuration |
| `ANARC_DATA_DIR` | `~/.local/share/anarc` | Browser preferences, history, and per-space CEF profiles |

Set both to fresh directories to test with a separate profile.


## Development

| Path | What lives there |
| --- | --- |
| [`src/app`](src/app) | Browser actions, application state, and the window shell |
| [`src/engine`](src/engine) | CEF integration, input handling, and page rendering |
| [`src/plugin`](src/plugin), [`lumen-plugin-sdk`](lumen-plugin-sdk), [`wit`](wit) | The plugin runtime and interfaces |
| [`plugins`](plugins) | Shipped plugin packs: theme, YouTube userstyle, vim-style keymap, search pack |
| [`examples/plugins/tidy-tabs`](examples/plugins/tidy-tabs) | WebAssembly guest example (needs `--features wasm-plugins` and a compiled component) |


## Contributing and feedback

Found a bug or want a feature? Open an issue on [GitHub Issues](https://github.com/rover-001/anarc/issues) and include your Linux distribution, desktop environment, and steps to reproduce. Try the **agent control** layer and tell us what your agents get stuck on.

If Anarc is useful to you, a ⭐ on the repo helps other people find it.


<div align="center">

Third-party notices for the bundled reader and filter lists are in [`assets`](assets).

Made with Rust 🦀 by [@rover-001](https://github.com/rover-001)

</div>
