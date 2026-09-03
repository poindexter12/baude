---
phase: 05-durable-repository-admission
plan: 03
subsystem: repository-orchestration
tags: [rust, git-worktree, persistence, tdd, session-lifecycle]

requires:
  - phase: 05-01
    provides: Canonical Git discovery, verified default resolution, and default-worktree ensure
  - phase: 05-02
    provides: Versioned repository aggregate, strict migration, and atomic persistence
provides:
  - Save-before-spawn local repository admission with stable primary dispatch
  - Fresh Git reconciliation before primary reuse, restart, or launch
  - Strict App and daemon persistence consumers that preserve malformed evidence
  - Converged launch-directory, Open/New, and clone-completion admission routes
affects: [phase-06-worktree-lifecycle, phase-07-local-hierarchy, phase-08-daemon-parity]

tech-stack:
  added: []
  patterns: [durable reservation before spawn, stable checkout-to-runtime association, fail-closed reconciliation, persistence-blocked lifecycle]

key-files:
  created: []
  modified: [baude/src/app.rs, baude-core/src/git.rs, bauded/src/manager.rs, bauded/src/api.rs]

key-decisions:
  - "Primary runtime dispatch is decided from durable active intent plus the runtime associated with a stable checkout key, never from a display name or cwd."
  - "A checkout authorizes reuse only after common directory, canonical path, full branch ref, and unlocked/non-prunable topology all reconcile."
  - "App and daemon load failures block saves and every subsequent process launch until the named state evidence is repaired."

patterns-established:
  - "Local admission follows discover, reserve, reconcile, validate, atomic save, then active-backend dispatch."
  - "Close removes only the primary runtime and clears active intent while retaining repository and checkout identity."

requirements-completed: [REPO-02, REPO-03, PERS-03]

duration: 14min
completed: 2026-08-30
---

# Phase 5 Plan 3: Durable Repository Admission Orchestration Summary

**Canonical local admission now atomically reserves one Git-reconciled default primary before active-backend spawn, then focuses, resumes, or retains it without duplication**

## Performance

- **Duration:** 14 min
- **Started:** 2026-08-30T17:43:39Z
- **Completed:** 2026-08-30T17:57:31Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- Added one durable local admission tracer spanning canonical discovery, verified default checkout selection, aggregate save, and active-workspace process dispatch.
- Reopening through launch directory, New/Open, clone completion, aliases, or linked worktrees converges on one primary checkout and one associated runtime.
- Added typed reconciliation for missing, changed, detached, locked, and prunable topology; stale persisted paths cannot authorize launch or mutation.
- Restore now migrates/loads hierarchy first, launches only active primaries, retains idle records on close, and blocks malformed state from being overwritten.
- Adapted daemon persistence to explicit Missing/Legacy/Current handling without adding hierarchy APIs or persisting backend identity.

## Task Commits

Each TDD task has a RED commit followed by its GREEN commit:

1. **Task 1 RED: primary dispatch and save-before-spawn tests** - `06f2c44` (test)
2. **Task 1 GREEN: durable idempotent primary admission** - `caace91` (feat)
3. **Task 2 RED: real-Git reconciliation matrix** - `c851844` (test)
4. **Task 2 GREEN: restore, close/reopen, and pre-launch reconciliation** - `1aca2a2` (feat)
5. **Task 3 RED: route convergence and daemon persistence tests** - `0ddb21d` (test)
6. **Task 3 GREEN: converged routes and strict daemon consumer** - `9cf4fbb` (feat)
7. **Validation fix: backend-independent Claude activity fixtures** - `3101b10` (fix)
8. **Security hardening: block daemon launches after load failure** - `df75e4e` (fix)

## Files Created/Modified

- `baude/src/app.rs` - Owns durable repository intent, checkout/runtime associations, admission, reconciliation, restore, retained close/reopen, and route convergence while preserving clipboard-wrap behavior from `fd1f8a6`.
- `baude-core/src/git.rs` - Adds deterministic managed-primary allocation plus typed fresh checkout reconciliation and real-Git tests.
- `bauded/src/manager.rs` - Safely consumes versioned state, migrates selected legacy sessions, blocks persistence and launch after load failure, and keeps backend selection spawn-time only.
- `bauded/src/api.rs` - Uses the backend-independent test metadata poll for existing activity API fixtures; production routes are unchanged.

## Decisions Made

- Keep runtime association in App/Manager maps keyed by persisted `CheckoutKey`; repository records remain UI- and PTY-free.
- Preserve daemon's existing flat API boundary while storing its sessions in the shared durable aggregate; hierarchy projection remains deferred to Phase 8.
- Treat locked or prunable observations as unavailable for launch even when path, common directory, and branch otherwise match.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Isolated Claude activity fixtures from the executor's active OpenCode workspace**
- **Found during:** Overall workspace verification
- **Issue:** Existing activity fixtures called backend-selected metadata polling, so `BAUDE_WORKSPACE=opencode` skipped their explicitly seeded Claude event files and failed two tests.
- **Fix:** Added a test-only deterministic Claude metadata poll and prevented real session metadata from replacing pinned fixture IDs.
- **Files modified:** `bauded/src/manager.rs`, `bauded/src/api.rs`
- **Verification:** Both focused activity tests and the full workspace suite pass with `BAUDE_WORKSPACE=opencode`.
- **Committed in:** `3101b10`

**2. [Rule 2 - Missing Critical] Blocked daemon create/restart after malformed state load**
- **Found during:** Final threat-model review of T-05-11
- **Issue:** The save guard preserved malformed bytes, but a later explicit daemon create or restart could still launch a process while durable ownership was blocked.
- **Fix:** Reject create and restart while `persistence_blocked`, with a regression assertion proving no process appears after malformed restore.
- **Files modified:** `bauded/src/manager.rs`
- **Verification:** `cargo test -p bauded manager_persistence` and the full CI triad pass.
- **Committed in:** `df75e4e`

---

**Total deviations:** 2 auto-fixed (1 Rule 1 bug, 1 Rule 2 missing critical safeguard)
**Impact on plan:** Both fixes strengthen deterministic verification and the planned fail-closed persistence boundary; no Phase 6-9 capability was added.

## Issues Encountered

- The inherited `BAUDE_WORKSPACE=opencode` environment exposed backend-sensitive Claude activity fixtures during the first full-suite run; the fixtures are now explicitly isolated.

## User Setup Required

None - no dependencies, manifests, external services, or configuration changes were added.

## TDD Gate Compliance

- RED `06f2c44` precedes GREEN `caace91` for Task 1.
- RED `c851844` precedes GREEN `1aca2a2` for Task 2.
- RED `0ddb21d` precedes GREEN `9cf4fbb` for Task 3.

## Verification

- `cargo test -p baude repository_admission -- --nocapture`
- `cargo test -p baude-core git::tests::reconciliation -- --nocapture`
- `cargo test -p baude admission_routes`
- `cargo test -p bauded manager_persistence`
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` — 238 tests passed (15 baude, 162 baude-core, 61 bauded).
- `Cargo.toml` and `Cargo.lock` are unchanged.

## Next Phase Readiness

- Phase 6 can build safe managed-worktree lifecycle operations on typed checkout health and fresh reconciliation.
- Phase 7 can project retained repository/checkout hierarchy without changing primary ownership semantics.
- No known stubs, blockers, or unplanned network/auth/schema threat surfaces remain.

## Self-Check: PASSED

- All four modified source files and this summary exist.
- RED/GREEN and validation commits `06f2c44`, `caace91`, `c851844`, `1aca2a2`, `0ddb21d`, `9cf4fbb`, `3101b10`, and `df75e4e` are present in Git history.

---
*Phase: 05-durable-repository-admission*
*Completed: 2026-08-30*
