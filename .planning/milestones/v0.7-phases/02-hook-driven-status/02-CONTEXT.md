# Phase 2: Hook-Driven Status - Context

**Gathered:** 2026-06-15
**Status:** Ready for planning

<domain>
## Phase Boundary

A managed session's working/waiting/done state is derived from Claude Code hook
events (`UserPromptSubmit`, `Stop`, `Notification`, `PostToolUse`) rather than the
PTY-output-silence heuristic. baude seeds its hook set into the managed session's
settings by merging into existing arrays, transports events via a per-session
file-tail (TUI-local) and `POST /sessions/{id}/event` (daemon), and preserves the
existing dual-source silence logic as a labeled fallback. The last tool a session
ran is captured. Rich tool-activity timeline rendering is OUT of scope (Phase 3);
remote permission approval is OUT of scope (Phase 4).

</domain>

<decisions>
## Implementation Decisions

### Hook Seeding & settings.json Merge
- Seed baude's hook set into the per-session **`.claude/settings.local.json`** in
  the session's cwd/worktree — gitignored by Claude convention, merges over the
  user's global config, and never modifies committed project settings or the
  user's global `~/.claude/settings.json`.
- Merge by reading existing JSON (if any) and **appending** baude's entries into
  each hook event's array, guarded by a **sentinel marker** so re-spawn/restart is
  idempotent and never duplicates entries. Preserve all existing keys — user hooks
  and a user `statusLine` survive intact. Apply the same schema-drift tolerance as
  `bridge.rs` (untyped `serde_json::Value`, snake/camel safe; never panic on a
  minimal/odd file).
- Add a new **`baude hook`** subcommand wired to all four events. Claude invokes it
  per event; it reads the hook JSON from **stdin** (which carries `session_id`,
  `hook_event_name`, and `tool_name`) and appends one normalized event line.
- **No cleanup on session close** — seeding is idempotent and additive; removal
  risks races and is unnecessary.

### Event Model & State Derivation
- Event lines are **schema-versioned JSONL**: `{schema:1, ts, session_id, event,
  tool?}`, read with **untyped `serde_json::Value` accessors** (no
  `#[derive(Deserialize)]`, no branching on schema) — the STL-02 back-compat
  discipline carried from Phase 1.
- Event→state mapping: `UserPromptSubmit` → **Busy** (working the moment a prompt is
  submitted); `Stop` → **Waiting** (turn done / awaiting next prompt);
  `Notification` → **Waiting**, labeled as needs-input/permission; `PostToolUse` →
  stay Busy and **record the last tool name**.
- Keep the existing `Status { Waiting, Busy, Exited }` enum **unchanged** — model
  "done" as `Waiting`. Add a separate **`StateSource`** reason field rather than new
  enum variants, to avoid UI churn and honor the no-regression constraint.
- **Capture** the last tool name + timestamp in `meta` now and surface it
  **minimally** in the info overlay (the Phase 1 capture-but-render-lightly
  pattern). Rich per-session tool-activity timeline rendering is deferred to
  Phase 3.

### Transport & Fallback Labeling
- A single **`baude hook`** command chooses transport at runtime: **POST** the event
  to **`$BAUDE_EVENT_URL`** when that env var is set (the daemon seeds it into the
  session env at spawn), otherwise **append** to `/tmp/baude-events-<sid>.jsonl`
  (the TUI-local path). This realizes the "local hook transport via per-session
  event files; HTTP only in the daemon" decision.
- The daemon exposes **`POST /sessions/{id}/event`**: it resolves the baude session
  id → Claude `session_id`, feeds the event into the **same consume path** (append
  to the per-session `/tmp` file, tailed by `meta.rs`), and returns 204. One event
  model serves both transports.
- **Precedence** for state: hook events (freshest) **>** Claude session-file status
  **>** silence heuristic (labeled fallback). Hooks win when present and recent.
- Add a **`StateSource { Hook, SessionFile, Silence }`** label exposed on the status.
  When no hook events exist for a session (hooks disabled/unavailable) or events are
  stale, fall back to the **existing dual-source (session-file + silence) logic
  unchanged** — no regression from v0.6.1, and the fallback is labeled as such.

### Claude's Discretion
- Exact sentinel-marker shape, event-file offset/tailing details (mirror
  `read_transcript_tail()`), `StateSource` rendering location in the overlay, and
  staleness thresholds are at Claude's discretion, guided by existing `meta.rs`
  conventions.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude-core/src/bridge.rs` — `build_bridge(v: &Value) -> Value` pure-function +
  `window()` snake/camel merge helper; the `baude statusline` subcommand entrypoint
  (`run()`) is the model for a new `baude hook` subcommand. Tests are pure-function
  JSON fixtures (`bridge.rs:141-252`).
- `baude-core/src/meta.rs` — `ClaudeMeta::poll()` orchestrator (`117-143`);
  `read_transcript_tail()` (`197-227`) is the offset-tracked incremental JSONL tail
  pattern to copy for `/tmp/baude-events-<sid>.jsonl`; `read_bridge_file()`
  (`267-291`) tails `/tmp/baude-usage-<sid>.json`. Event tailing slots in after
  `read_bridge_file()`. Tests use temp-file helpers (`meta.rs:477-726`).
- `baude-core/src/session.rs` — `Status` enum (`16-23`), `Session::status()`
  dual-source logic (`107-122`), `waiting_for_ms()` (`124-130`), `BUSY_WINDOW_MS`
  silence threshold (`9-11`). `meta.claude_status: Option<(bool,u64)>` is the
  session-file status source; hook events feed a parallel, higher-precedence source.

### Established Patterns
- No file-watch loop — `poll_meta()` re-reads files each tick (~50ms TUI in
  `baude/src/main.rs`, ~1s daemon in `bauded/src/api.rs`). Event tailing follows the
  same poll-and-offset model.
- Untyped `serde_json::Value` accessors everywhere Claude data is read, for schema
  drift tolerance (snake/camel). No `#[derive(Deserialize)]` on Claude payloads.
- Tests live next to production code, no mocking; pure-function tests for transforms,
  temp-file tests for I/O.

### Integration Points
- **Spawn seeding:** `baude/src/app.rs:394-427` (`App::add_session()`) and
  `bauded/src/manager.rs:200-250` (`Manager::spawn()`) — seed
  `.claude/settings.local.json` and (daemon) set `$BAUDE_EVENT_URL` before
  `Pty::spawn()`.
- **Daemon route:** `bauded/src/api.rs:20-41` `router()` — add
  `POST /sessions/{id}/event` after the `stream()` route (~line 124). Sessions keyed
  by `u64` baude id; Claude `session_id` lives in `Session.meta.session_id`.
- **State consume:** `meta.rs` poll orchestrator + `session.rs::status()` precedence.

</code_context>

<specifics>
## Specific Ideas

- Pin the verified Claude Code hook schema in a comment (as `bridge.rs::window()`
  pins verified versions) — hook payload field names must tolerate drift.
- Never clobber a user's existing hooks or statusLine in `settings.local.json` —
  this is HOOK-01's explicit acceptance criterion and mirrors the hard-won
  statusLine-seeding care.
- `StateSource` must make the silence path observably "fallback" so a regression
  to silence-only is visible, not silent.

</specifics>

<deferred>
## Deferred Ideas

- Live per-session tool-activity timeline UI (PWA + TUI) — **Phase 3** (ACT).
- Remote permission approval / opt-in permission-prompt mode — **Phase 4**.

</deferred>
