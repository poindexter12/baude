---
phase: 02-hook-driven-status
plan: 01
subsystem: hooks
tags: [claude-code-hooks, serde_json, settings-merge, file-tail, idempotent-merge]

# Dependency graph
requires:
  - phase: 01-full-status-line-capture
    provides: bridge.rs pure-transform + /tmp per-session file conventions reused as the analog for hook.rs
provides:
  - "baude-core::hook module — build_event, merge_hook_settings, event_path, append_event, baude_hook_command"
  - "`baude hook` subcommand dispatch (stdin -> normalized event line -> POST or /tmp append, always exit 0)"
  - "TUI add_session seeds .claude/settings.local.json idempotently before claude starts"
  - "Event-line schema {schema:1, ts, session_id, event, tool, notification_type} consumed by Plans 02/03"
affects: [02-02-meta-event-tail, 02-03-daemon-event-endpoint, phase-04-perm]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Idempotent sentinel-guarded JSON merge (command string IS the sentinel)"
    - "Dual-transport runtime selection ($BAUDE_EVENT_URL presence) converging on one /tmp tail"
    - "Best-effort never-block hook posture (always exit 0)"

key-files:
  created:
    - baude-core/src/hook.rs
  modified:
    - baude-core/src/lib.rs
    - baude/src/main.rs
    - baude/src/app.rs

key-decisions:
  - "Command string is the idempotency sentinel — no extra marker field to drift; insert iff no group already has command == baude's command"
  - "Seed current_exe() absolute path + ` hook` (baude_hook_command), not bare `baude hook`, so the hook resolves regardless of session PATH (research A2)"
  - "event_path sanitizes `/` and `..` in session_id (defense-in-depth, T-02-01) — single file always directly under /tmp"
  - "TUI sessions get NO $BAUDE_EVENT_URL, routing the hook to the /tmp append path; only the daemon (Plan 03) injects the var"

patterns-established:
  - "Pattern 1: Idempotent sentinel-guarded hook merge into hooks.<event> arrays; sibling keys (statusLine/permissions/env) never touched"
  - "Pattern 2: `baude hook` chooses POST vs append at runtime; both transports converge on /tmp/baude-events-<sid>.jsonl"

requirements-completed: [HOOK-01, HOOK-03]

# Metrics
duration: 18min
completed: 2026-06-15
---

# Phase 2 Plan 1: Hook Foundation Summary

**A new `baude-core::hook` module (pure `build_event` + idempotent `merge_hook_settings` + `/tmp` file-tail helpers), a `baude hook` subcommand that normalizes Claude Code lifecycle-event stdin into one schema-1 event line and routes it POST-or-append (always exiting 0), and TUI session-spawn seeding of `.claude/settings.local.json` that never clobbers a user's statusLine or hooks.**

## Performance

- **Duration:** ~18 min
- **Started:** 2026-06-15T19:16:05Z
- **Completed:** 2026-06-15
- **Tasks:** 3 completed
- **Files modified:** 4 (1 created, 3 modified)

## Accomplishments
- Created `baude-core/src/hook.rs` (the keystone of HOOK-01 + the TUI-local half of HOOK-03), modeled structurally on `bridge.rs`: module doc pinning verified CLI 2.1.177, untyped `Value` accessors throughout, 10 unit tests covering every `<behavior>` case.
- Wired the `baude hook` dispatch arm in `baude/src/main.rs` before any TUI init, with runtime transport selection ($BAUDE_EVENT_URL POST vs /tmp append) and an unconditional `exit(0)`.
- Added `seed_session_hooks(cwd)` to `app.rs::add_session`, seeding the session cwd's `.claude/settings.local.json` idempotently before `Pty::spawn` — best-effort so a seeding failure never aborts the spawn.

## Task Commits

Each task was committed atomically:

1. **Task 1: Create baude-core/src/hook.rs (build_event, merge_hook_settings, event_path, append_event)** - `04858bb` (feat)
2. **Task 2: Add `baude hook` dispatch arm to baude/src/main.rs** - `8d38816` (feat)
3. **Task 3: Seed .claude/settings.local.json before Pty::spawn in app.rs::add_session** - `43399d0` (feat)

_Note: Tasks 1 and 2 are TDD-typed tasks for fresh pure code; following the `bridge.rs` analog precedent (impl + `mod tests` ship together in one file), each was committed as a single `feat` commit with its full test coverage rather than split RED/GREEN commits. See TDD Gate Compliance below._

## Files Created/Modified
- `baude-core/src/hook.rs` (created) - `build_event` (pure Value->Value normalizer), `merge_hook_settings` (idempotent sentinel-guarded merge), `event_path` (/tmp path with `/`/`..` sanitization), `append_event` (O_APPEND writer), `baude_hook_command` (current_exe abs path + ` hook`), plus 10 unit tests.
- `baude-core/src/lib.rs` (modified) - `pub mod hook;` added alphabetically after `git`, before `meta`.
- `baude/src/main.rs` (modified) - `baude hook` dispatch arm before TUI init: stdin -> build_event -> POST-or-append -> exit(0).
- `baude/src/app.rs` (modified) - `seed_session_hooks(cwd)` helper + call before `Pty::spawn` in `add_session`.

## Key Behaviors Delivered (HOOK-01 / HOOK-03)
- `build_event`: maps all four events; carries `tool` (PostToolUse) and `notification_type` (Notification); empty `{}` never panics; `schema:1` + `ts` always present.
- `merge_hook_settings`: preserves a user `statusLine` byte-intact AND a user's own `PostToolUse` hook; adds baude's entry for all four events; applied twice -> exactly one baude entry per event (idempotent); minimal/non-object/`{"hooks":5}`/non-array-event inputs never panic.
- `append_event`: O_APPEND, two calls -> two lines in `/tmp/baude-events-<sid>.jsonl`.
- `baude hook`: $BAUDE_EVENT_URL set -> ureq POST; unset + non-empty session_id -> append; always `exit(0)`.

## Deviations from Plan

None — plan executed exactly as written. No bugs, missing functionality, or blocking issues encountered (Rules 1-4 not triggered). No new dependencies introduced (matches RESEARCH Package Legitimacy Audit: none).

## TDD Gate Compliance

Tasks 1 and 2 carry `tdd="true"`. For fresh pure modules the plan explicitly points to the `bridge.rs` analog, whose convention is to ship implementation and `#[cfg(test)] mod tests` together in a single file/commit (no separate RED commit lands in history). Accordingly the git log shows `feat(02-01)` commits with full test coverage rather than distinct `test(02-01)` RED commits. The behavior contract was nonetheless test-first authored and every `<behavior>` bullet has a corresponding passing assertion. Per the plan-level gate-sequence note, this divergence from the strict `test(...)` -> `feat(...)` commit ordering is recorded here as a warning for traceability; functional coverage is complete (10/10 hook:: tests green).

## Threat Model Notes
- **T-02-01** (path injection via `session_id`): mitigated — `event_path` replaces `..` and `/` before formatting; verified by `event_path_sanitizes_traversal`.
- **T-02-02** (untrusted stdin DoS): mitigated — untyped `Value` accessors + `unwrap_or(json!({}))` in the dispatch arm; `baude hook` always `exit(0)`.
- **T-02-04** (clobbering user statusLine/hooks): mitigated — merge only `.entry().or_insert()` into `hooks.<event>` arrays; verified by `merge_preserves_user_statusline_and_user_hook`.

No new security surface beyond the threat model. No threat flags.

## Verification
- `cargo test -p baude-core hook::` — 10/10 green.
- `cargo build -p baude` — clean.
- `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` — full CI triad green (workspace: 29 daemon + core + tui tests pass, clippy clean, fmt clean).

## Next Plan Dependencies
- Plan 02-02 (`meta.rs` event tail) consumes the `{schema:1, ts, session_id, event, tool, notification_type}` line schema and the `hook::event_path` seam to tail `/tmp/baude-events-<sid>.jsonl`.
- Plan 02-03 (daemon `POST /sessions/{id}/event`) mirrors the same `merge_hook_settings` call (with `$BAUDE_EVENT_URL` injection) and reuses `hook::append_event` for ingest.

## Self-Check: PASSED

- FOUND: baude-core/src/hook.rs
- FOUND: .planning/phases/02-hook-driven-status/02-01-SUMMARY.md
- FOUND commit 04858bb (Task 1)
- FOUND commit 8d38816 (Task 2)
- FOUND commit 43399d0 (Task 3)
