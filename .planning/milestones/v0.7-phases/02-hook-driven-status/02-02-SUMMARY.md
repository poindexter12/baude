---
phase: 02-hook-driven-status
plan: 02
subsystem: api
tags: [claude-code-hooks, serde_json, file-tail, offset-tracking, state-precedence]

# Dependency graph
requires:
  - phase: 02-hook-driven-status (Plan 01)
    provides: "hook::event_path + {schema:1, ts, session_id, event, tool, notification_type} event-line schema; /tmp/baude-events-<sid>.jsonl append transport"
provides:
  - "ClaudeMeta::read_event_tail — offset-tracked JSONL tail of the hook event stream feeding hook_status/last_tool/last_notification"
  - "ClaudeMeta fields: hook_status, last_tool, last_notification (+ private offset_events)"
  - "session.rs StateSource{Hook,SessionFile,Silence} + status_with_source() precedence (Hook>SessionFile>Silence) + HOOK_FRESH_MS staleness guard"
  - "decide_status pure precedence helper (unit-testable without a live Pty)"
affects: [02-03-daemon-event-endpoint, phase-03-act-timeline, phase-04-perm]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Offset-tracked incremental JSONL tail with a per-stream offset field (offset_events distinct from the transcript offset)"
    - "Pure precedence helper (decide_status) takes raw inputs so tiers are tested without constructing a live Pty/Session"
    - "Prepend-only no-regression edit: new highest-precedence branch added ahead of byte-identical legacy branches"

key-files:
  created: []
  modified:
    - baude-core/src/meta.rs
    - baude-core/src/session.rs

key-decisions:
  - "HOOK_FRESH_MS = 5000ms staleness window — a fresh UserPromptSubmit/Stop stays authoritative against the polled session file; a long-dead event falls through. Tunable; wrong value only causes brief mislabel (research A3)"
  - "Extracted a pure decide_status(exited, hook_status, now_unix, claude_status, last_output_ms, now_mono) helper so all precedence tiers (incl. exited + silence) are unit-tested without spawning a real Pty — session.rs had no test module and Pty only constructs via spawn()"
  - "feed_events test helper truncates on first feed (meta has no session yet) and appends after, so single-event tests are isolated and multi-tick offset tests still accumulate"
  - "last_notification captures (notification_type, ts) in meta only — NOT a structured SessionInfo field this phase (deferred to Phase 4 waiting_reason, research OQ#3)"

patterns-established:
  - "Pattern: offset_events-tracked event tail mirroring read_transcript_tail (complete-lines-only, partial trailing line deferred, malformed line skipped, never panics)"
  - "Pattern: decide_status pure precedence core delegated to by both status() and status_with_source(), keeping the public Status API total and call sites untouched"

requirements-completed: [HOOK-02]

# Metrics
duration: 6min
completed: 2026-06-15
---

# Phase 2 Plan 2: Hook-Driven State Derivation Summary

**Session working/waiting/done now derives from the Claude Code hook event stream via an offset-tracked `read_event_tail`, layered into `session.rs` behind a `StateSource{Hook,SessionFile,Silence}` precedence (Hook>SessionFile>Silence) with a `HOOK_FRESH_MS` staleness guard, while the v0.6.1 silence fallback stays byte-identical (no regression).**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-06-15T19:24:03Z
- **Completed:** 2026-06-15
- **Tasks:** 3 completed
- **Files modified:** 2 (0 created, 2 modified)

## Accomplishments
- Added `read_event_tail` to `meta.rs`: an offset-tracked incremental JSONL tail (separate `offset_events`, never touching the transcript `offset`) that maps `UserPromptSubmit/PostToolUse->Busy` and `Stop/Notification->Waiting`, records `last_tool` (PostToolUse) and `last_notification` (notification_type), wired into `poll()` immediately after `read_bridge_file()`.
- Added `StateSource{Hook,SessionFile,Silence}` + `status_with_source()` to `session.rs` with a fresh-hook branch prepended ahead of the byte-identical `claude_status` and silence branches; `status()` now delegates to `status_with_source().0`, keeping the public signature and all call sites unchanged.
- Filled the Wave-0 gap: `session.rs` had no `#[cfg(test)] mod tests` — added the scaffold (Task 0) then 9 precedence tests including an explicit silence no-regression assertion.

## Task Commits

Each task was committed atomically:

1. **Task 0: Add mod tests scaffold to session.rs** - `5c0c432` (test)
2. **Task 1: read_event_tail + hook_status/last_tool/last_notification/offset_events in meta.rs** - `4a3a3c9` (feat)
3. **Task 2: StateSource + status_with_source precedence in session.rs** - `4701530` (feat)

**Plan metadata:** committed separately (docs: complete plan).

_Note: Tasks 1 and 2 carry `tdd="true"`. See TDD Gate Compliance below — both ship impl + full `mod tests` coverage in a single `feat` commit, following the `bridge.rs`/`hook.rs` (Plan 01) precedent for fresh code that lives in one file._

## Files Created/Modified
- `baude-core/src/meta.rs` (modified) - Added `hook_status`/`last_tool`/`last_notification`/`offset_events` fields to `ClaudeMeta`; `read_event_tail` (offset-tracked tail copying `read_transcript_tail`'s seek/rfind/complete-lines machinery, untyped `Value`, event->state match); `poll()` calls it after `read_bridge_file()`; `feed_events` test helper + 6 tests (state mapping, notification capture, last_tool, offset no-reprocess, partial-line defer, malformed-line skip).
- `baude-core/src/session.rs` (modified) - `HOOK_FRESH_MS` const; `StateSource` enum; `decide_status` pure precedence helper; `status_with_source()`; `status()` delegating to `.0`; `mod tests` scaffold (Task 0) + 9 precedence tests.

## Decisions Made
- **HOOK_FRESH_MS = 5000ms** for the hook staleness window (research A3 — tunable, flicker-only-if-wrong).
- **Pure `decide_status` helper** extracted so precedence tiers (including the `exited` and silence branches that depend on `self.claude`/`Pty`) are unit-tested without spawning a live shell-backed `Pty`. `status_with_source` reads the six raw inputs off `self` and delegates.
- **`feed_events` truncates on the first feed, appends after** — isolates single-event tests while letting the offset test accumulate across ticks against a deterministic pid+suffix-keyed file.
- **`last_notification` stays in `meta` only** — no structured `SessionInfo` waiting_reason field this phase (deferred to Phase 4).

## Deviations from Plan

None - plan executed exactly as written. No bugs, missing functionality, or blocking issues (deviation Rules 1-4 not triggered). No new dependencies (matches the threat register's `accept` disposition for installs: none this phase).

The plan permitted extending `waiting_for_ms` to follow hooks "if a test reveals the waiting clock must follow hooks — but default to leaving it." No test required it; left unchanged per the no-regression-safe default.

## TDD Gate Compliance

Tasks 1 and 2 carry `tdd="true"`. `session.rs` had **no** test module before this plan (the Wave-0 gap closed by Task 0), so the precedence tests are net-new code authored test-first against the intended `decide_status`/`status_with_source` API. Following the `bridge.rs`/`hook.rs` analog the plan points to — whose convention is to ship implementation and `#[cfg(test)] mod tests` together in one file/commit — Tasks 1 and 2 each landed as a single `feat(02-02)` commit carrying full `<behavior>`-mapped coverage rather than split `test(...)` RED then `feat(...)` GREEN commits. Task 0 itself is a `test(02-02)` commit (the scaffold). Every `<behavior>` bullet has a corresponding passing assertion (6 meta:: event tests + 9 session:: precedence tests). This divergence from strict RED->GREEN commit ordering is recorded here for traceability per the plan-level gate note; functional coverage is complete.

## Threat Model Notes
- **T-02-05** (untrusted event-line DoS): mitigated — `read_event_tail` uses untyped `Value` accessors, `as_u64().unwrap_or(0)` for `ts`, `continue`s on a malformed line, and never panics. Verified by `event_tail_skips_malformed_line`.
- **T-02-06** (stale hook pinning wrong state): mitigated — `HOOK_FRESH_MS` guard in `decide_status` falls through to SessionFile/Silence when `now_unix - at >= HOOK_FRESH_MS`. Verified by `stale_hook_falls_through_to_session_file`.
- **T-02-07** (silence fallback altered during refactor): mitigated — prepend-only edit; the `claude_status` and silence branches are byte-identical to v0.6.1. Verified by `no_hook_silence_is_byte_identical_to_v0_6_1`.

No new security surface beyond the threat model. No threat flags.

## Issues Encountered
None. The pure-helper design sidestepped the only real friction (constructing a live `Pty` in a unit test).

## User Setup Required
None - no external service configuration required.

## Verification
- `cargo test -p baude-core meta::` — 13/13 green (6 new event-tail tests).
- `cargo test -p baude-core session::` — 10/10 green (9 precedence/no-regression tests + smoke).
- `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` — full CI triad green (workspace: baude-core 42, bauded 29; clippy clean with -D warnings; fmt clean).

## Next Phase Readiness
- `StateSource` is computed in core and ready to surface in the TUI overlay / daemon `SessionInfo` (a `source_str` mapper per PATTERNS Pattern remains; rendering location is at the consumer's discretion).
- Plan 02-03 (daemon `POST /sessions/{id}/event`) reuses `hook::append_event` to feed the same `/tmp` file this plan tails — the consume path is now complete end-to-end once the daemon ingest route lands.
- `last_tool`/`last_notification` are captured in `meta` for Phase 3 (tool-activity timeline) and Phase 4 (permission waiting_reason) without any structured `SessionInfo` field added prematurely.

## Self-Check: PASSED

- FOUND: baude-core/src/meta.rs
- FOUND: baude-core/src/session.rs
- FOUND: .planning/phases/02-hook-driven-status/02-02-SUMMARY.md
- FOUND commit 5c0c432 (Task 0)
- FOUND commit 4a3a3c9 (Task 1)
- FOUND commit 4701530 (Task 2)

---
*Phase: 02-hook-driven-status*
*Completed: 2026-06-15*
