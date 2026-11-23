#!/usr/bin/env python3
"""Automated tests for Anarc MCP Server and Plugin Tools."""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

# Import tools directly
sys.path.insert(0, str(Path(__file__).resolve().parent))
from server import (
    tool_browser_history,
    tool_browser_spaces,
    tool_install,
    tool_list,
    tool_package,
    tool_scaffold,
    tool_spec,
    tool_toggle,
    tool_uninstall,
    tool_validate,
)

class TestLumenMcp(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.mkdtemp(prefix="lumen_mcp_test_")

    def tearDown(self):
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_spec(self):
        manifest_spec = tool_spec("manifest")
        self.assertIn("lumen-plugin.toml", manifest_spec)
        self.assertIn("reverse-DNS", manifest_spec)

        wasm_spec = tool_spec("wasm")
        self.assertIn("wasm32-wasip2", wasm_spec)
        self.assertIn("package lumen:plugin", wasm_spec)

    def test_scaffold_pack_userstyle(self):
        target = Path(self.temp_dir) / "yt_dark"
        res = tool_scaffold(
            target_dir=str(target),
            plugin_id="com.test.yt-dark",
            name="YouTube Dark",
            kind="pack",
            description="Dark style for YouTube",
            pack_type="userstyle",
            match_urls=["https://www.youtube.com/*"],
        )
        self.assertTrue(res.get("success"), res)
        self.assertTrue((target / "lumen-plugin.toml").is_file())
        self.assertTrue((target / "styles" / "style.css").is_file())

        # Validate
        val = tool_validate(str(target))
        self.assertTrue(val.get("valid"), val)
        self.assertEqual(val.get("plugin_id"), "com.test.yt-dark")
        self.assertEqual(val.get("kind"), "pack")

    def test_scaffold_wasm_component(self):
        target = Path(self.temp_dir) / "wasm_tab"
        res = tool_scaffold(
            target_dir=str(target),
            plugin_id="org.example.tab-counter",
            name="Tab Counter",
            kind="wasm",
            description="Counts tabs",
        )
        self.assertTrue(res.get("success"), res)
        self.assertTrue((target / "lumen-plugin.toml").is_file())
        self.assertTrue((target / "component" / "Cargo.toml").is_file())
        self.assertTrue((target / "component" / "src" / "lib.rs").is_file())

        val = tool_validate(str(target))
        # WASM file isn't compiled yet, so it should give a warning, but valid manifest
        self.assertTrue(val.get("valid"), val)
        self.assertTrue(any("not found yet" in w for w in val.get("warnings", [])))

    def test_scaffold_panel(self):
        target = Path(self.temp_dir) / "notes_panel"
        res = tool_scaffold(
            target_dir=str(target),
            plugin_id="com.example.notes-panel",
            name="Notes Panel",
            kind="panel",
            description="Side notes",
        )
        self.assertTrue(res.get("success"), res)
        self.assertTrue((target / "lumen-plugin.toml").is_file())
        self.assertTrue((target / "index.html").is_file())

        val = tool_validate(str(target))
        self.assertTrue(val.get("valid"), val)

    def test_validation_errors(self):
        target = Path(self.temp_dir) / "bad_plugin"
        target.mkdir()
        # Invalid ID, missing name, missing files
        (target / "lumen-plugin.toml").write_text("""
[plugin]
id = "BadID"
name = ""
version = "1.0"
api = "1"
kind = "pack"

[[pack.userstyle]]
match = ["ftp://invalid.com"]
css = "nonexistent.css"
""", encoding="utf-8")

        val = tool_validate(str(target))
        self.assertFalse(val.get("valid"))
        errors_text = " ".join(val.get("errors", []))
        self.assertIn("reverse-DNS", errors_text)
        self.assertIn("name cannot be empty", errors_text)
        self.assertIn("major.minor.patch", errors_text)
        self.assertIn("http:// or https://", errors_text)
        self.assertIn("does not exist", errors_text)

    def test_package(self):
        target = Path(self.temp_dir) / "pkg_test"
        tool_scaffold(
            target_dir=str(target),
            plugin_id="com.test.packme",
            name="Pack Me",
            kind="pack",
            pack_type="theme",
        )
        zip_path = Path(self.temp_dir) / "packme.lumenplugin"
        pkg = tool_package(str(target), str(zip_path))
        self.assertTrue(pkg.get("success"), pkg)
        self.assertTrue(zip_path.is_file())
        self.assertGreater(pkg["entries"], 1)

    def test_install_and_lifecycle(self):
        target = Path(self.temp_dir) / "lifecycle_plugin"
        pid = "com.test.lifecycle-demo"
        tool_scaffold(
            target_dir=str(target),
            plugin_id=pid,
            name="Lifecycle Demo",
            kind="pack",
            pack_type="theme",
        )
        # Install
        inst = tool_install(str(target), symlink=False, enable=True)
        self.assertTrue(inst.get("success"), inst)

        # List
        lst = tool_list(include_builtins=True)
        found = next((p for p in lst["plugins"] if p["id"] == pid), None)
        self.assertIsNotNone(found, f"Plugin {pid} not found in list: {lst}")
        self.assertTrue(found["enabled"])

        # Toggle disable
        tog = tool_toggle(pid, False)
        self.assertTrue(tog.get("success"))
        lst2 = tool_list(include_builtins=True)
        found2 = next((p for p in lst2["plugins"] if p["id"] == pid), None)
        self.assertFalse(found2["enabled"])

        # Uninstall
        uninst = tool_uninstall(pid)
        self.assertTrue(uninst.get("success"), uninst)
        lst3 = tool_list(include_builtins=True)
        found3 = next((p for p in lst3["plugins"] if p["id"] == pid), None)
        self.assertIsNone(found3)

    def test_browser_inspection(self):
        # spaces.json
        spaces = tool_browser_spaces()
        self.assertIn("spaces", spaces)
        # history.sqlite
        hist = tool_browser_history(limit=5)
        self.assertIn("entries", hist)

    def test_json_rpc_stdio_server(self):
        server_py = Path(__file__).resolve().parent / "server.py"

        # Send initialize
        req1 = json.dumps({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {}},
        })
        # Send tools/list
        req2 = json.dumps({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {},
        })
        # Send tools/call
        req3 = json.dumps({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "lumen_plugin_spec",
                "arguments": {"topic": "manifest"},
            },
        })
        # Send resources/list
        req4 = json.dumps({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "resources/list",
            "params": {},
        })

        input_data = f"{req1}\n{req2}\n{req3}\n{req4}\n"
        proc = subprocess.run(
            [sys.executable, str(server_py)],
            input=input_data,
            capture_output=True,
            text=True,
            timeout=10,
        )
        self.assertEqual(proc.returncode, 0)
        lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
        self.assertEqual(len(lines), 4)

        resp1 = json.loads(lines[0])
        self.assertEqual(resp1["result"]["serverInfo"]["name"], "lumen-mcp")

        resp2 = json.loads(lines[1])
        tool_names = [t["name"] for t in resp2["result"]["tools"]]
        self.assertIn("lumen_plugin_scaffold", tool_names)
        self.assertIn("lumen_plugin_validate", tool_names)
        self.assertIn("lumen_plugin_install", tool_names)

        resp3 = json.loads(lines[2])
        self.assertIn("lumen-plugin.toml", resp3["result"]["content"][0]["text"])

        resp4 = json.loads(lines[3])
        uris = [r["uri"] for r in resp4["result"]["resources"]]
        self.assertIn("lumen://spec/manifest", uris)

if __name__ == "__main__":
    unittest.main()
