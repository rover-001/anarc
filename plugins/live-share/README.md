# Anarc Live Share (`com.anarc.live-share`)

**Anarc Live Share** is a real-time collaborative space-sharing plugin for the Anarc browser. It connects two (or more) users over a private **Tailscale** network via a small local daemon per machine, allowing them to share a browser space, co-browse, search the web together, and access each other's live tabs directly inside the browser.

---

## Features

- **Direct Peer-to-Peer over Tailscale**: A local daemon exchanges state with peers over the private Tailnet (IPv4 `100.x.y.z` or MagicDNS), protected by WireGuard. No public server or signaling service is involved.
- **Shared Account / Session Mirroring**: When a host starts sharing, the full session of that space's browser profile (account cookies and page logins, including Google) is exported and mirrored to the peer's `[Shared]` space on connect, so both browse under the same signed-in account.
- **Bi-directional Space Sync**:
  - User A hosts a shared space (e.g., "Research", "Project Alpha", "Personal").
  - User B connects using User A's Share Code.
  - The space is automatically synchronized across both browsers.
- **Live Collaborative Search**:
  - Either user can search the web (Google, DuckDuckGo, Bing) or enter a URL in the shared space.
  - The tab opens instantly for both users.
- **Tab Lifecycle Sync**:
  - Opening, navigating, or closing tabs in the shared space syncs in real-time.
  - Attribution tracking ("Shared by Peer" vs "You").
- **Audit & Activity Feed**:
  - Live activity log of all peer searches, navigations, and tab events.
- **Anarc Web Panel**:
  - Seamlessly embedded in Anarc via `anarc-plugin://com.anarc.live-share/`.

---

## Quick Start

### 1. Open Live Share in Anarc

- In **Settings -> Plugins**, click **Open** on the enabled **Anarc Live Share** card.
- Or simply open a new tab to `anarc-plugin://com.anarc.live-share/`.

Output:
```text
[Anarc Live Share] Running on http://0.0.0.0:7433
[Tailscale IP] 100.120.139.17
[Share Address] 100.120.139.17:7433
```

### 2. Share a Space (User A / Host)

1. Open the Live Share panel in Anarc (`lumen-plugin://com.lumen.live-share/index.html`) or visit `http://localhost:7433`.
2. Under **"1. Share a Space"**, select the space you want to share from the dropdown.
3. Click **"Share Space"**.
4. Click **"Copy Share Link"** (or give your partner your Tailscale IP: `100.120.139.17:7433`).

### 3. Connect to a Space (User B / Peer)

1. Open the Live Share panel in your Anarc browser.
2. Under **"2. Or Connect to Peer's Tailscale Space"**, enter User A's Tailscale address (e.g. `100.120.139.17:7433`).
3. Click **"Connect"**.
4. The shared space will automatically sync into your browser as `[Shared] <Space Name>`.

### 4. Co-Browsing & Searching

- **Search**: Type any query into the search box and click **"Launch"** (or press Enter). It opens a new tab in the shared space on both machines simultaneously.
- **Open in Anarc**: Click **"Open"** on any tab to view it in your local browser window.
- **Close**: Click **"×"** to close the tab for both peers.

---

## Architecture & Security

- **Network Boundary**: Binds to port `7433`. Because communication occurs over Tailscale, traffic is authenticated and encrypted by Tailscale's WireGuard mesh. Only machines on your Tailnet can connect.
- **Anarc Persistence**: Reads and atomically merges tabs into `~/.config/anarc/spaces.json` with conflict-free update locks and loop suppression.
- **Real-Time Push**: Uses Server-Sent Events (SSE) and HTTP event hooks for reactive updates. The embedded panel uses short adaptive polling because the `anarc-plugin://` proxy cannot stream SSE.
- **Fast Discovery**: Tailnet peer discovery is cached and refreshed in the background so `tailscale status` never blocks the browser's IO thread.
- **Session Handoff**: Session changes are published via `~/.config/anarc/live-share-session.json`; account cookies are mirrored through `~/.config/anarc/shared-cookies.json` into the peer's isolated `[Shared]` CEF profile.

---

## Automated Tests

Run the integration suite:
```sh
python3 plugins/live-share/test_live_share.py
```
Verifies two concurrent daemons, peer handshake, space sync, live search broadcasting, tab closure, and `spaces.json` convergence.
