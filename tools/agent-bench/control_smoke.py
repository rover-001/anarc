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
                        "active": "personal-tab-7", "tabs": [{"id": "personal-tab-7", "title": "Fixture", "url": url}]}],
        }))

        def call(*args):
            proc = subprocess.run([ctl, *args], env=env, text=True, capture_output=True, timeout=38)
            assert proc.returncode == 0, (args, proc.stderr)
            return proc.stdout

        with (root / "browser.log").open("w") as log:
            browser = subprocess.Popen([anarc], env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 20
                ref = None
                while time.monotonic() < deadline and browser.poll() is None:
                    try:
                        ref = re.search(r'(?m)^(i\d+) "Message"', call("see"))
                    except AssertionError:
                        pass
                    if ref:
                        break
                    time.sleep(0.1)
                assert ref, "Fixture editor did not appear"
                editor = ref.group(1)
                call("do", f'{editor}="trusted"; {editor}+" key"; key Enter')
                assert "Enter: trusted key" in call("read")
                result = call("do", f'{editor}="trusted submit"; submit')
                assert "submit via Enter" in result, result
                assert "Enter: trusted submit" in call("read")
                result = call("do", f'{editor}="fallback"; submit')
                assert "submit via button" in result, result
                assert "Button: fallback" in call("read")
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
