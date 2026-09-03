---
phase: 06-safe-managed-worktree-lifecycle
plan: 07
subsystem: lifecycle
tags: [rust, state-machine, persistence, pty, process-ownership, git-worktree]

requires:
  - phase: 06-safe-managed-worktree-lifecycle
    provides: execution-history activation, close, reopen, removal, and recovery primitives from plans 06-01 through 06-06
provides:
  - One core lifecycle reducer and effect driver shared by App and Manager
  - Strict schema-v2 lifecycle migration with derived activity and health views
  - Durable exact agent and optional shell ownership before PTY release
  - Mirrored adapter traces for success and injected persistence/effect failures
affects: [phase-07-local-tui-dogfood, lifecycle-review, linux-certification]

tech-stack:
  added: []
  patterns: [opaque durable candidates, write-ahead lifecycle effects, registered PTY release, generation-gated runtime ownership]

key-files:
  created: []
  modified:
    - baude-core/src/repository.rs
    - baude-core/src/lifecycle.rs
    - baude-core/src/persist.rs
    - baude-core/src/session.rs
    - baude-core/src/pty.rs
    - baude/src/app.rs
    - bauded/src/manager.rs
    - bauded/src/api.rs
    - .planning/phases/06-safe-managed-worktree-lifecycle/06-VALIDATION.md

key-decisions:
  - "LifecycleCandidate remains opaque: only the core engine selects lifecycle and ownership state; adapters may only apply and persist it."
  - "Pre-replacement persistence failure leaves the original runtime live, while a committed replacement continues the authorized effect and records dirty durability."
  - "App and Manager restarts use the same registered PTY lifecycle path as first launch rather than replacing process handles directly."
  - "Legacy activity and health fields are normalized from authoritative lifecycle during migration instead of preserving contradictory projections."

patterns-established:
  - "Persist before effect: every launch, release, stop, and extinction boundary is selected by LifecycleEngine."
  - "Exact ownership: agent and optional shell ProcessIdentity are durable before a paused PTY is released."
  - "Adapter parity: App and Manager execute independent adapters against the same canonical vectors and normalized traces."

requirements-completed: []

duration: 1h 9m
completed: 2026-08-30
---

# Phase 6 Plan 7: Shared Lifecycle Core Refactor Summary

**One durable lifecycle engine now governs App and Manager persistence, Git/process effects, exact PTY ownership, restart, close, removal, rollback, and recovery behavior.**

## Performance

- **Duration:** 1h 9m
- **Started:** 2026-08-31T05:15:50Z
- **Completed:** 2026-08-31T06:24:43Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments

- Replaced independently mutable checkout activity/health authority with strict schema-v2 `CheckoutLifecycle`, `OwnedRuntime`, and derived views.
- Added the sole core transition/effect engine, canonical adapter vectors, normalized traces, and idempotent process/removal-first recovery.
- Persisted exact agent and optional shell identities before releasing registered PTYs, including replacement/restart paths.
- Cut App and Manager lifecycle transactions over to the shared engine while retaining real-Git activation/removal and flat daemon HTTP status behavior.
- Passed the full local macOS gate with 322 workspace tests and no formatting, Clippy, manifest, or lockfile changes.

## Task Commits

Each task was committed atomically with TDD gates:

1. **Task 1: Define strict durable lifecycle and sole transition authority**
   - `987ac8c` — RED tests
   - `06c2076` — GREEN implementation
2. **Task 2: Enforce exact ownership, registration gate, and recovery**
   - `dc0978f` — RED tests
   - `6d9ebb4` — GREEN implementation
3. **Task 3: Cut both owners over to the shared protocol**
   - `9c01bf4` — initial RED mirrored vectors
   - `c97bbaf` — initial GREEN adapter semantics
   - `507e083` — strengthened RED independent failure traces
   - `7dad443` — final GREEN production cutover

## Files Created/Modified

- `baude-core/src/repository.rs` — strict lifecycle authority, runtime generations, exact owned-runtime data, derived views, and validation.
- `baude-core/src/lifecycle.rs` — legal transition table, opaque candidates, effect engine, canonical vectors, recovery, and removal transitions.
- `baude-core/src/persist.rs` — schema-v1 to schema-v2 normalization and replacement-commit-aware errors.
- `baude-core/src/session.rs` — exact two-process snapshots and whole-group teardown evidence.
- `baude-core/src/pty.rs` — private pre-exec registration gate and durable-before-release spawning.
- `baude-core/src/git.rs` — lifecycle fixture compatibility.
- `baude/src/app.rs` — production App adapter and engine-routed create/restart/close/removal flows.
- `bauded/src/manager.rs` — production Manager adapter with equivalent registered runtime and persistence behavior.
- `bauded/src/api.rs` — compatibility assertions through private lifecycle views.
- `.planning/phases/06-safe-managed-worktree-lifecycle/06-VALIDATION.md` — observed local evidence; certification remains pending.

## Decisions Made

- Core owns every legal transition and persistence/effect order; owners cannot write checkout lifecycle projections directly.
- Atomic rename failure before replacement preserves the existing live runtime. Directory-sync failure after replacement retains the committed state, performs the authorized effect, and surfaces dirty persistence.
- Runtime restart is a lifecycle launch, not an in-place PTY handle swap without durable ownership.
- Migration intentionally derives compatibility views from lifecycle authority, preventing malformed legacy combinations from becoming launchable.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Corrected persistence-boundary rollback expectations**
- **Found during:** Task 3 full App and Manager test runs
- **Issue:** Pre-replacement failures were still expected to kill and recreate an already valid runtime, while the engine correctly refused the effect before durable authorization.
- **Fix:** Preserved the original agent/shell on pre-replacement failure and updated regression assertions; committed replacements still stop the runtime and mark persistence dirty.
- **Files modified:** `baude/src/app.rs`, `bauded/src/manager.rs`
- **Verification:** Full workspace tests and atomic failure matrix passed.
- **Committed in:** `7dad443`

**2. [Rule 1 - Bug] Normalized contradictory migrated lifecycle projections**
- **Found during:** Task 3 full core test run
- **Issue:** Legacy migration could combine a protected lifecycle with `active_intent: true`, causing strict schema-v2 validation failures.
- **Fix:** Constructed migrated checkouts through the lifecycle-authoritative constructor so health and activity are derived consistently.
- **Files modified:** `baude-core/src/persist.rs`, `baude-core/src/lifecycle.rs`
- **Verification:** Migration, round-trip, non-UTF8, and atomic replacement tests passed.
- **Committed in:** `7dad443`

**3. [Rule 2 - Missing Critical] Routed replacement restarts through durable registration**
- **Found during:** Task 3 final authority audit
- **Issue:** App and Manager manual/exited-session restart helpers could replace a PTY directly without persisting the successor identity before release.
- **Fix:** Routed tracked restarts through reopen plus registered `LaunchRegistered`/`LaunchReleased` events and removed obsolete owner-local rollback helpers.
- **Files modified:** `baude/src/app.rs`, `bauded/src/manager.rs`
- **Verification:** Restart, activation reuse, Clippy, and full workspace tests passed.
- **Committed in:** `7dad443`

---

**Total deviations:** 3 auto-fixed (2 Rule 1 bugs, 1 Rule 2 missing critical functionality)
**Impact on plan:** All changes were required to enforce the planned lifecycle authority and exact ownership contract; no feature scope was added.

## Issues Encountered

- The initial Task 3 adapter implementation produced canonical values without exercising independent adapters. A second RED gate (`507e083`) required independent persistence/effect failure traces before the production cutover.
- Rust analyzer reported stale unmatched-brace diagnostics near file ends during edits; Cargo formatting, compilation, Clippy, and tests were authoritative and all passed.

## Local Gate Evidence

- Exact App and Manager canonical vector list/assert/run commands passed.
- Exact real-Git activation and removal regression commands passed.
- Exact flat daemon atomic-persistence HTTP compatibility command passed.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --all-targets -- -D warnings` passed.
- `cargo test` passed: 32 App + 212 core + 78 daemon tests.
- `git diff --exit-code -- Cargo.toml Cargo.lock` passed.

## TDD Gate Compliance

- RED and GREEN commits exist in order for all three tasks.
- Task 3 added a second RED commit after architectural review showed the initial mirrored test was tautological, followed by the final production GREEN commit.

## Known Stubs

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Local source implementation is complete and ready for Linux/runtime certification and an independent deep lifecycle review.
- Phase 6 remains intentionally incomplete. Nyquist approval, phase verification, CORE-01 through CORE-06 requirement checkoff, and Phase 6 completion wait for the pending certification gates documented in `06-VALIDATION.md`.
- No push, PR, publication, or release action was performed.

## Self-Check: PASSED

- All key modified files exist.
- All eight RED/GREEN task commits exist in repository history.
- Local validation evidence is recorded without claiming pending external certification.

---
*Phase: 06-safe-managed-worktree-lifecycle*
*Completed: 2026-08-30*
