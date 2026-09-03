---
phase: 07-local-tui-dogfood-release
plan: 04
subsystem: testing
tags: [rust, real-git, restart, dedup, axum, compatibility, tdd]

requires:
  - phase: 07-local-tui-dogfood-release
    plan: 03
    provides: Durable local hierarchy actions, responsive UI, and deterministic selection
  - phase: 06-safe-managed-worktree-lifecycle
    plan: 07
    provides: Shared persisted lifecycle authority and registered runtime ownership
provides:
  - Isolated real-Git App dogfood across admission, activation, close, restart, reopen, and safe removal
  - Durable repository/checkout key, order, runtime-cardinality, branch-retention, and fixture-boundary evidence
  - Explicit flat remote TUI and daemon SessionInfo compatibility proof
affects: [07-05, 07-06, release-certification, manual-dogfood]

tech-stack:
  added: []
  patterns:
    - Run environment-sensitive real-Git dogfood in an exact child test process with fixture-scoped HOME and XDG data
    - Assert compatibility through existing App action projection and Axum router rather than adding routes

key-files:
  created: [.planning/phases/07-local-tui-dogfood-release/07-04-SUMMARY.md]
  modified: [baude/src/app.rs, bauded/src/api.rs]

key-decisions:
  - "The real-Git dogfood runs in an exact child test process so HOME/XDG isolation cannot race the parallel workspace test harness."
  - "Restart selection proves the first rendered local repository parent; retained child restoration requires explicit CheckoutKey reselection."
  - "Daemon compatibility remains the existing flat SessionInfo array and retained-close DELETE; hierarchy and remove-worktree routes remain absent."

patterns-established:
  - "Dogfood boundary: every Git worktree and persisted state path is asserted inside one process-unique fixture root."
  - "Compatibility proof: remote runtime IDs cannot synthesize durable local parents or dispatch branch/removal actions."

requirements-completed: [SURF-05, REL-01]

duration: 16min
completed: 2026-08-31
---

# Phase 7 Plan 4: Restart/Dedup Dogfood and Flat Compatibility Summary

**A fixture-isolated production App flow now proves durable real-Git restart/reopen/removal behavior while remote TUI rows and daemon sessions remain flat and non-destructive.**

## Performance

- **Duration:** 16 min
- **Started:** 2026-08-31T09:27:39Z
- **Completed:** 2026-08-31T09:44:01Z
- **Tasks:** 2
- **Files modified:** 2 source files plus this summary

## Accomplishments

- Automated open/admit, managed branch creation and existing activation, retained close, App replacement, explicit durable reselection, reopen dedup, dirty-work refusal, double-preflight safe removal, and retained branch proof against a real bare-origin/main fixture.
- Proved one canonical repository parent, unique durable checkout keys, unchanged first-seen order, at most one runtime per checkout key, exact Git inventory, deterministic restart/removal selection, and persisted state equality at lifecycle boundaries.
- Isolated HOME, XDG data, state, origin, checkout, managed worktree, Git identity, and sleeping fake backend in one process-unique root; the parent test never mutates process-global environment.
- Locked remote App rows to their flat runtime IDs and existing open/restart/close/archive actions, with no local branch, hierarchy, or Shift+X removal dispatch.
- Locked `GET /sessions` to a flat `SessionInfo` array, compatibility `DELETE /sessions/{id}` to retained close, and candidate hierarchy/remove-worktree routes to 404.

## Real-Git Dogfood Evidence

| Stage | Evidence |
|---|---|
| Admit/default | One repository and one `PrimaryDefault` checkout; exact main-only inventory and one runtime association |
| Create/activate | Distinct managed child, stable parent/child keys and order, exact two-worktree inventory; repeat activation focuses the same runtime |
| Retained close | Child and primary keys/order remain persisted, runtime associations reach zero, user file/worktree/branch remain |
| Restart | Reloaded structural state equals the closed persisted state; initial selection is the first rendered repository parent |
| Explicit reopen | Reselects the retained `CheckoutKey`, spawns once, then focuses the same runtime without a second spawn |
| Safe remove | Untracked user work blocks preflight unchanged; after test cleanup, first preflight is pure and confirmation performs a fresh safe removal |
| Post-remove | Managed path and child are absent, primary remains selected, inventory is main-only, and `refs/heads/feature/restart-dedup-dogfood` survives |
| Isolation | Every discovered worktree and persistence root is under the process-unique fixture; local Git config and bare origin avoid credentials/network/global config |

## Task Commits

1. **Task 1: Automate the isolated real-Git restart and dedup dogfood flow**
   - `5a884e1` — RED exact dogfood test
   - `a402d03` — GREEN isolated production-path dogfood automation
2. **Task 2: Lock flat remote and daemon compatibility against hierarchy or destructive drift**
   - `1319c81` — RED exact App/API compatibility tests
   - `da493aa` — GREEN flat TUI and retained-close daemon compatibility proof

## Files Created/Modified

- `baude/src/app.rs` — Real-Git restart/dedup dogfood harness, fixture-root factoring, and flat remote action/identity contract.
- `bauded/src/api.rs` — Flat SessionInfo, retained-close DELETE, missing-route, persistence, branch, and Git topology compatibility test.
- `.planning/phases/07-local-tui-dogfood-release/07-04-SUMMARY.md` — Execution and verification evidence.

## Decisions Made

- Spawned the exact dogfood test as a child test process with fixture-scoped `HOME` and `XDG_DATA_HOME`. This proves isolation without mutating environment variables shared by parallel tests.
- Used a complete new managed-branch case and repeated activation of that same branch to cover both creation and existing activation/dedup behavior.
- Closed both live fixture runtimes before App replacement, then proved restart does not invent runtime ownership and explicit child reselection launches exactly once.
- Exercised compatibility solely through existing production App methods, router, and Manager behavior. No production route or wire shape changed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Restored milestone progress after state-handler miscalculation**
- **Found during:** Plan metadata finalization
- **Issue:** `state.update-progress` counted zero completed milestone phases because Phase 6 remains in certification, resetting the displayed progress from 33% to 0%; `state.advance-plan` also could not parse the intentionally certification-focused Current Position.
- **Fix:** Preserved Phase 6 as the certification focus while restoring one completed phase, 33% progress, 14 completed plans, 280 execution minutes, and Phase 7 at 4/6.
- **Files modified:** `.planning/STATE.md`
- **Verification:** State reports one of three milestone phases complete and records Plan 07-04 without claiming Phase 6 or Phase 7 certification.
- **Committed in:** Plan metadata commit

---

**Total deviations:** 1 auto-fixed bug
**Impact on plan:** Metadata accuracy only; source scope, automated evidence, and pending manual certification are unchanged.

## Issues Encountered

- macOS reports temporary paths through both `/var` and `/private/var`; fixture paths are canonicalized before exact Git inventory and containment comparisons.
- Existing lifecycle tests continue to print benign Git cleanup diagnostics for already-removed temporary worktrees; the full workspace gate remains green.

## Verification

- Guarded exact `app::tests::local_tui_dogfood_real_git_flow_survives_restart_without_duplicates` passed.
- Guarded exact `app::tests::hierarchy_flat_remote_compatibility_has_no_local_parent_or_remove_action` passed.
- Guarded exact `api::tests::flat_session_api_remains_a_non_hierarchical_compatibility_projection` passed.
- Guarded exact `api::tests::real_atomic_persistence_failures_are_503_for_every_mutation` passed.
- Direct activation, close, reopen, and safe-removal App regressions passed.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed: 45 App/TUI, 213 core, and 79 daemon tests (337 total).
- `git diff --check` passed.
- No dependency, manifest, lockfile, production endpoint, publication, push, or PR change was made.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None found in files created or modified by this plan.

## TDD Gate Compliance

- RED commits: `5a884e1`, `1319c81`.
- GREEN commits: `a402d03`, `da493aa`.
- Both exact RED gates failed on the intentionally absent dogfood/compatibility harness behavior before their corresponding GREEN implementations.

## Next Phase Readiness

- Automated REL-01 and SURF-05 evidence is green and ready for packaging/readiness plans.
- Manual wide/narrow TUI dogfood, screenshots, Linux/runtime certification, independent review, phase verification, Nyquist approval, and publication decision remain pending. Nothing was published, pushed, or submitted as a PR.

## Self-Check: PASSED

- Both modified source files and this summary exist.
- All four RED/GREEN task commits are present in git history.
- `git diff --check` passes.

---
*Phase: 07-local-tui-dogfood-release*
*Completed: 2026-08-31*
