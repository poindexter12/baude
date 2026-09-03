---
phase: 07-local-tui-dogfood-release
plan: 02
subsystem: ui
tags: [rust, ratatui, lifecycle, worktree, capability-gating, tdd]

requires:
  - phase: 06-safe-managed-worktree-lifecycle
    provides: Shared activation, retained-close, reopen, and two-preflight managed-removal authority
  - phase: 07-local-tui-dogfood-release
    plan: 01
    provides: Durable repository/checkout hierarchy and key-based selection
provides:
  - Pure core LifecycleCapability projection for manually dispatchable retry paths
  - Exhaustive durable-target local hierarchy action dispatch
  - Distinct retained-close and confirmed managed-worktree removal interactions
  - Exact typed refusals that preserve state, runtime association, Git facts, order, and selection
affects: [07-03, 07-04, 07-05, local-tui, release-certification]

tech-stack:
  added: []
  patterns:
    - Core lifecycle capability is the only retry authorization exposed to presentation
    - Durable RepositoryKey and CheckoutKey resolve action targets; runtime state is optional decoration
    - Typed failures select exact refusal copy while Display text remains fallback detail

key-files:
  created: []
  modified: [baude-core/src/lifecycle.rs, baude/src/hierarchy.rs, baude/src/app.rs, baude/src/ui.rs]

key-decisions:
  - "Only eligible inactive retained checkouts expose RetryReopen; only implemented activation, teardown, and stopped-active paths expose RetryRecovery."
  - "Lowercase x is retained close only, while Shift+X alone enters separately named and freshly rechecked managed-worktree removal."
  - "Local action authorization resolves durable repository and checkout keys and never derives permission from glyphs, status colors, cause strings, or runtime absence."

patterns-established:
  - "Capability-gated dispatch: map durable lifecycle state to a small core enum, then dispatch only the corresponding implemented App adapter."
  - "Defensive hidden-action refusal: stale key events produce target-specific guidance without optimistic mutation."

requirements-completed: []

duration: 34min
completed: 2026-08-31
---

# Phase 7 Plan 2: Capability-Gated Local Lifecycle Actions Summary

**Durable hierarchy targets now create or activate local branches, retain-close and reopen sessions, and separately remove safe managed worktrees through Phase 6 lifecycle authority.**

## Performance

- **Duration:** 34 min
- **Started:** 2026-08-31T08:08:18Z
- **Completed:** 2026-08-31T08:42:35Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Added a pure `LifecycleCapability` projection and hierarchy `ActionView` so retry hints and dispatch share one core authority.
- Routed the full repository/main/managed/external/unavailable/recovery/remote key matrix through durable targets and existing Phase 6 activation, close, reopen, and removal adapters.
- Kept lowercase close non-destructive and moved managed-worktree removal behind distinct Shift+X preflight and confirmation semantics.
- Added exact target-specific branch, topology, recovery, removal, shell, editor, archive, and retry refusals with non-mutation evidence.
- Passed guarded exact tests, Phase 6 reopen/removal regressions, formatting, workspace Clippy, and all 329 workspace tests.

## Capability Mapping

| Durable lifecycle state | Capability | App dispatch |
|---|---|---|
| Eligible inactive retained checkout | `RetryReopen` | `reopen_checkout` through durable `CheckoutKey` |
| Pending/failed activation recovery | `RetryRecovery` | existing activation recovery adapter |
| Teardown pending | `RetryRecovery` | retained-close recovery adapter |
| Stopped-active ownership recovery | `RetryRecovery` | runtime-extinct reconciliation to inactive |
| Running, removal tombstone, generic unavailable topology | none | `r` defensively refused |

## Exact Action-Table Evidence

- Guarded exact test `app::tests::hierarchy_action_matrix_dispatches_only_authorized_local_actions` covers repository, main, managed, external, unavailable, recovery, and remote selections across Enter, `w`, `x`, `r`, Shift+X, `t`, `e`, `i`, `v`, `g`, and `a`.
- Valid repository and child branch actions open one durable-parent activation path; invalid refs, remote-only refs, path collisions, and occupied protected targets fail with typed target context.
- Refusal vectors assert unchanged `RepositoryState`, runtime map, row order, and selection; real Git fixtures assert blocked and canceled removal preserve inventory and branch refs.
- Close confirmation retains the checkout; removal confirmation names the exact branch and path and preserves the Phase 6 fresh second-preflight transaction.

## Task Commits

Each TDD task was committed as a RED/GREEN pair:

1. **Task 1: Project only manually dispatchable lifecycle capabilities**
   - `c0aafd8` — test RED
   - `7e0391b` — feature GREEN
2. **Task 2: Dispatch the exhaustive hierarchy lifecycle action matrix**
   - `318d277` — test RED
   - `b661784` — feature GREEN

## Files Created/Modified

- `baude-core/src/lifecycle.rs` — Core retry capability projection, typed occupied-protected activation failure, and stopped-active recovery transition.
- `baude/src/hierarchy.rs` — Durable selection-kind `ActionView` projection consumed by App dispatch.
- `baude/src/app.rs` — Exhaustive key matrix, durable target resolution, exact refusals, lifecycle adapter dispatch, and guarded regression evidence.
- `baude/src/ui.rs` — Non-destructive retained-close confirmation wording and keys.

## Decisions Made

- Retry authority is deliberately narrower than availability: removal tombstones and generic unavailable topology expose no manual retry capability.
- Repository and child `w` actions converge on the durable canonical parent; branch input remains literal data for existing typed Git validation.
- Occupied protected activation is represented by a typed lifecycle error carrying `CheckoutKey` and `UnavailableCause`, avoiding Display-text parsing for refusal selection.
- Requirements remain pending until Phase 7 dogfood UAT and release certification are complete.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added the stopped-active recovery transition advertised by capability projection**
- **Found during:** Task 1 (Project only manually dispatchable lifecycle capabilities)
- **Issue:** `StoppedActiveRecovery` had a manual recovery capability but no reducer transition for the runtime-extinct evidence used by its App dispatcher.
- **Fix:** Added the exact protected-state transition to inactive with `PersistInactive` evidence and covered it in the capability contract.
- **Files modified:** `baude-core/src/lifecycle.rs`
- **Verification:** Guarded capability test and all 213 core tests pass.
- **Committed in:** `b661784`

**2. [Rule 1 - Bug] Restored pre-activation durable state on pre-Git activation failure**
- **Found during:** Task 2 (Dispatch the exhaustive hierarchy lifecycle action matrix)
- **Issue:** Clearing a pending activation from the working copy could retain optimistic state after activation failed before Git commitment.
- **Fix:** Restore the complete pre-request state before persisting the failure rollback.
- **Files modified:** `baude/src/app.rs`
- **Verification:** Branch rollback, activation recovery, exact action-matrix, and full workspace tests pass.
- **Committed in:** `b661784`

**3. [Rule 1 - Bug] Made archive work for retained runtime-less checkouts**
- **Found during:** Task 2 (Dispatch the exhaustive hierarchy lifecycle action matrix)
- **Issue:** The action table exposed archive for applicable checkout rows, but `toggle_archive` returned without changing durable state when no runtime was associated.
- **Fix:** Persist archive state directly on the durable retained session with atomic-save rollback semantics.
- **Files modified:** `baude/src/app.rs`
- **Verification:** Exact action-matrix evidence covers retained archive and the full workspace suite passes.
- **Committed in:** `b661784`

**4. [Rule 2 - Missing Critical] Removed destructive removal guidance from retained-close confirmation**
- **Found during:** Task 2 (Dispatch the exhaustive hierarchy lifecycle action matrix)
- **Issue:** Existing UI copy instructed users to press `r` to remove a worktree from the close modal, conflicting with the new capability-gated retry contract and distinct Shift+X removal boundary.
- **Fix:** Changed the modal to an explicit retained-close question with only y/Enter close and n/Esc cancel guidance.
- **Files modified:** `baude/src/ui.rs`
- **Verification:** UI tests and full workspace tests pass; removal remains available only through Shift+X.
- **Committed in:** `b661784`

**5. [Rule 1 - Bug] Restored milestone progress after state-handler miscalculation**
- **Found during:** Plan metadata finalization
- **Issue:** `state.update-progress` again counted zero completed milestone phases because Phase 6 remains in certification, resetting the displayed milestone progress from 33% to 0%; its aggregate velocity rows also remained stale.
- **Fix:** Restored the completed Phase 5 count and 33% phase progress, then updated the observed 12-plan and Phase 7 execution metrics without marking Phase 6 or Phase 7 complete.
- **Files modified:** `.planning/STATE.md`
- **Verification:** State reports one of three milestone phases complete, 12 of 16 plans executed, and Phase 7 at 2/6 while certification remains pending.
- **Committed in:** Plan metadata commit

---

**Total deviations:** 5 auto-fixed (3 bugs, 2 missing critical functionality)
**Impact on plan:** All fixes were required to make advertised actions real, preserve failure atomicity, keep close/remove intent unambiguous, and retain accurate planning state. No dependency, network, daemon hierarchy, or remote-destructive scope was added.

## Issues Encountered

- Rust editor diagnostics repeatedly reported stale trailing syntax errors after edits; authoritative Cargo format, check, Clippy, and test commands compiled cleanly.
- The retained-archive test fixture initially violated repository path invariants; its retained main/worktree paths were corrected before verification.

## Verification

- Guarded capability contract: passed.
- Guarded exhaustive hierarchy action matrix: passed.
- Guarded Phase 6 second-preflight/compensation removal regression: passed.
- Guarded Phase 6 one-runtime reopen regression: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed — 38 App/TUI, 213 core, and 78 daemon tests.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None found in files created or modified by this plan.

## TDD Gate Compliance

- RED commits: `c0aafd8`, `318d277`
- GREEN commits: `7e0391b`, `b661784`
- Both guarded RED commits preceded their corresponding GREEN implementations.

## Next Phase Readiness

- The safe lifecycle action matrix is ready for responsive hint/detail surfaces and local dogfood UAT in subsequent Phase 7 plans.
- WORK requirement checkoff, Phase 7 completion, and release certification intentionally remain pending.

## Self-Check: PASSED

- All four implementation files and this summary exist.
- All four RED/GREEN task commits are present in git history.
- `git diff --check` passes.

---
*Phase: 07-local-tui-dogfood-release*
*Completed: 2026-08-31*
