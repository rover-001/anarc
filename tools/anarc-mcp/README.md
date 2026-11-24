# Anarc MCP Server (`lumen-mcp`)

The **Anarc MCP Server** provides tools, resources, and templates for AI coding agents (and human developers) to build, validate, test, package, install, and manage plugins for the Anarc / Anarc browser.

It implements the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) over `stdio`, making it compatible with any MCP-capable client:
- **Google Antigravity**
- **Claude Desktop / Claude Code**
- **Cursor**
- **Zed**
- **Windsurf**
- **OpenCode**

---

## Capabilities & Tools

| Tool | Description |
| :--- | :--- |
| `lumen_plugin_spec` | Returns official specifications for manifests, declarative packs, WASM WIT bindings, web panels, or theme tokens. |
| `lumen_plugin_scaffold` | Generates a complete, ready-to-run plugin folder with valid `lumen-plugin.toml`, CSS/JS assets, or WASM component bindings. |
| `lumen_plugin_validate` | Validates a plugin directory or `lumen-plugin.toml` against Anarc's strict engine requirements (reverse-DNS, SemVer, file paths, permissions). |
| `lumen_plugin_build` | Compiles a WebAssembly component plugin to `wasm32-wasip2`. |
| `lumen_plugin_install` | Installs a plugin into Anarc's live plugins directory (`$ANARC_DATA_DIR/plugins/<id>`), with support for `--symlink` during development. |
| `lumen_plugin_uninstall` | Uninstalls a plugin and cleans up configuration. |
| `lumen_plugin_list` | Lists all installed third-party and built-in plugins with active status. |
| `lumen_plugin_toggle` | Enables or disables an installed plugin in `plugins.toml`. |
| `lumen_plugin_package` | Packages a plugin into a standard `.lumenplugin` zip bundle. |
| `lumen_browser_spaces` | Inspects currently open workspaces and tabs in Anarc (`spaces.json`). |
| `lumen_browser_history` | Searches local SQLite browsing history (`history.sqlite`). |

---

## MCP Resources

The server exposes read-only MCP resources:
- `lumen://spec/manifest` — Manifest configuration reference (`lumen-plugin.toml`).
- `lumen://spec/wit` — Full WebAssembly component model interface (`wit/lumen.wit`).
- `lumen://plugins/installed` — Live JSON list of all installed plugins.
- `lumen://browser/spaces` — Live JSON view of open workspaces and tabs.

---

## Quick Start: Using in MCP Clients

### 1. Antigravity / Gemini CLI
Configured in `~/.gemini/antigravity/mcp_config.json`:
```json
{
  "mcpServers": {
    "lumen": {
      "command": "/home/rover/.local/bin/lumen-mcp",
      "args": ["serve"]
    }
  }
}
```

### 2. Claude Desktop (`claude_desktop_config.json`)
```json
{
  "mcpServers": {
    "lumen": {
      "command": "lumen-mcp",
      "args": ["serve"]
    }
  }
}
```

### 3. Cursor (`.cursor/mcp.json`)
```json
{
  "mcpServers": {
    "lumen": {
      "command": "lumen-mcp",
      "args": ["serve"]
    }
  }
}
```

---

## Direct CLI Usage

The `lumen-mcp` binary can also be used directly from the command line:

```sh
# View plugin specification
lumen-mcp spec manifest
lumen-mcp spec packs
lumen-mcp spec wasm

# Scaffold a YouTube dark mode userstyle pack
lumen-mcp scaffold --dir ./my-youtube-theme \
  --id com.example.yt-dark \
  --name "YouTube Dark" \
  --kind pack \
  --pack-type userstyle \
  --match "https://*.youtube.com/*"

# Scaffold a WASM tab management plugin
lumen-mcp scaffold --dir ./my-tab-manager \
  --id com.example.tab-manager \
  --name "Tab Manager" \
  --kind wasm

# Validate a plugin
lumen-mcp validate ./my-youtube-theme

# Install for live development (symlinked into Anarc)
lumen-mcp install ./my-youtube-theme --symlink

# List all plugins
lumen-mcp list

# Toggle plugin state
lumen-mcp toggle com.example.yt-dark --disable
lumen-mcp toggle com.example.yt-dark --enable

# Package into .lumenplugin archive
lumen-mcp package ./my-youtube-theme -o yt-dark.lumenplugin

# Inspect open tabs and workspaces
lumen-mcp spaces

# Search browsing history
lumen-mcp history "github"
```
