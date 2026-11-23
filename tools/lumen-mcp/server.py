#!/usr/bin/env python3
"""Anarc MCP Server

Model Context Protocol (MCP) server for Anarc / Anarc browser plugin development.
Exposes tools, resources, and templates to scaffold, validate, test, build,
install, package, and manage plugins, as well as inspect browser state.
"""

import io
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

SERVER_NAME = "lumen-mcp"
SERVER_VERSION = "1.0.0"
PROTOCOL_VERSION = "2024-11-05"

# Environment / Default Paths
ANARC_DATA_DIR = Path(os.environ.get("ANARC_DATA_DIR", Path.home() / ".local" / "share" / "anarc"))
ANARC_CONFIG_DIR = Path(os.environ.get("ANARC_CONFIG_DIR", Path.home() / ".config" / "anarc"))
PLUGINS_DIR = ANARC_DATA_DIR / "plugins"
PLUGINS_CONFIG_FILE = ANARC_CONFIG_DIR / "plugins.toml"
SPACES_FILE = ANARC_CONFIG_DIR / "spaces.json"
HISTORY_FILE = ANARC_DATA_DIR / "history.sqlite"

# Find Anarc source root if available
LUMEN_ROOT: Optional[Path] = None
for candidate in [
    Path.cwd(),
    Path(__file__).resolve().parent.parent.parent,
    Path.home() / "Projects" / "kreon" / "lumen",
]:
    if (candidate / "Cargo.toml").is_file() and (candidate / "wit" / "lumen.wit").is_file():
        LUMEN_ROOT = candidate
        break

# Documentation References
SPEC_MANIFEST = """# Anarc Plugin Manifest Specification (`lumen-plugin.toml`)

All plugins require a root `lumen-plugin.toml` manifest.

```toml
[plugin]
id = "com.example.my-plugin"       # Lowercase reverse-DNS, >=2 parts, [a-z0-9-]
name = "My Plugin"                 # Human readable name (required)
version = "0.1.0"                  # SemVer major.minor.patch (required)
api = "1"                          # Host API level (currently "1")
kind = "pack" | "wasm" | "panel"   # Plugin type
description = "Brief summary"      # Optional description
```

### Kinds
1. `kind = "pack"`: Declarative modifications (userstyles, userscripts, themes, keymaps, search engines, filter lists).
2. `kind = "wasm"`: Sandboxed WebAssembly component (`wasm32-wasip2`) conforming to `wit/lumen.wit`.
3. `kind = "panel"`: HTML/CSS/JS sidebar views served via CEF under `lumen-plugin://<id>/`.
"""

SPEC_PACKS = """# Anarc Declarative Packs Reference

Declarative packs modify styles, scripts, keymaps, and themes without WASM overhead.

```toml
[plugin]
id = "com.example.youtube-clean"
name = "YouTube Clean"
version = "1.0.0"
api = "1"
kind = "pack"
description = "Custom CSS and search engine for YouTube"

# Userstyles (injected into matching pages)
[[pack.userstyle]]
match = ["https://*.youtube.com/*"]
exclude = ["https://music.youtube.com/*"]   # Optional
css = "styles/youtube.css"
run_at = "document_start"                  # or "document_idle"

# Userscripts (DOM manipulation)
[[pack.userscript]]
match = ["https://github.com/*"]
js = "scripts/main.js"
run_at = "document_idle"

# Custom Search Engines
[[pack.search_engine]]
id = "gh"
name = "GitHub"
url = "https://github.com/search?q=%s"
keyword = "!gh"

# Keymap contributions
[pack.keymap]
"ctrl-j" = "tabs.next"
"ctrl-k" = "tabs.prev"

# Theme Overrides
[pack.theme]
mode = "dark" # "dark", "light", or "system"
[pack.theme.tokens]
base = "#0f1014"
surface = "#17181d"
accent = "#9aa6ff"
text = "#e6e7eb"
muted = "#8a8d98"
card_radius = 16
ui_scale = 0.85
font_family = "Inter"

# Motion springs
[pack.theme.motion]
spring_stiffness = 380
spring_damping = 32
```
"""

SPEC_WASM = """# Anarc WASM Component Plugins Reference

WASM plugins run sandboxed WebAssembly components (`wasm32-wasip2`) with host imports defined in `wit/lumen.wit`.

### Manifest:
```toml
[plugin]
id = "com.example.tab-counter"
name = "Tab Counter"
version = "0.1.0"
api = "1"
kind = "wasm"
entry = "tab_counter.wasm"
description = "Tracks and logs tab activities"

[inject]
required = ["tabs"]

[permissions]
tabs = "read-write"       # or "read-only", or false
history = false           # or "read-only", "read-write"
storage = true            # scoped key-value persistence
network = ["api.example.com"] # allowlisted HTTPS hosts

[[contributes.commands]]
id = "tab-counter.report"
title = "Log tab count"
palette = true
```

### Building:
Compile with Rust and `cargo-component`:
```sh
cargo component build --target wasm32-wasip2 --release
```
"""

SPEC_PANELS = """# Anarc Web Panels Reference

Web panels provide embedded sidebar views rendered inside CEF with a secure `lumen-plugin://<id>/` origin and CSP.

```toml
[plugin]
id = "com.example.notes-panel"
name = "Quick Notes"
version = "1.0.0"
api = "1"
kind = "panel"
entry = "index.html"
description = "Side notes panel"

[permissions]
storage = true
```

The directory must include `index.html` and any bundled assets (CSS/JS).
"""

def get_wit_schema() -> str:
    if LUMEN_ROOT:
        wit_file = LUMEN_ROOT / "wit" / "lumen.wit"
        if wit_file.is_file():
            return wit_file.read_text(encoding="utf-8")
    return """package lumen:plugin@1.0.0;

interface host {
    record tab {
        id: string,
        title: string,
        url: string,
        active: bool,
        pinned: bool,
    }

    record space {
        id: string,
        name: string,
    }

    record history-entry {
        url: string,
        title: string,
        visits: u64,
        last-visited-unix-ms: u64,
    }

    list-tabs: func() -> result<list<tab>, string>;
    open-tab: func(url: string) -> result<string, string>;
    close-tab: func(tab-id: string) -> result<_, string>;
    activate-tab: func(tab-id: string) -> result<_, string>;
    move-tab: func(tab-id: string, index: u32) -> result<_, string>;

    list-spaces: func() -> result<list<space>, string>;
    switch-space: func(space-id: string) -> result<_, string>;

    navigate: func(tab-id: string, url: string) -> result<_, string>;
    reload: func(tab-id: string) -> result<_, string>;

    search-history: func(query: string, limit: u32) -> result<list<history-entry>, string>;
    clear-history: func() -> result<_, string>;

    storage-get: func(key: string) -> result<option<list<u8>>, string>;
    storage-set: func(key: string, value: list<u8>) -> result<_, string>;
    storage-delete: func(key: string) -> result<_, string>;

    notify: func(message: string) -> result<_, string>;
}

interface guest {
    record manifest {
        id: string,
        version: string,
        api: string,
        requested-permissions: list<string>,
    }

    record event {
        name: string,
        payload: string,
    }

    get-manifest: func() -> manifest;
    apply: func(config: string) -> result<_, string>;
    on-event: func(event: event) -> result<option<string>, string>;
    run-command: func(command-id: string) -> result<_, string>;
}

world lumen-plugin {
    import host;
    export guest;
}
"""

def validate_reverse_dns(id_str: str) -> Optional[str]:
    labels = id_str.split(".")
    if len(labels) < 2:
        return f"plugin.id '{id_str}' must be a lowercase reverse-DNS identifier with at least two labels (e.g. com.example.plugin)"
    for label in labels:
        if not label:
            return f"Empty label in plugin.id '{id_str}' (must be a valid reverse-DNS identifier)"
        if not re.match(r"^[a-z0-9][a-z0-9\-]*[a-z0-9]$|^[a-z0-9]$", label):
            return f"Invalid label '{label}' in plugin.id '{id_str}': reverse-DNS labels must be lowercase alphanumeric and may contain hyphens"
    return None

def validate_semver(ver_str: str) -> Optional[str]:
    core = ver_str.split("-")[0].split("+")[0]
    parts = core.split(".")
    if len(parts) != 3 or not all(p.isdigit() for p in parts):
        return f"Version '{ver_str}' must follow major.minor.patch SemVer form (e.g. 1.0.0)"
    return None

# ==================== Core Tools Implementation ====================

def tool_spec(topic: str = "overview") -> str:
    topic = topic.lower().strip()
    if topic in ("manifest", "lumen-plugin.toml"):
        return SPEC_MANIFEST
    elif topic in ("pack", "packs", "userstyle", "userscript", "theme"):
        return SPEC_PACKS
    elif topic in ("wasm", "component", "wit"):
        return f"{SPEC_WASM}\n\n## Canonical WIT Definition\n\n```wit\n{get_wit_schema()}\n```"
    elif topic in ("panel", "panels"):
        return SPEC_PANELS
    elif topic == "tokens":
        return """# Anarc Theme Tokens

```toml
[pack.theme.tokens]
base = "#0f1014"         # Main background
surface = "#17181d"      # Cards and panels
accent = "#9aa6ff"       # Active highlights, focus rings
text = "#e6e7eb"         # Primary typography
muted = "#8a8d98"        # Secondary typography
border_hover = "#2b2d35" # Hover border
card_radius = 16         # Corner rounding in px
ui_scale = 0.85          # UI scale factor (0.5 - 2.0)
font_family = "Inter"    # Font family name

[pack.theme.motion]
spring_stiffness = 380   # Spring responsiveness
spring_damping = 32      # Oscillation damping
```
"""
    return f"""# Anarc Plugin System Overview

Anarc supports three plugin kinds:
1. **Packs (`kind = "pack"`)**: Declarative themes, userstyles (CSS), userscripts (JS), search engines, and keymaps.
2. **WASM Plugins (`kind = "wasm"`)**: Sandboxed WebAssembly components implementing `wit/lumen.wit`.
3. **Web Panels (`kind = "panel"`)**: CEF sidebar views under `lumen-plugin://<id>/`.

Call `lumen_plugin_spec` with `topic`:
- `manifest`: Root manifest configuration rules.
- `packs`: Userstyles, userscripts, keymaps, themes.
- `wasm`: WIT guest/host interface and build guidelines.
- `panels`: Web panels and CSP.
- `tokens`: Theme styling tokens and spring motion.
"""

def tool_scaffold(
    target_dir: str,
    plugin_id: str,
    name: str,
    kind: str = "pack",
    description: str = "",
    pack_type: str = "userstyle",
    match_urls: Optional[List[str]] = None,
) -> Dict[str, Any]:
    out_dir = Path(target_dir).resolve()
    
    # Validation of parameters
    id_err = validate_reverse_dns(plugin_id)
    if id_err:
        return {"error": id_err}
    if not name.strip():
        return {"error": "Plugin name cannot be empty"}
    if kind not in ("pack", "wasm", "panel"):
        return {"error": "kind must be 'pack', 'wasm', or 'panel'"}

    out_dir.mkdir(parents=True, exist_ok=True)
    created_files: List[str] = []

    if not description:
        description = f"{name} plugin for Anarc"

    if kind == "pack":
        if not match_urls:
            match_urls = ["https://example.com/*"]
            
        toml_content = f"""[plugin]
id = "{plugin_id}"
name = "{name}"
version = "1.0.0"
api = "1"
kind = "pack"
description = "{description}"
"""
        if pack_type == "userstyle":
            toml_content += f"""
[[pack.userstyle]]
match = {json.dumps(match_urls)}
css = "styles/style.css"
run_at = "document_start"
"""
            styles_dir = out_dir / "styles"
            styles_dir.mkdir(exist_ok=True)
            css_file = styles_dir / "style.css"
            css_file.write_text("/* Custom userstyle for Anarc */\nbody {\n  /* Add overrides here */\n}\n", encoding="utf-8")
            created_files.append("styles/style.css")

        elif pack_type == "userscript":
            toml_content += f"""
[[pack.userscript]]
match = {json.dumps(match_urls)}
js = "scripts/main.js"
run_at = "document_idle"
"""
            scripts_dir = out_dir / "scripts"
            scripts_dir.mkdir(exist_ok=True)
            js_file = scripts_dir / "main.js"
            js_file.write_text("// Custom userscript for Anarc\nconsole.log('Anarc userscript active on:', window.location.href);\n", encoding="utf-8")
            created_files.append("scripts/main.js")

        elif pack_type == "theme":
            toml_content += """
[pack.theme]
mode = "dark"

[pack.theme.tokens]
base = "#0f1014"
surface = "#17181d"
accent = "#9aa6ff"
text = "#e6e7eb"
muted = "#8a8d98"
card_radius = 16
ui_scale = 0.85
font_family = "Inter"

[pack.theme.motion]
spring_stiffness = 380
spring_damping = 32
"""
        elif pack_type == "keymap":
            toml_content += """
[pack.keymap]
"ctrl-j" = "tabs.next"
"ctrl-k" = "tabs.prev"
"""
        elif pack_type == "search_engine":
            toml_content += f"""
[[pack.search_engine]]
id = "custom"
name = "{name}"
url = "https://example.com/search?q=%s"
keyword = "!c"
"""

        manifest_file = out_dir / "lumen-plugin.toml"
        manifest_file.write_text(toml_content.strip() + "\n", encoding="utf-8")
        created_files.append("lumen-plugin.toml")

    elif kind == "wasm":
        crate_name = plugin_id.split(".")[-1].replace("-", "_")
        wasm_file_name = f"{crate_name}.wasm"
        toml_content = f"""[plugin]
id = "{plugin_id}"
name = "{name}"
version = "0.1.0"
api = "1"
kind = "wasm"
entry = "{wasm_file_name}"
description = "{description}"

[inject]
required = ["tabs"]

[permissions]
tabs = "read-write"
storage = true
network = []

[[contributes.commands]]
id = "{plugin_id}.run"
title = "Run {name}"
palette = true
"""
        manifest_file = out_dir / "lumen-plugin.toml"
        manifest_file.write_text(toml_content.strip() + "\n", encoding="utf-8")
        created_files.append("lumen-plugin.toml")

        # Create component Rust source
        comp_dir = out_dir / "component"
        comp_dir.mkdir(parents=True, exist_ok=True)
        src_dir = comp_dir / "src"
        src_dir.mkdir(parents=True, exist_ok=True)

        cargo_toml = f"""[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "=0.57.1"
"""
        (comp_dir / "Cargo.toml").write_text(cargo_toml.strip() + "\n", encoding="utf-8")
        created_files.append("component/Cargo.toml")

        lib_rs = """#[allow(warnings)]
mod bindings;

use bindings::exports::lumen::plugin::guest::{Event, Guest, Manifest};
use bindings::lumen::plugin::host;

struct Component;

impl Guest for Component {
    fn get_manifest() -> Manifest {
        Manifest {
            id: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            api: "1".to_string(),
            requested_permissions: vec!["tabs".to_string(), "storage".to_string()],
        }
    }

    fn apply(_config: String) -> Result<(), String> {
        let _ = host::notify("Plugin initialized");
        Ok(())
    }

    fn on_event(_event: Event) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn run_command(command_id: String) -> Result<(), String> {
        if command_id.ends_with(".run") {
            let tabs = host::list_tabs()?;
            let _ = host::notify(&format!("Total tabs open: {}", tabs.len()));
        }
        Ok(())
    }
}

bindings::export!(Component with_types_in bindings);
"""
        (src_dir / "lib.rs").write_text(lib_rs, encoding="utf-8")
        created_files.append("component/src/lib.rs")

    elif kind == "panel":
        toml_content = f"""[plugin]
id = "{plugin_id}"
name = "{name}"
version = "1.0.0"
api = "1"
kind = "panel"
entry = "index.html"
description = "{description}"

[permissions]
storage = true
"""
        manifest_file = out_dir / "lumen-plugin.toml"
        manifest_file.write_text(toml_content.strip() + "\n", encoding="utf-8")
        created_files.append("lumen-plugin.toml")

        html_content = f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>{name}</title>
  <style>
    body {{
      margin: 0;
      padding: 16px;
      font-family: system-ui, -apple-system, sans-serif;
      background: #0f1014;
      color: #e6e7eb;
    }}
    h1 {{ font-size: 16px; margin-bottom: 8px; }}
    p {{ font-size: 13px; color: #8a8d98; }}
  </style>
</head>
<body>
  <h1>{name}</h1>
  <p>{description}</p>
</body>
</html>
"""
        (out_dir / "index.html").write_text(html_content, encoding="utf-8")
        created_files.append("index.html")

    # Write README
    readme = f"""# {name}

ID: `{plugin_id}`
Kind: `{kind}`

{description}

### How to Install
Run with the Anarc MCP tool:
```sh
lumen-mcp install --plugin-path "{out_dir}" --symlink
```
Or copy directly into `$ANARC_DATA_DIR/plugins/{plugin_id}`.
"""
    (out_dir / "README.md").write_text(readme.strip() + "\n", encoding="utf-8")
    created_files.append("README.md")

    return {
        "success": True,
        "plugin_id": plugin_id,
        "kind": kind,
        "target_dir": str(out_dir),
        "created_files": created_files,
    }

def tool_validate(plugin_path: str) -> Dict[str, Any]:
    p = Path(plugin_path).resolve()
    manifest_path = p / "lumen-plugin.toml" if p.is_dir() else p
    root = manifest_path.parent

    if not manifest_path.is_file():
        return {"valid": False, "errors": [f"Manifest file not found at {manifest_path}"]}

    try:
        data = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    except Exception as e:
        return {"valid": False, "errors": [f"Malformed TOML in {manifest_path}: {e}"]}

    errors: List[str] = []
    warnings: List[str] = []

    plugin_sec = data.get("plugin")
    if not isinstance(plugin_sec, dict):
        return {"valid": False, "errors": ["Missing [plugin] table in manifest"]}

    # Required fields
    pid = plugin_sec.get("id")
    pname = plugin_sec.get("name")
    pver = plugin_sec.get("version")
    papi = plugin_sec.get("api")
    pkind = plugin_sec.get("kind")

    if not pid:
        errors.append("Missing required field 'plugin.id'")
    else:
        err = validate_reverse_dns(str(pid))
        if err:
            errors.append(err)

    if not pname or not str(pname).strip():
        errors.append("plugin.name cannot be empty")

    if not pver:
        errors.append("Missing required field 'plugin.version'")
    else:
        err = validate_semver(str(pver))
        if err:
            errors.append(err)

    if papi != "1":
        errors.append(f"Unsupported 'plugin.api' '{papi}'; Anarc host currently requires api = '1'")

    if pkind not in ("pack", "wasm", "panel"):
        errors.append(f"Invalid 'plugin.kind' '{pkind}'; must be 'pack', 'wasm', or 'panel'")

    # Kind-specific checks
    if pkind == "pack":
        pack_sec = data.get("pack", {})
        # Userstyles
        for i, s in enumerate(pack_sec.get("userstyle", [])):
            matches = s.get("match", [])
            if not matches or not isinstance(matches, list):
                errors.append(f"userstyle[{i}]: 'match' must be a non-empty list of URL patterns")
            for m in matches:
                if not (m.startswith("http://") or m.startswith("https://")):
                    errors.append(f"userstyle[{i}]: match pattern must begin with http:// or https://: '{m}'")
                if any(c.isspace() for c in m):
                    errors.append(f"userstyle[{i}]: match pattern cannot contain whitespace: '{m}'")
            css = s.get("css")
            if not css:
                errors.append(f"userstyle[{i}]: missing 'css' asset reference")
            else:
                asset_file = (root / css).resolve()
                if not asset_file.is_file():
                    errors.append(f"userstyle[{i}]: CSS file '{css}' does not exist on disk")

        # Userscripts
        for i, s in enumerate(pack_sec.get("userscript", [])):
            matches = s.get("match", [])
            if not matches or not isinstance(matches, list):
                errors.append(f"userscript[{i}]: 'match' must be a non-empty list of URL patterns")
            for m in matches:
                if not (m.startswith("http://") or m.startswith("https://")):
                    errors.append(f"userscript[{i}]: match pattern must begin with http:// or https://: '{m}'")
                if any(c.isspace() for c in m):
                    errors.append(f"userscript[{i}]: match pattern cannot contain whitespace: '{m}'")
            js = s.get("js")
            if not js:
                errors.append(f"userscript[{i}]: missing 'js' asset reference")
            else:
                asset_file = (root / js).resolve()
                if not asset_file.is_file():
                    errors.append(f"userscript[{i}]: JS file '{js}' does not exist on disk")

        # Theme
        theme_sec = pack_sec.get("theme")
        if theme_sec:
            mode = theme_sec.get("mode")
            if mode and mode not in ("dark", "light", "system"):
                errors.append("pack.theme.mode must be 'dark', 'light', or 'system'")

        # Search engines
        for i, eng in enumerate(pack_sec.get("search_engine", [])):
            url = eng.get("url", "")
            if "%s" not in url:
                errors.append(f"search_engine[{i}]: URL must contain '%s' query placeholder")

    elif pkind == "wasm":
        entry = plugin_sec.get("entry")
        if not entry:
            errors.append("Missing required field 'plugin.entry' for wasm plugin")
        else:
            wasm_file = (root / entry).resolve()
            if not wasm_file.is_file():
                warnings.append(f"WASM component binary '{entry}' not found yet (build component first)")

    elif pkind == "panel":
        entry = plugin_sec.get("entry")
        if not entry:
            errors.append("Missing required field 'plugin.entry' for panel plugin")
        else:
            entry_file = (root / entry).resolve()
            if not entry_file.is_file():
                errors.append(f"Panel entry file '{entry}' does not exist on disk")

        # Validate network host patterns
        perm_sec = data.get("permissions", {})
        for host in perm_sec.get("network", []):
            h = host.strip().lower()
            if h.startswith("https://"):
                h = h[8:]
            if "/" in h or ":" in h or "@" in h or not h:
                errors.append(f"Invalid panel network host: '{host}'")
                continue
            bare = h[2:] if h.startswith("*.") else h
            if not bare or bare.startswith(".") or bare.endswith(".") or ".." in bare or not all(c.isalnum() or c in ".-" for c in bare):
                errors.append(f"Invalid panel network host pattern: '{host}' (wildcards must be '*.<domain>')")

    return {
        "valid": len(errors) == 0,
        "plugin_id": pid,
        "kind": pkind,
        "errors": errors,
        "warnings": warnings,
    }

def tool_build_wasm(plugin_path: str) -> Dict[str, Any]:
    root = Path(plugin_path).resolve()
    manifest_file = root / "lumen-plugin.toml" if root.is_dir() else root
    root = manifest_file.parent

    if not manifest_file.is_file():
        return {"error": f"Manifest not found at {manifest_file}"}

    try:
        manifest = tomllib.loads(manifest_file.read_text(encoding="utf-8"))
    except Exception as e:
        return {"error": f"Failed to parse manifest: {e}"}

    entry = manifest.get("plugin", {}).get("entry", "plugin.wasm")
    comp_cargo = root / "component" / "Cargo.toml"
    root_cargo = root / "Cargo.toml"

    cargo_file = comp_cargo if comp_cargo.is_file() else (root_cargo if root_cargo.is_file() else None)
    if not cargo_file:
        return {"error": f"No Cargo.toml found in {root} or {root / 'component'}"}

    # Run cargo component build or standard cargo build
    cmd = ["cargo", "component", "build", "--release", "--manifest-path", str(cargo_file)]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        # Fall back to cargo build target wasm32-wasip2
        cmd = ["cargo", "build", "--target", "wasm32-wasip2", "--release", "--manifest-path", str(cargo_file)]
        res = subprocess.run(cmd, capture_output=True, text=True)
        if res.returncode != 0:
            return {
                "success": False,
                "error": f"Build failed with exit code {res.returncode}:\n{res.stderr or res.stdout}"
            }

    # Find the built wasm
    target_dir = cargo_file.parent / "target" / "wasm32-wasip2" / "release"
    wasm_candidates = list(target_dir.glob("*.wasm"))
    if not wasm_candidates:
        return {"success": False, "error": f"Build succeeded but no .wasm found in {target_dir}"}

    target_wasm = wasm_candidates[0]
    dest_wasm = root / entry
    shutil.copy2(target_wasm, dest_wasm)

    return {
        "success": True,
        "built_wasm": str(dest_wasm),
        "size_bytes": dest_wasm.stat().st_size,
    }

def tool_install(plugin_path: str, symlink: bool = False, enable: bool = True) -> Dict[str, Any]:
    val = tool_validate(plugin_path)
    if not val.get("valid"):
        return {"error": "Plugin validation failed before installation", "details": val.get("errors")}

    pid = val["plugin_id"]
    source_dir = Path(plugin_path).resolve()
    if source_dir.is_file():
        source_dir = source_dir.parent

    PLUGINS_DIR.mkdir(parents=True, exist_ok=True)
    dest_dir = PLUGINS_DIR / pid

    if dest_dir.exists() or dest_dir.is_symlink():
        if dest_dir.is_symlink() or dest_dir.is_file():
            dest_dir.unlink()
        else:
            shutil.rmtree(dest_dir)

    if symlink:
        dest_dir.symlink_to(source_dir, target_is_directory=True)
    else:
        shutil.copytree(source_dir, dest_dir)

    if enable:
        tool_toggle(pid, True)

    return {
        "success": True,
        "plugin_id": pid,
        "installed_path": str(dest_dir),
        "method": "symlink" if symlink else "copy",
        "enabled": enable,
    }

def tool_uninstall(plugin_id: str) -> Dict[str, Any]:
    dest_dir = PLUGINS_DIR / plugin_id
    if not dest_dir.exists() and not dest_dir.is_symlink():
        return {"error": f"Plugin '{plugin_id}' is not installed in {PLUGINS_DIR}"}

    if dest_dir.is_symlink() or dest_dir.is_file():
        dest_dir.unlink()
    else:
        shutil.rmtree(dest_dir)

    # Remove from plugins.toml
    if PLUGINS_CONFIG_FILE.is_file():
        try:
            content = PLUGINS_CONFIG_FILE.read_text(encoding="utf-8")
            data = tomllib.loads(content)
            if "plugins" in data and plugin_id in data["plugins"]:
                del data["plugins"][plugin_id]
                lines = []
                for k, v in data.get("plugins", {}).items():
                    lines.append(f'[plugins."{k}"]')
                    for prop, val in v.items():
                        lines.append(f'{prop} = {"true" if val is True else ("false" if val is False else json.dumps(val))}')
                    lines.append("")
                PLUGINS_CONFIG_FILE.write_text("\n".join(lines).strip() + "\n", encoding="utf-8")
        except Exception:
            pass

    return {"success": True, "plugin_id": plugin_id}

def tool_list(include_builtins: bool = True) -> Dict[str, Any]:
    installed: List[Dict[str, Any]] = []

    # Read config state
    config_enabled: Dict[str, bool] = {}
    if PLUGINS_CONFIG_FILE.is_file():
        try:
            cfg = tomllib.loads(PLUGINS_CONFIG_FILE.read_text(encoding="utf-8"))
            for k, v in cfg.get("plugins", {}).items():
                config_enabled[k] = v.get("enabled", True)
        except Exception:
            pass

    # Builtins
    builtins = [
        ("lumen.downloads", "Downloads", "Downloads manager page and tracking"),
        ("lumen.history", "History", "SQLite browsing history store"),
        ("lumen.stats", "Stats", "Performance metrics and resource counters"),
        ("lumen.reader", "Reader Mode", "Clean article distraction-free viewer"),
        ("lumen.shield", "Ad & Tracker Shield", "Built-in adblocking filters"),
        ("lumen.split-view", "Split View", "Up to four tab panes"),
    ]
    if include_builtins:
        for bid, bname, bdesc in builtins:
            installed.append({
                "id": bid,
                "name": bname,
                "version": "builtin",
                "kind": "builtin",
                "description": bdesc,
                "enabled": config_enabled.get(bid, True),
                "path": "builtin",
            })

    # Third-party installed plugins
    if PLUGINS_DIR.is_dir():
        for item in sorted(PLUGINS_DIR.iterdir()):
            if item.is_dir() or item.is_symlink():
                manifest_file = item / "lumen-plugin.toml"
                if manifest_file.is_file():
                    try:
                        m = tomllib.loads(manifest_file.read_text(encoding="utf-8"))
                        p = m.get("plugin", {})
                        pid = p.get("id", item.name)
                        enabled_by_default = bool(p.get("enabled_by_default", True))
                        installed.append({
                            "id": pid,
                            "name": p.get("name", item.name),
                            "version": p.get("version", "unknown"),
                            "kind": p.get("kind", "pack"),
                            "description": p.get("description", ""),
                            "enabled": config_enabled.get(pid, enabled_by_default),
                            "path": str(item),
                            "is_symlink": item.is_symlink(),
                        })
                    except Exception as e:
                        installed.append({
                            "id": item.name,
                            "name": item.name,
                            "version": "error",
                            "kind": "unknown",
                            "description": f"Failed to load: {e}",
                            "enabled": False,
                            "path": str(item),
                            "error": str(e),
                        })

    return {"plugins": installed, "total": len(installed)}

def tool_toggle(plugin_id: str, enable: bool) -> Dict[str, Any]:
    ANARC_CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    data: Dict[str, Any] = {}
    if PLUGINS_CONFIG_FILE.is_file():
        try:
            data = tomllib.loads(PLUGINS_CONFIG_FILE.read_text(encoding="utf-8"))
        except Exception:
            data = {}

    if "plugins" not in data:
        data["plugins"] = {}
    if plugin_id not in data["plugins"]:
        data["plugins"][plugin_id] = {}

    data["plugins"][plugin_id]["enabled"] = enable

    # Serialize simple TOML
    lines = []
    for k, v in sorted(data.get("plugins", {}).items()):
        lines.append(f'[plugins."{k}"]')
        for prop, val in sorted(v.items()):
            lines.append(f'{prop} = {"true" if val is True else ("false" if val is False else json.dumps(val))}')
        lines.append("")

    PLUGINS_CONFIG_FILE.write_text("\n".join(lines).strip() + "\n", encoding="utf-8")
    return {"success": True, "plugin_id": plugin_id, "enabled": enable}

def tool_package(plugin_path: str, output_file: Optional[str] = None) -> Dict[str, Any]:
    val = tool_validate(plugin_path)
    if not val.get("valid"):
        return {"error": "Plugin validation failed before packaging", "details": val.get("errors")}

    pid = val["plugin_id"]
    root = Path(plugin_path).resolve()
    if root.is_file():
        root = root.parent

    if not output_file:
        output_file = str(root / f"{pid}.lumenplugin")

    out_zip = Path(output_file).resolve()
    out_zip.parent.mkdir(parents=True, exist_ok=True)

    # Use lumen-signer to produce cryptographically signed .lumenplugin archive
    signer_bin = shutil.which("lumen-signer") or "/home/rover/.local/bin/lumen-signer"
    if os.path.isfile(signer_bin):
        res = subprocess.run([signer_bin, str(root), str(out_zip)], capture_output=True, text=True)
        if res.returncode == 0 and out_zip.is_file():
            return {
                "success": True,
                "archive_path": str(out_zip),
                "archive_bytes": out_zip.stat().st_size,
                "signed": True,
            }

    # Fallback to standard ZIP
    total_uncompressed = 0
    entries_count = 0

    with zipfile.ZipFile(out_zip, "w", compression=zipfile.ZIP_DEFLATED) as z:
        for file_path in root.rglob("*"):
            if file_path.is_file():
                rel_path = file_path.relative_to(root).as_posix()
                if "target/" in rel_path or rel_path.endswith(".lumenplugin"):
                    continue
                size = file_path.stat().st_size
                total_uncompressed += size
                entries_count += 1
                z.write(file_path, arcname=rel_path)

    return {
        "success": True,
        "archive_path": str(out_zip),
        "entries": entries_count,
        "uncompressed_bytes": total_uncompressed,
        "archive_bytes": out_zip.stat().st_size,
        "signed": False,
    }

def tool_browser_spaces(space_id: Optional[str] = None) -> Dict[str, Any]:
    if not SPACES_FILE.is_file():
        return {"error": f"Spaces file not found at {SPACES_FILE}"}

    try:
        data = json.loads(SPACES_FILE.read_text(encoding="utf-8"))
    except Exception as e:
        return {"error": f"Failed to parse spaces: {e}"}

    spaces = data.get("spaces", [])
    result_spaces = []
    for sp in spaces:
        sp_id = sp.get("id")
        if space_id and sp_id != space_id:
            continue
        tabs = [
            {
                "id": t.get("id"),
                "title": t.get("title", ""),
                "url": t.get("url", ""),
                # Favorites and pins are one kept-tab section. Emit both keys
                # for backwards compatibility with older MCP clients.
                "pinned": bool(t.get("pinned", False) or t.get("favorite", False)),
                "favorite": bool(t.get("pinned", False) or t.get("favorite", False)),
                "kept_group": t.get("kept_group"),
                "state": t.get("state", "Loaded"),
            }
            for t in sp.get("tabs", [])
        ]
        result_spaces.append({
            "id": sp_id,
            "name": sp.get("name", ""),
            "tab_count": len(tabs),
            "tabs": tabs,
        })

    return {"spaces": result_spaces, "total_spaces": len(result_spaces)}

def tool_browser_history(query: Optional[str] = None, limit: int = 20) -> Dict[str, Any]:
    if not HISTORY_FILE.is_file():
        return {"error": f"History database not found at {HISTORY_FILE}"}

    try:
        conn = sqlite3.connect(f"file:{HISTORY_FILE}?mode=ro", uri=True)
        cursor = conn.cursor()
        if query:
            q = f"%{query}%"
            cursor.execute(
                "SELECT url, title, visit_time, visit_count FROM history WHERE url LIKE ? OR title LIKE ? ORDER BY visit_time DESC LIMIT ?",
                (q, q, limit),
            )
        else:
            cursor.execute(
                "SELECT url, title, visit_time, visit_count FROM history ORDER BY visit_time DESC LIMIT ?",
                (limit,),
            )
        rows = cursor.fetchall()
        entries = [
            {"url": r[0], "title": r[1], "visit_time": r[2], "visit_count": r[3]}
            for r in rows
        ]
        conn.close()
        return {"entries": entries, "count": len(entries)}
    except Exception as e:
        return {"error": f"Database query failed: {e}"}

# ==================== MCP Tool & Resource Handlers ====================

TOOLS_REGISTRY = [
    {
        "name": "lumen_plugin_spec",
        "description": "Get official specifications and references for Anarc plugins (manifests, packs, WASM WIT, web panels, theme tokens).",
        "parameters": {
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "enum": ["overview", "manifest", "packs", "wasm", "panels", "tokens"],
                    "description": "Topic to retrieve documentation for.",
                }
            },
        },
    },
    {
        "name": "lumen_plugin_scaffold",
        "description": "Scaffold a brand new Anarc plugin with all files, manifest, templates, and scripts.",
        "parameters": {
            "type": "object",
            "required": ["target_dir", "plugin_id", "name"],
            "properties": {
                "target_dir": {"type": "string", "description": "Target folder where plugin will be created."},
                "plugin_id": {"type": "string", "description": "Lowercase reverse-DNS identifier (e.g. com.example.my-style)."},
                "name": {"type": "string", "description": "Human-readable plugin name."},
                "kind": {"type": "string", "enum": ["pack", "wasm", "panel"], "default": "pack", "description": "Plugin kind."},
                "description": {"type": "string", "description": "Short description."},
                "pack_type": {
                    "type": "string",
                    "enum": ["userstyle", "userscript", "theme", "keymap", "search_engine"],
                    "default": "userstyle",
                    "description": "Sub-type if kind is 'pack'.",
                },
                "match_urls": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "URL match patterns for userstyle/userscript (e.g. ['https://*.youtube.com/*']).",
                },
            },
        },
    },
    {
        "name": "lumen_plugin_validate",
        "description": "Validate a plugin directory or lumen-plugin.toml manifest against Anarc engine constraints.",
        "parameters": {
            "type": "object",
            "required": ["plugin_path"],
            "properties": {
                "plugin_path": {"type": "string", "description": "Path to plugin directory or lumen-plugin.toml."}
            },
        },
    },
    {
        "name": "lumen_plugin_build",
        "description": "Build a WASM component plugin using cargo component / cargo wasm32-wasip2.",
        "parameters": {
            "type": "object",
            "required": ["plugin_path"],
            "properties": {
                "plugin_path": {"type": "string", "description": "Path to WASM plugin root containing component."}
            },
        },
    },
    {
        "name": "lumen_plugin_install",
        "description": "Install a plugin into Anarc's live plugins directory (~/.local/share/anarc/plugins/<id>).",
        "parameters": {
            "type": "object",
            "required": ["plugin_path"],
            "properties": {
                "plugin_path": {"type": "string", "description": "Source path of plugin."},
                "symlink": {"type": "boolean", "default": False, "description": "Symlink plugin for live development reload."},
                "enable": {"type": "boolean", "default": True, "description": "Enable in plugins.toml after install."},
            },
        },
    },
    {
        "name": "lumen_plugin_uninstall",
        "description": "Remove an installed plugin from Anarc's plugins directory.",
        "parameters": {
            "type": "object",
            "required": ["plugin_id"],
            "properties": {
                "plugin_id": {"type": "string", "description": "Plugin ID to remove."}
            },
        },
    },
    {
        "name": "lumen_plugin_list",
        "description": "List all installed plugins and built-in plugins with their enabled status.",
        "parameters": {
            "type": "object",
            "properties": {
                "include_builtins": {"type": "boolean", "default": True, "description": "Include built-in modules."}
            },
        },
    },
    {
        "name": "lumen_plugin_toggle",
        "description": "Enable or disable a plugin in plugins.toml.",
        "parameters": {
            "type": "object",
            "required": ["plugin_id", "enable"],
            "properties": {
                "plugin_id": {"type": "string", "description": "Plugin ID."},
                "enable": {"type": "boolean", "description": "True to enable, False to disable."}
            },
        },
    },
    {
        "name": "lumen_plugin_package",
        "description": "Package a plugin into a validated .lumenplugin archive ready for distribution.",
        "parameters": {
            "type": "object",
            "required": ["plugin_path"],
            "properties": {
                "plugin_path": {"type": "string", "description": "Path to plugin directory."},
                "output_file": {"type": "string", "description": "Optional destination path."}
            },
        },
    },
    {
        "name": "lumen_browser_spaces",
        "description": "Inspect open spaces and active tabs in the Anarc browser.",
        "parameters": {
            "type": "object",
            "properties": {
                "space_id": {"type": "string", "description": "Filter by specific space ID."}
            },
        },
    },
    {
        "name": "lumen_browser_history",
        "description": "Search browser history recorded in history.sqlite.",
        "parameters": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Optional search term for URL or title."},
                "limit": {"type": "integer", "default": 20, "description": "Maximum entries to return."}
            },
        },
    },
]

RESOURCES_REGISTRY = [
    {
        "uri": "lumen://spec/manifest",
        "name": "Anarc Plugin Manifest Specification",
        "mimeType": "text/markdown",
        "description": "Documentation of lumen-plugin.toml configuration options and constraints.",
    },
    {
        "uri": "lumen://spec/wit",
        "name": "Anarc WIT Guest-Host Interface",
        "mimeType": "text/plain",
        "description": "Canonical WebAssembly Component Model WIT interface definition.",
    },
    {
        "uri": "lumen://plugins/installed",
        "name": "Installed Anarc Plugins",
        "mimeType": "application/json",
        "description": "Live JSON list of all installed and active plugins.",
    },
    {
        "uri": "lumen://browser/spaces",
        "name": "Anarc Open Spaces & Tabs",
        "mimeType": "application/json",
        "description": "Live JSON state of all browser spaces and open tabs.",
    },
]

def handle_tool_call(name: str, arguments: Dict[str, Any]) -> Any:
    if name == "lumen_plugin_spec":
        return tool_spec(arguments.get("topic", "overview"))
    elif name == "lumen_plugin_scaffold":
        return tool_scaffold(
            target_dir=arguments["target_dir"],
            plugin_id=arguments["plugin_id"],
            name=arguments["name"],
            kind=arguments.get("kind", "pack"),
            description=arguments.get("description", ""),
            pack_type=arguments.get("pack_type", "userstyle"),
            match_urls=arguments.get("match_urls"),
        )
    elif name == "lumen_plugin_validate":
        return tool_validate(arguments["plugin_path"])
    elif name == "lumen_plugin_build":
        return tool_build_wasm(arguments["plugin_path"])
    elif name == "lumen_plugin_install":
        return tool_install(
            plugin_path=arguments["plugin_path"],
            symlink=arguments.get("symlink", False),
            enable=arguments.get("enable", True),
        )
    elif name == "lumen_plugin_uninstall":
        return tool_uninstall(arguments["plugin_id"])
    elif name == "lumen_plugin_list":
        return tool_list(include_builtins=arguments.get("include_builtins", True))
    elif name == "lumen_plugin_toggle":
        return tool_toggle(arguments["plugin_id"], arguments["enable"])
    elif name == "lumen_plugin_package":
        return tool_package(arguments["plugin_path"], arguments.get("output_file"))
    elif name == "lumen_browser_spaces":
        return tool_browser_spaces(arguments.get("space_id"))
    elif name == "lumen_browser_history":
        return tool_browser_history(arguments.get("query"), arguments.get("limit", 20))
    else:
        raise ValueError(f"Unknown tool: {name}")

def handle_resource_read(uri: str) -> Dict[str, Any]:
    if uri == "lumen://spec/manifest":
        return {"contents": [{"uri": uri, "mimeType": "text/markdown", "text": SPEC_MANIFEST}]}
    elif uri == "lumen://spec/wit":
        return {"contents": [{"uri": uri, "mimeType": "text/plain", "text": get_wit_schema()}]}
    elif uri == "lumen://plugins/installed":
        data = tool_list(include_builtins=True)
        return {"contents": [{"uri": uri, "mimeType": "application/json", "text": json.dumps(data, indent=2)}]}
    elif uri == "lumen://browser/spaces":
        data = tool_browser_spaces()
        return {"contents": [{"uri": uri, "mimeType": "application/json", "text": json.dumps(data, indent=2)}]}
    raise ValueError(f"Unknown resource URI: {uri}")

# ==================== JSON-RPC Protocol Loop ====================

def run_stdio_server():
    """Main stdio JSON-RPC 2.0 loop."""
    sys.stdout.reconfigure(line_buffering=True)
    
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            err_resp = {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": f"Parse error: {e}"}}
            sys.stdout.write(json.dumps(err_resp) + "\n")
            sys.stdout.flush()
            continue

        req_id = req.get("id")
        method = req.get("method")
        params = req.get("params", {})

        if method == "initialize":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {"listChanged": False},
                        "resources": {"listChanged": False, "subscribe": False},
                    },
                    "serverInfo": {
                        "name": SERVER_NAME,
                        "version": SERVER_VERSION,
                    },
                },
            }
        elif method == "notifications/initialized":
            # Notification, no response required
            continue
        elif method == "ping":
            resp = {"jsonrpc": "2.0", "id": req_id, "result": {}}
        elif method == "tools/list":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "tools": TOOLS_REGISTRY,
                },
            }
        elif method == "tools/call":
            tool_name = params.get("name")
            tool_args = params.get("arguments", {})
            try:
                out = handle_tool_call(tool_name, tool_args)
                text_out = json.dumps(out, indent=2) if isinstance(out, (dict, list)) else str(out)
                resp = {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "content": [{"type": "text", "text": text_out}],
                    },
                }
            except Exception as e:
                resp = {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "content": [{"type": "text", "text": f"Error executing tool: {e}"}],
                        "isError": True,
                    },
                }
        elif method == "resources/list":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "resources": RESOURCES_REGISTRY,
                },
            }
        elif method == "resources/read":
            uri = params.get("uri")
            try:
                res = handle_resource_read(uri)
                resp = {"jsonrpc": "2.0", "id": req_id, "result": res}
            except Exception as e:
                resp = {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {"code": -32602, "message": str(e)},
                }
        else:
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "error": {"code": -32601, "message": f"Method '{method}' not found"},
            }

        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()

if __name__ == "__main__":
    run_stdio_server()
