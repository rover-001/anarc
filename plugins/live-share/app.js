"use strict";

const $ = (id) => document.getElementById(id);
const chooseSpaceUrl = "anarc-plugin://com.anarc.live-share/__anarc_choose_space";
const searchUrls = {
  google: "https://www.google.com/search?q=",
  duckduckgo: "https://duckduckgo.com/?q=",
  bing: "https://www.bing.com/search?q=",
};

let mode = "host";
let phase = "ready";
let selectedSpace = null;
let sharedSpaceName = "";
let tabs = [];
let peer = null;
let channel = null;

function setFeedback(message = "", isError = false) {
  const feedback = $("feedback");
  feedback.textContent = message;
  feedback.dataset.error = String(isError);
}

function tabCount(count) {
  return `${count} web ${count === 1 ? "tab" : "tabs"}`;
}

function isConnected() {
  return channel?.readyState === "open";
}

function render() {
  const isHost = mode === "host";
  const connected = isConnected();
  const processing = phase === "working" || phase === "connecting";
  const pickerAvailable = document.documentElement.dataset.anarcSpacePicker === "available";
  const statusText = {
    ready: "Ready",
    working: "Preparing code",
    exchange: "Waiting for partner",
    connecting: "Connecting",
    connected: "Connected",
  };
  $("status").textContent = statusText[phase];
  $("status").dataset.state = phase;
  $("setup").hidden = connected;
  $("session").hidden = !connected;
  $("host-mode").setAttribute("aria-pressed", String(isHost));
  $("join-mode").setAttribute("aria-pressed", String(!isHost));
  $("host-setup").hidden = !isHost;
  $("join-setup").hidden = isHost;
  $("choose-space").disabled = processing || !pickerAvailable;
  $("picker-unavailable").hidden = pickerAvailable;
  $("choose-space").textContent = selectedSpace ? "Change space" : "Choose space";
  $("space-name").textContent = selectedSpace?.name ?? "No space selected";
  $("space-tab-count").hidden = !selectedSpace;
  $("space-tab-count").textContent = selectedSpace ? tabCount(selectedSpace.tabs.length) : "";
  $("create-invite-btn").disabled = !selectedSpace || processing;
  $("create-invite-btn").hidden = phase === "exchange" || phase === "connecting";
  $("host-exchange").hidden = !$("share-code").value;
  $("join-exchange").hidden = !$("response-code").value;
  for (const form of [$("join-form"), $("confirm-form")]) {
    for (const control of form.elements) control.disabled = processing;
  }
  if (connected) {
    $("session-name").textContent = sharedSpaceName || "Shared space";
    $("session-tab-count").textContent = tabCount(tabs.length);
    renderTabs();
  }
}

function webUrl(value) {
  if (typeof value !== "string") return null;
  try {
    const url = new URL(value);
    return url.protocol === "https:" || url.protocol === "http:" ? url.href : null;
  } catch {
    return null;
  }
}

function normalizeTab(value) {
  const url = webUrl(value?.url);
  if (!url || typeof value.id !== "string" || !value.id) return null;
  return {
    id: value.id,
    title: typeof value.title === "string" && value.title.trim() ? value.title : url,
    url,
  };
}

function normalizeTabs(input) {
  if (!Array.isArray(input)) return [];
  const byId = new Map();
  for (const value of input) {
    const tab = normalizeTab(value);
    if (tab) byId.set(tab.id, tab);
  }
  return Array.from(byId.values());
}

function openUrl(url) {
  window.open(url, "_blank", "noopener,noreferrer");
}

function renderTabs() {
  $("empty-tabs").hidden = tabs.length > 0;
  const rows = document.createDocumentFragment();
  for (const tab of tabs) {
    const row = document.createElement("li");
    row.className = "tab-row";
    const details = document.createElement("div");
    details.className = "tab-details";
    const title = document.createElement("strong");
    title.textContent = tab.title;
    const url = document.createElement("p");
    url.className = "tab-url";
    url.textContent = tab.url;
    details.append(title, url);
    const actions = document.createElement("div");
    actions.className = "tab-actions";
    const open = document.createElement("button");
    open.type = "button";
    open.textContent = "Open";
    open.setAttribute("aria-label", `Open ${tab.title}`);
    open.addEventListener("click", () => openUrl(tab.url));
    const remove = document.createElement("button");
    remove.type = "button";
    remove.textContent = "Remove";
    remove.setAttribute("aria-label", `Remove ${tab.title} from shared tabs`);
    remove.addEventListener("click", () => {
      send({type: "close_tab", tab_id: tab.id});
      tabs = tabs.filter(({id}) => id !== tab.id);
      render();
      $("launch-input").focus();
    });
    actions.append(open, remove);
    row.append(details, actions);
    rows.append(row);
  }
  $("shared-tabs").replaceChildren(rows);
}

function resetPeer() {
  const oldChannel = channel;
  const oldPeer = peer;
  channel = null;
  peer = null;
  oldChannel?.close();
  oldPeer?.close();
  phase = "ready";
  tabs = [];
  sharedSpaceName = "";
  for (const id of ["share-code", "response-code", "response-input", "share-input", "launch-input"]) {
    $(id).value = "";
  }
}

function setMode(nextMode) {
  if (mode === nextMode) return;
  resetPeer();
  mode = nextMode;
  setFeedback();
  render();
}

function createPeerConnection() {
  const connection = new RTCPeerConnection({
    iceServers: [{urls: "stun:stun.l.google.com:19302"}],
  });
  peer = connection;
  connection.addEventListener("connectionstatechange", () => {
    if (peer !== connection) return;
    if (connection.connectionState === "failed") {
      resetPeer();
      setFeedback("The connection failed. Generate a new code to try again.", true);
      render();
    }
  });
  return connection;
}

function attachChannel(nextChannel) {
  channel = nextChannel;
  nextChannel.addEventListener("open", () => {
    if (channel !== nextChannel) return;
    phase = "connected";
    setFeedback();
    if (mode === "host") {
      send({type: "sync_state", space_name: sharedSpaceName, tabs});
    }
    render();
    $("launch-input").focus();
  });
  nextChannel.addEventListener("message", ({data}) => {
    if (channel !== nextChannel || typeof data !== "string") return;
    let message;
    try {
      message = JSON.parse(data);
    } catch {
      return;
    }
    switch (message?.type) {
      case "sync_state":
        if (mode === "join") {
          sharedSpaceName = typeof message.space_name === "string" ? message.space_name : "";
          tabs = normalizeTabs(message.tabs);
        }
        break;
      case "open_tab": {
        const tab = normalizeTab(message.tab);
        if (tab && !tabs.some(({id}) => id === tab.id)) tabs.push(tab);
        break;
      }
      case "close_tab":
        tabs = tabs.filter(({id}) => id !== message.tab_id);
        break;
      default:
        return;
    }
    render();
  });
  nextChannel.addEventListener("close", () => {
    if (channel !== nextChannel) return;
    resetPeer();
    setFeedback("Your partner disconnected.");
    render();
  });
}

function send(message) {
  if (isConnected()) {
    try {
      channel.send(JSON.stringify(message));
    } catch {
      setFeedback("The update could not be sent to your partner.", true);
    }
  }
}

function waitForIce(connection) {
  if (connection.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const done = () => {
      clearTimeout(timeout);
      connection.removeEventListener("icegatheringstatechange", check);
      resolve();
    };
    const check = () => {
      if (connection.iceGatheringState === "complete") done();
    };
    const timeout = setTimeout(done, 2500);
    connection.addEventListener("icegatheringstatechange", check);
  });
}

function decodeCode(value, expectedType) {
  const code = JSON.parse(atob(value.trim()));
  if (code?.type !== expectedType || typeof code.sdp !== "string") {
    throw new Error("Unexpected code type");
  }
  return code;
}

async function createShareCode() {
  if (!selectedSpace || phase !== "ready") return;
  phase = "working";
  sharedSpaceName = selectedSpace.name;
  tabs = selectedSpace.tabs.slice();
  setFeedback();
  render();
  let connection;
  try {
    connection = createPeerConnection();
    attachChannel(connection.createDataChannel("anarc-live-share"));
    await connection.setLocalDescription(await connection.createOffer());
    await waitForIce(connection);
    if (peer !== connection) return;
    $("share-code").value = btoa(JSON.stringify(connection.localDescription));
    phase = "exchange";
    render();
    $("copy-share").focus();
  } catch {
    if (connection && peer !== connection) return;
    resetPeer();
    setFeedback("The share code could not be generated. Try again.", true);
    render();
  }
}

async function joinSession(event) {
  event.preventDefault();
  const input = $("share-input");
  let offer;
  try {
    offer = decodeCode(input.value, "offer");
  } catch {
    setFeedback("Paste a valid share code from your host.", true);
    input.focus();
    return;
  }
  const shareCode = input.value;
  resetPeer();
  input.value = shareCode;
  phase = "working";
  setFeedback();
  render();
  let connection;
  try {
    connection = createPeerConnection();
    connection.addEventListener("datachannel", ({channel: incoming}) => {
      if (peer === connection) attachChannel(incoming);
    });
    await connection.setRemoteDescription(offer);
    await connection.setLocalDescription(await connection.createAnswer());
    await waitForIce(connection);
    if (peer !== connection) return;
    $("response-code").value = btoa(JSON.stringify(connection.localDescription));
    phase = "exchange";
    render();
    $("copy-response").focus();
  } catch {
    if (connection && peer !== connection) return;
    resetPeer();
    input.value = shareCode;
    setFeedback("The response code could not be generated. Check the share code and try again.", true);
    render();
    input.focus();
  }
}

async function confirmSession(event) {
  event.preventDefault();
  const connection = peer;
  if (!connection || phase !== "exchange") return;
  const input = $("response-input");
  try {
    const answer = decodeCode(input.value, "answer");
    phase = "connecting";
    setFeedback();
    render();
    await connection.setRemoteDescription(answer);
  } catch {
    if (peer !== connection) return;
    phase = "exchange";
    setFeedback("Paste the response code your partner generated for this share code.", true);
    render();
    input.focus();
  }
}

async function copyCode(id) {
  const input = $(id);
  try {
    await navigator.clipboard.writeText(input.value);
    setFeedback("Code copied.");
  } catch {
    input.focus();
    input.select();
    setFeedback("Code selected. Use your copy shortcut to copy it.");
  }
}

function launchTab(event) {
  event.preventDefault();
  const input = $("launch-input");
  const query = input.value.trim();
  if (!query || !isConnected()) return;
  const url = webUrl(query) ?? searchUrls[$("search-engine").value] + encodeURIComponent(query);
  const tab = {id: crypto.randomUUID(), title: query, url};
  tabs.push(tab);
  send({type: "open_tab", tab});
  input.value = "";
  render();
  openUrl(url);
}

$("host-mode").addEventListener("click", () => setMode("host"));
$("join-mode").addEventListener("click", () => setMode("join"));
$("choose-space").addEventListener("click", () => { window.location.href = chooseSpaceUrl; });
window.addEventListener("anarc:space-picker-ready", render);
window.addEventListener("anarc:space-selected", ({detail}) => {
  if (typeof detail?.name !== "string" || !Array.isArray(detail.tabs)) return;
  resetPeer();
  selectedSpace = {name: detail.name, tabs: normalizeTabs(detail.tabs)};
  setFeedback();
  render();
  if (mode === "host") $("create-invite-btn").focus();
});
$("create-invite-btn").addEventListener("click", createShareCode);
$("join-form").addEventListener("submit", joinSession);
$("confirm-form").addEventListener("submit", confirmSession);
$("copy-share").addEventListener("click", () => copyCode("share-code"));
$("copy-response").addEventListener("click", () => copyCode("response-code"));
$("launch-form").addEventListener("submit", launchTab);
$("disconnect").addEventListener("click", () => {
  resetPeer();
  setFeedback();
  render();
  $(mode === "host" ? "host-mode" : "join-mode").focus();
});
render();
