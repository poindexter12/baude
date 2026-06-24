# Phase 3: Tool-Activity Timeline - Context

**Gathered:** 2026-06-15
**Status:** Ready for planning

<domain>
## Phase Boundary

The Phase 2 hook event stream (schema-1 JSONL in `/tmp/baude-events-<sid>.jsonl`,
consumed by `meta.rs::read_event_tail`) is exposed as a live, capped (~200)
per-session tool-activity feed rendered in BOTH the PWA chat view (collapsible
activity strip) and the TUI (`v` activity overlay). Requirements ACT-01..04.
Remote permission approval is OUT of scope (Phase 4). No new event production —
this phase only retains, serves, and renders the events Phase 2 already emits.

</domain>

<decisions>
## Implementation Decisions

### Event Buffer & Data Model
- The ~200-event ring buffer lives in **`ClaudeMeta` (baude-core)** as a capped
  `VecDeque<HookEvent>` that `read_event_tail` appends to as it tails the event
  file — a single source of truth. TUI-local sessions read it directly; the
  daemon's Sessions hold `ClaudeMeta` so the daemon serves it for free. No second
  mirrored buffer to keep in sync.
- Expose a typed **`HookEvent`** struct serialized to JSON: `{event, tool?,
  notification_type?, ts}`.
- Timeline includes **`PostToolUse`** (the tools — primary), plus
  **`UserPromptSubmit`/`Stop`** as turn-boundary markers and **`Notification`**.
- Cap **~200** events, **drop-oldest ring** (named const, e.g. `ACTIVITY_CAP`).

### Daemon API & SSE Transport
- **`GET /sessions/{id}/activity`** returns recent events as a JSON array, with an
  optional **`?limit`** (default = cap).
- Live updates ride a **standalone `GET /sessions/{id}/activity-stream` SSE
  channel** — keeps the chat `/stream` clean and mirrors the existing `stream()`
  offset-tail handler.
- The stream **tails `/tmp/baude-events-<sid>.jsonl` with an offset** (reuse the
  `Tail` pattern used by the message stream), not a push from the in-memory ring.
- The PWA **fetches recent via GET, then goes live via SSE, buffering during the
  load** — the same discipline as `openChat` (stream connects before history,
  buffers, then merges).

### PWA Activity Strip UX
- A **collapsible strip below the chat history / above the composer**, **collapsed
  by default** (toggle button).
- **One line per event**: icon + tool/type + relative time (e.g. `⚙ Bash · 12:01`),
  **newest at bottom** (chronological, matches chat), auto-scroll when near bottom.
- The strip shows the **recent ~30** (scrollable); the full ~200 is retained
  server-side and reachable via the GET endpoint.
- **Live append on each SSE event**, same buffering discipline as chat.

### TUI Activity Overlay
- **`v`** opens a new **`Modal::Activity`** (ROADMAP SC4 specifies `v`).
- **Local sessions** read the `s.meta` activity buffer directly. **Remote
  sessions** get a bounded recent set (**~30 events**) bundled into the
  `/sessions` list JSON via a new `RemoteInfo.activity` field (no extra round-trip).
- Render as a **scrollable list, newest at bottom**, icon + tool + relative time,
  mirroring the existing `i` info-overlay style (`Clear` + `Paragraph` + `Block`).
- **Live refresh while open** via the existing idle redraw tick (local meta ~1s,
  remote snapshot ~3s) — no special refresh handling.

### Claude's Discretion
- Exact `HookEvent` field naming/icons, the `ACTIVITY_CAP` value (target ~200),
  relative-time formatting, scroll mechanics, and the collapse-toggle button id are
  at Claude's discretion, guided by existing `meta.rs`/`ui.rs`/`app.js` conventions.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `bauded/src/api.rs:362-397` — `stream()` SSE handler (`async_stream::stream!` +
  offset `Tail::read_new()`, yields `Event` with id). The exact template for
  `GET /sessions/{id}/activity-stream`. Router at `api.rs:19-39`.
- `baude-core/src/meta.rs:356-417` — `read_event_tail` (offset `offset_events`,
  parses the JSONL); `ClaudeMeta` struct at `meta.rs:81-136` (already holds
  `last_tool`/`hook_status`/`last_notification`/`event_path`). Add `activity:
  VecDeque<HookEvent>` here.
- `bauded/src/manager.rs:609-632` — `session_info()` builds the `SessionInfo` row;
  extend `SessionInfo` (and `RemoteInfo`) with a bounded `activity` for remote.
- `bauded/web/app.js:143-169` (`openChat` EventSource + buffer) and `455-510`
  (`renderChat`/`msgHtml`) — the SSE-then-render pattern to mirror for the strip.
- `baude/src/app.rs:98-118` (`Modal` enum), `815-828` (`handle_sidebar_key`
  `i`/`g` dispatch), `baude/src/ui.rs:779-937` (`draw_modal` overlay pattern) —
  add `Modal::Activity` + `v` dispatch + a render arm.
- `baude/src/remote.rs:19-40` (`RemoteInfo`), `56-88` (`RemotePoller` 3s poll) —
  add `activity` to `RemoteInfo`.

### Established Patterns
- SSE handlers tail files by offset and poll at `STREAM_POLL_MS` (~750ms); the
  daemon meta loop polls at `META_POLL_MS` (~1s). PWA is vanilla JS, no build step.
- TUI overlays: `Clear` + `Paragraph` + `Block`, dismissed by `handle_modal_key`.
- Untyped `serde_json::Value` for inbound Claude data; typed structs for our own
  client-facing API rows are fine (`SessionInfo`/`RemoteInfo` are already typed).

### Integration Points
- New route in `api.rs::router`; new `Manager`/`ClaudeMeta` accessor for activity;
  new PWA strip in `app.js`; new `Modal::Activity` + `v` in the TUI; `RemoteInfo`
  activity field + remote render path.

</code_context>

<specifics>
## Specific Ideas

- Keep the chat `/stream` untouched — the activity channel is standalone so a
  client can subscribe to one, both, or neither.
- The TUI overlay must work for BOTH local (meta buffer) and remote (RemoteInfo)
  sessions, matching how `i`/Info already branches on local vs remote.
- The ring buffer is the same data for all surfaces — derive PWA, TUI-local, and
  TUI-remote views from one `HookEvent` model.

</specifics>

<deferred>
## Deferred Ideas

- Remote permission approval / opt-in permission-prompt mode — **Phase 4** (PERM).
- Filtering/search within the activity feed, or persisting activity across daemon
  restart — not in scope; the ring is in-memory and live only.

</deferred>
