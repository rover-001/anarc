# Lumen

Lumen is an Arc-style minimal browser built with Rust, GPUI, and CEF. The current code contains the Phase 0 scaffold and Phase 1 GPUI shell; web content is not wired yet.

## Build and run

Use Rust stable and run:

```sh
cargo run --release
```

The Phase 1 shell does not need CEF binaries. The app logs time to its first GPUI render to stderr. The initial UI includes Personal and Work spaces, tab selection, a new-tab control, and a collapsible sidebar.

## CEF setup (Phase 2+)

CEF is pinned to `cef = 156.4.0+156.0.26` when the engine integration is introduced. The cef-rs `cef-dll-sys` build script can download and unpack the matching binary distribution automatically. To share a downloaded copy across builds, install the CEF binary distribution and export:

```sh
export CEF_PATH="$HOME/.local/share/cef"
export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$CEF_PATH"
```

For Wayland sessions, run the eventual CEF process with `--ozone-platform=x11` to use XWayland. Do not use `--single-process`.

## Dependencies

The manifest pins `gpui-kit` 0.9.0 and serde 1.0.228. GPUI Kit brings in a matching GPUI snapshot through its facade, so the app uses `gpui_kit::*` and does not pin a separate GPUI crate. `raw-window-handle`, `sysinfo`, CEF, directories, serde_json, and Brave adblock are added with their implementation phases.

## Project layout

- `src/ui/`: sidebar, spaces, tab list, and command palette view
- `src/engine/`: browser-engine boundary
- `src/model/`: serializable Space and Tab types
- `src/lifecycle.rs`: future freeze/discard policy
- `src/stats.rs`: future process and startup statistics
- `src/persist.rs`: future session persistence
