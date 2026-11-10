# Anarc

<div align="center">
  <img src="assets/anarc_logo.png" alt="Anarc Logo" width="128" height="128" />
  <h3>Arc-style keyboard-centric native browser built with Rust, GPUI Kit, and CEF</h3>
</div>

---

**Anarc** is a fast, minimal, keyboard-first browser designed for Linux. It combines the ergonomic spaces, pinned tabs, and split-screen workflow of Arc with the high-performance GPU-rendered UI of GPUI Kit and the web compatibility of Chromium Embedded Framework (CEF).

---

## Features

- **Spaces & Folders**: Seamlessly organize work into isolated spaces with per-space theme accents, separate CEF profiles, and pinned folders.
- **Fast GPU-accelerated UI**: Fluid animations, responsive layouts, and spring physics rendered natively via GPUI.
- **Split Views**: Open multiple tabs side-by-side with live draggable dividers and synchronized keyboard focus.
- **Command Palette & Quick Switcher**: Rapid tab jumping, history searches, and browser command dispatching.
- **Built-in AdBlock Shield**: High-speed EasyList-compatible blocking running on an isolated background worker.
- **Extensible Plugin Platform**: Cordis-style microkernel architecture supporting declarative packs, custom user styles, search engines, and sandboxed WASM plugins.

---

## Prerequisites

### 1. Rust Toolchain

Ensure you have Rust stable (edition 2024 support, Rust 1.85+) installed:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. System Dependencies (Linux)

Anarc embeds native child windows and uses system X11 / Wayland interfaces and SQLite. Install the required development packages for your distribution:

#### Arch Linux / EndeavourOS
```sh
sudo pacman -S --needed base-devel libx11 libxcursor libxrandr libxi libxkbcommon pkgconf sqlite
```

#### Ubuntu / Debian / Pop!_OS
```sh
sudo apt-get update
sudo apt-get install -y build-essential libx11-dev libxcursor-dev libxrandr-dev libxi-dev \
    libxkbcommon-dev libxkbcommon-x11-dev pkg-config libsqlite3-dev
```

#### Fedora
```sh
sudo dnf install -y @development-tools libX11-devel libXcursor-devel libXrandr-devel \
    libXi-devel libxkbcommon-devel libxkbcommon-x11-devel pkgconf-pkg-config sqlite-devel
```

---

## CEF (Chromium Embedded Framework) Setup

Anarc pins `cef = 156.4.0+156.0.26`.

### Option A: Automatic Download (Default)
When building without `CEF_PATH` specified, the `cef-dll-sys` build script automatically downloads and extracts the matching CEF binary distribution on the first build. This requires active internet access during build time.

### Option B: Pre-downloaded / Reusable CEF Distribution
If you already have the CEF distribution extracted, point `CEF_PATH` to its location and ensure it is on your dynamic linker search path:

```sh
export CEF_PATH="$HOME/.local/share/cef"
export LD_LIBRARY_PATH="$CEF_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

> **Note for Wayland users:** CEF window embedding currently uses the Linux X11 child-window interface. Anarc runs seamlessly under XWayland (`--ozone-platform=x11`).

---

## Installation & Building

### 1. Clone the Repository

```sh
git clone https://github.com/rover-001/anarc.git
cd anarc
```

### 2. Build

#### Standard Release Build
```sh
cargo build --release
```

#### Optional: Build with WebAssembly Plugin Support
```sh
cargo build --release --features wasm-plugins
```

---

## Running Anarc

Run via cargo:

```sh
cargo run --release
```

Or run the built binary directly:

```sh
# If CEF is in a custom path, ensure LD_LIBRARY_PATH is set:
LD_LIBRARY_PATH="${CEF_PATH:-$HOME/.local/share/cef}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ./target/release/lumen
```

---

## Configuration & Paths

Anarc stores its runtime state according to XDG base directory specifications (or custom environment variables):

| Variable | Default Path | Description |
|---|---|---|
| `ANARC_CONFIG_DIR` | `~/.config/anarc` | Spaces, folders, and browser preferences (`spaces.json`, settings) |
| `ANARC_DATA_DIR` | `~/.local/share/anarc` | SQLite visit history, recent searches, and CEF storage root (`cef/space-*`) |
| `ANARC_CACHE_DIR` | `~/.cache/anarc` | Temporary caches and runtime scratch |

### Isolated Test Run
To run Anarc without touching your standard user profile:

```sh
test_root="$(mktemp -d)"
mkdir -p "$test_root/home" "$test_root/config" "$test_root/data" "$test_root/cache"

HOME="$test_root/home" \
ANARC_CONFIG_DIR="$test_root/config" \
ANARC_DATA_DIR="$test_root/data" \
ANARC_CACHE_DIR="$test_root/cache" \
ANARC_STATS=1 \
LD_LIBRARY_PATH="$CEF_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
./target/release/lumen
```

---

## Project Structure

```
.
├── assets/                  # Icons, branding logos, and bundled filter lists (EasyList)
├── examples/plugins/        # Reference plugin packs and WASM components
├── lumen-plugin-sdk/        # Rust SDK and WIT definitions for plugins
├── wit/                     # Component model WebAssembly interface definitions
└── src/
    ├── main.rs              # CLI initialization, process bootstrap, and GPUI runner
    ├── app/                 # Application state, actions, and UI shell composition
    ├── engine/              # Stable browser engine API and CEF runtime implementation
    ├── navigation/          # Address normalization, URL routing, and smart autocomplete
    ├── model/               # Serializable Space, Folder, Tab, and Split types
    ├── plugin/              # Cordis-style microkernel, pack loader, and WASM runtime
    ├── ui/                  # Reusable GPUI components (sidebar, command palette, tabs)
    ├── shield.rs            # AdBlock filter engine and background worker
    ├── history.rs           # SQLite history storage and search indexing
    └── paths.rs             # Multi-space filesystem paths and profiles
```

---

## License

This project is licensed under the Apache-2.0 / MIT license or project-specific terms. See [LICENSE](LICENSE) for details.
