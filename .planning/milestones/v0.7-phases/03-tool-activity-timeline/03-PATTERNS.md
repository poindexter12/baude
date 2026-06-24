# Phase 3: Tool-Activity Timeline - Pattern Map

**Mapped:** 2026-06-15
**Files analyzed:** 10 (1 new type, 9 modified)
**Analogs found:** 10 / 10 (every change mirrors existing in-repo code)

## File Classification

| Modified/New File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/meta.rs` (`HookEvent` struct) | model | transform | `Usage`/`GsdState` serde-adjacent structs (`meta.rs:38-78`) + `SessionInfo` serde row | role-match |
| `baude-core/src/meta.rs` (ring append in `read_event_tail`) | model | event-driven | existing `read_event_tail` match arms (`meta.rs:393-416`) | exact |
| `bauded/src/manager.rs` (`activity()` + `event_path()` accessors) | service | CRUD/read | `transcript_path()` (`manager.rs:453-456`) | exact |
| `bauded/src/manager.rs` (`SessionInfo.activity` field + populate) | model | transform | `SessionInfo` + `session_info()` (`manager.rs:44-69`, `609-632`) | exact |
| `bauded/src/api.rs` (`get_activity` + route) | controller | request-response | `get_messages` + router (`api.rs:147-162`, `19-39`) | exact |
| `bauded/src/api.rs` (`activity_stream` + route) | controller | streaming (SSE) | `stream()` (`api.rs:362-397`) | exact |
| event-line `Tail` (new, in `transcript.rs` or new module) | utility | streaming/file-I/O | `Tail` (`transcript.rs:197-235`) + `read_event_tail` parse (`meta.rs:393-416`) | role-match (parse target differs — Pitfall 1) |
| `bauded/src/notify.rs` (test constructor) | test | — | `info()` `#[cfg(test)]` constructor (`notify.rs:101-123`) | exact |
| `bauded/web/app.js` (`openActivity` + strip render + toggle) | component | streaming (SSE) | `openChat`+`closeStream` (`app.js:130-169`), `renderChat` (`455-501`), `toggleScreen` (`289-297`) | exact |
| `bauded/web/style.css` (`.activity-strip`) | config/style | — | existing `#screen` drawer styles | role-match |
| `bauded/web/sw.js` (CACHE bump) | config | — | `CACHE = "baude-v2"` (`sw.js:4`) | exact |
| `baude/src/app.rs` (`Modal::Activity` + `v` dispatch + dismiss) | store/controller | event-driven | `Modal` enum (`app.rs:98-118`), `i` dispatch (`815-819`), dismiss arm (`835-837`) | exact |
| `baude/src/ui.rs` (`Modal::Activity` render arm) | component | request-response | `Modal::Info` local/remote branch (`ui.rs:779-938`) | exact |
| `baude/src/remote.rs` (`RemoteInfo.activity` field) | model | transform | `RemoteInfo` (`remote.rs:19-40`) | exact |

---

## Pattern Assignments

### `baude-core/src/meta.rs` — `HookEvent` struct (model, transform)

**Analog:** the typed serde rows precedent. `meta.rs` has no serde imports today (it uses untyped `Value`); add `use serde::{Deserialize, Serialize};`. `Usage`/`GsdState` at `meta.rs:38-78` show the `#[derive(Default, Clone)]` struct convention; `SessionInfo` (`manager.rs:44`) is the serde-derive precedent.

**New type** (CONTEXT-sanctioned, field naming at discretion):
```rust
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    pub event: String,                       // PostToolUse | UserPromptSubmit | Stop | Notification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,                // present for PostToolUse
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,   // present for Notification
    pub ts: u64,                             // unix ms
}
```
**Do NOT** derive `Serialize` on `ClaudeMeta` itself — only `HookEvent` needs serde (anti-pattern, RESEARCH:252).

---

### `baude-core/src/meta.rs` — ring append in `read_event_tail` (model, event-driven)

**Analog:** the existing match loop at `meta.rs:393-416` (read this; the new `push_back` slots into the SAME `for line in buf[..consumed].lines()` loop that already drives `hook_status`/`last_tool`/`last_notification`).

**Existing loop to extend** (`meta.rs:393-416`):
```rust
for line in buf[..consumed].lines() {
    let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
    let ts = v["ts"].as_u64().unwrap_or(0);
    match v["event"].as_str() {
        Some("UserPromptSubmit") => self.hook_status = Some((true, ts)),
        Some("Stop") => self.hook_status = Some((false, ts)),
        Some("Notification") => { /* ... last_notification ... */ }
        Some("PostToolUse") => { /* ... last_tool ... */ }
        _ => {}
    }
    // NEW (after the existing match, capturing the same `event`/`ts`):
    if let Some(ev) = v["event"].as_str() {
        self.activity.push_back(HookEvent {
            event: ev.to_string(),
            tool: v["tool"].as_str().map(str::to_string),
            notification_type: v["notification_type"].as_str().map(str::to_string),
            ts,
        });
        if self.activity.len() > ACTIVITY_CAP { self.activity.pop_front(); }
    }
}
```
Add `use std::collections::VecDeque;` (not currently imported), the field `activity: VecDeque<HookEvent>` to `ClaudeMeta` (`meta.rs:81-136`, near `last_tool`/`last_notification`), `const ACTIVITY_CAP: usize = 200;`, and a `pub fn activity(&self) -> &VecDeque<HookEvent>` accessor (the field is private like `offset_events`/`event_path`).

**Path-rotation reset** — extend the EXISTING WR-03 block at `meta.rs:366-372` that already clears the event-derived fields:
```rust
if self.event_path.as_ref() != Some(&path) {
    self.event_path = Some(path.clone());
    self.offset_events = 0;
    self.hook_status = None;
    self.last_tool = None;
    self.last_notification = None;
    // NEW:
    self.activity.clear();
}
```

**Test seam** — extend the `feed_events` helper at `meta.rs:820-840` (truncate-on-first / append-after, deterministic sid) and follow `event_tail_drives_state` (`meta.rs:842-861`) for the ring/cap/clear assertions.

---

### `bauded/src/manager.rs` — `event_path()` + `activity()` accessors (service, read)

**Analog:** `transcript_path()` at `manager.rs:453-456`:
```rust
pub fn transcript_path(&self, id: u64) -> Result<Option<PathBuf>> {
    let s = self.session(id)?;
    Ok(s.meta.transcript_path().map(Path::to_path_buf))
}
```
Mirror it (the `self.session(id)?` Err-is-404 pattern is what `api.rs::not_found` keys on):
```rust
pub fn event_path(&self, id: u64) -> Result<Option<PathBuf>> {
    let s = self.session(id)?;
    Ok(s.meta.session_id.as_ref()
        .map(|sid| PathBuf::from(baude_core::hook::event_path(sid))))
}

pub fn activity(&self, id: u64, limit: usize) -> Result<Vec<HookEvent>> {
    let s = self.session(id)?;
    let act = s.meta.activity();
    let start = act.len().saturating_sub(limit);
    Ok(act.iter().skip(start).cloned().collect())
}
```
`baude_core::hook::event_path(sid)` already sanitizes `..`/`/` (`hook.rs:44-47`). Import `HookEvent` from `baude_core::meta` (manager already does `use baude_core::meta::{now_unix_ms, ClaudeMeta};` at `manager.rs:13`).

---

### `bauded/src/manager.rs` — `SessionInfo.activity` field (model, transform)

**Analog:** `SessionInfo` struct (`manager.rs:44-69`) + `session_info()` builder (`manager.rs:609-632`).

Add a bounded field (default `~30`):
```rust
// In SessionInfo (manager.rs:44-69), after last_tool:
pub activity: Vec<HookEvent>,
```
Populate in `session_info()` (`manager.rs:609-632`, alongside `last_tool: s.meta.last_tool...`):
```rust
activity: {
    let act = s.meta.activity();
    let start = act.len().saturating_sub(30);  // bounded remote set (CONTEXT ~30)
    act.iter().skip(start).cloned().collect()
},
```
**Pitfall 3:** this addition breaks the `notify.rs` test constructor — fix in the same task (below).

---

### `bauded/src/api.rs` — `get_activity` handler + route (controller, request-response)

**Analog:** `get_messages` (`api.rs:147-162`) + its `MessagesQuery` (`141-145`) + router (`19-39`).

Query + handler (mirror `MessagesQuery`/`get_messages`; clamp limit per Security V5):
```rust
#[derive(Deserialize)]
struct ActivityQuery { limit: Option<usize> }

async fn get_activity(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    Query(q): Query<ActivityQuery>,
) -> Result<Json<Vec<HookEvent>>, ApiError> {
    let limit = q.limit.unwrap_or(ACTIVITY_CAP).min(ACTIVITY_CAP);
    let act = lock(&state).activity(id, limit).map_err(not_found)?;
    Ok(Json(act))
}
```
Route, in `router()` (`api.rs:19-39`, next to the `/stream` and `/event` lines):
```rust
.route("/sessions/{id}/activity", get(get_activity))
.route("/sessions/{id}/activity-stream", get(activity_stream))
```
**Test:** mirror `post_event_appends_and_404s_unknown` (`api.rs:476+`) — `Manager::new("sleep 30".into(), false)`, `create`, `session_id_for_test`, tower `oneshot`; assert JSON array + 404 on unknown id (`unknown_session_is_404` table at `api.rs:438`).

---

### `bauded/src/api.rs` — `activity_stream` handler + route (controller, streaming SSE)

**Analog:** `stream()` at `api.rs:362-397` — clone near-verbatim. `STREAM_POLL_MS = 750` const is already at `api.rs:357`.

**Template to clone** (`api.rs:362-397`):
```rust
async fn stream(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    lock(&state).transcript_path(id).map_err(not_found)?;   // existence guard → event_path(id)
    let stream = async_stream::stream! {
        let mut current: Option<PathBuf> = None;
        let mut tail = Tail::default();                       // → EventTail
        loop {
            let path = match lock(&state).transcript_path(id) {  // → event_path(id)
                Ok(p) => p, Err(_) => break,
            };
            if let Some(path) = path {
                if current.as_ref() != Some(&path) {
                    tail = if current.is_none() { Tail::end_of(&path) } else { Tail::default() };
                    current = Some(path.clone());
                }
                for m in tail.read_new(&path) {              // → yields HookEvent
                    let data = serde_json::to_string(&m).unwrap_or_default();
                    yield Ok(Event::default().event("message").id(m.uuid.clone()).data(data));
                }
            }
            tokio::time::sleep(Duration::from_millis(STREAM_POLL_MS)).await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
```
**Three swaps for the activity version:**
1. `transcript_path(id)` → `event_path(id)` (both guard and loop).
2. `Tail`/`ChatMessage` → the new event-line tail yielding `HookEvent` (**Pitfall 1** — do NOT reuse `transcript::Tail`; it parses `ChatMessage` and would skip every event line).
3. **`Event::id`** (Pitfall 2): hook events have no `uuid`. Drop `.id(m.uuid.clone())` and rely on append-only ordering + the GET-then-buffer seam, OR use a per-connection monotonic counter / byte offset. Do NOT key dedup on `ts`.

---

### event-line `Tail` (utility, streaming/file-I/O) — NEW

**Analog (offset machinery):** `transcript::Tail` (`transcript.rs:197-235`) — copy the offset/truncation-reset/`rfind('\n')`/complete-lines-only logic. **Analog (parse step):** `read_event_tail`'s `serde_json::from_str::<Value>` + `Value` accessors (`meta.rs:393-411`), NOT `transcript::parse_line`.

**`Tail::read_new` skeleton to copy** (`transcript.rs:211-234`), changing ONLY the final parse line:
```rust
pub fn read_new(&mut self, path: &Path) -> Vec<HookEvent> {
    let Ok(mut f) = fs::File::open(path) else { return vec![] };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len < self.offset { self.offset = 0; }           // truncation reset
    if len == self.offset || f.seek(SeekFrom::Start(self.offset)).is_err() { return vec![] }
    let mut buf = String::new();
    if f.read_to_string(&mut buf).is_err() { return vec![] }
    let consumed = match buf.rfind('\n') { Some(i) => i + 1, None => return vec![] };
    let events = buf[..consumed].lines().filter_map(parse_event_line).collect();  // ← CHANGED
    self.offset += consumed as u64;
    events
}
```
where `parse_event_line(line) -> Option<HookEvent>` mirrors the `meta.rs:393-411` match (untyped `Value`, `as_u64().unwrap_or(0)`, skip-malformed). **Test (Wave 0 gap):** a `/tmp` JSONL fixture with mixed event types + a malformed line — mirror `feed_events` (`meta.rs:820-840`).

---

### `bauded/src/notify.rs` — test constructor (test, Pitfall 3)

**Analog/exact target:** the `#[cfg(test)] fn info()` at `notify.rs:101-123`. Adding `activity` to `SessionInfo` breaks this with a missing-field error. Add `activity: vec![]`:
```rust
fn info(id: u64, status: &'static str, waiting_ms: Option<u64>) -> SessionInfo {
    SessionInfo {
        id,
        name: format!("s{id}"),
        // ... existing fields ...
        archived: false,
        activity: vec![],   // NEW
    }
}
```

---

### `bauded/web/app.js` — `openActivity` + strip + toggle (component, streaming SSE)

**Analog (state):** `state` object (`app.js:8-24`) — add `activity: []`, `activityOpen: false`, `aes: null`, `aesBuffer: null`.

**Analog (GET-then-SSE-with-buffer):** `openChat` (`app.js:143-169`) — mirror exactly: connect EventSource into a buffer first, then GET history, append, drain buffer, null buffer, render:
```rust
async function openActivity(sid) {            // mirror openChat (app.js:143)
  state.aesBuffer = [];
  const es = new EventSource(`/sessions/${sid}/activity-stream`);
  state.aes = es;
  es.onmessage = (ev) => {
    if (state.sid !== sid) return;
    const e = JSON.parse(ev.data);
    if (state.aesBuffer) state.aesBuffer.push(e);
    else { state.activity.push(e); render(); }
  };
  const recent = await api(`/sessions/${sid}/activity?limit=30`);
  if (state.sid !== sid) return;
  state.activity = recent;
  for (const e of state.aesBuffer || []) state.activity.push(e);
  state.aesBuffer = null;
  render();
}
```
**Analog (cleanup on view exit):** `closeStream` (`app.js:130-134`) — add an `if (state.aes) state.aes.close(); state.aes = null; state.aesBuffer = null;` (call it wherever `closeStream` is called on navigation, so the second SSE never leaks — Pitfall 5).

**Analog (toggle):** `toggleScreen` (`app.js:289-297`) — flip `state.activityOpen`, `render()` (collapsed by default).

**Analog (render + escaping):** `renderChat` (`app.js:455-501`) builds `$app.innerHTML`; the strip mounts below `#chat`/above `#composer`. One line per event using `esc()` (`app.js:28`) on EVERY event field (Security: XSS — never interpolate `e.tool`/`e.notification_type` raw) and `humanMs()` (`app.js:34`) for relative time. Wire the toggle button's `onclick` in the same handler-binding block as `screenbtn`/`archbtn` (`app.js:503-507`).

---

### `bauded/web/style.css` — `.activity-strip` (style)

**Analog:** the `#screen` drawer styles (the collapsible terminal-peek panel rendered by `screenDrawer` at `app.js:471-477`). Mirror its collapsed/expanded + scrollable container approach for a one-line-per-row strip showing the recent ~30 (scrollable, newest at bottom). Class/id naming at discretion.

---

### `bauded/web/sw.js` — CACHE bump (config, Pitfall 4)

**Analog/exact target:** `sw.js:4`:
```js
const CACHE = "baude-v2";   // → "baude-v3"
```
Bump to `"baude-v3"`. The activate handler (`sw.js:11-17`) already evicts non-matching caches; `SHELL` (`sw.js:5`) already lists `/app.js` + `/style.css` so no new entry is needed. PWA assets are `include_bytes!`-embedded (`web.rs`), so a `cargo build` is required for app.js/style.css edits to take effect.

---

### `baude/src/app.rs` — `Modal::Activity` + `v` dispatch + dismiss (store/controller, event-driven)

**Analog (enum):** `Modal` (`app.rs:98-118`) — add a unit variant next to `Info`/`Gsd`:
```rust
pub enum Modal {
    None, Help, Info, Gsd,
    Activity,   // NEW: per-session tool-activity timeline
    Input { .. }, ConfirmKill { .. }, ConfirmCloseWorktree { .. },
}
```
**Analog (dispatch):** the `i` arm in `handle_sidebar_key` (`app.rs:815-819`) — copy it for `v`, branching local-or-remote exactly like `i` (CONTEXT: overlay must work for both):
```rust
KeyCode::Char('v') => {
    if self.selected().is_some() || self.selected_remote().is_some() {
        self.modal = Modal::Activity;
    }
}
```
**Analog (dismiss):** the dismiss-on-any-key arm in `handle_modal_key` (`app.rs:835-837`) — add `Activity`:
```rust
Modal::Help | Modal::Info | Modal::Gsd | Modal::Activity => { self.modal = Modal::None; }
```
`selected()`/`selected_remote()` are at `app.rs:360-365`/`345-350`. Live refresh is free — the 50ms draw loop re-renders the open modal every tick (no special handling, CONTEXT decision).

---

### `baude/src/ui.rs` — `Modal::Activity` render arm (component, request-response)

**Analog:** the `Modal::Info` arm at `ui.rs:779-938` — copy its exact structure:
1. `if let Some(r) = app.selected_remote() { ... render r.activity ... return; }` (remote branch FIRST, `ui.rs:780-837`).
2. `let Some(s) = app.selected() else { return };` then render `s.meta.activity()` (local branch, `ui.rs:838+`).

Both branches use the same `row(...)` closure (`ui.rs:783-788`/`841-846`), build a `Vec<Line>`, then `centered(area, W, lines.len()+2)` (`ui.rs:826`/`926`), `frame.render_widget(Clear, rect)`, and a bordered `Paragraph` (`ui.rs:828-835`/`928-937`). Render each event as `row(icon+tool/type, relative-time)`, newest at bottom; clip the recent ~30 that fit (RESEARCH Open Q1 recommendation — render-last-N, no scroll-offset widget needed for the first cut). Relative time: `now_unix_ms() - ev.ts` formatted (helpers `short_model`/`human_tokens` show the in-file formatting convention).

---

### `baude/src/remote.rs` — `RemoteInfo.activity` field (model, transform)

**Analog/exact target:** `RemoteInfo` (`remote.rs:19-40`). Add a backward-compatible field (older daemons omit it):
```rust
#[serde(default)]
pub activity: Vec<HookEvent>,
```
`RemoteInfo` derives `Deserialize, Clone, Default` and already uses `#[serde(default)]` on `state_source`/`last_tool`/`archived` (`remote.rs:33-39`) — follow that exactly. The `RemotePoller` (`remote.rs:56-88`) deserializes `Vec<RemoteInfo>` from `/sessions` via `ureq` every 3s; the new field rides along with no poller change. `HookEvent` must be importable client-side — re-export from `baude_core::meta` (the `baude` TUI crate depends on `baude-core`).

---

## Shared Patterns

### Offset file-tail (complete-lines-only, truncation-reset)
**Source:** `transcript.rs:211-234` (`Tail::read_new`) and `meta.rs:373-416` (`read_event_tail`).
**Apply to:** the new event-line tail (`activity_stream`) and the ring append.
Both share: open-or-bail, `len < offset` → reset to 0, `seek(offset)`, `read_to_string`, `rfind('\n')` for complete lines only, advance `offset += consumed`. The ONLY difference between the two existing tails is the per-line parse target — reuse the machinery, swap the parse (Pitfall 1).

### SSE handler shape
**Source:** `api.rs:362-397` (`stream`).
**Apply to:** `activity_stream`.
`async_stream::stream!` + `loop { path-guard → break-on-Err; read_new; yield Event; sleep(STREAM_POLL_MS) }` wrapped in `Sse::new(...).keep_alive(KeepAlive::default())`. Existence-guard the session up front so an unknown id is a clean 404 (`map_err(not_found)`).

### 404-on-unknown via `self.session(id)?`
**Source:** `manager.rs` accessors (`transcript_path`:453) + `api.rs::not_found` (`api.rs:97-99`).
**Apply to:** `event_path`, `activity`, `get_activity`, `activity_stream`.
Manager accessors return `Result`; `Err` (no such session) maps to `StatusCode::NOT_FOUND`. `Path<u64>` rejects non-numeric ids at the framework layer (Security V5).

### XSS escaping in the PWA
**Source:** `esc()` (`app.js:28-32`), applied throughout `renderChat`/`msgHtml` (`app.js:450-451`).
**Apply to:** every event field rendered in the activity strip (`e.tool`, `e.notification_type`, `e.event`). Never interpolate raw into `innerHTML`.

### Serde row convention for client-facing API types
**Source:** `SessionInfo` (`manager.rs:44`, `#[derive(Serialize, Clone)]`) and `RemoteInfo` (`remote.rs:19`, `#[derive(Deserialize, Clone, Default)]`, `#[serde(default)]` for new fields).
**Apply to:** `HookEvent` (both Serialize+Deserialize since it round-trips daemon→client) and the new `activity` fields (use `#[serde(default)]` on `RemoteInfo.activity` for backward compat).

### TUI overlay (Clear + Paragraph + Block, local/remote branch)
**Source:** `ui.rs:779-938` (`Modal::Info`).
**Apply to:** `Modal::Activity`. `selected_remote()` branch first (renders the bundled `RemoteInfo.activity`), then `selected()` (renders `s.meta.activity()`). Dismissed by the shared `Help|Info|Gsd|Activity` arm in `handle_modal_key`.

---

## No Analog Found

None. Every change in this phase mirrors an existing in-repo construct. The one new TYPE (`HookEvent`) has a clear precedent (`SessionInfo`/`RemoteInfo` serde rows), and the one true gap is the parse target of the event-line tail (Pitfall 1) — which still reuses the `transcript::Tail` offset skeleton and the `read_event_tail` parse, so it is a recombination, not a from-scratch pattern.

## Metadata

**Analog search scope:** `baude-core/src/{meta,hook}.rs`, `bauded/src/{api,manager,transcript,notify}.rs`, `bauded/web/{app.js,sw.js}`, `baude/src/{app,ui,remote}.rs`.
**Files scanned:** 11 source files read directly (no re-reads; targeted offset reads for the large `app.rs`/`ui.rs`/`api.rs`/`meta.rs`).
**Pattern extraction date:** 2026-06-15

## PATTERN MAPPING COMPLETE
