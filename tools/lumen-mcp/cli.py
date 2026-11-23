#!/usr/bin/env python3
"""CLI interface for Anarc Plugin Tools & MCP Server.

Can be run in terminal directly or spawned by MCP clients (Claude, Antigravity, Cursor, Zed).
"""

import argparse
import json
import sys
from pathlib import Path

# Add directory to sys.path so server can be imported
sys.path.insert(0, str(Path(__file__).resolve().parent))
from server import (
    run_stdio_server,
    tool_browser_history,
    tool_browser_spaces,
    tool_build_wasm,
    tool_install,
    tool_list,
    tool_package,
    tool_scaffold,
    tool_spec,
    tool_toggle,
    tool_uninstall,
    tool_validate,
)

def main():
    parser = argparse.ArgumentParser(
        description="Anarc Browser Plugin Tool & MCP Server",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    subparsers = parser.add_subparsers(dest="command", help="Subcommand to run")

    # serve (default if no args or stdio)
    subparsers.add_parser("serve", help="Run as JSON-RPC MCP server over stdio")

    # spec
    spec_parser = subparsers.add_parser("spec", help="Print specification / reference docs")
    spec_parser.add_argument("topic", nargs="?", default="overview", choices=["overview", "manifest", "packs", "wasm", "panels", "tokens"])

    # scaffold
    scaffold_parser = subparsers.add_parser("scaffold", help="Scaffold a new plugin")
    scaffold_parser.add_argument("--dir", required=True, help="Target directory")
    scaffold_parser.add_argument("--id", required=True, help="Plugin reverse-DNS ID (e.g. com.example.plugin)")
    scaffold_parser.add_argument("--name", required=True, help="Human-readable name")
    scaffold_parser.add_argument("--kind", default="pack", choices=["pack", "wasm", "panel"])
    scaffold_parser.add_argument("--desc", default="", help="Description")
    scaffold_parser.add_argument("--pack-type", default="userstyle", choices=["userstyle", "userscript", "theme", "keymap", "search_engine"])
    scaffold_parser.add_argument("--match", action="append", help="Match URL patterns")

    # validate
    val_parser = subparsers.add_parser("validate", help="Validate a plugin")
    val_parser.add_argument("path", help="Plugin folder or lumen-plugin.toml path")

    # list
    subparsers.add_parser("list", help="List installed plugins")

    # install
    inst_parser = subparsers.add_parser("install", help="Install a plugin into Anarc")
    inst_parser.add_argument("path", help="Plugin folder")
    inst_parser.add_argument("--symlink", action="store_true", help="Symlink for live editing")
    inst_parser.add_argument("--no-enable", action="store_true", help="Do not auto-enable in plugins.toml")

    # uninstall
    uninst_parser = subparsers.add_parser("uninstall", help="Uninstall a plugin")
    uninst_parser.add_argument("id", help="Plugin ID")

    # toggle
    tog_parser = subparsers.add_parser("toggle", help="Enable or disable a plugin")
    tog_parser.add_argument("id", help="Plugin ID")
    tog_parser.add_argument("--enable", action="store_true", default=None)
    tog_parser.add_argument("--disable", action="store_true", default=None)

    # package
    pkg_parser = subparsers.add_parser("package", help="Package a plugin into .lumenplugin")
    pkg_parser.add_argument("path", help="Plugin folder")
    pkg_parser.add_argument("-o", "--output", help="Output archive path")

    # spaces
    spaces_parser = subparsers.add_parser("spaces", help="List active spaces and tabs")
    spaces_parser.add_argument("--space-id", help="Filter by space ID")

    # history
    hist_parser = subparsers.add_parser("history", help="Query browsing history")
    hist_parser.add_argument("query", nargs="?", default=None, help="Search query")
    hist_parser.add_argument("-n", "--limit", type=int, default=20, help="Max entries")

    args = parser.parse_args()

    if args.command is None or args.command == "serve":
        run_stdio_server()
        return

    if args.command == "spec":
        print(tool_spec(args.topic))
    elif args.command == "scaffold":
        res = tool_scaffold(
            target_dir=args.dir,
            plugin_id=args.id,
            name=args.name,
            kind=args.kind,
            description=args.desc,
            pack_type=args.pack_type,
            match_urls=args.match,
        )
        print(json.dumps(res, indent=2))
        sys.exit(0 if res.get("success") else 1)
    elif args.command == "validate":
        res = tool_validate(args.path)
        print(json.dumps(res, indent=2))
        sys.exit(0 if res.get("valid") else 1)
    elif args.command == "list":
        res = tool_list(include_builtins=True)
        print(json.dumps(res, indent=2))
    elif args.command == "install":
        res = tool_install(args.path, symlink=args.symlink, enable=not args.no_enable)
        print(json.dumps(res, indent=2))
        sys.exit(0 if res.get("success") else 1)
    elif args.command == "uninstall":
        res = tool_uninstall(args.id)
        print(json.dumps(res, indent=2))
        sys.exit(0 if res.get("success") else 1)
    elif args.command == "toggle":
        enable = True
        if args.disable:
            enable = False
        res = tool_toggle(args.id, enable)
        print(json.dumps(res, indent=2))
    elif args.command == "package":
        res = tool_package(args.path, args.output)
        print(json.dumps(res, indent=2))
        sys.exit(0 if res.get("success") else 1)
    elif args.command == "spaces":
        res = tool_browser_spaces(args.space_id)
        print(json.dumps(res, indent=2))
    elif args.command == "history":
        res = tool_browser_history(args.query, args.limit)
        print(json.dumps(res, indent=2))

if __name__ == "__main__":
    main()
