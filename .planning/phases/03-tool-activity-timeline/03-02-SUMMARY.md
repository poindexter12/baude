---
phase: 03-tool-activity-timeline
plan: 02
subsystem: api
tags: [rust, axum, sse, serde, file-tail, hook-events, daemon-rest]

# Dependency graph
requires:
  - phase: 03-tool-activity-timeline
    provides: HookEvent struct, ACTIVITY_CAP const, ClaudeMeta.activity() ring accessor (03-01)
provides:
  - GET /sessions/{id}/activity → Json<Vec<HookEvent>> with ?limit clamped to ACTIVITY_CAP, 404 on unknown id
  - GET /sessions/{id}/activity-stream → standalone SSE channel offset-tailing /tmp/baude-events-<sid>.jsonl
  - Manager::event_path(id) + Manager::activity(id, limit) accessors (Err → 404 upstream)
  - SessionInfo.activity (bounded ~30) riding the existing /sessions poll
  - transcript::EventTail (HookEvent-yielding tail) + parse_event_line
affects: [03-03-pwa-activity-strip, 03-04-tui-activity-overlay]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Second standalone SSE channel cloned from stream() with three swaps (event_path / EventTail / no Event::id)"
    - "Distinct EventTail type so the event channel can never be wired to the ChatMessage Tail (Pitfall 1)"
    - "Security V5 limit clamp: q.limit.unwrap_or(ACTIVITY_CAP).min(ACTIVITY_CAP)"

key-files:
  created: []
  modified:
    - bauded/src/transcript.rs
    - bauded/src/manager.rs
    - bauded/src/notify.rs
    - bauded/src/api.rs

key-decisions:
  - "EventTail is a distinct type copying Tail's offset machinery; only the per-line parse target differs (parse_event_line vs parse_line) — Pitfall 1"
  - "activity_stream sets no Event::id (hook events have no uuid) — append-only ordering + PWA GET-then-buffer dedup (Pitfall 2)"
  - "Task 1 split test()/feat() not used: EventTail's tests can't compile without the type, so Task 1 landed as one feat with co-located tests; Task 2 likewise (route/handler + tests)"
  - "Task-1 accessors/EventTail carried a scoped #[allow(dead_code)] removed in Task 2 once wired (mirrors the 03-01 deferred-import precedent for clippy-clean per-task commits)"

patterns-established:
  - "Daemon serves the activity feed two ways: snapshot (GET /activity, ring-backed) + live (GET /activity-stream, file-tail) — independent of the chat /stream"
  - "SessionInfo bundles a bounded recent set so the remote TUI overlay needs no extra round-trip"

requirements-completed: [ACT-02]

# Metrics
duration: 5min
completed: 2026-06-15
---

# Phase 3 Plan 02: Daemon Activity API Summary

**The server half of the activity feed: a ring-backed `GET /sessions/{id}/activity` JSON endpoint (clamped `?limit`) and a standalone `GET /sessions/{id}/activity-stream` SSE channel that offset-tails the on-disk hook-event file via a dedicated `HookEvent` tail — never the ChatMessage `Tail`.**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-06-15T20:48Z
- **Completed:** 2026-06-15T20:53Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Added `transcript::EventTail` + `parse_event_line`: copies the `Tail::read_new` offset/truncation-reset/complete-lines-only machinery verbatim and swaps ONLY the per-line parse to yield `HookEvent` (Pitfall 1 — the ChatMessage `Tail` would yield zero hook events).
- Added `Manager::event_path(id)` (analog of `transcript_path`, sid sanitized via `baude_core::hook::event_path`) and `Manager::activity(id, limit)` (reads the `ClaudeMeta` ring, newest-at-back slice). Both route Err → 404 via `self.session(id)?`.
- Added bounded (`~30`) `SessionInfo.activity` populated in `session_info()` so the remote TUI overlay rides the existing `/sessions` poll.
- Fixed the `notify.rs` `#[cfg(test)]` `SessionInfo` constructor with `activity: vec![]` (Pitfall 3 — blocking compile fix for `cargo test -p bauded`, the exact 02-03 Rule-3 precedent).
- Added `GET /sessions/{id}/activity` (`get_activity` + `ActivityQuery`) with the `?limit` clamp `unwrap_or(ACTIVITY_CAP).min(ACTIVITY_CAP)` (Security V5 / T-03-06), and `GET /sessions/{id}/activity-stream` (`activity_stream`, a near-verbatim `stream()` clone with the three swaps: `event_path` guard+loop, `EventTail`, no `Event::id` for Pitfall 2). Both routes registered next to `/stream` and `/event`.

## Task Commits

Each task committed atomically:

1. **Task 1: event-line tail + Manager accessors + SessionInfo.activity + notify fix** — `fcddbf5` (feat)
2. **Task 2: GET /activity + /activity-stream routes and handlers** — `f278c8b` (feat)

_Both tasks landed as single `feat` commits with co-located tests: a tail/handler cannot compile separately from the tests that exercise it. Task 1's `EventTail`/accessors carried a scoped `#[allow(dead_code)]` (clippy-clean under `-D --all-targets`) that Task 2 removed once they were wired into the SSE handler._

## Files Created/Modified
- `bauded/src/transcript.rs` — Added `use baude_core::meta::HookEvent`, `parse_event_line()`, the `EventTail` type + `end_of`/`read_new`, and two unit tests (mixed/malformed/offset-advance/truncation-reset + `end_of` skips history).
- `bauded/src/manager.rs` — Imported `HookEvent`; added `event_path()` + `activity()` accessors; added `SessionInfo.activity` field + bounded `~30` population in `session_info()`; added two tests (`event_path` per-sid + 404, `activity` recent slice + `SessionInfo.activity` + 404).
- `bauded/src/notify.rs` — Added `activity: vec![]` to the test `SessionInfo` constructor (Pitfall 3).
- `bauded/src/api.rs` — Imported `HookEvent`/`ACTIVITY_CAP`/`EventTail`; added `ActivityQuery`, `get_activity`, `activity_stream`; registered both routes; added two integration tests (activity JSON + `?limit` clamp + 404; activity-stream content-type guard + 404).

## Decisions Made
- `EventTail` is a distinct type (not a generic over `Tail`) so the activity channel can never be wired to the transcript parse by accident (Pitfall 1 is structural, not just a comment).
- `activity_stream` drops `.id(...)` entirely rather than synthesizing a counter — the PWA does GET-then-buffer dedup and the channel is append-only (Pitfall 2). No `ts`-based dedup.
- The SSE integration test asserts route existence + content-type + the 404 guard (not a full live-tail), since the live-tail behavior is fully covered by the `EventTail` unit test and is otherwise a PWA UAT in plan 03.

## Deviations from Plan

None — plan executed exactly as written. (The `#[allow(dead_code)]` on the Task-1 accessors is the planned per-task-commit clippy hygiene, removed in Task 2; not a deviation.)

## Threat Model Compliance
- **T-03-05 (path injection via session id):** mitigated — `event_path` resolves through `baude_core::hook::event_path` (replaces `..`/`/`); `Path<u64>` rejects non-numeric ids at the framework layer. `event_path_resolves_per_sid_and_404s_unknown` proves the path matches the sanitized form.
- **T-03-06 (unbounded ?limit DoS):** mitigated — `q.limit.unwrap_or(ACTIVITY_CAP).min(ACTIVITY_CAP)` clamp; `activity_returns_events_clamps_limit_and_404s_unknown` proves `?limit=100000` returns the bounded set with no 500.
- **T-03-07 (malformed/oversized event line on the SSE tail):** mitigated — `parse_event_line` is untyped `Value`-or-skip, complete-lines-only, truncation-reset; never panics. `event_tail_yields_hook_events_skips_malformed_and_advances` proves non-JSON and event-less lines are skipped.
- **T-03-08 (unbounded SSE growth):** mitigated — `activity_stream` offset-tails one file at `STREAM_POLL_MS` and ends via the `Err(_) => break` guard on session deletion (same lifecycle as `stream()`).
- **T-03-04 (unauthenticated GET reachable):** accepted — inherits the project loopback/tailnet bind; no new exposure beyond the existing REST surface.

## Known Stubs
None. The activity-stream live-tail is deliberately asserted at the route/guard level (full tail covered by the `EventTail` unit test + plan-03 PWA UAT) — this is documented test scoping, not a stub.

## Issues Encountered
- CI `clippy --all-targets -D warnings` flagged the Task-1 `EventTail` methods and `Manager::event_path`/`activity` as dead code (they are only consumed by Task 2's SSE handler). Resolved with a scoped `#[allow(dead_code)]` in the Task-1 commit, removed in Task 2 — the planned clippy hygiene, mirroring 03-01's deferred `VecDeque` import.

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- Plan 03 (PWA): `GET /sessions/{id}/activity?limit=30` (GET-then-SSE backfill) and `GET /sessions/{id}/activity-stream` (EventSource) are live; `HookEvent` serializes as `{event, tool?, notification_type?, ts}`.
- Plan 04 (TUI): `SessionInfo.activity` (bounded ~30) rides `/sessions`; `RemoteInfo.activity` can deserialize it with `#[serde(default)]`.
- CI triad green: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (100 tests, no regressions).

## Self-Check: PASSED

---
*Phase: 03-tool-activity-timeline*
*Completed: 2026-06-15*
