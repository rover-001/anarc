"use strict";

const $ = (id) => document.getElementById(id);
const API_BASE = "anarc-plugin://com.anarc.live-share/api-proxy";
const chooseSpaceUrl = "anarc-plugin://com.anarc.live-share/__anarc_choose_space";
const switchSpaceUrl = "anarc-plugin://com.anarc.live-share/__anarc_switch_to_space?id=shared-space";

const searchUrls = {
  google: "https://www.google.com/search?q=",
  duckduckgo: "https://duckduckgo.com/?q=",
  bing: "https://www.bing.com/search?q=",
};

let mode = "host";
let selectedSpace = null;
let currentStatus = null;
let tailscaleInfo = null;

function setFeedback(message = "", isError = false) {
  const feedback = $("feedback");
  feedback.textContent = message;
  feedback.dataset.error = String(isError);
}

function setMode(nextMode) {
  if (mode === nextMode) return;
  mode = nextMode;
  $("host-mode").setAttribute("aria-pressed", String(mode === "host"));
  $("join-mode").setAttribute("aria-pressed", String(mode === "join"));
  $("host-setup").hidden = mode !== "host";
  $("join-setup").hidden = mode === "host";
  setFeedback();
}

async function apiFetch(path, options = {}, retried = false) {
  let url = `${API_BASE}${path}`;
  if (options.body && typeof options.body === "string") {
    const sep = url.includes("?") ? "&" : "?";
    url = `${url}${sep}__body=${encodeURIComponent(options.body)}`;
  }
  try {
    const res = await fetch(url, {
      headers: { "Content-Type": "application/json" },
      ...options,
    });
    if (res.ok) {
      return await res.json();
    }
  } catch (_) {}
  if (!retried) {
    try {
      await fetch("anarc-plugin://com.anarc.live-share/__anarc_start_service").catch(() => {});
      await new Promise((r) => setTimeout(r, 600));
      return await apiFetch(path, options, true);
    } catch (_) {}
  }
  console.warn("Live Share API error:", path);
  return null;
}

async function refreshTailscale() {
  const data = await apiFetch("/api/tailscale/peers");
  if (!data) return;
  tailscaleInfo = data;
  if (data.self) {
    $("my-ts-machine").textContent = data.self.hostname || "This Machine";
    $("my-ts-ip").textContent = data.self.ip ? `(${data.self.ip})` : "";
  }
  renderPeers(data.peers || []);
}

function renderPeers(peers) {
  const list = $("peers-list");
  const activePeers = (peers || []).filter((p) => p.online && p.has_browser === true);
  if (!activePeers.length) {
    list.innerHTML = `<p class="empty-peers">No active Anarc browser peers found on your Tailnet. Start sharing on the other machine, or connect by IP below.</p>`;
    return;
  }
  list.innerHTML = "";
  for (const peer of activePeers) {
    const card = document.createElement("div");
    card.className = "peer-card";

    const info = document.createElement("div");
    info.className = "peer-info";

    const name = document.createElement("strong");
    name.textContent = peer.hostname || peer.ip;

    const ip = document.createElement("span");
    ip.className = "ts-ip";
    ip.textContent = peer.ip;

    const badge = document.createElement("span");
    badge.className = "peer-badge";
    badge.dataset.online = "true";
    badge.textContent = "Browser Ready";

    info.append(name, ip, badge);

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "primary sm-btn";
    btn.textContent = "Connect";
    btn.addEventListener("click", () => connectToAddress(`${peer.ip}:7433`));

    card.append(info, btn);
    list.appendChild(card);
  }
}

async function connectToAddress(address) {
  let addr = address.trim();
  if (!addr) return;
  if (!addr.includes(":")) {
    addr = `${addr}:7433`;
  }
  setFeedback(`Connecting to ${addr}...`);
  const res = await apiFetch("/api/connect", {
    method: "POST",
    body: JSON.stringify({ peer_address: addr }),
  });
  if (res?.success) {
    setFeedback("Connected to peer! Space added to sidebar.");
    await refreshStatus();
    // Switch to the shared space in Anarc
    window.location.href = switchSpaceUrl;
  } else {
    setFeedback(res?.error || "Connection failed. Make sure the host has shared a space.", true);
  }
}

async function loadSpaces() {
  const data = await apiFetch("/api/spaces");
  const eligible = (data?.spaces || []).filter(
    (s) => s.id !== "shared-space" && !s.name?.startsWith("[Shared]")
  );
  if (eligible.length && !selectedSpace) {
    const first = eligible[0];
    selectedSpace = {
      id: first.id,
      name: first.name,
      tabs: first.tabs || [],
    };
    $("space-name").textContent = first.name;
    $("space-tab-count").hidden = false;
    const count = first.tabs?.length || 0;
    $("space-tab-count").textContent = `${count} web ${count === 1 ? "tab" : "tabs"}`;
  }
}

async function startSharing() {
  const spaceId = selectedSpace?.id || selectedSpace?.name || "personal";
  setFeedback("Starting Live Share on Tailscale...");
  let res = await apiFetch("/api/share", {
    method: "POST",
    body: JSON.stringify({ space_id: spaceId }),
  });
  if (!res?.success) {
    await fetch("anarc-plugin://com.anarc.live-share/__anarc_start_service").catch(() => {});
    await new Promise((r) => setTimeout(r, 800));
    res = await apiFetch("/api/share", {
      method: "POST",
      body: JSON.stringify({ space_id: spaceId }),
    }, true);
  }
  if (res?.success) {
    setFeedback(`Now sharing "${selectedSpace?.name || spaceId}" on Tailscale!`);
    await refreshStatus();
  } else {
    setFeedback("Failed to start sharing. Ensure Anarc Live Share service is running.", true);
  }
}

async function disconnect() {
  setFeedback("Ending session...");
  await apiFetch("/api/disconnect", { method: "POST" });
  selectedSpace = null;
  $("space-name").textContent = "No space selected";
  $("space-tab-count").hidden = true;
  $("start-share-btn").disabled = true;
  await refreshStatus();
  setFeedback("Session ended.");
}

async function refreshStatus() {
  const status = await apiFetch("/api/status");
  if (!status) return;
  currentStatus = status;

  const hasSharedSpace = Boolean(status.shared_space_id);
  const isConnected = hasSharedSpace;

  $("setup").hidden = isConnected;
  $("session").hidden = !isConnected;
  $("status").textContent = isConnected ? "Connected" : "Ready";
  $("status").dataset.state = isConnected ? "connected" : "ready";

  if (isConnected) {
    $("session-name").textContent = status.shared_space_name
      ? (status.is_host ? status.shared_space_name : `[Shared] ${status.shared_space_name}`)
      : "Shared space";
    
    const count = status.tabs?.length || 0;
    const noun = count === 1 ? "tab" : "tabs";
    const peerText = status.peers?.length
      ? `with ${status.peers.map((p) => p.address).join(", ")}`
      : "(Hosting on Tailscale)";
    $("session-peer-info").textContent = `${count} web ${noun} ${peerText}`;

    const banner = document.querySelector(".shared-banner");
    if (banner) {
      if (status.is_host) {
        banner.querySelector("strong").textContent = "You are hosting this space live!";
        banner.querySelector("p").textContent = "All peer tab operations and cursor movements will sync live in this space.";
        $("switch-space-btn").hidden = true;
      } else {
        banner.querySelector("strong").textContent = "This space is now live in your sidebar!";
        banner.querySelector("p").textContent = "Switch to it using the globe icon at the bottom of your sidebar to browse tabs in the main window. All tab actions are synchronized live.";
        $("switch-space-btn").hidden = false;
      }
    }
  }
}

function initSSE() {
  // SSE cannot tunnel through the anarc-plugin:// proxy; poll instead.
  setInterval(refreshStatus, 5000);
}

// Global paste fallback for inputs: prevent repeat triggers and double pasting
document.addEventListener("keydown", async (e) => {
  if ((e.ctrlKey || e.metaKey) && (e.key === "v" || e.key === "V")) {
    if (e.repeat) {
      e.preventDefault();
      return;
    }
    const active = document.activeElement;
    if (active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA")) {
      e.preventDefault();
      try {
        if (navigator.clipboard?.readText) {
          const text = await navigator.clipboard.readText();
          if (text) {
            const start = active.selectionStart ?? active.value.length;
            const end = active.selectionEnd ?? active.value.length;
            active.setRangeText(text, start, end, "end");
            active.dispatchEvent(new Event("input", { bubbles: true }));
            active.dispatchEvent(new Event("change", { bubbles: true }));
          }
        }
      } catch (_) {}
    }
  }
});

// Event listeners
$("host-mode").addEventListener("click", () => setMode("host"));
$("join-mode").addEventListener("click", () => setMode("join"));
$("refresh-peers").addEventListener("click", refreshTailscale);

$("choose-space").addEventListener("click", () => {
  window.location.href = chooseSpaceUrl;
});

window.addEventListener("anarc:space-selected", ({ detail }) => {
  if (!detail || typeof detail.name !== "string") return;
  selectedSpace = detail;
  $("space-name").textContent = detail.name;
  $("space-tab-count").hidden = false;
  const count = detail.tabs?.length || 0;
  $("space-tab-count").textContent = `${count} web ${count === 1 ? "tab" : "tabs"}`;
  $("start-share-btn").disabled = false;
  setFeedback(`Selected "${detail.name}". Click "Start sharing" to begin.`);
});

$("start-share-btn").addEventListener("click", startSharing);

$("direct-connect-form").addEventListener("submit", (e) => {
  e.preventDefault();
  connectToAddress($("peer-address-input").value);
});

$("paste-address")?.addEventListener("click", async () => {
  try {
    if (navigator.clipboard?.readText) {
      const text = await navigator.clipboard.readText();
      if (text) $("peer-address-input").value = text.trim();
    }
  } catch (_) {}
});

$("disconnect").addEventListener("click", disconnect);
$("switch-space-btn").addEventListener("click", () => {
  const targetId = currentStatus?.is_host ? (currentStatus.shared_space_id || "personal") : "shared-space";
  window.location.href = `anarc-plugin://com.anarc.live-share/__anarc_switch_to_space?id=${encodeURIComponent(targetId)}`;
});

// Initialize
fetch("anarc-plugin://com.anarc.live-share/__anarc_start_service").catch(() => {});
loadSpaces();
refreshTailscale();
refreshStatus();
initSSE();
setInterval(refreshTailscale, 10000);
