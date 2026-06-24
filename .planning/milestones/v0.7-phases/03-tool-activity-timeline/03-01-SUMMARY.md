---
phase: 03-tool-activity-timeline
plan: 01
subsystem: api
tags: [rust, serde, vecdeque, ring-buffer, hook-events, claude-meta]

# Dependency graph
requires:
  - phase: 02-hook-event-stream
    provides: read_event_tail offset-tracked event-line tail + hook_status/last_tool/last_notification state and the WR-03 path-rotation reset
provides:
  - HookEvent struct ({event, tool?, notification_type?, ts}) — the one serde-Serialize type produced by ClaudeMeta
  - ACTIVITY_CAP const (200, drop-oldest)
  - ClaudeMeta.activity VecDeque<HookEvent> ring + activity() accessor
  - ring append/cap/rotation-reset wired into read_event_tail
affects: [03-02-daemon-activity-api, 03-04-tui-activity-overlay]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Typed serde row for the single client-facing event shape (HookEvent) while inbound parsing stays untyped serde_json::Value"
    - "Drop-oldest VecDeque ring bounded by a named const, appended inside the existing event-tail parse loop"

key-files:
  created: []
  modified:
    - baude-core/src/meta.rs

key-decisions:
  - "HookEvent gets Serialize+Deserialize; ClaudeMeta itself stays non-serializable (anti-pattern per RESEARCH:252)"
  - "Ring append reuses the SAME parsed Value/event guard the existing hook_status match keys on — no second parse pass"
  - "Task 1 (struct + serde) committed as one feat; Task 2 split test()→feat() for the ring logic per the planning brief preference"

patterns-established:
  - "Single source of truth ring buffer in ClaudeMeta — TUI reads directly, daemon serves from the ClaudeMeta it holds, no mirrored buffer"
  - "Event-derived ring cleared in the WR-03 rotation block alongside last_tool/hook_status/last_notification"

requirements-completed: [ACT-01]

# Metrics
duration: 12min
completed: 2026-06-15
---

# Phase 3 Plan 01: Tool-Activity Ring Buffer Summary

**Capped (200) drop-oldest `VecDeque<HookEvent>` ring on `ClaudeMeta`, appended by `read_event_tail` and cleared on session rotation — the single source of truth for the activity timeline.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-06-15T20:00Z
- **Completed:** 2026-06-15T20:12Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- Introduced `HookEvent` — the phase's one new typed struct ({event, tool?, notification_type?, ts}) with `skip_serializing_if` omitting `None` optionals and a verified serde round-trip.
- Added `ACTIVITY_CAP = 200` and a private `activity: VecDeque<HookEvent>` ring on `ClaudeMeta` with a public `activity()` accessor for downstream plans 02/04.
- Wired ring append + drop-oldest cap into `read_event_tail`'s existing per-line loop, reusing the same parsed `Value` and event guard (no second parse).
- Extended the WR-03 event-path-rotation block with `activity.clear()` so the ring never carries stale cross-session events.

## Task Commits

Each task committed atomically:

1. **Task 1: HookEvent struct + serde round-trip** - `2b42bd8` (feat)
2. **Task 2 (RED): failing tests for activity ring** - `9197cd4` (test)
3. **Task 2 (GREEN): activity ring append, cap, rotation-reset** - `dca7f65` (feat)

_Task 1's struct+serde landed as one feat (a serde test cannot compile without the type); Task 2 used the split test()→feat() cycle for the ring logic per the planning brief._

## Files Created/Modified
- `baude-core/src/meta.rs` - Added `VecDeque`/serde imports, `HookEvent` struct, `ACTIVITY_CAP`, `ClaudeMeta.activity` field + `activity()` accessor, ring append/cap in `read_event_tail`, `activity.clear()` in the WR-03 rotation block, and 7 new unit tests (3 serde + 4 ring).

## Decisions Made
- `HookEvent` is the only serde type; `ClaudeMeta` stays non-serializable (RESEARCH:252 anti-pattern).
- The ring append reads the same `v["event"]`/`v["tool"]`/`v["notification_type"]`/`ts` already parsed for the status match — one parse, one guard (event-less lines skipped identically).
- `VecDeque` import was added in Task 2 (not Task 1) to keep Task 1's commit clippy-clean under `-D warnings`.

## Deviations from Plan

None - plan executed exactly as written.

## Threat Model Compliance
- **T-03-01 (unbounded ring DoS):** mitigated — `ACTIVITY_CAP` drop-oldest cap; `activity_ring_caps_drop_oldest` proves cap=200 with oldest evicted.
- **T-03-02 (malformed/oversized line DoS):** mitigated — inherited untyped `Value`-or-skip + `as_u64().unwrap_or(0)`; `activity_ring_skips_malformed_lines` proves non-JSON and event-less lines are skipped without panic.
- **T-03-03 (stale cross-session events):** mitigated — `activity.clear()` in the WR-03 block; `activity_ring_clears_on_path_rotation` proves the ring reflects only the current session.

## Known Stubs
None.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `HookEvent`, `ACTIVITY_CAP`, and `ClaudeMeta.activity()` are public and consumable by:
  - Plan 02 (`Manager::activity`, `SessionInfo.activity`, `get_activity`/`activity-stream`, `?limit` clamp to `ACTIVITY_CAP`).
  - Plan 04 (`RemoteInfo.activity`, local TUI `Modal::Activity` overlay reading `s.meta.activity()`).
- CI triad green: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (94 tests across crates, no regressions).

## Self-Check: PASSED

- `baude-core/src/meta.rs` exists with `struct HookEvent`, `activity.push_back`, `activity.clear` (both key_link patterns present).
- Commits `2b42bd8`, `9197cd4`, `dca7f65` all present in git history.
- SUMMARY.md created.

---
*Phase: 03-tool-activity-timeline*
*Completed: 2026-06-15*
