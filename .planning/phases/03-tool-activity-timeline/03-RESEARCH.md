# Phase 3: Tool-Activity Timeline - Research

**Researched:** 2026-06-15
**Domain:** Rust (axum SSE, VecDeque ring buffer, serde), vanilla-JS PWA (EventSource), ratatui TUI overlay
**Confidence:** HIGH — this is a codebase-internal phase; every pattern it needs already exists in-repo and was read directly this session. No external packages, no new dependencies.

## Summary

Phase 3 surfaces the Phase 2 hook event stream as a live, capped per-session
tool-activity feed in both the PWA and the TUI. Crucially, **no new event
production happens** — Phase 2 already writes `{schema:1, ts, session_id, event,
tool, notification_type}` lines to `/tmp/baude-events-<sid>.jsonl` and `meta.rs::
read_event_tail` already parses them. This phase only (a) **retains** the events
in a capped `VecDeque` ring inside `ClaudeMeta`, (b) **serves** them via a new
`GET /sessions/{id}/activity` + a standalone `GET /sessions/{id}/activity-stream`
SSE channel, and (c) **renders** them in a PWA collapsible strip and a TUI
`Modal::Activity` overlay.

Every building block is a direct mirror of existing code: the SSE handler clones
`api.rs::stream()` (`async_stream::stream!` + `Tail` offset-tail), the ring append
slots into the existing `read_event_tail` parse loop, the PWA strip mirrors
`openChat`'s GET-then-SSE-with-buffer discipline, and the TUI overlay mirrors the
`Modal::Info` local/remote branch in `ui.rs`. The single genuinely new type is a
serde-`Serialize` `HookEvent` struct in baude-core — explicitly sanctioned by
CONTEXT (typed structs for our own client-facing API are already the norm:
`SessionInfo`/`RemoteInfo`).

**Primary recommendation:** Add `HookEvent` (serde `Serialize`/`Deserialize`,
`Clone`) + `activity: VecDeque<HookEvent>` to `ClaudeMeta`; push into it from the
existing `read_event_tail` match arms with a drop-oldest cap (`ACTIVITY_CAP`);
add `Manager::activity(id)` + `Manager::event_path(id)` accessors; clone
`stream()` into an `activity_stream()` that tails the event file and serializes
`HookEvent`; mirror `openChat` for the PWA strip; mirror `Modal::Info` for the
TUI overlay; bundle a bounded `activity: Vec<HookEvent>` into `SessionInfo`/
`RemoteInfo` for the remote TUI path. Bump the sw.js cache version so PWA clients
refetch.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Event retention (ring buffer) | baude-core (`ClaudeMeta`) | — | Single source of truth; both TUI-local and daemon hold `ClaudeMeta`, so the daemon serves it "for free" (CONTEXT decision). No mirrored buffer. |
| Serve recent events (REST) | bauded API (`api.rs`) | manager (`Manager::activity`) | REST surface owns client-facing endpoints; reads the ring via a Manager accessor. |
| Live event delivery (SSE) | bauded API (`activity_stream`) | transcript (`Tail`) | Mirrors the message `stream()`; tails the on-disk `/tmp` event file by offset, NOT the in-memory ring (CONTEXT decision — keeps the channel stateless/restart-safe and independent of the meta poll). |
| PWA activity strip render | PWA (`app.js` `renderChat`) | — | Browser tier owns DOM; second EventSource + buffer is client-local. |
| TUI activity overlay render | TUI (`ui.rs` `draw_modal`) | app (`Modal::Activity`, key `v`) | TUI owns ratatui rendering; local reads `s.meta.activity`, remote reads `RemoteInfo.activity`. |
| Remote activity transport | bauded (`SessionInfo.activity`) | TUI client (`RemoteInfo.activity`) | Bounded ~30 events bundled into `/sessions` JSON — no extra round-trip for the remote overlay (CONTEXT decision). |

## Standard Stack

This phase introduces **no new libraries**. Everything is already in the workspace.

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axum` | 0.8 | REST + SSE (`Sse`, `Event`, `KeepAlive`) | Already the daemon framework; `stream()` is the proven SSE template `[VERIFIED: bauded/Cargo.toml:13, api.rs:362]` |
| `async-stream` | 0.3 | `async_stream::stream!` generator for SSE bodies | Already used by `stream()` `[VERIFIED: bauded/Cargo.toml:12]` |
| `futures-core` | 0.3 | `Stream` trait bound on the SSE return type | Already imported in `stream()` signature `[VERIFIED: bauded/Cargo.toml:17]` |
| `serde` (workspace) | 1 (derive) | `#[derive(Serialize, Deserialize)]` on `HookEvent` | Workspace dep; `SessionInfo`/`RemoteInfo` already derive serde `[VERIFIED: Cargo.toml:14, manager.rs:44]` |
| `serde_json` (workspace) | 1 | Parse event lines (`Value`) + serialize `HookEvent` | Already used in `read_event_tail` `[VERIFIED: meta.rs:394]` |
| `std::collections::VecDeque` | std | The ~200 drop-oldest ring | std; not yet imported in meta.rs (add `use`) `[VERIFIED: grep — no VecDeque import in meta.rs]` |
| `tokio` | 1 | `tokio::time::sleep` poll cadence in SSE | Already used by `stream()` `[VERIFIED: bauded/Cargo.toml:24]` |
| `ureq` | 2 (json) | TUI client fetch of `/sessions` (carries remote activity) | Already used by `RemotePoller` `[VERIFIED: remote.rs:63]` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `ratatui` widgets (`Clear`/`Paragraph`/`Block`) | in-repo | TUI overlay render | `Modal::Activity` arm — mirror `Modal::Info` `[VERIFIED: ui.rs:779-938]` |
| Browser `EventSource` | native | PWA second SSE channel | The activity strip live tail — mirror `openChat` `[VERIFIED: app.js:147]` |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Standalone `/activity-stream` SSE | Fold activity into the existing `/stream` | CONTEXT **locks** standalone; folding would couple the chat tail to activity and break the "subscribe to one, both, or neither" property. Do not relitigate. |
| SSE tailing the `/tmp` file by offset | SSE pushing from the in-memory ring | CONTEXT **locks** file-offset tail (mirrors `stream()`); a push-from-ring would need a broadcast channel and lose restart/offset semantics. Do not relitigate. |
| `VecDeque` drop-oldest | `ringbuf`/`arraydeque` crate | std `VecDeque` with `push_back` + `if len > cap { pop_front() }` is trivial and dependency-free; a crate adds supply-chain surface for zero benefit. |

**Installation:** None. `cargo add` is not invoked in this phase.

## Package Legitimacy Audit

**No external packages are installed in this phase.** Every dependency the work
touches (`axum`, `async-stream`, `serde`, `serde_json`, `tokio`, `ureq`,
`futures-core`, `ratatui`) is already a vetted, in-tree workspace dependency
shipped and CI-green since v0.4–v0.6.1. `std::collections::VecDeque` is standard
library.

| Package | Registry | Disposition |
|---------|----------|-------------|
| (none) | — | No installs this phase |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
                        Phase 2 (already shipped)
  claude session ──hook──> `baude hook` ──┬──> /tmp/baude-events-<sid>.jsonl  (TUI-local append)
                                          └──> POST /sessions/{id}/event ──> Manager::ingest_event
                                                                                   │ append
                                                                                   ▼
                                                            /tmp/baude-events-<sid>.jsonl
  ============================================================================================
                        Phase 3 (this phase)
                                          ┌──────────────────────────────────────┐
   ClaudeMeta::poll() ──> read_event_tail │ parse line ──> match event:          │
                                          │   PostToolUse / UserPromptSubmit /    │
                                          │   Stop / Notification                 │
                                          │     ├─ (existing) set hook_status/... │
                                          │     └─ (NEW) push HookEvent into       │
                                          │        activity: VecDeque (cap ~200,   │
                                          │        drop-oldest)                    │
                                          └──────────────┬───────────────────────┘
                                                         │
                  ┌──────────────────────────────────────┼───────────────────────────────┐
                  ▼ (TUI-local)                           ▼ (daemon serves)                │
         s.meta.activity ──> Modal::Activity      GET /sessions/{id}/activity  ──JSON─┐    │
         overlay (ui.rs)                          (Manager::activity, ?limit)         │    │
                                                                                       ▼    │
                                          GET /sessions/{id}/activity-stream (SSE) ─tail─┐  │
                                          (clone of stream(): Tail offset on the          │  │
                                           /tmp event file, serialize HookEvent)          │  │
                                                                                          ▼  ▼
   PWA chat view: openChat-style GET-then-SSE ──> collapsible activity strip (newest at bottom)
                                                                                          ▲
   /sessions JSON carries bounded activity[] (SessionInfo.activity) ──> RemoteInfo.activity
                                                                  ──> TUI remote Modal::Activity
```

Trace the primary use case: a tool runs → Phase-2 hook appends a line → on the
next poll `read_event_tail` pushes a `HookEvent` into the ring AND the file grows
→ the PWA's `/activity-stream` EventSource (offset-tailing that file) emits the
new line → the strip appends one row. The TUI overlay reads the ring directly
(local) or the bundled `RemoteInfo.activity` (remote) and re-renders on the
50 ms draw tick.

### Component Responsibilities

| File | Change |
|------|--------|
| `baude-core/src/meta.rs` | New `HookEvent` struct; `use std::collections::VecDeque`; `activity: VecDeque<HookEvent>` on `ClaudeMeta`; push + cap inside `read_event_tail`; public accessor (e.g. `pub fn activity(&self) -> &VecDeque<HookEvent>` or expose the field). Reset on event-path rotation (same WR-03 block at meta.rs:366-372 that clears `last_tool`). |
| `bauded/src/manager.rs` | `Manager::activity(id, limit) -> Result<Vec<HookEvent>>` (read `s.meta.activity`); `Manager::event_path(id) -> Result<Option<PathBuf>>` (resolve `hook::event_path(sid)` — analog of `transcript_path`); add bounded `activity: Vec<HookEvent>` to `SessionInfo` populated in `session_info()`. |
| `bauded/src/api.rs` | Two routes in `router()`: `GET /sessions/{id}/activity` (Query `?limit`) + `GET /sessions/{id}/activity-stream`; `get_activity` handler + `activity_stream` handler (clone of `stream()`). |
| `bauded/src/transcript.rs` (or a new tail) | `Tail` parses `ChatMessage`, not `HookEvent` — see Pitfall 1. Need an event-line tail that yields `HookEvent`. |
| `bauded/src/notify.rs` | Update the `#[cfg(test)]` `SessionInfo` constructor for the new `activity` field (Phase 2 hit this exact compile break — see 02-03 Rule-3 fix). |
| `bauded/web/app.js` | `state.activity`/`state.activityOpen`/`state.aes` (+ buffer); `openActivity(sid)` (GET-then-SSE mirror of `openChat`); render the strip in `renderChat`; toggle handler (mirror `toggleScreen`). |
| `bauded/web/style.css` | `.activity-strip` styles (collapsed/expanded, one-line rows, scrollable recent ~30). |
| `bauded/web/sw.js` | Bump `CACHE = "baude-v2"` → `"baude-v3"` so clients refetch app.js/style.css (Pitfall 4). |
| `baude/src/app.rs` | `Modal::Activity` variant; `v` dispatch in `handle_sidebar_key` (mirror `i` at app.rs:815); dismiss in `handle_modal_key` (add to the `Help|Info|Gsd` arm). |
| `baude/src/ui.rs` | `Modal::Activity` arm in `draw_modal` (mirror `Modal::Info` local/remote branch). |
| `baude/src/remote.rs` | `#[serde(default)] activity: Vec<HookEvent>` on `RemoteInfo` (backward-compatible against an older daemon). |

### Pattern 1: Standalone SSE channel (clone of `stream()`)
**What:** A second, independent SSE endpoint that offset-tails the `/tmp` event
file and serializes `HookEvent` instead of `ChatMessage`.
**When to use:** `activity_stream` handler.
**Example (the verified template to clone — `[VERIFIED: bauded/src/api.rs:362-397]`):**
```rust
async fn stream(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    lock(&state).transcript_path(id).map_err(not_found)?;   // existence guard
    let stream = async_stream::stream! {
        let mut current: Option<PathBuf> = None;
        let mut tail = Tail::default();
        loop {
            let path = match lock(&state).transcript_path(id) {  // → event_path(id)
                Ok(p) => p, Err(_) => break, // session deleted
            };
            if let Some(path) = path {
                if current.as_ref() != Some(&path) {
                    tail = if current.is_none() { Tail::end_of(&path) } else { Tail::default() };
                    current = Some(path.clone());
                }
                for m in tail.read_new(&path) {           // → yields HookEvent
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
For activity: swap `transcript_path` → `event_path`, swap `Tail`/`ChatMessage` →
an event-line tail yielding `HookEvent`, and use a stable event id (events have a
`ts` but no `uuid` — see Pitfall 2). Keep `STREAM_POLL_MS = 750`.

### Pattern 2: Ring-buffer append in the existing parse loop
**What:** Push a `HookEvent` into the capped `VecDeque` from inside the
already-present `match v["event"].as_str()` arms.
**When to use:** `read_event_tail` (meta.rs:393-415).
**Example (extending the verified loop — `[VERIFIED: meta.rs:393-416]`):**
```rust
const ACTIVITY_CAP: usize = 200; // CONTEXT: ~200, drop-oldest

for line in buf[..consumed].lines() {
    let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
    let ts = v["ts"].as_u64().unwrap_or(0);
    let event = match v["event"].as_str() {
        Some(e) => e.to_string(),
        None => continue,
    };
    // (existing) drive hook_status / last_tool / last_notification ...
    // (NEW) retain for the activity timeline:
    self.activity.push_back(HookEvent {
        event,
        tool: v["tool"].as_str().map(str::to_string),
        notification_type: v["notification_type"].as_str().map(str::to_string),
        ts,
    });
    if self.activity.len() > ACTIVITY_CAP { self.activity.pop_front(); }
}
```
Reset `self.activity.clear()` in the existing event-path-rotation block
(meta.rs:366-372) alongside the `last_tool = None` resets (WR-03).

### Pattern 3: GET-then-SSE-with-buffer (PWA)
**What:** Connect the SSE first into a buffer, then GET the recent set, then
merge — so nothing falls between the snapshot and the live tail.
**When to use:** `openActivity(sid)` strip init.
**Example (mirror of `openChat` — `[VERIFIED: app.js:143-169]`):** connect
`new EventSource('/sessions/${sid}/activity-stream')` into `state.aesBuffer`,
`await api('/sessions/${sid}/activity')`, append history then drain the buffer,
null the buffer, `render()`. Reuse `closeStream`'s discipline on view exit
(`app.js:130` — add an `if (state.aes) state.aes.close()`).

### Pattern 4: TUI overlay mirroring `Modal::Info`
**What:** A `Clear`+`Paragraph`+`Block` overlay that branches local
(`s.meta.activity`) vs remote (`RemoteInfo.activity`).
**When to use:** the `Modal::Activity` arm in `draw_modal`.
**Example:** clone the structure at `ui.rs:779-938` — the `selected_remote()`
branch first (render `r.activity`), then `selected()` (render `s.meta.activity`),
each as `row(icon+tool, relative-time)` lines, `centered(...)`, `Clear`, then a
bordered `Paragraph`. Live refresh is automatic: the 50 ms draw loop
(`main.rs:121`) re-renders every tick — no special handling (CONTEXT decision).

### Anti-Patterns to Avoid
- **Reusing `transcript::Tail` for the event file:** `Tail::read_new` returns
  `Vec<ChatMessage>` via `parse_line` (transcript schema). Pointing it at the
  event JSONL would parse-fail every line. Write an event tail (or generalize the
  offset machinery) — see Pitfall 1.
- **Pushing from the in-memory ring into the SSE:** CONTEXT locks the file-offset
  tail. A push channel would diverge from `stream()` and lose offset/restart
  semantics.
- **Folding activity into `/stream`:** locked out — keep channels independent.
- **Deriving `Serialize` on the whole `ClaudeMeta`:** only `HookEvent` needs
  serde. `ClaudeMeta` holds non-serde fields and is intentionally `Clone`-only.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Capped event retention | A manual index + array shift | `VecDeque::push_back` + `pop_front` when `> cap` | O(1) ends, zero deps, idiomatic |
| Offset file-tail for SSE | A bespoke inotify/watch | The `Tail`-style offset+`rfind('\n')` machinery (`transcript.rs:211`, `meta.rs:389`) | Already proven across two tails (transcript + events); complete-lines-only, malformed-line-skip, truncation-reset all solved |
| SSE plumbing | Manual chunked HTTP | `axum::response::sse::{Sse, Event, KeepAlive}` + `async_stream` | `stream()` is the working template |
| PWA live updates | Polling `setInterval` | `EventSource` (already the chat pattern) | Native reconnect, server-pushed |
| Relative time | A date library | A small `humanMs`-style helper (`app.js:34`) / Rust `now_unix_ms() - ts` | `humanMs` already exists; no dep |

**Key insight:** This phase is almost entirely *composition of existing seams*.
The highest-leverage move is recognizing that `read_event_tail` already parses
exactly the lines the ring needs — the ring is one `push_back` away — and that
`stream()` is a near-verbatim template for the second SSE channel.

## Runtime State Inventory

This is **not** a rename/refactor/migration phase (it adds a feature). The
section is included only to record that nothing stateful is being renamed:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — the ring is in-memory and live-only; CONTEXT explicitly defers persistence. The `/tmp` event file is Phase-2-owned and unchanged. | None |
| Live service config | None — no external service config touched. | None |
| OS-registered state | None. | None |
| Secrets/env vars | None — no new env vars. (`$BAUDE_EVENT_URL` is Phase-2-owned, unchanged.) | None |
| Build artifacts | PWA assets are `include_bytes!`-embedded at compile time (`web.rs:20`); editing `app.js`/`style.css` requires a `cargo build` to take effect, and the `sw.js` `CACHE` constant must bump so deployed clients refetch (Pitfall 4). | Rebuild + bump sw cache version |

## Common Pitfalls

### Pitfall 1: `Tail` parses transcript records, not hook events
**What goes wrong:** Reusing `transcript::Tail` for the activity SSE silently
yields zero events — `Tail::read_new` runs each line through `parse_line`
(transcript schema with `type`/`uuid`/`message`), which returns `vec![]` for a
`{schema:1, event, tool, ...}` line.
**Why it happens:** Both tails share offset machinery but differ in the parse
step. The name `Tail` invites blind reuse.
**How to avoid:** Write a dedicated event-line tail (offset + `rfind('\n')` +
`serde_json` into `HookEvent`, skip malformed) OR generalize the offset reader to
take a line-parser closure. Mirror `read_event_tail`'s parse, not `parse_line`.
**Warning signs:** The strip stays empty while the file grows; `/activity` (which
reads the ring directly) works but `/activity-stream` emits nothing.

### Pitfall 2: Hook events have no `uuid` for the SSE `Event::id`
**What goes wrong:** `stream()` uses `m.uuid` for the SSE event id and the PWA
dedups chat by uuid. Hook events carry only `ts` (and are not globally unique —
two `PostToolUse` can share a millisecond).
**Why it happens:** The activity stream is byte-offset-based, not uuid-based.
**How to avoid:** Use the byte offset (or a monotonic per-connection counter) as
the SSE `id`, or omit the id and rely on append-only ordering (the strip appends
in arrival order; no dedup needed because the GET snapshot + buffered SSE cover
the seam exactly like `openChat`). Do **not** invent a `seen`-set keyed on `ts`.
**Warning signs:** Events dropped as "duplicate" when two share a timestamp.

### Pitfall 3: SessionInfo field addition breaks the notify.rs test constructor
**What goes wrong:** Adding `activity` to `SessionInfo` fails `cargo test -p
bauded` with a missing-field error in `notify.rs`'s `#[cfg(test)] fn info(...)`.
**Why it happens:** `notify.rs` constructs `SessionInfo` by hand in tests.
**How to avoid:** Update that constructor in the same task (Phase 2's 02-03
SUMMARY documents this exact Rule-3 fix for `state_source`/`last_tool`). Default
`activity: vec![]`.
**Warning signs:** `cargo clippy/test --workspace` red on an otherwise-clean diff.

### Pitfall 4: Stale service-worker cache serves old PWA assets
**What goes wrong:** A deployed phone keeps the old `app.js`/`style.css` from the
service-worker cache; the strip never appears.
**Why it happens:** `sw.js` precaches `["/", "/app.js", "/style.css", ...]` under
`CACHE = "baude-v2"` (`sw.js:4-5`) and the activate handler only evicts caches
whose key differs from `CACHE`.
**How to avoid:** Bump `CACHE` to `"baude-v3"`. The activate handler then evicts
v2 and refetches. (The asset *routes* already exist, so no new SHELL entry is
needed — only the version bump.)
**Warning signs:** Desktop (no SW or hard-refresh) shows the strip; phone doesn't.

### Pitfall 5: Two concurrent EventSources per session view (browser connection cap)
**What goes wrong:** Concern that the chat `/stream` + activity `/activity-stream`
exhaust the browser's per-host HTTP/1.1 6-connection limit.
**Why it happens:** HTTP/1.1 caps ~6 connections per origin; each `EventSource`
holds one open. Two SSE on the chat view = 2 of 6.
**How to avoid:** Two is well within the cap for a single-view client — not a
real concern here. `[ASSUMED]` If bauded ever serves over HTTP/2 (TLS via the
Tailscale sidecar terminating elsewhere), SSE multiplexes over one connection and
the cap is moot. Keep the strip's EventSource scoped to the chat view and close
it on exit (mirror `closeStream`) so it never leaks across navigations. Do not
pre-optimize into a shared multiplexed channel — CONTEXT locks two standalone
channels.
**Warning signs:** A *third+* simultaneous SSE (e.g. multiple tabs) stalling —
not expected in the phone-first single-view UX.

## Code Examples

### Typed HookEvent in baude-core (the one new type)
```rust
// baude-core/src/meta.rs — typed, serde-serializable (CONTEXT-sanctioned for
// our own client-facing rows, like SessionInfo/RemoteInfo). Field naming/icons
// at Claude's discretion.
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

### Manager event-path accessor (analog of transcript_path)
```rust
// bauded/src/manager.rs — mirror transcript_path (manager.rs:453)
pub fn event_path(&self, id: u64) -> Result<Option<PathBuf>> {
    let s = self.session(id)?;
    Ok(s.meta.session_id.as_ref()
        .map(|sid| PathBuf::from(baude_core::hook::event_path(sid))))
}

pub fn activity(&self, id: u64, limit: usize) -> Result<Vec<HookEvent>> {
    let s = self.session(id)?;
    let act = s.meta.activity();                // &VecDeque<HookEvent>
    let start = act.len().saturating_sub(limit);
    Ok(act.iter().skip(start).cloned().collect())
}
```

### TUI `v` dispatch (mirror of `i` at app.rs:815)
```rust
// baude/src/app.rs — handle_sidebar_key
KeyCode::Char('v') => {
    if self.selected().is_some() || self.selected_remote().is_some() {
        self.modal = Modal::Activity;
    }
}
// and in handle_modal_key, add Activity to the dismiss-on-any-key arm:
Modal::Help | Modal::Info | Modal::Gsd | Modal::Activity => { self.modal = Modal::None; }
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Tool activity inferred from PTY output | First-party hook event stream | Phase 2 (this milestone) | The feed is exact, not heuristic — Phase 3 just retains/serves/renders it |

**Deprecated/outdated:** none relevant to this phase.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Two concurrent EventSources per chat view stay within browser connection caps and need no shared multiplexing | Pitfall 5 | Low — worst case is a rare third-tab stall; mitigated by scoping/closing the strip ES on view exit. Single-view phone UX makes this near-zero. |
| A2 | TUI "scrollable newest-at-bottom" overlay is acceptable as render-last-N-that-fit (no scrollback widget exists in-repo) | Open Questions Q1 | Low — CONTEXT caps the visible set at ~30 and grants scroll-mechanics discretion; clipping to the box bottom matches the cap. A scroll offset can be added later if needed. |

**Note:** A1/A2 are minor and explicitly within Claude's-discretion or
no-regression-safe territory; neither blocks planning.

## Open Questions

1. **TUI overlay scroll mechanics**
   - What we know: No scrollbar/scroll-offset widget exists in `ui.rs` today;
     `Modal::Info` renders a static variable-height `Paragraph`. CONTEXT says
     "scrollable, newest at bottom" but grants scroll-mechanics discretion.
   - What's unclear: Whether to (a) render the most-recent N events that fit the
     box (clip older off the top), or (b) add a `scroll_offset` to `App` + arrow
     handling in `handle_modal_key`.
   - Recommendation: Ship (a) for the first cut — recent ~30 capped, render the
     tail that fits via `Paragraph::scroll` anchored to the bottom; it satisfies
     "newest at bottom" and the cap. Add (b) only if a verification test or UAT
     demands paging through the full ~200 in the TUI (the GET endpoint already
     covers full retrieval). Flag as a one-line plan note.

2. **Event id for the activity SSE**
   - What we know: events have `ts` (not unique) and no `uuid`.
   - What's unclear: whether to set `Event::id` at all.
   - Recommendation: Use the running byte offset as the id (stable, monotonic) or
     omit it; rely on `openChat`-style GET+buffer to cover the snapshot/live seam
     (no client dedup needed). Decided in Pitfall 2.

## Environment Availability

This phase is pure in-repo Rust + embedded vanilla JS/CSS — no external runtime
tools, services, or package installs beyond the existing Rust toolchain (already
required for the workspace) and a browser (for the PWA, already the deploy
target). No probing needed.

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (cargo/fmt/clippy) | build + CI triad | ✓ (workspace baseline) | stable | — |
| Browser w/ EventSource + Service Worker | PWA strip | ✓ (existing PWA target) | — | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none.

## Validation Architecture

> `workflow.nyquist_validation = true` (config.json) — section included.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Built-in Rust test harness (`#[cfg(test)] mod tests`) — no external test crate |
| Config file | none — `cargo test` per crate |
| Quick run command | `cargo test -p baude-core meta::` (ring) / `cargo test -p bauded api::` (endpoints) |
| Full suite command | `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` |

The repo convention (Phases 1–2) is impl + `#[cfg(test)] mod tests` in the same
file, committed as one `feat` commit with full `<behavior>` coverage (TDD-typed
tasks ship tests with impl — see every Phase-2 SUMMARY's TDD Gate Compliance).

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ACT-01 | Ring retains events; caps at `ACTIVITY_CAP`; drops oldest; clears on event-path rotation | unit | `cargo test -p baude-core meta::` (extend `feed_events` helper from 02-02) | ✅ `meta.rs mod tests` exists |
| ACT-01 | `Manager::activity(id, limit)` returns the recent slice | unit | `cargo test -p bauded manager::` | ✅ `manager.rs mod tests` exists |
| ACT-02 | `GET /sessions/{id}/activity` returns JSON array (+ `?limit`); 404 unknown id | integration | `cargo test -p bauded api::` (tower `oneshot`, mirror `post_event_appends_and_404s_unknown`) | ✅ `api.rs mod tests` exists |
| ACT-02 | `GET /sessions/{id}/activity-stream` exists + 404s unknown; event-line tail yields `HookEvent` | unit/integration | `cargo test -p bauded` (tail unit test; stream existence-guard like `stream()`) | ✅ |
| ACT-02 | `HookEvent` serde round-trips `{event, tool?, notification_type?, ts}` | unit | `cargo test -p baude-core meta::` | ✅ |
| ACT-03 | PWA strip renders + appends live | manual-only | UAT (browser + live session) — JS has no test harness, no build step | n/a (manual) |
| ACT-04 | TUI `v` opens `Modal::Activity`; dismiss returns to `None`; local vs remote branch | unit | `cargo test -p baude` (if an `app.rs` key-dispatch test seam exists) / else UAT | ⚠️ verify app.rs test seam in Wave 0 |

### Sampling Rate
- **Per task commit:** the crate-scoped quick command for the touched crate.
- **Per wave merge:** `cargo test --workspace`.
- **Phase gate:** full CI triad (`fmt --check` + `clippy -D warnings` +
  `test --workspace`) green before `/gsd-verify-work` — this is the project's
  hard push gate (PROJECT.md).

### Wave 0 Gaps
- [ ] Event-line tail unit test fixture (a `/tmp` JSONL with mixed event types +
  a malformed line) — mirror 02-02's `feed_events` truncate-then-append helper.
- [ ] Confirm whether `baude/src/app.rs` has a key-dispatch test seam for
  asserting `v → Modal::Activity` without a live terminal; if not, ACT-04's
  open/dismiss behavior is UAT-only (acceptable — TUI rendering is inherently
  manual, consistent with Phase-2's human-verify UAT for hook-driven state).
- [ ] PWA strip (ACT-03) is **manual-only** by construction (vanilla JS, no build
  step, no JS test runner — PROJECT constraint). Capture as a `checkpoint:
  human-verify` UAT, not an automated test. Do not add a JS test framework.

*(No framework install needed — the Rust harness covers all automatable
requirements.)*

## Security Domain

> `security_enforcement = true`, `security_asvs_level = 1`, `security_block_on =
> high` (config.json). Threat model inherits the project baseline: **security is
> the VPN/Tailscale bind, no auth layer** (PROJECT.md; api.rs:1 module doc).

### Applicable ASVS Categories (L1)

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | By design — single-user, VPN-bound; no auth layer added (structural, REQUIREMENTS Out of Scope) |
| V3 Session Management | no | No sessions/cookies; stateless REST/SSE on the tailnet |
| V4 Access Control | no | No multi-user; tailnet membership IS the access boundary |
| V5 Input Validation | yes | `Path<u64>` rejects non-numeric ids at the framework layer; `?limit` parsed via typed `Query` (clamp to `ACTIVITY_CAP`); event lines parsed with untyped `Value` + skip-malformed (never panic) — mirrors `read_event_tail`'s T-02-05 posture |
| V6 Cryptography | no | No new crypto; no secrets introduced |

### Known Threat Patterns for {axum SSE + file-tail + PWA}

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Unauthenticated `GET /activity` / `/activity-stream` reachable | Information Disclosure | **Accepted** per project baseline — inherits the existing tailnet/loopback bind; no new exposure beyond the current REST surface (same disposition as Phase-2 T-02-08). |
| Path injection via session id into the `/tmp` event path | Tampering | Mitigated upstream — `hook::event_path` already replaces `..` and `/` (hook.rs:45, Phase-2 T-02-01); the activity path reuses it. |
| Malformed/oversized event-line DoS on the tail | Denial of Service | Mitigated — untyped-`Value`-or-skip parse, complete-lines-only, `as_u64().unwrap_or(0)`, truncation-reset; never panics (mirror `read_event_tail` T-02-05). |
| Unbounded `?limit` exhausting memory/response | Denial of Service | Clamp `limit` to `ACTIVITY_CAP`; the ring itself is capped at ~200 so the response is bounded regardless. |
| Unbounded SSE connection growth | Denial of Service | Each stream is offset-tailing one file at `STREAM_POLL_MS`; ends when the session is deleted (the `Err(_) => break` guard) — same lifecycle as `stream()`. |
| XSS via tool/notification strings rendered in the PWA strip | Tampering/Info Disclosure | Escape all event fields with the existing `esc()` helper (app.js:28) before inserting into `innerHTML`. **Do not** interpolate `m.tool`/`m.notification_type` raw. |

**Block-on-high check:** No high/critical findings introduced — all surfaces
inherit accepted-baseline or are mitigated by existing Phase-2 controls. The one
net-new client-side concern (strip XSS) is mitigated by the existing `esc()`
convention and must be a verification step.

## Sources

### Primary (HIGH confidence) — codebase, read this session
- `bauded/src/api.rs` (router:19-39, get_messages:147, post_event:232, stream:362-397, ApiError/not_found:95-99, mod tests:399+) — SSE + handler templates
- `bauded/src/transcript.rs` (Tail:198-235) — offset-tail machinery + the `ChatMessage`-parse caveat
- `baude-core/src/meta.rs` (ClaudeMeta:81-136, poll:139-150, read_event_tail:350-417, typed structs:38-65) — ring insertion point + serde-struct precedent
- `baude-core/src/hook.rs` (event_path:44-47, append_event:128) — `/tmp` path + sanitization
- `bauded/src/manager.rs` (SessionInfo:43-69, transcript_path:453, list/info:504-510, session_info:609-632, event_url/seed notes) — accessors + SessionInfo extension
- `bauded/src/web.rs` (asset! include_bytes!:10-46) — no-build-step embedding
- `bauded/web/app.js` (state:8-24, esc:28, openChat:143-169, closeStream:130, toggleScreen:289, renderChat:455-514) — PWA patterns
- `bauded/web/sw.js` (CACHE/SHELL:4-5, activate evict:14) — service-worker cache bump
- `baude/src/app.rs` (Modal enum:98-118, selected/selected_remote:345-367, handle_sidebar_key i/g:815-828, handle_modal_key:833+, tick:484) — TUI dispatch
- `baude/src/ui.rs` (draw_modal Info local/remote:779-938) — overlay template
- `baude/src/remote.rs` (RemoteInfo:18-40, RemotePoller:56-88) — remote field + poll
- `baude/src/main.rs` (run loop 50ms tick:118-128) — auto-refresh cadence
- Phase 2 SUMMARYs (02-01/02/03) — event schema, `Tail` reuse, notify.rs Rule-3 fix, TDD/threat conventions
- `Cargo.toml` / `bauded/Cargo.toml` / `baude-core/Cargo.toml` — verified dep versions

### Secondary (MEDIUM confidence)
- None — no web/doc lookups were needed (all search providers disabled in config; phase is fully codebase-internal).

### Tertiary (LOW confidence)
- Browser per-host SSE/HTTP-1.1 connection cap reasoning (Pitfall 5 / A1) — general knowledge, marked `[ASSUMED]`; non-blocking.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all deps verified in Cargo.toml; no new packages.
- Architecture: HIGH — every pattern read directly in-repo; this phase composes existing seams.
- Pitfalls: HIGH — Pitfalls 1/3/4 are concrete code facts (Tail parse target, notify.rs constructor, sw.js CACHE); Pitfall 5 is the only `[ASSUMED]` item and is non-blocking.

**Research date:** 2026-06-15
**Valid until:** 2026-07-15 (stable — internal codebase; only invalidated by a refactor of `api.rs::stream`, `meta.rs::read_event_tail`, or the PWA `openChat` pattern).
