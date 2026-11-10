# Lumen

Lumen is an Arc-style minimal browser built with Rust, GPUI Kit, and CEF. The Linux shell includes a native CEF child browser, a sidebar with spaces and tabs, and an address/search bar.

## Build and run

Use Rust stable and run:

```sh
cargo run --release
```

## Testing

Set these variables to isolate a test run from the normal Lumen profile:

- `LUMEN_CONFIG_DIR`: directory for `spaces.json` (default: `~/.config/lumen`)
- `LUMEN_DATA_DIR`: directory for recent searches and CEF storage (default: `~/.local/share/lumen`). CEF's root cache is `$LUMEN_DATA_DIR/cef`, with per-space profiles as direct children at `cef/space-<sanitized_space_id>`.
- `LUMEN_CACHE_DIR`: directory for non-profile Lumen caches (default: `${XDG_CACHE_HOME:-~/.cache}/lumen/cef`). It does not control the CEF root or per-space profiles.
- `LUMEN_STATS=1`: print OSR paints/s, root UI renders/s, average frame-copy time, and process RSS to stderr once per second

For a safe live run, create temporary directories and point all Lumen state there:

```sh
test_root="$(mktemp -d)"
mkdir -p "$test_root/home" "$test_root/config" "$test_root/data" "$test_root/cache"
HOME="$test_root/home" \
LUMEN_CONFIG_DIR="$test_root/config" \
LUMEN_DATA_DIR="$test_root/data" \
LUMEN_CACHE_DIR="$test_root/cache" \
LUMEN_STATS=1 \
LD_LIBRARY_PATH="$CEF_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
./target/release/lumen
```

The temporary state can be removed after the run with `rm -r "$test_root"`.

The UI appears before CEF is initialized. After the first frame, Lumen starts CEF and pumps its external message loop from GPUI. The app logs time to its first GPUI render to stderr. Type a URL or search in the top address field and click **Go** or press Enter; use **+ New tab** to add a tab. URLs without a scheme get `https://`; plain text is searched with DuckDuckGo. Clicking **Go** on an empty address opens DuckDuckGo.

## CEF setup (Phase 2+)

The manifest pins `cef = 156.4.0+156.0.26`. The cef-rs `cef-dll-sys` build script downloads and unpacks the matching CEF binary distribution when `CEF_PATH` is not set (the initial download needs network access). To reuse a downloaded copy, point `CEF_PATH` at the extracted Linux distribution directory and add it to the runtime loader path:

```sh
export CEF_PATH="$HOME/.local/share/cef"
export LD_LIBRARY_PATH="$CEF_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

Then run `cargo run --release` from this directory. The CEF package version and binary distribution must match. On Wayland, use an XWayland session (`--ozone-platform=x11`); window embedding currently uses the Linux X11 native child-window path. Do not use `--single-process`.

The address field uses the visible **Go** button or Enter to submit. Lumen updates tab titles and favicons from CEF callbacks, routes user-requested popups to tabs, blocks unsolicited popups, records downloads, and applies app keyboard shortcuts. The built-in Ad blocker uses EasyList plus Lumen's starter rules and can be controlled from the puzzle-piece button beside Downloads. It is not the uBlock Origin extension. Spaces have independent CEF request contexts. Session state, recent searches, history, lifecycle discarding, and runtime statistics are implemented in the modules listed below.

## Dependencies

The manifest pins `gpui-kit` 0.9.0, CEF 156.4.0, raw-window-handle 0.6.2, x11 2.21.0, and serde 1.0.228. GPUI Kit brings in the matching GPUI snapshot through its facade. The small `x11` dependency is used to position, map, and unmap CEF's native child windows on Linux.

## Project layout

- `src/main.rs`: CLI import handling and GPUI/CEF process bootstrap
- `src/app/state.rs`: session state and modal/menu state
- `src/app/actions/`: tab, space, folder, navigation, shortcut, palette, and resize actions
- `src/app/render.rs`: root composition and CEF scheduling
- `src/app/render/sidebar.rs`: sidebar composition and sidebar event wiring
- `src/app/render/shell.rs`: top bar, address bar, content frame, and resize interactions
- `src/app/render/context_menus.rs` and `src/app/render/dialogs.rs`: context menus and modal content
- `src/navigation/`: address normalization, submission, suggestions, and selection policy
- `src/model/`: serializable Space, Folder, SplitPair, and Tab types
- `src/ui/`: reusable sidebar rows, app pages, spaces, tab list, command palette, split view, and web viewport components
- `src/engine/`: stable engine API plus CEF runtime, browser, navigation, and callback modules
- `src/persist.rs`: session serialization and restore
- `src/history.rs`: recent searches and SQLite visit history
- `src/lifecycle.rs`: freeze/discard policy for inactive tabs
- `src/stats.rs`: startup, render, paint, and process statistics
- `src/import.rs`: browser profile, bookmark, and history import
- `src/paths.rs`: configuration, data, cache, and CEF profile paths
