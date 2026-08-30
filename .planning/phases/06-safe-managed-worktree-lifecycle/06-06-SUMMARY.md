---
phase: 06-safe-managed-worktree-lifecycle
plan: 06
subsystem: repository-lifecycle
tags: [rust, git-worktree, tdd, double-preflight, compensation, persistence]

requires:
  - phase: 06-03
    provides: Fail-closed removal preflight and opaque verified targets
  - phase: 06-04
    provides: Retained runtime context and commit-aware close behavior
  - phase: 06-05
    provides: Targeted one-runtime reopen compensation
provides:
  - Plain non-force verified removal with branch, parent, inventory, and path postconditions
  - Confirmed stop-between-preflights transaction with runtime compensation
  - Commit-stage-aware unavailable recovery state after topology commitment
  - Distinct target-naming local remove confirmation with typed blockers
affects: [phase-07-hierarchy, phase-08-daemon-parity, phase-09-pwa-parity]

tech-stack:
  added: []
  patterns: [opaque removal capability, fresh authorization after confirmation, pre-Git compensation, truthful cross-system commit stages]

key-files:
  created: []
  modified: [baude-core/src/git.rs, baude-core/src/lifecycle.rs, baude/src/app.rs, baude/src/ui.rs, bauded/src/manager.rs]

key-decisions:
  - "The first safe-removal preflight supplies target-naming confirmation data but its verified Git token is discarded; confirmation always obtains a new token after runtime stop."
  - "Failures before plain Git removal restore one runtime when one was active, while postcondition or persistence failures after Git commitment never recreate topology."
  - "A pre-replacement final-save failure keeps an unavailable recovery child in memory while old durable context remains on disk; a committed replacement keeps child deletion in memory."

patterns-established:
  - "Safe removal sequence: reserve, inspect live, confirm, capture context, stop, inspect fresh, plain Git remove, verify postconditions, delete exact child, save."
  - "Only opaque verified targets reach user-requested Git removal; creation compensation has a separate crate-private option-terminated helper."

requirements-completed: [WORK-05]

duration: 16min
completed: 2026-08-30
---

# Phase 6 Plan 6: Confirmed Double-Preflight Worktree Removal Summary

**Clean baude-managed linked worktrees now require a target-specific confirmation, a fresh post-stop authorization, plain Git removal, preservation proofs, and commit-aware runtime recovery**

## Performance

- **Duration:** 16 min
- **Started:** 2026-08-30T21:19:51Z
- **Completed:** 2026-08-30T21:35:41Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- Replaced arbitrary-path user removal with an opaque verified target and typed postconditions proving the exact path and inventory record are absent while the repository parent, siblings, branch ref, and branch OID remain.
- Added shared App/Manager removal semantics that discard the first authorization, stop the active runtime, inspect again, compensate every pre-Git failure, and delete only the exact durable child after verified Git commitment.
- Preserved truthful state across final-save boundaries: unavailable recovery context remains after pre-replacement failure, while memory follows committed child deletion after replacement durability failure.
- Split retained close from physical removal through a distinct local confirmation naming the exact full branch ref and worktree path; cancellation and first-preflight blockers leave state and runtime unchanged.
- Added deterministic coverage for path/branch races, dirty-after-confirmation, stop failure, Git refusal, compensation failure, and both atomic-save commit stages.

## Task Commits

Each behavior task followed RED then GREEN TDD, with focused cleanup and failure-matrix coverage:

1. **Task 1 RED: verified removal postconditions** - `b4709fe` (test)
2. **Task 1 GREEN: plain remove and preservation proof** - `06198c8` (feat)
3. **Task 2 RED: double-preflight owner contracts** - `bd46ea3` (test)
4. **Task 2 GREEN: compensation and commit-stage transaction** - `9595660` (feat)
5. **Task 3 RED: distinct target-naming confirmation** - `f427834` (test)
6. **Task 3 GREEN: local confirmed remove flow** - `6760585` (feat)
7. **REFACTOR: remove legacy arbitrary-path seam** - `2d698f4` (refactor)
8. **Failure matrix: stop, Git refusal, and compensation failures** - `2f826db` (test)

## Files Created/Modified

- `baude-core/src/git.rs` - Typed verified-removal outcome/errors, plain option-terminated remove, preservation postconditions, race injection, and removal tests.
- `baude-core/src/lifecycle.rs` - Fresh confirmation capability, second inspection, child-only transition, unavailable recovery marking, and removal outcomes.
- `baude/src/app.rs` - Local preparation/confirmation, runtime capture/stop/compensation, commit-stage persistence behavior, modal dispatch, and failure seams.
- `baude/src/ui.rs` - Exact branch/path destructive confirmation and retained-branch explanation.
- `bauded/src/manager.rs` - Matching internal daemon-owner semantics without a Phase 8 endpoint.

## Decisions Made

- The first preflight never grants a reusable mutation capability. `RemovalConfirmation` retains durable identity and display facts only; the opaque `VerifiedRemovalTarget` is recreated after stop.
- Git refusal remains pre-commit and compensating, while `RemoveVerifiedError::Postcondition` means topology may have committed and therefore produces degraded unavailable state rather than runtime restart or topology reconstruction.
- Manager safe removal remains implemented but deliberately unreachable through network APIs until Phase 8; the existing compatibility DELETE continues to mean retained close.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Reconciled SDK-generated progress metadata**
- **Found during:** Plan metadata update
- **Issue:** `state.update-progress` found all nine v2.0 summaries but projected zero completed phases and 0%, leaving Phase 6 velocity and duration stale.
- **Fix:** Reconciled milestone plan progress, the completed Phase 5 count, Phase 6's 6/6 totals, and execution duration against summaries and ROADMAP.
- **Files modified:** `.planning/STATE.md`
- **Verification:** STATE reports 9/9 v2.0 plans, one previously verified completed phase, 6/6 Phase 6 plans, and 100% plan progress while retaining ready-for-verification status.
- **Committed in:** plan metadata commit

---

**Total deviations:** 1 auto-fixed (1 Rule 3 blocking issue)
**Impact on plan:** Documentation-only reconciliation; source behavior and scope are unchanged.

## Issues Encountered

- macOS temporary paths have `/var` and `/private/var` aliases. Tests compare Git-canonical paths captured by the opaque target rather than caller spellings.

## User Setup Required

None - no dependencies, manifests, external services, or configuration changes were added.

## TDD Gate Compliance

- RED `b4709fe` precedes GREEN `06198c8` for Task 1.
- RED `bd46ea3` precedes GREEN `9595660` for Task 2.
- RED `f427834` precedes GREEN `6760585` for Task 3.
- Refactor `2d698f4` and failure-matrix coverage `2f826db` preserve all focused and workspace gates.

## Verification

- `cargo test -p baude-core git::tests::lifecycle::remove_postconditions -- --nocapture` - 3 passed.
- `cargo test lifecycle_remove_clean -- --nocapture` - 4 App/Manager tests passed.
- `cargo test -p baude remove_confirmation -- --nocapture` - 2 passed.
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` - 300 tests passed (29 baude, 196 baude-core, 75 bauded).
- `Cargo.toml` and `Cargo.lock` are unchanged.
- Production search finds no legacy `git::is_dirty`, arbitrary-path `git::remove_worktree`, force removal, or UI-direct Git removal call.

## Known Stubs

None.

## Self-Check: PASSED

- All five modified source files and this summary exist.
- RED/GREEN/refactor commits `b4709fe`, `06198c8`, `bd46ea3`, `9595660`, `f427834`, `6760585`, `2d698f4`, and `2f826db` are present in Git history.

## Next Phase Readiness

- Phase 7 can project the retained local branch as dormant immediately after successful child-only removal.
- Phase 8 can expose Manager's already-matching prepare/confirm transaction through daemon-authoritative identity without changing lifecycle semantics.
- No new endpoint, authentication path, dependency, schema, or unplanned trust-boundary surface was introduced.

---
*Phase: 06-safe-managed-worktree-lifecycle*
*Completed: 2026-08-30*
