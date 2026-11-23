# Anarc

**Anarc is a keyboard-first browser for Linux.** Organize tabs into spaces, keep your go-to sites in the sidebar, and work across two pages in a split view. Anarc uses a native Rust interface built with GPUI Kit and renders web pages with Chromium Embedded Framework (CEF).

[![Anarc browser demo showing the Personal space and sidebar](assets/anarc-screenshot.png)](https://anarc.pages.dev/#workspace)

## Features

- **Spaces** with their own colors, pinned tabs, favorites, folders, and browser profiles.
- **Split views** for viewing tabs side by side with a draggable divider.
- **Command palette** for switching tabs, searching history, and running browser commands.
- **Built-in Shield** for blocking ads and trackers and upgrading eligible HTTP URLs to HTTPS.
- **Reader mode** and an importer for bookmarks from Chrome, Chromium, Brave, and Edge.
- **Plugin packs** for themes, site styles, search engines, and extensions. Sandboxed WebAssembly plugins are available with the optional `wasm-plugins` build feature.

## Installation & Building

Build Anarc from source on Linux. Use a current stable Rust toolchain, a C/C++ compiler, CMake, `pkg-config`, and the X11 and keyboard development libraries. On a Wayland desktop, install XWayland so Anarc can use its X11 backend.

<details>
<summary>Install build dependencies</summary>

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

```sh
git clone https://github.com/rover-001/anarc.git
cd anarc
./install.sh
```

The installer builds Anarc, installs the CEF runtime, and adds a desktop launcher. Open **Anarc** from your app launcher or run `~/.local/bin/anarc`. The first build downloads the version of CEF pinned in [Cargo.lock](Cargo.lock). If you already have the matching CEF distribution, set `CEF_PATH` to the extracted directory containing `libcef.so` and the CEF archive metadata, then run the installer.

Run `./install.sh --help` for installer options.

To run directly from the checkout, use `cargo run --locked --release`. For a build with WebAssembly plugin support, use `cargo run --locked --release --features wasm-plugins`.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+L` | Focus the address bar |
| `Ctrl+K` | Open the command palette |
| `Ctrl+T` / `Ctrl+W` | Open or close a tab |
| `Ctrl+Shift+T` | Reopen a closed tab |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Cycle through tabs |
| `Ctrl+Shift+S` | Split the view |
| `Ctrl+F` | Find on the current page |
| `Ctrl+Shift+B` | Toggle Shield for the active tab |

## Configuration

| Environment variable | Default | Contents |
| --- | --- | --- |
| `ANARC_CONFIG_DIR` | `~/.config/anarc` | Spaces and plugin configuration |
| `ANARC_DATA_DIR` | `~/.local/share/anarc` | Browser preferences, history, and per-space CEF profiles |

Set both variables to fresh directories to test with a separate profile.

## Development

- [`src/app`](src/app) contains browser actions, application state, and the window shell.
- [`src/engine`](src/engine) integrates with CEF and handles input and page rendering.
- [`src/plugin`](src/plugin), [`lumen-plugin-sdk`](lumen-plugin-sdk), and [`wit`](wit) define the plugin runtime and interfaces.
- [`plugins`](plugins) contains the shipped plugin packs (theme, YouTube userstyle, vim-style keymap, search pack) and the Live Share panel. [`examples/plugins/tidy-tabs`](examples/plugins/tidy-tabs) is the WebAssembly guest example; it needs `--features wasm-plugins` and a compiled component to run.

Report bugs in [GitHub Issues](https://github.com/rover-001/anarc/issues). Include your Linux distribution, desktop environment, and steps to reproduce.

Third-party notices for the bundled reader and filter lists are in [assets](assets).
