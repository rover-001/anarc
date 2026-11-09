# Lumen

Lumen is an Arc-style minimal browser built with Rust, GPUI Kit, and CEF. The Linux shell includes a native CEF child browser, a sidebar with spaces and tabs, and an address/search bar.

## Build and run

Use Rust stable and run:

```sh
cargo run --release
```

The UI appears before CEF is initialized. After the first frame, Lumen starts CEF and pumps its external message loop from GPUI. The app logs time to its first GPUI render to stderr. Type a URL or search in the top address field and click **Go** or press Enter; use **+ New tab** to add a tab. URLs without a scheme get `https://`; plain text is searched with DuckDuckGo. Clicking **Go** on an empty address opens DuckDuckGo.

## CEF setup (Phase 2+)

The manifest pins `cef = 156.4.0+156.0.26`. The cef-rs `cef-dll-sys` build script downloads and unpacks the matching CEF binary distribution when `CEF_PATH` is not set (the initial download needs network access). To reuse a downloaded copy, point `CEF_PATH` at the extracted Linux distribution directory and add it to the runtime loader path:

```sh
export CEF_PATH="$HOME/.local/share/cef"
export LD_LIBRARY_PATH="$CEF_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

Then run `cargo run --release` from this directory. The CEF package version and binary distribution must match. On Wayland, use an XWayland session (`--ozone-platform=x11`); window embedding currently uses the Linux X11 native child-window path. Do not use `--single-process`.

The address field currently uses the visible **Go** button to submit. CEF page title/favicon callbacks, popups, downloads, keyboard shortcuts, and independent per-space request contexts are not wired yet.

## Dependencies

The manifest pins `gpui-kit` 0.9.0, CEF 156.4.0, raw-window-handle 0.6.2, x11 2.21.0, and serde 1.0.228. GPUI Kit brings in the matching GPUI snapshot through its facade. The small `x11` dependency is used to position, map, and unmap CEF's native child windows on Linux.

## Project layout

- `src/ui/`: sidebar, spaces, tab list, and command palette view
- `src/engine/`: browser-engine boundary
- `src/model/`: serializable Space and Tab types
- `src/lifecycle.rs`: future freeze/discard policy
- `src/stats.rs`: future process and startup statistics
- `src/persist.rs`: future session persistence
