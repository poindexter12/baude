// baude PWA — triage-first client for bauded.
// Hash routes: '#/' session list, '#/s/<id>' chat view.
"use strict";

const $app = document.getElementById("app");
const $toast = document.getElementById("toast");

const state = {
  sessions: [],
  fetchedAt: 0, // Date.now() of the last /sessions fetch (waiting timers tick from here)
  sid: null,
  msgs: [], // ordered chat messages
  seen: new Set(), // message uuids
  es: null, // EventSource
  esBuffer: null, // events received before history loaded
  online: true,
  pollTimer: null,
  tickTimer: null,
  screenOpen: false,
  screenText: "",
  screenTimer: null,
  queue: [], // messages typed while busy, not yet picked up
};

// ---- helpers ----

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[c]);
}

function humanMs(ms) {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  return `${Math.floor(s / 3600)}h${Math.floor((s % 3600) / 60)}m`;
}

function shortModel(m) {
  return m && !m.startsWith("<") ? m.replace(/^claude-/, "") : null;
}

function shortMode(m) {
  return { bypassPermissions: "bypass", acceptEdits: "accept", plan: "plan", default: "ask" }[m] || null;
}

function timeOf(ts) {
  if (!ts) return "";
  const d = new Date(ts);
  return isNaN(d) ? "" : d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

let toastTimer = null;
function toast(text) {
  $toast.textContent = text;
  $toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { $toast.hidden = true; }, 2600);
}

async function api(path, opts = {}) {
  const res = await fetch(path, {
    headers: opts.body ? { "content-type": "application/json" } : {},
    ...opts,
  });
  state.online = true;
  if (!res.ok) {
    const text = (await res.text().catch(() => "")) || res.statusText;
    throw Object.assign(new Error(text), { status: res.status });
  }
  const ct = res.headers.get("content-type") || "";
  return ct.includes("json") ? res.json() : null;
}

function session(id) {
  return state.sessions.find((s) => s.id === id);
}

// ---- routing ----

function route() {
  const m = location.hash.match(/^#\/s\/(\d+)$/);
  const sid = m ? Number(m[1]) : null;
  if (sid !== state.sid) {
    closeStream();
    state.sid = sid;
    state.msgs = [];
    state.seen = new Set();
    state.screenOpen = false;
    state.screenText = "";
    state.queue = [];
    clearInterval(state.screenTimer);
    if (sid !== null) openChat(sid);
  }
  render();
  refresh();
}
window.addEventListener("hashchange", route);

// ---- data ----

async function refresh() {
  try {
    state.sessions = await api("/sessions");
    state.fetchedAt = Date.now();
    if (state.sid !== null) {
      state.queue = await api(`/sessions/${state.sid}/queue`).catch(() => []);
    }
  } catch {
    state.online = false;
  }
  render();
}

function statusIcon(s) {
  if (s.status === "waiting") return '<span class="dot waiting">●</span>';
  if (s.status === "busy") return '<span class="dot busy">◐</span>';
  return '<span class="dot exited">✗</span>';
}

function waitingFor(s) {
  if (s.status !== "waiting" || s.waiting_for_ms == null) return "";
  return humanMs(s.waiting_for_ms + (Date.now() - state.fetchedAt));
}

// ---- chat stream ----

function closeStream() {
  if (state.es) state.es.close();
  state.es = null;
  state.esBuffer = null;
}

function addMsg(m) {
  if (state.seen.has(m.uuid)) return false;
  state.seen.add(m.uuid);
  state.msgs.push(m);
  return true;
}

async function openChat(sid) {
  // Connect the stream first and buffer, then load history, then merge —
  // nothing can fall between history and the live tail.
  state.esBuffer = [];
  const es = new EventSource(`/sessions/${sid}/stream`);
  state.es = es;
  es.onmessage = (ev) => {
    if (state.sid !== sid) return;
    const m = JSON.parse(ev.data);
    if (state.esBuffer) state.esBuffer.push(m);
    else if (addMsg(m)) render();
  };
  es.onerror = () => {
    // EventSource auto-reconnects; a closed session ends the stream silently.
  };
  try {
    const history = await api(`/sessions/${sid}/messages`);
    if (state.sid !== sid) return;
    for (const m of history) addMsg(m);
    for (const m of state.esBuffer || []) addMsg(m);
    state.esBuffer = null;
    render();
    scrollChat(true);
  } catch (e) {
    if (e.status === 404) { location.hash = "#/"; toast("session is gone"); }
  }
}

function scrollChat(force) {
  const el = document.getElementById("chat");
  if (!el) return;
  const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 160;
  if (force || nearBottom) el.scrollTop = el.scrollHeight;
}

// ---- actions ----

async function sendMessage() {
  const ta = document.getElementById("input");
  const text = ta.value.trim();
  if (!text || state.sid === null) return;
  const btn = document.querySelector("#composer .send");
  btn.disabled = true;
  try {
    await api(`/sessions/${state.sid}/messages`, { method: "POST", body: JSON.stringify({ text }) });
    ta.value = "";
    ta.style.height = "auto";
    const s = session(state.sid);
    if (s && s.status === "busy") toast("queued — claude is busy");
  } catch (e) {
    toast(e.status === 409 ? e.message : `send failed: ${e.message}`);
  }
  btn.disabled = false;
  ta.focus();
}

async function interrupt() {
  if (state.sid === null) return;
  try {
    await api(`/sessions/${state.sid}/interrupt`, { method: "POST" });
    toast("sent esc");
  } catch (e) {
    toast(`interrupt failed: ${e.message}`);
  }
}

// ---- terminal peek ----

async function pollScreen() {
  if (!state.screenOpen || state.sid === null) return;
  try {
    const shot = await api(`/sessions/${state.sid}/screen`);
    if (shot.text !== state.screenText) {
      state.screenText = shot.text;
      const pre = document.getElementById("screentext");
      if (pre) pre.textContent = shot.text;
    }
  } catch { /* session gone; list poll will redirect */ }
}

function toggleScreen() {
  state.screenOpen = !state.screenOpen;
  clearInterval(state.screenTimer);
  if (state.screenOpen) {
    pollScreen();
    state.screenTimer = setInterval(pollScreen, 2000);
  }
  render();
}

async function sendKey(key) {
  if (state.sid === null) return;
  try {
    await api(`/sessions/${state.sid}/keys`, { method: "POST", body: JSON.stringify({ keys: [key] }) });
    setTimeout(pollScreen, 300);
  } catch (e) {
    toast(`key failed: ${e.message}`);
  }
}

async function restartSession() {
  if (state.sid === null) return;
  try {
    await api(`/sessions/${state.sid}/restart`, { method: "POST" });
    toast("restarting claude…");
    refresh();
  } catch (e) {
    toast(`restart failed: ${e.message}`);
  }
}

async function deleteSession() {
  const s = session(state.sid);
  if (!s || !confirm(`Kill session "${s.name}"?`)) return;
  try {
    await api(`/sessions/${state.sid}`, { method: "DELETE" });
    location.hash = "#/";
    toast("session killed");
  } catch (e) {
    toast(`delete failed: ${e.message}`);
  }
}

async function createSession(ev) {
  ev.preventDefault();
  const repo = document.getElementById("f-repo").value.trim();
  const worktree = document.getElementById("f-branch").value.trim();
  const name = document.getElementById("f-name").value.trim();
  if (!repo) return;
  try {
    const s = await api("/sessions", {
      method: "POST",
      body: JSON.stringify({ repo, worktree: worktree || null, name: name || null }),
    });
    closeModal();
    location.hash = `#/s/${s.id}`;
  } catch (e) {
    toast(`create failed: ${e.message}`);
  }
}

function openModal() {
  document.getElementById("modal")?.remove();
  const div = document.createElement("div");
  div.id = "modal";
  div.innerHTML = `
    <form class="box" id="newform">
      <h2>New session</h2>
      <label for="f-repo">repo path (on the server)</label>
      <input id="f-repo" placeholder="/repos/myproject" autocapitalize="off" autocorrect="off">
      <label for="f-branch">worktree branch — optional</label>
      <input id="f-branch" placeholder="fix-thing" autocapitalize="off" autocorrect="off">
      <label for="f-name">name — optional</label>
      <input id="f-name" autocapitalize="off" autocorrect="off">
      <div class="actions">
        <button type="button" class="ghost" id="cancelnew">cancel</button>
        <button type="submit" class="primary">start</button>
      </div>
    </form>`;
  document.body.appendChild(div);
  div.addEventListener("click", (e) => { if (e.target === div) closeModal(); });
  document.getElementById("cancelnew").onclick = closeModal;
  document.getElementById("newform").onsubmit = createSession;
  document.getElementById("f-repo").focus();
}

function closeModal() {
  document.getElementById("modal")?.remove();
}

// ---- rendering ----

function render() {
  if (state.sid === null) renderList();
  else renderChat();
}

function renderList() {
  const conn = state.online ? "" : '<div id="conn">disconnected — retrying…</div>';
  const rows = state.sessions.map((s) => {
    const meta = [
      shortModel(s.model),
      s.context_used_pct != null ? `${s.context_used_pct}%` : null,
      shortMode(s.permission_mode),
      s.branch ? `⎇ ${esc(s.branch)}` : null,
      s.session_cost_usd != null ? `$${s.session_cost_usd.toFixed(2)}` : null,
    ].filter(Boolean).join(" · ");
    return `
      <button class="row" data-sid="${s.id}">
        <div class="line1">${statusIcon(s)}<span class="name">${esc(s.name)}</span>
          <span class="wait">${waitingFor(s)}</span></div>
        ${s.title ? `<div class="title">${esc(s.title)}</div>` : ""}
        ${meta ? `<div class="line2">${meta}</div>` : ""}
      </button>`;
  }).join("");
  const waiting = state.sessions.filter((s) => s.status === "waiting").length;
  $app.innerHTML = `
    <header>
      <h1>baude${waiting ? `<span class="sub">● ${waiting} waiting</span>` : ""}</h1>
      <button class="iconbtn" id="newbtn" aria-label="new session">＋</button>
    </header>
    ${conn}
    <div id="list">${rows || '<div class="empty">no sessions — tap ＋ to start one</div>'}</div>`;
  document.getElementById("newbtn").onclick = openModal;
  for (const el of document.querySelectorAll(".row")) {
    el.onclick = () => { location.hash = `#/s/${el.dataset.sid}`; };
  }
}

function msgHtml(m) {
  if (m.kind === "tool_use") {
    return `<div class="msg tool"><div class="bubble">⚙ ${esc(m.text)}</div></div>`;
  }
  const who = m.role === "user" ? "user" : "assistant";
  return `
    <div class="msg ${who}">
      <div class="bubble">${esc(m.text)}</div>
      <div class="meta">${timeOf(m.timestamp)}</div>
    </div>`;
}

function renderChat() {
  const s = session(state.sid);
  const name = s ? s.name : `#${state.sid}`;
  const sub = s
    ? (s.status === "waiting" ? `waiting ${waitingFor(s)}` : s.status)
    : "";
  const typing = s && s.status === "busy"
    ? '<div id="typing"><span class="dot busy">◐</span> claude is working…</div>' : "";
  const conn = state.online ? "" : '<div id="conn">disconnected — retrying…</div>';
  const chatEl = document.getElementById("chat");
  const atBottom = chatEl ? chatEl.scrollHeight - chatEl.scrollTop - chatEl.clientHeight < 160 : true;

  const KEYS = [
    ["↑", "up"], ["↓", "down"], ["←", "left"], ["→", "right"],
    ["⇥", "tab"], ["⇧⇥", "shift+tab"], ["↵", "enter"], ["esc", "esc"],
  ];
  const screenDrawer = !state.screenOpen ? "" : `
    <div id="screen">
      <pre id="screentext">${esc(state.screenText)}</pre>
      <div class="keys">
        ${KEYS.map(([label, k]) => `<button class="key" data-key="${k}">${label}</button>`).join("")}
      </div>
    </div>`;

  $app.innerHTML = `
    <header>
      <button class="iconbtn" id="backbtn" aria-label="back">‹</button>
      <h1>${esc(name)}<span class="sub">${esc(sub)}</span></h1>
      <button class="iconbtn${state.screenOpen ? " active" : ""}" id="screenbtn" title="terminal peek">▦</button>
      <button class="iconbtn" id="escbtn" title="interrupt (esc)">⎋</button>
      <button class="iconbtn danger" id="killbtn" title="kill session">✕</button>
    </header>
    ${conn}
    <div id="chat">${state.msgs.map(msgHtml).join("")}${state.queue.map((q) => `
      <div class="msg user queued"><div class="bubble">${esc(q)}</div>
        <div class="meta">queued</div></div>`).join("")}</div>
    ${typing}
    ${screenDrawer}
    ${s && s.status === "exited" ? `
    <form id="composer">
      <button type="button" class="send" id="restartbtn" style="flex:1">claude exited — restart</button>
    </form>` : `
    <form id="composer">
      <textarea id="input" rows="1" placeholder="message claude…" enterkeyhint="send"></textarea>
      <button type="submit" class="send">send</button>
    </form>`}`;

  document.getElementById("backbtn").onclick = () => { location.hash = "#/"; };
  document.getElementById("screenbtn").onclick = toggleScreen;
  document.getElementById("escbtn").onclick = interrupt;
  document.getElementById("killbtn").onclick = deleteSession;
  for (const el of document.querySelectorAll("#screen .key")) {
    el.onclick = () => sendKey(el.dataset.key);
  }
  document.getElementById("composer").onsubmit = (e) => { e.preventDefault(); sendMessage(); };
  const restartBtn = document.getElementById("restartbtn");
  if (restartBtn) restartBtn.onclick = restartSession;
  const ta = document.getElementById("input");
  if (ta) {
    ta.oninput = () => {
      ta.style.height = "auto";
      ta.style.height = `${Math.min(ta.scrollHeight, 140)}px`;
    };
    ta.onkeydown = (e) => {
      if (e.key === "Enter" && !e.shiftKey && !isTouch()) {
        e.preventDefault();
        sendMessage();
      }
    };
  }
  if (atBottom) scrollChat(true);
}

function isTouch() {
  return matchMedia("(pointer: coarse)").matches;
}

// renderChat rebuilds the whole view; preserve composer text across renders.
const _renderChat = renderChat;
renderChat = function () {
  const ta = document.getElementById("input");
  const saved = ta ? { text: ta.value, focus: document.activeElement === ta } : null;
  _renderChat();
  const nta = document.getElementById("input");
  if (saved && nta) {
    nta.value = saved.text;
    nta.dispatchEvent(new Event("input"));
    if (saved.focus) nta.focus();
  }
};

// ---- timers ----

function startTimers() {
  stopTimers();
  state.pollTimer = setInterval(() => {
    if (!document.hidden) refresh();
  }, 3000);
  state.tickTimer = setInterval(() => {
    // tick the waiting timers without refetching
    if (!document.hidden && state.sessions.some((s) => s.status === "waiting")) render();
  }, 1000);
}

function stopTimers() {
  clearInterval(state.pollTimer);
  clearInterval(state.tickTimer);
}

document.addEventListener("visibilitychange", () => {
  if (!document.hidden) {
    refresh();
    if (state.sid !== null && !state.es) openChat(state.sid);
  }
});

// ---- boot ----

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js").catch(() => {});
}
startTimers();
route();
