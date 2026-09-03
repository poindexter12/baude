---
phase: 05-durable-repository-admission
plan: 02
subsystem: persistence
tags: [rust, serde, atomic-write, migration, repository-state]

requires: []
provides:
  - Workspace-scoped durable repository and checkout aggregate with opaque monotonic keys
  - Lossless Unix pathname persistence and complete retained legacy session state
  - Strict selected-source migration with typed blocking load outcomes
  - Sibling-temp atomic state replacement with failure preservation
affects: [05-03, repository-admission, daemon-persistence, session-restore]

tech-stack:
  added: []
  patterns: [validated durable aggregate, strict versioned envelope, selected-source migration, atomic sibling replacement]

key-files:
  created: [baude-core/src/repository.rs]
  modified: [baude-core/src/persist.rs, baude-core/src/lib.rs]

key-decisions:
  - "Repository and checkout keys are persisted monotonic u64 newtypes scoped to one workspace state file."
  - "Legacy migration accepts reconciled identity as an injected value and never infers baude ownership from is_worktree."
  - "Only NotFound is first-run state; malformed, unsupported, unreadable, and invalid aggregates are path-aware blocking errors."

patterns-established:
  - "PersistedPath copies Unix OsStr bytes directly and reconstructs PathBuf without UTF-8 conversion."
  - "State replacement serializes and validates before create_new, write, flush, sync, close, and same-directory rename."

requirements-completed: [PERS-01, PERS-02, PERS-04]

duration: 14min
completed: 2026-08-30
---

# Phase 5 Plan 2: Durable Repository State Summary

**Validated repository intent with byte-exact Unix paths, one-time selected-source legacy migration, typed corruption failures, and synced atomic replacement**

## Performance

- **Duration:** 14 min
- **Started:** 2026-08-30T17:25:24Z
- **Completed:** 2026-08-30T17:38:56Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Added a UI-free repository aggregate that independently retains ownership, role, health, branch, ordering, active intent, and all eight legacy session fields per workspace.
- Migrated only the workspace-selected local or daemon source, grouped sessions by reconciled identity, retained unavailable records, and proved repeat loads are idempotent.
- Replaced silent corruption recovery and direct writes with typed path-aware load errors and unique sibling temporary files synced before rename.
- Proved non-UTF-8 Unix repository and checkout paths retain their exact bytes through discovery adaptation, JSON, load, and reconciliation adaptation.

## Task Commits

Each task followed RED/GREEN TDD commits:

1. **Task 1: Repository aggregate and current-schema round trip** - `d0d3f29` (test), `85756d1` (feat)
2. **Task 2: Selected-source legacy migration** - `475d836` (test), `acde27c` (feat)
3. **Task 3: Fail-visible load and atomic replacement** - `1f70ce6` (test), `efd0e5c` (feat)
4. **Validation hardening discovered during plan verification** - `c694604` (test), `dfa0b10` (fix)

## Files Created/Modified

- `baude-core/src/repository.rs` - Opaque keys, byte-preserving paths, health and role types, retained session intent, allocation, and aggregate validation.
- `baude-core/src/persist.rs` - Versioned envelope, strict load outcomes/errors, isolated roots, legacy migration, and atomic save implementation and fixtures.
- `baude-core/src/lib.rs` - Public repository module export.

## Decisions Made

- Kept repository identity workspace-local and path-independent; observed paths remain revalidation facts rather than keys.
- Made the legacy reconciliation seam explicit so persistence never treats stale paths or `is_worktree` as proof of repository or managed ownership.
- Used only standard-library filesystem primitives and the existing serde stack; Cargo manifests and lockfile remain unchanged.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Rejected zeroed counters in otherwise empty current aggregates**
- **Found during:** Overall validation after Task 3
- **Issue:** Aggregate validation rejected counters behind existing records but accepted zero counters when records were empty, violating the persisted monotonic origin.
- **Fix:** Added a failing regression test and rejected zero repository, checkout, and first-seen counters.
- **Files modified:** `baude-core/src/repository.rs`
- **Verification:** `cargo test -p baude-core repository::`
- **Committed in:** `c694604`, `dfa0b10`

---

**Total deviations:** 1 auto-fixed (1 Rule 1 bug)
**Impact on plan:** The fix closes a malformed-current-state edge case without expanding scope.

## Issues Encountered

- A concurrently executing 05-01 RED commit temporarily prevented the Rust test harness from compiling during early RED sampling. Once its GREEN commit landed, all 05-02 behavioral filters and the complete 160-test `baude-core` suite passed.

## User Setup Required

None - no external services or dependencies were added.

## Next Phase Readiness

- Plan 05-03 can inject live Git reconciliation and adopt `LoadOutcome`/`LoadError` in App and daemon owners.
- The current core API intentionally requires callers to handle blocked persistence rather than receiving an empty hierarchy.

## Self-Check: PASSED

- Created `baude-core/src/repository.rs` exists.
- Modified `baude-core/src/persist.rs` and `baude-core/src/lib.rs` exist.
- All eight listed task and hardening commits exist in repository history.
- No known stubs or unplanned threat surfaces were found in the changed files.

---
*Phase: 05-durable-repository-admission*
*Completed: 2026-08-30*
