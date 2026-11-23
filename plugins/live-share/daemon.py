#!/usr/bin/env python3
"""Anarc Live Share Daemon

Peer-to-peer browser space sharing and real-time tab sync over Tailscale.
Provides HTTP/SSE API, watches local spaces.json, and synchronizes with peers.
"""

import argparse
import copy
import json
import os
import re
import socket
import struct
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Dict, List, Optional, Set, Tuple

DEFAULT_PORT = 7433
CONFIG_DIR = Path(os.environ.get("ANARC_CONFIG_DIR", Path.home() / ".config" / "anarc"))
SPACES_FILE = CONFIG_DIR / "spaces.json"
COOKIES_FILE = CONFIG_DIR / "shared-cookies.json"
SESSION_FILE = CONFIG_DIR / "live-share-session.json"
PLUGIN_DIR = Path(__file__).resolve().parent
WIN_NO_WINDOW = 0x08000000 if sys.platform == "win32" else 0

# Tailnet discovery shells out to `tailscale status` and probes every peer's
# port. Doing that on every /api/tailscale/peers request used to block the
# browser's CEF IO thread (the plugin proxy is synchronous) for seconds at a
# time. Cache the result and refresh it in the background instead.
TS_CACHE_TTL = 20.0
_ts_cache_lock = threading.Lock()
_ts_cache = {"data": None, "at": 0.0}
_ts_refresh_started = False


def get_tailscale_ip() -> str:
    """Discovers IPv4 address from tailscale daemon."""
    try:
        res = subprocess.run(["tailscale", "ip", "-4"], capture_output=True, text=True, timeout=2, creationflags=WIN_NO_WINDOW)
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

def check_browser(ip: str) -> bool:
    try:
        with socket.create_connection((ip, DEFAULT_PORT), timeout=0.2):
            return True
    except Exception:
        return False


def get_tailscale_peers(force: bool = False) -> Dict[str, Any]:
    """Cached tailnet discovery. Returns the cached snapshot instantly; the
    snapshot is refreshed by a background thread every TS_CACHE_TTL seconds."""
    now = time.monotonic()
    with _ts_cache_lock:
        cached = _ts_cache["data"]
        if not force and cached is not None and (now - _ts_cache["at"]) < TS_CACHE_TTL:
            return cached
    data = _query_tailscale_peers()
    with _ts_cache_lock:
        # Keep the older snapshot when a refresh fails (tailscale daemon busy).
        if not data.get("error") or _ts_cache["data"] is None:
            _ts_cache["data"] = data
            _ts_cache["at"] = now
        else:
            data = _ts_cache["data"]
    return data


def _ts_refresh_loop():
    """Warm the discovery cache at startup and keep it fresh off the request path."""
    while True:
        get_tailscale_peers(force=True)
        time.sleep(TS_CACHE_TTL)


def _query_tailscale_peers() -> Dict[str, Any]:
    """Discovers self machine and peers via tailscale status --json, filtering for online browser peers."""
    try:
        res = subprocess.run(["tailscale", "status", "--json"], capture_output=True, text=True, timeout=2, creationflags=WIN_NO_WINDOW)
        if res.returncode != 0:
            return {"self": None, "peers": []}
        data = json.loads(res.stdout)
        self_node = data.get("Self", {})
        self_ips = self_node.get("TailscaleIPs", [])
        self_ip = next((ip for ip in self_ips if "." in ip), "")
        self_info = {
            "hostname": self_node.get("HostName", "This Machine"),
            "ip": self_ip,
            "dns_name": self_node.get("DNSName", "").rstrip("."),
        }

        raw_peers = []
        for p in (data.get("Peer") or {}).values():
            if not p.get("Online", False):
                continue
            hostname = p.get("HostName", "")
            dns = p.get("DNSName", "").rstrip(".")
            if not dns and not p.get("OS"):
                # Skip internal ingress/relay nodes
                continue
            ips = p.get("TailscaleIPs", [])
            ipv4 = next((ip for ip in ips if "." in ip), "")
            if not ipv4 or ipv4 == self_ip:
                continue
            display_name = dns.split(".")[0] if dns else hostname
            raw_peers.append({
                "hostname": display_name,
                "dns_name": dns,
                "ip": ipv4,
                "online": True,
                "os": p.get("OS", ""),
            })

        # Only include online peers that have Anarc Live Share running
        peers = []
        if raw_peers:
            with ThreadPoolExecutor(max_workers=min(len(raw_peers), 8)) as ex:
                checks = {ex.submit(check_browser, rp["ip"]): rp for rp in raw_peers}
                for fut in checks:
                    rp = checks[fut]
                    try:
                        if fut.result():
                            rp["has_browser"] = True
                            peers.append(rp)
                    except Exception:
                        pass

        peers.sort(key=lambda x: x["hostname"])
        return {"self": self_info, "peers": peers}
    except Exception as e:
        return {"error": str(e), "self": None, "peers": []}

AUTH_PATTERNS = [
    "accounts.google.",
    "accounts.youtube.",
    "workspace.google.com",
    "google.com/gmail/about",
    "google.com/intl/",
    "login.microsoftonline.com",
    "login.live.com",
    "appleid.apple.com",
    "/oauth2/",
    "/oauth/",
    "/signin/v2/",
    "/v3/signin/",
    "/signin",
    "/login",
    "/logout",
    "/accountchooser",
    "auth0.com",
    "okta.com",
    "github.com/login",
    "github.com/session",
]

def is_auth_url(url: str) -> bool:
    if not url:
        return False
    u = url.lower()
    return any(domain in u for domain in AUTH_PATTERNS)

def canonical_url(url: str, pinned_url: Optional[str] = None) -> str:
    """Returns a clean canonical URL for shared space tabs, avoiding transient auth/landing redirects."""
    if pinned_url and not is_auth_url(pinned_url):
        return pinned_url
    if not is_auth_url(url):
        return url
    u = url.lower()
    if "gmail" in u or "mail.google" in u or "workspace.google.com" in u:
        return "https://mail.google.com/"
    if "google.com" in u:
        return "https://www.google.com/"
    if "login.microsoft" in u or "live.com" in u:
        return "https://outlook.live.com/"
    return url


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
        self.theme: Optional[Dict[str, Any]] = None
        self.cookies: List[Dict[str, Any]] = []
        if COOKIES_FILE.is_file():
            try:
                self.cookies = json.loads(COOKIES_FILE.read_text(encoding="utf-8"))
            except Exception:
                pass
        self.activity: List[Dict[str, Any]] = []
        self.sse_subscribers: Set[Any] = set()
        self.last_spaces_mtime: float = 0.0
        self.suppress_local_watch: bool = False
        # Bumped on every session transition. The browser watches this file and
        # exports the host's full session (account cookies / page logins) as soon
        # as sharing starts, before any peer connects.
        self.session_generation = 0
        self._write_session_file()


    def _write_session_file(self):
        """Publish session state so the embedded browser can react without polling HTTP."""
        with self.lock:
            payload = {
                "active": bool(self.shared_space_id),
                "is_host": self.is_host,
                "generation": self.session_generation,
                "space_id": self.shared_space_id,
            }
        try:
            tmp = SESSION_FILE.with_suffix(".json.tmp")
            tmp.write_text(json.dumps(payload), encoding="utf-8")
            tmp.replace(SESSION_FILE)
        except Exception:
            pass

    def bump_session_generation(self):
        with self.lock:
            self.session_generation += 1
        self._write_session_file()

    @staticmethod
    def _save_cookies(cookies: List[Dict[str, Any]]):
        """Atomically persist the session snapshot shared with peers."""
        try:
            tmp = COOKIES_FILE.with_suffix(".json.tmp")
            tmp.write_text(json.dumps(cookies, indent=2), encoding="utf-8")
            tmp.replace(COOKIES_FILE)
        except Exception as e:
            print(f"[Error saving shared-cookies.json] {e}", file=sys.stderr)

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
                "theme": copy.deepcopy(self.theme),
                "cookies": copy.deepcopy(self.cookies),
                "activity": copy.deepcopy(self.activity),
            }

    def share_local_space(self, space_id: Optional[str]) -> bool:
        if not space_id:
            return self.disconnect_session()

        with self.lock:
            self.shared_space_id = space_id
            self.is_host = True
            # Load tabs and theme from spaces.json
            tabs, name, theme = self._read_space_tabs(space_id)
            self.shared_space_name = name or space_id
            self.tabs = tabs
            self.theme = theme
            self.log_activity("Local", f"Started sharing space '{self.shared_space_name}' ({len(tabs)} tabs)")
        self.bump_session_generation()

        # Broadcast update to peers
        self.broadcast_to_peers("sync_state", {
            "space_id": self.shared_space_id,
            "space_name": self.shared_space_name,
            "tabs": self.tabs,
            "theme": self.theme,
            "cookies": self.cookies,
        })
        self.notify_sse()
        return True

    def disconnect_session(self) -> bool:
        peers_to_notify = []
        is_host = False
        with self.lock:
            is_host = self.is_host
            self.shared_space_id = None
            self.shared_space_name = None
            self.tabs.clear()
            self.theme = None
            peers_to_notify = list(self.connected_peers.keys())
            self.connected_peers.clear()
            self.is_host = False
            self.log_activity("Local", "Ended Live Share session")

        event = "session_ended" if is_host else "peer_left"
        for peer in peers_to_notify:
            self.send_to_peer(peer, event, {"peer_address": f"{self.tailscale_ip}:{self.port}"})

        self._remove_shared_space_from_file()
        self.bump_session_generation()
        self.notify_sse()
        return True

    def _remove_shared_space_from_file(self):
        with self.lock:
            self.suppress_local_watch = True
        try:
            if not self.spaces_file.is_file():
                return
            text = self.spaces_file.read_text(encoding="utf-8")
            data = json.loads(text)
            spaces = data.get("spaces", [])
            filtered = [
                sp for sp in spaces
                if sp.get("id") != "shared-space" and not sp.get("name", "").startswith("[Shared]")
            ]
            if len(filtered) != len(spaces):
                data["spaces"] = filtered
                tmp = self.spaces_file.with_suffix(".tmp")
                tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
                tmp.replace(self.spaces_file)
                self.last_spaces_mtime = self.spaces_file.stat().st_mtime
        except Exception as e:
            print(f"[Error removing shared space] {e}", file=sys.stderr)
        finally:
            with self.lock:
                self.suppress_local_watch = False

    def _read_space_tabs(self, space_id: str) -> Tuple[List[Dict[str, Any]], Optional[str], Optional[Dict[str, Any]]]:
        if not self.spaces_file.is_file():
            return [], None, None
        try:
            data = json.loads(self.spaces_file.read_text(encoding="utf-8"))
            for sp in data.get("spaces", []):
                if sp.get("id") == space_id:
                    color = sp.get("color", 10135039)
                    theme = {
                        "color": color,
                        "theme_colors": sp.get("theme_colors", [color]),
                        "theme_appearance": sp.get("theme_appearance", 0),
                        "theme_intensity": sp.get("theme_intensity", 0.5),
                        "theme_grain": sp.get("theme_grain", 0.0),
                    }
                    tabs = []
                    for t in sp.get("tabs", []):
                        raw_url = t.get("url", "").strip()
                        if raw_url.startswith("anarc-plugin://") or raw_url.startswith("anarc-internal://"):
                            continue
                        pinned_url = t.get("pinned_url")
                        is_fav = bool(t.get("favorite", False))
                        is_pin = bool(t.get("pinned", False))
                        safe_url = canonical_url(raw_url, pinned_url)
                        effective_pinned_url = pinned_url or (safe_url if (is_fav or is_pin) else None)
                        effective_title = t.get("pinned_title") or t.get("title", "")
                        tabs.append({
                            "id": t.get("id"),
                            "title": effective_title,
                            "url": safe_url,
                            "pinned": is_pin,
                            "favorite": is_fav,
                            "pinned_url": effective_pinned_url,
                            "pinned_title": effective_title if (is_fav or is_pin) else t.get("pinned_title"),
                            "folder": t.get("folder"),
                            "shared_by": "You",
                        })
                    return tabs, sp.get("name"), theme
        except Exception as e:
            print(f"[Error reading spaces.json] {e}", file=sys.stderr)
        return [], None, None

    def connect_to_peer(self, peer_address: str) -> Tuple[bool, Optional[str]]:
        peer_address = peer_address.strip()
        if not peer_address:
            return False, "Empty peer address"

        port = DEFAULT_PORT
        host = peer_address
        if ":" in peer_address:
            parts = peer_address.split(":", 1)
            host = parts[0].strip()
            try:
                port = int(parts[1].strip())
            except ValueError:
                port = DEFAULT_PORT

        # Resolve peer hostname if not already an IP
        if not re.match(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$", host):
            ts_data = get_tailscale_peers()
            resolved = None
            target = host.lower()
            for p in ts_data.get("peers", []):
                h = p.get("hostname", "").lower()
                d = p.get("dns_name", "").lower()
                if target == h or target == d or target == d.split(".")[0]:
                    resolved = p.get("ip")
                    break
            if resolved:
                host = resolved
            else:
                try:
                    host = socket.gethostbyname(host)
                except Exception:
                    pass

        peer_address = f"{host}:{port}"
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
                self.shared_space_id = "shared-space"
                self.shared_space_name = data.get("space_name") or "Shared Space"
                self.is_host = False
                self.tabs = data.get("tabs", [])
                self.theme = data.get("theme")
                if data.get("cookies"):
                    self.cookies = data.get("cookies")
                    self._save_cookies(self.cookies)
                self.log_activity(peer_address, f"Connected to peer's space '{self.shared_space_name}'")

            # Merge into local spaces.json
            self._merge_into_spaces_file()
            self.notify_sse()

            # Start client poller to continuously sync host updates in real time
            self._start_client_poller(peer_address)

            # Notify the remote peer that we connected
            self.send_to_peer(peer_address, "peer_joined", {"peer_address": f"{self.tailscale_ip}:{self.port}"})
            self.bump_session_generation()
            return True, None
        except Exception as e:
            return False, f"Connection failed: {e}"

    def _start_client_poller(self, host_address: str):
        def loop():
            while True:
                time.sleep(0.35)
                with self.lock:
                    if self.is_host or not self.connected_peers or host_address not in self.connected_peers:
                        break
                try:
                    url = f"http://{host_address}/api/peer/sync"
                    req = urllib.request.Request(url, headers={"User-Agent": "LumenLiveShare/1.0"})
                    with urllib.request.urlopen(req, timeout=1.5) as resp:
                        if resp.status == 200:
                            data = json.loads(resp.read().decode("utf-8"))
                            new_tabs = data.get("tabs", [])
                            space_name = data.get("space_name")
                            new_theme = data.get("theme")
                            new_cookies = data.get("cookies")
                            diff = False
                            with self.lock:
                                if len(new_tabs) != len(self.tabs):
                                    diff = True
                                else:
                                    for nt, ot in zip(new_tabs, self.tabs):
                                        if (nt.get("id") != ot.get("id")
                                            or nt.get("url") != ot.get("url")
                                            or nt.get("title") != ot.get("title")
                                            or bool(nt.get("favorite")) != bool(ot.get("favorite"))
                                            or bool(nt.get("pinned")) != bool(ot.get("pinned"))
                                            or nt.get("folder") != ot.get("folder")):
                                            diff = True
                                            break
                                if new_theme and new_theme != self.theme:
                                    diff = True
                                    self.theme = new_theme
                                if new_cookies and new_cookies != self.cookies:
                                    diff = True
                                    self.cookies = new_cookies
                                    try:
                                        self._save_cookies(self.cookies)
                                    except Exception:
                                        pass
                                if diff:
                                    self.tabs = new_tabs
                                    if space_name:
                                        self.shared_space_name = space_name
                            if diff:
                                self._merge_into_spaces_file()
                                self.notify_sse()
                except Exception:
                    pass
        t = threading.Thread(target=loop, daemon=True)
        t.start()

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
            # Update peer_ips in spaces.json
            self._merge_into_spaces_file()
            # Reply with current full state
            self.send_to_peer(peer_addr, "sync_state", {
                "space_id": self.shared_space_id,
                "space_name": self.shared_space_name,
                "tabs": self.tabs,
                "theme": self.theme,
                "cookies": self.cookies,
            })

        elif event_type == "cursor":
            x = float(payload.get("x", 0.0))
            y = float(payload.get("y", 0.0))
            name = str(payload.get("name", "Peer"))
            typing = str(payload.get("typing", ""))
            try:
                if typing:
                    t_bytes = typing.encode("utf-8")[:65535]
                    packet = b"AT" + struct.pack("<ffH", x, y, len(t_bytes)) + t_bytes + name.encode("utf-8")
                else:
                    packet = b"AC" + struct.pack("<ff", x, y) + name.encode("utf-8")
                s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                s.sendto(packet, ("127.0.0.1", 7434))
                s.close()
            except Exception:
                pass

        elif event_type == "sync_cookies":
            cookies = payload.get("cookies", [])
            with self.lock:
                self.cookies = cookies
            self._save_cookies(cookies)
            self.log_activity(origin, f"Synchronized {len(cookies)} session cookies")

        elif event_type == "sync_state":
            if self.is_host:
                # Host is the master of the shared space; ignore sync_state from guest peers
                return
            with self.lock:
                self.shared_space_id = "shared-space"
                self.shared_space_name = payload.get("space_name") or self.shared_space_name
                self.tabs = payload.get("tabs", [])
                if payload.get("theme"):
                    self.theme = payload.get("theme")
                if payload.get("cookies"):
                    self.cookies = payload.get("cookies")
                    try:
                        self._save_cookies(self.cookies)
                    except Exception:
                        pass
                self.log_activity(origin, f"Synchronized {len(self.tabs)} tabs from peer")
            self._merge_into_spaces_file()

        elif event_type == "session_ended":
            with self.lock:
                self.shared_space_id = None
                self.shared_space_name = None
                self.tabs.clear()
                self.connected_peers.clear()
                self.log_activity(origin, "Session ended")
            self._remove_shared_space_from_file()
            self.bump_session_generation()
            self.notify_sse()

        elif event_type == "peer_left":
            peer_addr = payload.get("peer_address") or origin
            with self.lock:
                self.connected_peers.pop(peer_addr, None)
                self.log_activity(peer_addr, f"Peer {peer_addr} left the session")
                if not self.is_host and not self.connected_peers:
                    self.shared_space_id = None
                    self.shared_space_name = None
                    self.tabs.clear()
            if self.is_host:
                self._merge_into_spaces_file()
            elif not self.connected_peers:
                self._remove_shared_space_from_file()
            self.notify_sse()

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
                if self.is_host:
                    self.broadcast_to_peers("tab_opened", {"tab": tab})

        elif event_type == "tab_closed":
            tab_id = payload.get("tab_id")
            if tab_id:
                raw_id = tab_id.removeprefix("shared-")
                with self.lock:
                    self.tabs = [t for t in self.tabs if t.get("id") not in (tab_id, raw_id, f"shared-{tab_id}")]
                    self.log_activity(origin, f"Closed tab ({tab_id})")
                self._merge_into_spaces_file()
                if self.is_host:
                    self.broadcast_to_peers("tab_closed", {"tab_id": raw_id})

        elif event_type == "tab_navigated":
            tab_id = payload.get("tab_id")
            new_url = payload.get("url")
            new_title = payload.get("title", new_url)
            if tab_id and new_url:
                if is_auth_url(new_url):
                    return
                raw_id = tab_id.removeprefix("shared-")
                with self.lock:
                    for t in self.tabs:
                        if t.get("id") in (tab_id, raw_id, f"shared-{tab_id}"):
                            t["url"] = new_url
                            if not t.get("favorite") and not t.get("pinned"):
                                t["title"] = new_title
                    self.log_activity(origin, f"Navigated to '{new_title}'")
                self._merge_into_spaces_file()
                if self.is_host:
                    self.broadcast_to_peers("tab_navigated", {"tab_id": raw_id, "url": new_url, "title": new_title})

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
        raw_id = tab_id.removeprefix("shared-")
        with self.lock:
            self.tabs = [t for t in self.tabs if t.get("id") not in (tab_id, raw_id, f"shared-{tab_id}")]
            self.log_activity("You", f"Closed tab ({tab_id})")

        self._merge_into_spaces_file()
        self.broadcast_to_peers("tab_closed", {"tab_id": raw_id})
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

            # Target space ID: if we are a client, MUST be "shared-space", never a local space like "personal"
            target_id = "shared-space" if not self.is_host else self.shared_space_id

            target_space = None
            for sp in spaces:
                if sp.get("id") == target_id:
                    target_space = sp
                    break

            if target_space is None:
                if self.is_host:
                    return
                # Create a new shared space
                space_id = "shared-space"
                space_name = self.shared_space_name or "Shared Space"
                default_color = self.theme.get("color", 10135039) if self.theme else 10135039
                target_space = {
                    "id": space_id,
                    "name": f"[Shared] {space_name}",
                    "icon": "globe",
                    "color": default_color,
                    "theme_colors": self.theme.get("theme_colors", [default_color]) if self.theme else [10135039, 4907263],
                    "theme_appearance": self.theme.get("theme_appearance", 0) if self.theme else 0,
                    "theme_intensity": self.theme.get("theme_intensity", 0.5) if self.theme else 0.5,
                    "theme_grain": self.theme.get("theme_grain", 0.0) if self.theme else 0.0,
                    "tabs": [],
                    "folders": [],
                }
                spaces.append(target_space)

            # Update theme in target_space for guest
            if self.theme and not self.is_host:
                target_space["color"] = self.theme.get("color", target_space.get("color", 10135039))
                target_space["theme_colors"] = self.theme.get("theme_colors", target_space.get("theme_colors", [target_space.get("color", 10135039)]))
                target_space["theme_appearance"] = self.theme.get("theme_appearance", target_space.get("theme_appearance", 0))
                target_space["theme_intensity"] = self.theme.get("theme_intensity", target_space.get("theme_intensity", 0.5))
                target_space["theme_grain"] = self.theme.get("theme_grain", target_space.get("theme_grain", 0.0))

            # Update tabs in target_space
            existing_tabs = target_space.get("tabs", [])
            merged_tabs = []
            for st in self.tabs:
                st_url = st.get("url", "")
                is_fav = bool(st.get("favorite", False))
                is_pin = bool(st.get("pinned", False))
                pinned_url = st.get("pinned_url") or (st_url if (is_fav or is_pin) else None)
                pinned_title = st.get("pinned_title") or st.get("title")

                st_id = st.get("id", "")
                if not self.is_host and not st_id.startswith("shared-"):
                    local_id = f"shared-{st_id}"
                else:
                    local_id = st_id

                match = next((et for et in existing_tabs if et.get("id") in (local_id, st_id)), None)
                if match:
                    match["id"] = local_id
                    if not is_auth_url(st_url):
                        match["url"] = st_url
                        match["title"] = st.get("title", match.get("title", st_url))
                    match["pinned"] = is_pin
                    match["favorite"] = is_fav
                    match["pinned_url"] = pinned_url
                    if pinned_title:
                        match["pinned_title"] = pinned_title
                    if st.get("folder") is not None:
                        match["folder"] = st.get("folder")
                    merged_tabs.append(match)
                else:
                    merged_tabs.append({
                        "id": local_id,
                        "title": st.get("title", ""),
                        "url": st_url,
                        "pinned_url": pinned_url,
                        "pinned_title": pinned_title,
                        "favicon_path": None,
                        "pinned": is_pin,
                        "favorite": is_fav,
                        "folder": st.get("folder"),
                        "last_active": 0,
                        "scroll_y": 0.0,
                        "state": "Live",
                    })
            # Preserve internal plugin tabs (e.g. Live Share panel) that exist locally
            for et in existing_tabs:
                u = et.get("url", "")
                if u.startswith("anarc-plugin://") or u.startswith("anarc-internal://"):
                    if not any(mt.get("id") == et.get("id") for mt in merged_tabs):
                        merged_tabs.append(et)
            target_space["tabs"] = merged_tabs
            if (not target_space.get("active") or not any(t.get("id") == target_space.get("active") for t in merged_tabs)) and merged_tabs:
                target_space["active"] = merged_tabs[0]["id"]
            with self.lock:
                target_space["peer_ips"] = [p.split(":")[0] for p in self.connected_peers.keys()]

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
        """Background thread watching for local changes made inside Anarc."""
        while True:
            time.sleep(0.25)
            if not self.spaces_file.is_file() or not self.shared_space_id:
                continue

            with self.lock:
                if self.suppress_local_watch:
                    continue

            try:
                mtime = self.spaces_file.stat().st_mtime
                if mtime > self.last_spaces_mtime:
                    self.last_spaces_mtime = mtime
                    fresh_tabs, _, fresh_theme = self._read_space_tabs(self.shared_space_id)
                    if self.is_host:
                        diff = False
                        with self.lock:
                            if len(fresh_tabs) != len(self.tabs):
                                diff = True
                            else:
                                for ft, ot in zip(fresh_tabs, self.tabs):
                                    if (ft.get("id") != ot.get("id")
                                        or ft.get("url") != ot.get("url")
                                        or ft.get("pinned") != ot.get("pinned")
                                        or ft.get("favorite") != ot.get("favorite")
                                        or ft.get("folder") != ot.get("folder")):
                                        diff = True
                                        break
                            if fresh_theme and fresh_theme != self.theme:
                                diff = True
                                self.theme = fresh_theme
                            if diff:
                                self.tabs = fresh_tabs
                                self.log_activity("Anarc", f"Host space updated ({len(fresh_tabs)} tabs)")

                        if diff:
                            self.broadcast_to_peers("sync_state", {
                                "space_id": self.shared_space_id,
                                "space_name": self.shared_space_name,
                                "tabs": self.tabs,
                                "theme": self.theme,
                                "cookies": self.cookies,
                            })
                            self.notify_sse()
                    else:
                        # Client: detect discrete actions without overwriting the host's canonical state
                        # 1. New tabs opened locally by client
                        for ft in fresh_tabs:
                            ft_id = ft.get("id", "")
                            raw_id = ft_id.removeprefix("shared-")
                            if not any(ot.get("id") in (ft_id, raw_id) for ot in self.tabs):
                                ft_copy = dict(ft)
                                ft_copy["shared_by"] = f"{self.tailscale_ip}:{self.port}"
                                with self.lock:
                                    self.tabs.append(ft_copy)
                                self.broadcast_to_peers("tab_opened", {"tab": ft_copy})
                        # 2. Tabs closed locally by client
                        for ot in list(self.tabs):
                            ot_id = ot.get("id", "")
                            shared_id = f"shared-{ot_id}"
                            if not any(ft.get("id") in (ot_id, shared_id) for ft in fresh_tabs):
                                with self.lock:
                                    self.tabs = [t for t in self.tabs if t.get("id") != ot_id]
                                self.broadcast_to_peers("tab_closed", {"tab_id": ot_id})
                        # 3. Tabs navigated locally by client
                        for ft in fresh_tabs:
                            ft_id = ft.get("id", "")
                            raw_id = ft_id.removeprefix("shared-")
                            for ot in self.tabs:
                                if ot.get("id") in (ft_id, raw_id):
                                    if ft.get("url") != ot.get("url") and ft.get("url") != "about:blank":
                                        if not is_auth_url(ft.get("url", "")):
                                            ot["url"] = ft.get("url")
                                            ot["title"] = ft.get("title", ot.get("title"))
                                            self.broadcast_to_peers("tab_navigated", {
                                                "tab_id": ot.get("id"),
                                                "url": ft.get("url"),
                                                "title": ft.get("title", ft.get("url")),
                                            })
            except Exception:
                pass


# HTTP Request Handler
class LiveShareHandler(BaseHTTPRequestHandler):
    state: LiveShareState

    def do_OPTIONS(self):
        self.send_response(HTTPStatus.OK)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, X-Peer-Tailscale, Access-Control-Request-Private-Network")
        self.send_header("Access-Control-Allow-Private-Network", "true")
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

        elif path == "/api/tailscale/peers":
            self.send_json(get_tailscale_peers())

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
                "theme": status.get("theme"),
                "cookies": status.get("cookies", []),
            })

        elif path == "/api/cookies":
            with self.state.lock:
                self.send_json({"cookies": copy.deepcopy(self.state.cookies)})

        elif path == "/api/stream":
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Connection", "keep-alive")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Access-Control-Allow-Private-Network", "true")
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

        elif path == "/api/disconnect":
            ok = self.state.disconnect_session()
            self.send_json({"success": ok})

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

        elif path == "/api/cookies":
            cookies = body.get("cookies", [])
            with self.state.lock:
                self.state.cookies = cookies
                space_id = self.state.shared_space_id
                space_name = self.state.shared_space_name
                tabs = copy.deepcopy(self.state.tabs)
                theme = copy.deepcopy(self.state.theme)
            self.state._save_cookies(cookies)
            if self.state.is_host:
                # Push both the dedicated cookie event and a full state snapshot:
                # a peer that connected before the browser finished exporting the
                # account/session data still converges without waiting for a poll.
                self.state.broadcast_to_peers("sync_cookies", {"cookies": cookies})
                self.state.broadcast_to_peers("sync_state", {
                    "space_id": space_id,
                    "space_name": space_name,
                    "tabs": tabs,
                    "theme": theme,
                    "cookies": cookies,
                })
            self.send_json({"success": True, "count": len(cookies)})

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
        self.send_header("Access-Control-Allow-Private-Network", "true")
        self.end_headers()
        self.wfile.write(content)

    def read_body_json(self) -> Dict[str, Any]:
        length = int(self.headers.get("Content-Length", 0))
        if length > 0:
            try:
                raw = self.rfile.read(length)
                data = json.loads(raw.decode("utf-8"))
                if isinstance(data, dict) and data:
                    return data
            except Exception:
                pass
        # Fallback: check query parameter __body
        try:
            parsed = urllib.parse.urlparse(self.path)
            qs = urllib.parse.parse_qs(parsed.query)
            if "__body" in qs:
                data = json.loads(qs["__body"][0])
                if isinstance(data, dict):
                    return data
        except Exception:
            pass
        return {}

    def send_json(self, data: Any, status: HTTPStatus = HTTPStatus.OK):
        body = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Private-Network", "true")
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

    # Warm tailnet discovery off the request path so the synchronous CEF plugin
    # proxy never blocks on `tailscale status` (up to seconds per call).
    global _ts_refresh_started
    if not _ts_refresh_started:
        _ts_refresh_started = True
        threading.Thread(target=_ts_refresh_loop, daemon=True).start()

    # Auto-resume peer connection if shared-space is in spaces.json
    try:
        if spaces_file.is_file():
            s_data = json.loads(spaces_file.read_text(encoding="utf-8"))
            for sp in s_data.get("spaces", []):
                if sp.get("id") == "shared-space" and sp.get("peer_ips"):
                    peer_ip = sp["peer_ips"][0]
                    threading.Thread(target=lambda: state.connect_to_peer(peer_ip), daemon=True).start()
                    break
    except Exception:
        pass

    print(f"[Anarc Live Share] Running on http://{host}:{port}")
    print(f"[Tailscale IP] {state.tailscale_ip}")
    print(f"[Share Address] {state.tailscale_ip}:{port}")
    return server

def main():
    parser = argparse.ArgumentParser(description="Anarc Live Share Daemon")
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
