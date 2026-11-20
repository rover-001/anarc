#!/usr/bin/env python3
"""Lumen Live Share Daemon

Peer-to-peer browser space sharing and real-time tab sync over Tailscale.
Provides HTTP/SSE API, watches local spaces.json, and synchronizes with peers.
"""

import argparse
import copy
import json
import os
import re
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Dict, List, Optional, Set, Tuple

DEFAULT_PORT = 7433
CONFIG_DIR = Path(os.environ.get("ANARC_CONFIG_DIR", Path.home() / ".config" / "anarc"))
SPACES_FILE = CONFIG_DIR / "spaces.json"
PLUGIN_DIR = Path(__file__).resolve().parent

def get_tailscale_ip() -> str:
    """Discovers IPv4 address from tailscale daemon."""
    try:
        res = subprocess.run(["tailscale", "ip", "-4"], capture_output=True, text=True, timeout=2)
        if res.returncode == 0:
            ip = res.stdout.strip()
            if ip:
                return ip
    except Exception:
        pass
    # Fallback: check hostname or interfaces
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("8.8.8.8", 80))
        local_ip = s.getsockname()[0]
        s.close()
        return local_ip
    except Exception:
        return "127.0.0.1"

class LiveShareState:
    def __init__(self, spaces_file: Path = SPACES_FILE, port: int = DEFAULT_PORT, host_override: Optional[str] = None):
        self.lock = threading.Lock()
        self.spaces_file = spaces_file
        self.port = port
        self.tailscale_ip = host_override or get_tailscale_ip()
        self.shared_space_id: Optional[str] = None
        self.shared_space_name: Optional[str] = None
        self.is_host: bool = False
        self.connected_peers: Dict[str, Dict[str, Any]] = {}  # peer_addr -> {last_seen, status}
        self.tabs: List[Dict[str, Any]] = []
        self.activity: List[Dict[str, Any]] = []
        self.sse_subscribers: Set[Any] = set()
        self.last_spaces_mtime: float = 0.0
        self.suppress_local_watch: bool = False

    def log_activity(self, actor: str, message: str):
        now_str = datetime.now().strftime("%H:%M:%S")
        item = {"time": now_str, "actor": actor, "message": message}
        self.activity.append(item)
        if len(self.activity) > 50:
            self.activity.pop(0)

    def notify_sse(self):
        status = self.get_status_dict()
        payload = f"data: {json.dumps({'type': 'state_update', 'status': status})}\n\n"
        with self.lock:
            dead_subs = set()
            for sub in list(self.sse_subscribers):
                try:
                    sub.wfile.write(payload.encode("utf-8"))
                    sub.wfile.flush()
                except Exception:
                    dead_subs.add(sub)
            self.sse_subscribers.difference_update(dead_subs)

    def get_status_dict(self) -> Dict[str, Any]:
        with self.lock:
            return {
                "tailscale_ip": self.tailscale_ip,
                "shared_space_id": self.shared_space_id,
                "shared_space_name": self.shared_space_name,
                "is_host": self.is_host,
                "peers": [
                    {"address": addr, "status": info.get("status", "connected")}
                    for addr, info in self.connected_peers.items()
                ],
                "tabs": copy.deepcopy(self.tabs),
                "activity": copy.deepcopy(self.activity),
            }

    def share_local_space(self, space_id: Optional[str]) -> bool:
        with self.lock:
            if not space_id:
                self.shared_space_id = None
                self.shared_space_name = None
                self.tabs.clear()
                self.log_activity("Local", "Stopped sharing space")
                return True

            self.shared_space_id = space_id
            self.is_host = True
            # Load tabs from spaces.json
            tabs, name = self._read_space_tabs(space_id)
            self.shared_space_name = name or space_id
            self.tabs = tabs
            self.log_activity("Local", f"Started sharing space '{self.shared_space_name}' ({len(tabs)} tabs)")

        # Broadcast update to peers
        self.broadcast_to_peers("sync_state", {
            "space_id": self.shared_space_id,
            "space_name": self.shared_space_name,
            "tabs": self.tabs,
        })
        self.notify_sse()
        return True

    def _read_space_tabs(self, space_id: str) -> Tuple[List[Dict[str, Any]], Optional[str]]:
        if not self.spaces_file.is_file():
            return [], None
        try:
            data = json.loads(self.spaces_file.read_text(encoding="utf-8"))
            for sp in data.get("spaces", []):
                if sp.get("id") == space_id:
                    tabs = []
                    for t in sp.get("tabs", []):
                        tabs.append({
                            "id": t.get("id"),
                            "title": t.get("title", ""),
                            "url": t.get("url", ""),
                            "pinned": t.get("pinned", False),
                            "shared_by": "You",
                        })
                    return tabs, sp.get("name")
        except Exception as e:
            print(f"[Error reading spaces.json] {e}", file=sys.stderr)
        return [], None

    def connect_to_peer(self, peer_address: str) -> Tuple[bool, Optional[str]]:
        peer_address = peer_address.strip()
        if not peer_address:
            return False, "Empty peer address"
        if ":" not in peer_address:
            peer_address = f"{peer_address}:{DEFAULT_PORT}"

        url = f"http://{peer_address}/api/peer/sync"
        try:
            req = urllib.request.Request(
                url,
                headers={"X-Peer-Tailscale": self.tailscale_ip, "User-Agent": "LumenLiveShare/1.0"},
            )
            with urllib.request.urlopen(req, timeout=5) as resp:
                if resp.status != 200:
                    return False, f"Peer HTTP status {resp.status}"
                data = json.loads(resp.read().decode("utf-8"))

            with self.lock:
                self.connected_peers[peer_address] = {"last_seen": time.time(), "status": "connected"}
                self.shared_space_id = data.get("space_id") or "shared-space"
                self.shared_space_name = data.get("space_name") or "Shared Space"
                self.is_host = False
                self.tabs = data.get("tabs", [])
                self.log_activity(peer_address, f"Connected to peer's space '{self.shared_space_name}'")

            # Merge into local spaces.json
            self._merge_into_spaces_file()
            self.notify_sse()

            # Notify the remote peer that we connected
            self.send_to_peer(peer_address, "peer_joined", {"peer_address": f"{self.tailscale_ip}:{self.port}"})
            return True, None
        except Exception as e:
            return False, f"Connection failed: {e}"

    def broadcast_to_peers(self, event_type: str, payload: Dict[str, Any]):
        threads = []
        with self.lock:
            peers = list(self.connected_peers.keys())
        for peer in peers:
            t = threading.Thread(target=self.send_to_peer, args=(peer, event_type, payload))
            t.daemon = True
            t.start()
            threads.append(t)

    def send_to_peer(self, peer_address: str, event_type: str, payload: Dict[str, Any]):
        url = f"http://{peer_address}/api/peer/event"
        body = json.dumps({
            "type": event_type,
            "origin": f"{self.tailscale_ip}:{self.port}",
            "payload": payload,
        }).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=body,
            headers={"Content-Type": "application/json", "User-Agent": "LumenLiveShare/1.0"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=4) as resp:
                pass
        except Exception as e:
            print(f"[Warning] Failed to send {event_type} to {peer_address}: {e}", file=sys.stderr)

    def handle_peer_event(self, event_type: str, origin: str, payload: Dict[str, Any]):
        peer_addr = payload.get("peer_address") or origin
        with self.lock:
            self.connected_peers[peer_addr] = {"last_seen": time.time(), "status": "connected"}

        if event_type == "peer_joined":
            self.log_activity(peer_addr, f"Peer {peer_addr} joined the shared space")
            # Reply with current full state
            self.send_to_peer(peer_addr, "sync_state", {
                "space_id": self.shared_space_id,
                "space_name": self.shared_space_name,
                "tabs": self.tabs,
            })

        elif event_type == "sync_state":
            with self.lock:
                self.shared_space_id = payload.get("space_id") or self.shared_space_id
                self.shared_space_name = payload.get("space_name") or self.shared_space_name
                self.tabs = payload.get("tabs", [])
                self.log_activity(origin, f"Synchronized {len(self.tabs)} tabs from peer")
            self._merge_into_spaces_file()

        elif event_type == "tab_opened":
            tab = payload.get("tab")
            if tab:
                tab["shared_by"] = origin
                with self.lock:
                    # Deduplicate by url/id
                    self.tabs = [t for t in self.tabs if t.get("id") != tab.get("id")]
                    self.tabs.append(tab)
                    self.log_activity(origin, f"Opened tab '{tab.get('title') or tab.get('url')}'")
                self._merge_into_spaces_file()

        elif event_type == "tab_closed":
            tab_id = payload.get("tab_id")
            if tab_id:
                with self.lock:
                    self.tabs = [t for t in self.tabs if t.get("id") != tab_id]
                    self.log_activity(origin, f"Closed tab ({tab_id})")
                self._merge_into_spaces_file()

        elif event_type == "tab_navigated":
            tab_id = payload.get("tab_id")
            new_url = payload.get("url")
            new_title = payload.get("title", new_url)
            if tab_id and new_url:
                with self.lock:
                    for t in self.tabs:
                        if t.get("id") == tab_id:
                            t["url"] = new_url
                            t["title"] = new_title
                    self.log_activity(origin, f"Navigated to '{new_title}'")
                self._merge_into_spaces_file()

        self.notify_sse()

    def local_action_open_tab(self, url: str, title: str) -> Dict[str, Any]:
        tab_id = f"share-tab-{int(time.time() * 1000) % 1000000}"
        tab = {
            "id": tab_id,
            "title": title or url,
            "url": url,
            "pinned": False,
            "shared_by": "You",
        }
        with self.lock:
            self.tabs.append(tab)
            self.log_activity("You", f"Opened tab '{tab['title']}'")

        self._merge_into_spaces_file()
        self.broadcast_to_peers("tab_opened", {"tab": tab})
        self.notify_sse()
        return {"success": True, "tab": tab}

    def local_action_close_tab(self, tab_id: str) -> Dict[str, Any]:
        with self.lock:
            self.tabs = [t for t in self.tabs if t.get("id") != tab_id]
            self.log_activity("You", f"Closed tab ({tab_id})")

        self._merge_into_spaces_file()
        self.broadcast_to_peers("tab_closed", {"tab_id": tab_id})
        self.notify_sse()
        return {"success": True}

    def _merge_into_spaces_file(self):
        """Atomically merges the shared tabs into local spaces.json."""
        if not self.shared_space_id:
            return

        with self.lock:
            self.suppress_local_watch = True

        try:
            if not self.spaces_file.is_file():
                return

            text = self.spaces_file.read_text(encoding="utf-8")
            data = json.loads(text)
            spaces = data.get("spaces", [])

            # Find matching space or create one
            target_space = None
            for sp in spaces:
                if sp.get("id") == self.shared_space_id:
                    target_space = sp
                    break

            if target_space is None:
                # Create a new shared space
                space_id = self.shared_space_id or "shared-space"
                space_name = self.shared_space_name or "Shared Space"
                target_space = {
                    "id": space_id,
                    "name": f"[Shared] {space_name}",
                    "icon": "share-2",
                    "color": 10135039,  # #9AA6FF
                    "theme_colors": [10135039, 4907263],
                    "theme_appearance": 0,
                    "theme_intensity": 0.5,
                    "theme_grain": 0.0,
                    "tabs": [],
                    "folders": [],
                }
                spaces.append(target_space)

            # Update tabs in target_space
            existing_tabs = target_space.get("tabs", [])
            merged_tabs = []
            for st in self.tabs:
                # Keep existing state if already there
                match = next((et for et in existing_tabs if et.get("id") == st.get("id")), None)
                if match:
                    match["url"] = st.get("url")
                    match["title"] = st.get("title", st.get("url"))
                    merged_tabs.append(match)
                else:
                    merged_tabs.append({
                        "id": st.get("id"),
                        "title": st.get("title", ""),
                        "url": st.get("url", ""),
                        "pinned": st.get("pinned", False),
                        "favorite": False,
                        "state": "Live",
                    })
            target_space["tabs"] = merged_tabs

            # Write out atomically
            tmp = self.spaces_file.with_suffix(".tmp")
            tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
            tmp.replace(self.spaces_file)
            self.last_spaces_mtime = self.spaces_file.stat().st_mtime
        except Exception as e:
            print(f"[Error writing spaces.json] {e}", file=sys.stderr)
        finally:
            with self.lock:
                self.suppress_local_watch = False

    def watch_spaces_file(self):
        """Background thread watching for local changes made inside Lumen."""
        while True:
            time.sleep(1.0)
            if not self.spaces_file.is_file() or not self.shared_space_id:
                continue

            with self.lock:
                if self.suppress_local_watch:
                    continue

            try:
                mtime = self.spaces_file.stat().st_mtime
                if mtime > self.last_spaces_mtime:
                    self.last_spaces_mtime = mtime
                    # Read current tabs
                    fresh_tabs, _ = self._read_space_tabs(self.shared_space_id)
                    diff = False
                    with self.lock:
                        if len(fresh_tabs) != len(self.tabs):
                            diff = True
                        else:
                            for ft, ot in zip(fresh_tabs, self.tabs):
                                if ft.get("id") != ot.get("id") or ft.get("url") != ot.get("url"):
                                    diff = True
                                    break
                        if diff:
                            self.tabs = fresh_tabs
                            self.log_activity("Lumen", f"Local space updated ({len(fresh_tabs)} tabs)")

                    if diff:
                        self.broadcast_to_peers("sync_state", {
                            "space_id": self.shared_space_id,
                            "space_name": self.shared_space_name,
                            "tabs": self.tabs,
                        })
                        self.notify_sse()
            except Exception:
                pass


# HTTP Request Handler
class LiveShareHandler(BaseHTTPRequestHandler):
    state: LiveShareState

    def do_OPTIONS(self):
        self.send_response(HTTPStatus.OK)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, X-Peer-Tailscale")
        self.end_headers()

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path

        if path in ("/", "/index.html"):
            self.serve_file(PLUGIN_DIR / "index.html", "text/html; charset=utf-8")
        elif path == "/styles.css":
            self.serve_file(PLUGIN_DIR / "styles.css", "text/css; charset=utf-8")
        elif path == "/app.js":
            self.serve_file(PLUGIN_DIR / "app.js", "application/javascript; charset=utf-8")

        elif path == "/api/status":
            self.send_json(self.state.get_status_dict())

        elif path == "/api/spaces":
            spaces = []
            if self.state.spaces_file.is_file():
                try:
                    data = json.loads(self.state.spaces_file.read_text(encoding="utf-8"))
                    spaces = data.get("spaces", [])
                except Exception:
                    pass
            self.send_json({"spaces": spaces})

        elif path == "/api/peer/sync":
            status = self.state.get_status_dict()
            self.send_json({
                "space_id": status["shared_space_id"],
                "space_name": status["shared_space_name"],
                "tabs": status["tabs"],
            })

        elif path == "/api/stream":
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Connection", "keep-alive")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()

            with self.state.lock:
                self.state.sse_subscribers.add(self)

            # Send initial state
            init_payload = f"data: {json.dumps({'type': 'state_update', 'status': self.state.get_status_dict()})}\n\n"
            try:
                self.wfile.write(init_payload.encode("utf-8"))
                self.wfile.flush()
                while True:
                    time.sleep(15)
                    self.wfile.write(b": keepalive\n\n")
                    self.wfile.flush()
            except Exception:
                with self.state.lock:
                    self.state.sse_subscribers.discard(self)

        else:
            self.send_response(HTTPStatus.NOT_FOUND)
            self.end_headers()

    def do_POST(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path
        body = self.read_body_json()

        if path == "/api/share":
            space_id = body.get("space_id")
            ok = self.state.share_local_space(space_id)
            self.send_json({"success": ok, "shared_space_id": self.state.shared_space_id})

        elif path == "/api/connect":
            peer_addr = body.get("peer_address", "")
            ok, err = self.state.connect_to_peer(peer_addr)
            self.send_json({"success": ok, "error": err})

        elif path == "/api/action":
            action = body.get("action")
            if action == "open_tab":
                url = body.get("url", "")
                title = body.get("title", url)
                res = self.state.local_action_open_tab(url, title)
                self.send_json(res)
            elif action == "close_tab":
                tab_id = body.get("tab_id", "")
                res = self.state.local_action_close_tab(tab_id)
                self.send_json(res)
            else:
                self.send_json({"error": f"Unknown action {action}"}, status=HTTPStatus.BAD_REQUEST)

        elif path == "/api/peer/event":
            event_type = body.get("type", "")
            origin = body.get("origin") or self.headers.get("X-Peer-Tailscale") or self.client_address[0]
            payload = body.get("payload", {})
            self.state.handle_peer_event(event_type, origin, payload)
            self.send_json({"success": True})

        else:
            self.send_response(HTTPStatus.NOT_FOUND)
            self.end_headers()

    def serve_file(self, file_path: Path, content_type: str):
        if not file_path.is_file():
            self.send_response(HTTPStatus.NOT_FOUND)
            self.end_headers()
            return
        content = file_path.read_bytes()
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(content)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(content)

    def read_body_json(self) -> Dict[str, Any]:
        length = int(self.headers.get("Content-Length", 0))
        if length <= 0:
            return {}
        raw = self.rfile.read(length)
        try:
            return json.loads(raw.decode("utf-8"))
        except Exception:
            return {}

    def send_json(self, data: Any, status: HTTPStatus = HTTPStatus.OK):
        body = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        # Quiet logger
        pass

def start_daemon(host: str = "0.0.0.0", port: int = DEFAULT_PORT, spaces_file: Path = SPACES_FILE) -> ThreadingHTTPServer:
    host_override = host if host not in ("0.0.0.0", "") else None
    state = LiveShareState(spaces_file=spaces_file, port=port, host_override=host_override)
    handler_cls = type("BoundLiveShareHandler", (LiveShareHandler,), {"state": state})
    server = ThreadingHTTPServer((host, port), handler_cls)

    # Start watcher
    watcher_thread = threading.Thread(target=state.watch_spaces_file, daemon=True)
    watcher_thread.start()

    print(f"[Lumen Live Share] Running on http://{host}:{port}")
    print(f"[Tailscale IP] {state.tailscale_ip}")
    print(f"[Share Address] {state.tailscale_ip}:{port}")
    return server

def main():
    parser = argparse.ArgumentParser(description="Lumen Live Share Daemon")
    parser.add_argument("--host", default="0.0.0.0", help="Bind address (default 0.0.0.0)")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port (default 7433)")
    parser.add_argument("--spaces", default=str(SPACES_FILE), help="Path to spaces.json")
    args = parser.parse_args()

    server = start_daemon(host=args.host, port=args.port, spaces_file=Path(args.spaces))
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping Live Share daemon...")
        server.shutdown()

if __name__ == "__main__":
    main()
