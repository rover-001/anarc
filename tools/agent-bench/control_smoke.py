"""Run the live Anarc agent-control regressions against local fixtures."""

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import time


def run(anarc, ctl):
    fixtures = Path(__file__).with_name("fixtures")
    handler = partial(SimpleHTTPRequestHandler, directory=fixtures)
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server, tempfile.TemporaryDirectory() as tmp:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        root = Path(tmp)
        url = f"http://127.0.0.1:{server.server_port}/trusted_input.html"
        env = dict(os.environ, ANARC_CONFIG_DIR=str(root / "config"), ANARC_DATA_DIR=str(root / "data"),
                   ANARC_CACHE_DIR=str(root / "cache"), ANARC_SOCKET_PATH=str(root / "ctl.sock"))
        config = root / "config"
        config.mkdir()
        (config / "plugins.toml").write_text('[plugins."anarc.agent"]\nenabled = true\n')
        (config / "spaces.json").write_text(json.dumps({
            "active_space": "personal",
            "spaces": [{"id": "personal", "name": "Personal", "icon": "home", "color": 0,
                        "active": "personal-tab-7", "tabs": [{"id": "personal-tab-7", "title": "New tab", "url": "about:blank"}]}],
        }))

        def call(*args, ok=True):
            proc = subprocess.run([ctl, *args], env=env, text=True, capture_output=True, timeout=38)
            assert (proc.returncode == 0) == ok, (args, proc.stdout, proc.stderr)
            return proc.stdout if ok else proc.stderr

        def mcp_shot(*options):
            request = {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "shot"}}
            proc = subprocess.run([ctl, "mcp", *options], env=env, input=json.dumps(request) + "\n",
                                  text=True, capture_output=True, timeout=20, check=True)
            return json.loads(proc.stdout)["result"]["content"][0]

        def check_png(line):
            saved = re.fullmatch(r"saved (.+) (\d+)x(\d+) \(image file: do not read_file/cat it\)\n?", line)
            assert saved, "Expected a one-line screenshot receipt"
            path = Path(saved.group(1))
            assert path.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
            path.unlink()

        with (root / "browser.log").open("w") as log:
            browser = subprocess.Popen([anarc], env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 20
                while not (root / "ctl.sock").exists() and time.monotonic() < deadline and browser.poll() is None:
                    time.sleep(0.05)
                call("do", f"go {url}")
                ref = None
                last = ""
                while time.monotonic() < deadline and browser.poll() is None:
                    last = call("see")
                    ref = re.search(r'(?m)^(i\d+) "Message"', last)
                    if ref:
                        break
                    time.sleep(0.1)
                assert ref, f"Fixture editor did not appear: {last}"
                editor = ref.group(1)
                call("do", f'{editor}="trusted"; {editor}+" key"; key Enter')
                assert "Enter: trusted key" in call("read")
                result = call("do", f'{editor}="trusted submit"; submit')
                assert "submit via Enter" in result, result
                assert "Enter: trusted submit" in call("read")
                result = call("do", f'{editor}="fallback"; submit')
                assert "submit via button" in result, result
                assert "Button: fallback" in call("read")
                call("see")
                confirm = call("do", 'b:"Pay $40"', ok=False)
                assert "needs_confirm: waiting for user approval in Anarc (intent=purchase)." in confirm, confirm
                start = time.monotonic()
                result = call("do", 'b:"Stream"; wait stable')
                assert "streaming" not in result and time.monotonic() - start >= 4.3, result
                assert call("read", "last").strip() == "stream 1 2 3 4 5 6 7 8 9 10 complete"
                assert "ok 1" in call("wait", "stable", "100")
                check_png(call("shot"))
                image = mcp_shot()
                assert image["type"] == "image" and image["mimeType"] == "image/png"
                assert image["data"].startswith("iVBORw0KGgo")
                check_png(mcp_shot("--shot-as", "path")["text"])
                stats = call("stats")
                assert re.search(r"(?m)^shot 3 .* 0\.0%$", stats), stats
                log = root / "data" / "agent" / "log.jsonl"
                entries = [json.loads(line) for line in log.read_text().splitlines()]
                assert all(entry.keys() == {"op", "ok", "reply_bytes", "est_tokens"} for entry in entries)
                assert any(entry["op"] == "do" and not entry["ok"] for entry in entries)
            except Exception:
                print((root / "browser.log").read_text()[-3000:])
                raise
            finally:
                browser.terminate()
                try:
                    browser.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    browser.kill()
                    browser.wait()
    print("Agent control smoke passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--anarc", default="target/release/anarc", type=Path)
    parser.add_argument("--ctl", default="target/release/anarc-ctl", type=Path)
    args = parser.parse_args()
    run(args.anarc.resolve(), args.ctl.resolve())
