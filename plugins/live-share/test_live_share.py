#!/usr/bin/env python3
"""Integration tests for Lumen Live Share over Tailscale / Loopback."""

import json
import shutil
import tempfile
import threading
import time
import unittest
import urllib.request
from pathlib import Path

from daemon import start_daemon

class TestLiveShare(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.mkdtemp(prefix="lumen_live_share_test_")
        self.spaces_a = Path(self.temp_dir) / "spaces_a.json"
        self.spaces_b = Path(self.temp_dir) / "spaces_b.json"

        # Mock spaces for Node A
        self.spaces_a.write_text(json.dumps({
            "spaces": [
                {
                    "id": "personal",
                    "name": "Personal Space",
                    "tabs": [
                        {"id": "tab-1", "title": "LeetCode", "url": "https://leetcode.com"},
                        {"id": "tab-2", "title": "GitHub", "url": "https://github.com"},
                    ],
                }
            ]
        }), encoding="utf-8")

        # Mock spaces for Node B
        self.spaces_b.write_text(json.dumps({
            "spaces": [
                {
                    "id": "work",
                    "name": "Work Space",
                    "tabs": [],
                }
            ]
        }), encoding="utf-8")

        # Start Node A on 17433
        self.server_a = start_daemon(host="127.0.0.1", port=17433, spaces_file=self.spaces_a)
        self.thread_a = threading.Thread(target=self.server_a.serve_forever, daemon=True)
        self.thread_a.start()

        # Start Node B on 17434
        self.server_b = start_daemon(host="127.0.0.1", port=17434, spaces_file=self.spaces_b)
        self.thread_b = threading.Thread(target=self.server_b.serve_forever, daemon=True)
        self.thread_b.start()

        time.sleep(0.2)

    def tearDown(self):
        self.server_a.shutdown()
        self.server_b.shutdown()
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_full_live_share_flow(self):
        # 1. Check Node A status
        with urllib.request.urlopen("http://127.0.0.1:17433/api/status") as resp:
            data_a = json.loads(resp.read().decode("utf-8"))
        self.assertIsNotNone(data_a.get("tailscale_ip"))

        # 2. Node A shares space "personal"
        req = urllib.request.Request(
            "http://127.0.0.1:17433/api/share",
            data=json.dumps({"space_id": "personal"}).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            share_res = json.loads(resp.read().decode("utf-8"))
        self.assertTrue(share_res.get("success"))
        self.assertEqual(share_res.get("shared_space_id"), "personal")

        # Verify Node A has 2 tabs loaded
        with urllib.request.urlopen("http://127.0.0.1:17433/api/status") as resp:
            status_a = json.loads(resp.read().decode("utf-8"))
        self.assertEqual(len(status_a["tabs"]), 2)

        # 3. Node B connects to Node A over loopback/Tailscale address
        req = urllib.request.Request(
            "http://127.0.0.1:17434/api/connect",
            data=json.dumps({"peer_address": "127.0.0.1:17433"}).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            conn_res = json.loads(resp.read().decode("utf-8"))
        self.assertTrue(conn_res.get("success"), conn_res)

        # Verify Node B synchronized Node A's space and tabs
        with urllib.request.urlopen("http://127.0.0.1:17434/api/status") as resp:
            status_b = json.loads(resp.read().decode("utf-8"))
        self.assertEqual(status_b["shared_space_id"], "shared-space")
        self.assertEqual(len(status_b["tabs"]), 2)
        self.assertEqual(status_b["tabs"][0]["title"], "LeetCode")

        # Verify Node B's spaces_b.json received the shared space
        b_spaces_data = json.loads(self.spaces_b.read_text(encoding="utf-8"))
        shared_space_in_b = next((s for s in b_spaces_data["spaces"] if s["id"] == "shared-space"), None)
        self.assertIsNotNone(shared_space_in_b)
        self.assertEqual(len(shared_space_in_b["tabs"]), 2)

        # 4. Node B searches the web in the shared space
        search_query = "rust tailscale peer to peer"
        search_url = f"https://www.google.com/search?q={urllib.parse.quote(search_query)}"
        req = urllib.request.Request(
            "http://127.0.0.1:17434/api/action",
            data=json.dumps({
                "action": "open_tab",
                "url": search_url,
                "title": search_query,
            }).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            action_res = json.loads(resp.read().decode("utf-8"))
        self.assertTrue(action_res.get("success"))

        # Wait for sync
        time.sleep(0.3)

        # Verify Node A received the new tab opened by Node B
        with urllib.request.urlopen("http://127.0.0.1:17433/api/status") as resp:
            status_a_after = json.loads(resp.read().decode("utf-8"))
        self.assertEqual(len(status_a_after["tabs"]), 3)
        new_tab = status_a_after["tabs"][-1]
        self.assertEqual(new_tab["title"], search_query)
        self.assertEqual(new_tab["url"], search_url)

        # Verify Node A's spaces_a.json file has the new tab
        a_spaces_data = json.loads(self.spaces_a.read_text(encoding="utf-8"))
        personal_space_a = next(s for s in a_spaces_data["spaces"] if s["id"] == "personal")
        self.assertEqual(len(personal_space_a["tabs"]), 3)

        # 5. Node A closes a tab
        tab_to_close = new_tab["id"]
        req = urllib.request.Request(
            "http://127.0.0.1:17433/api/action",
            data=json.dumps({
                "action": "close_tab",
                "tab_id": tab_to_close,
            }).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req) as resp:
            close_res = json.loads(resp.read().decode("utf-8"))
        self.assertTrue(close_res.get("success"))

        time.sleep(0.3)

        # Verify Node B no longer has the closed tab
        with urllib.request.urlopen("http://127.0.0.1:17434/api/status") as resp:
            status_b_after = json.loads(resp.read().decode("utf-8"))
        self.assertEqual(len(status_b_after["tabs"]), 2)
        self.assertIsNone(next((t for t in status_b_after["tabs"] if t["id"] == tab_to_close), None))

if __name__ == "__main__":
    unittest.main()
