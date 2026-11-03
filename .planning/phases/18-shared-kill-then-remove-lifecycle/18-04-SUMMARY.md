---
phase: 18-shared-kill-then-remove-lifecycle
plan: 04
subsystem: lifecycle
tags: [rust, lifecycle, removal, signals, pty, tdd]

# Dependency graph
requires:
  - phase: 18-shared-kill-then-remove-lifecycle
    provides: 18-01 select_rows, report_from_set, RemovalReport.readers_parked, RemovalError with boxed RuntimeSurvived, DiskRemovalFailure; 18-02 git::remove_managed_worktree_forced and ForcedRemovalError; 18-03 RepositoryState::remove_rows; 18-08 Session::kill_and_wait group extinction, Pty::reader_finished, fail_next_teardown_for_test keyed by pid
provides:
  - RowRuntime { row: RowKey, session: Session } owning handle type in baude-core/src/removal.rs
  - removal::remove(state, target, &mut Vec<RowRuntime>, save) with the locked stage order select_rows, preflight, candidate plus validate_for_save, kill_stage, disk_stage, persist_stage
  - impl From<git::ForcedRemovalError> for DiskRemovalFailure, the only place removal.rs names the git error type
  - private preflight, kill_stage, disk_stage with DiskResults, owner_paths, disk_error, persist_stage, StageResults; report_from_set(set, Option<StageResults>) shared by preview and remove
  - test fixture RemovalFixture (new, repo, managed_root, managed_worktree, state_root, state_path, common_dir), spawn_trapping_runtime with the set +m group-membership proof, descendant_in_group, spawn_parked_reader_runtime, HolderGuard, session_for, handle_for, LiveHandles guard, counting_save, proc_state, wait_for, assert_reaped_promptly, group_is_extinct, group_is_alive, worktree_list, mismatch_row
affects: [18-05, 18-06, 18-07, 20-tui-remove-action, 21-daemon-and-pwa-parity]

# Actuals (#2632) - estimateTokens scale, chars/4 over the realized diff
actuals:
  tokens: 13526
  tasks: 3
  commits: 3
plan_head_before: 3c36366621a1bcf3f35ce8d62bbeaf2b91c839f5
plan_head_after: 32a777083ee366acf45ebec1559e46581744216a

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Preflight proves every caller-supplied handle against the persisted OwnedRuntime in full (pid, start_time, process_group, session, exact ShellOwnership, one handle per row, none for a row without a persisted runtime) before any signal, so the set of processes execution may signal equals the set the state-only preview names"
    - "Candidate state is cloned, mutated and validated with validate_for_save before the kill stage; the irreversible stages (kill, disk) run only once the save is known to be acceptable, and the in-memory state plus the handles are committed only after the single save succeeds or reports a committed replacement"
    - "A panic-safe guard that owns test runtimes calls the idempotent kill twice, because a debug-injected teardown failure is consumed by the first call without signalling anything"

key-files:
  created: []
  modified:
    - baude-core/src/removal.rs

key-decisions:
  - "AlreadyRemoved rows match ForcedRemovalError::NotRegistered on the git error directly rather than on the converted DiskRemovalFailureKind, so the tolerated case is named where it originates and the plan's literal acceptance grep holds"
  - "The parked-reader fixture uses the exact 18-08 holder shape including trap '' HUP: without it the leader's exit SIGHUP reaches the perl child before setsid runs and holder.pid is never written"
  - "Task 3 refactor was performed: remove rebuilt four report fields report_from_set had derived from the set, so report_from_set now takes Option<StageResults> and both preview and remove use one assembly path"

patterns-established:
  - "spawn_trapping_runtime and descendant_in_group live in removal::tests for plans 05 and 06 to reuse; every trapping command starts with set +m and the helper refuses to return until a sleep row carries the recorded pgid"

requirements-completed: [KILL-01, KILL-02, KILL-03, RMV-06, RMV-05]

coverage:
  - id: D1
    description: "A live TERM- and HUP-trapping runtime's group is extinct after remove, the leader is reaped, the managed directory is deleted, the row is gone from state and the saved file, the save ran once and the handle was consumed"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#live_runtime_group_is_extinct_after_remove"
        status: pass
    human_judgment: false
  - id: D2
    description: "Agent and shell are one kill unit: both groups extinct, both pids reaped and reported"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#agent_and_shell_are_both_extinct_after_remove"
        status: pass
    human_judgment: false
  - id: D3
    description: "A reader still parked behind an out-of-group slave holder is listed in readers_parked (Linux) or joined (macOS, tty revoked at leader exit); removal still completes and the holder is never signalled"
    requirement: KILL-02
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#reader_parked_on_out_of_group_slave_holder_is_reported"
        status: pass
    human_judgment: false
  - id: D4
    description: "A survivor leaves state, file bytes, directory and handle untouched; a parent removal aborts on the second child's survivor before any disk or state change"
    requirement: KILL-03
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#surviving_runtime_leaves_row_and_disk"
        status: pass
      - kind: unit
        ref: "baude-core/src/removal.rs#parent_kill_failure_aborts_before_disk_and_state"
        status: pass
    human_judgment: false
  - id: D5
    description: "Missing, mismatched, duplicate and unrecorded handles are refused before any signal (five mismatch shapes)"
    requirement: KILL-03
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#missing_runtime_handle_is_typed_error_before_any_kill"
        status: pass
      - kind: unit
        ref: "baude-core/src/removal.rs#mismatched_runtime_identity_is_refused_before_any_kill"
        status: pass
    human_judgment: false
  - id: D6
    description: "One save per removed row set; an already-removed managed row still has its git administrative entry pruned and the branch kept"
    requirement: RMV-06
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#already_removed_managed_row_prunes_git_registration"
        status: pass
    human_judgment: false
  - id: D7
    description: "remove's rows_removed equals preview_removal's for the same target; paths_kept, paths_deleted and persistence Saved reported through the shared assembly"
    requirement: RMV-05
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#remove_report_matches_preview_rows"
        status: pass
    human_judgment: false

duration: 31 min
completed: 2026-11-03
status: complete
---

# Phase 18 Plan 04: Kill-Then-Remove-Then-Save Removal Operation Summary

**`removal::remove` now composes the wave 1 primitives in the locked order, with every caller-supplied handle proven against the persisted runtime before a signal is sent, the candidate validated before any irreversible stage, one save per row set, and nine live-process tests covering group extinction, the shell kill unit, survivor abort, all-or-nothing parent abort, five identity refusals, stale-entry pruning, preview parity and parked-reader reporting.**

## Performance

- **Duration:** 31 min
- **Started:** 2026-11-03T23:16:39Z
- **Completed:** 2026-11-03T23:47:00Z
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments

- `pub fn remove(state, target, runtimes: &mut Vec<RowRuntime>, save)` whose body reads as the stage sequence: `select_rows`, `preflight`, clone plus `remove_rows` plus `validate_for_save`, `kill_stage`, `disk_stage`, `persist_stage`, then `report_from_set` with the real stage results
- `preflight` refuses before any signal: duplicate handles, a handle for a row with no persisted runtime, a missing handle for a live row, an agent identity that differs in any of pid, start_time, process_group or session, a persisted `Owned` shell the handle does not carry equally, and a live shell on a handle whose persisted ownership is `Closed`
- `kill_stage` walks `set.rows` in order through `Session::kill_and_wait`, wrapping the first `SessionTeardownError` as `RemovalError::RuntimeSurvived` with the boxed error 18-01 declared, so disk and state stay untouched
- `disk_stage` calls `git::remove_managed_worktree_forced` for both `RemoveManagedWorktree` and `AlreadyRemoved` rows, tolerating `ForcedRemovalError::NotRegistered` for the latter so a directory deleted out of band still has its `git worktree list` entry pruned while `refs/heads` is untouched
- `persist_stage` saves once, commits `*state` on `Ok` or on `replacement_committed()`, sweeps `reader_finished()` across every consumed agent and shell Pty while the handles are still owned, records the parked rows in `readers_parked`, then drains the consumed handles in the one `runtimes.retain` call
- `RemovalFixture` runs each test on a real `git init` repository with a managed worktree under the `TestRedirect` root; `spawn_trapping_runtime` proves from `ps -eo pid=,pgid=,comm=` that the `sleep` descendant sits in the recorded group before any test signals, and `LiveHandles` plus `HolderGuard` stop every child during unwinding

## TDD Gate Compliance

| Gate | Evidence |
| ---- | -------- |
| RED | Commit `680a371` (`test(18-04): add failing removal pipeline tests`). `timeout 600 cargo test -p baude-core removal::` exit non-zero, zero `error[E` lines, 30 passed / 9 failed in 22.44 s; the 30 are plan 01's 7 `removal::tests` plus the 23 `git::tests::forced_removal` cases the broader filter matched, so under the plan's `removal::tests` filter the figures are 7 passed / 9 failed as 18-VALIDATION row 18-04-01 expects. Every one of the nine failures is the stub's typed result against the planned assertion: `expected RuntimeHandleMissing, got UnknownTarget(Checkout(CheckoutKey(2)))`, `expected RuntimeIdentityMismatch, got UnknownTarget(...)`, `expected RuntimeSurvived, got UnknownTarget(...)` twice, and `remove: UnknownTarget(...)` plus `already-removed row is pruned: UnknownTarget(...)` and `remove repository: UnknownTarget(...)` on the `expect` arms. Semantic assessment: each target test executed its fixture (real worktree, live trapping runtime with proven group membership) and failed on its intended assertion because `remove` did nothing. `cargo clippy -p baude-core --all-targets -- -D warnings` exit 0 on the RED tree. |
| GREEN | Commit `b175b67` (`feat(18-04): add kill-then-remove-then-save removal operation`). `cargo test -p baude-core removal::tests -- --test-threads=1`: 16 passed, 0 failed in 39.34 s. `cargo clippy -p baude-core --all-targets -- -D warnings` exit 0 with no warnings; `cargo fmt --check` exit 0. Acceptance greps on the GREEN tree: forbidden helper names 0, stage functions 4, `validate_for_save` at region line 16 before `kill_stage(` 18, `disk_stage(` 19, `persist_stage(` 20; `replacement_committed()` 1; `runtimes.retain` 1; inside `persist_stage` `reader_finished()` at line 25 before `runtimes.retain` at 35; joined claims inside `remove` 0; `ForcedRemovalError::NotRegistered` 1. |
| REFACTOR | Commit `32a7770` (`refactor(18-04): share report assembly between preview and remove`). The condition held: `remove` overwrote `runtimes_killed`, `paths_deleted`, `paths_kept` and `already_removed` after `report_from_set` had derived them from the set. `report_from_set(set, Option<StageResults>)` now serves both callers. 16 passed, 0 failed in 27.53 s; `cargo fmt --check` exit 0; public API grep 3. |

`gsd_run check tdd-red-evidence` has no adapter for `cargo test` text and reports `INVALID_RED` / `unreadable_record` on it, the same gap plans 18-01, 18-02, 18-03 and 18-08 recorded. The plan's own RED gate (exit non-zero, no `error[E`, nine named `FAILED` lines, failure reason the stub's typed result) was applied instead and passed.

## Task Commits

1. **Task 1 (RED): RowRuntime, From<ForcedRemovalError>, remove stub, live fixture and nine tests** - `680a371` (test)
2. **Task 2 (GREEN): preflight, candidate validation, kill, disk and persist stages** - `b175b67` (feat)
3. **Task 3 (REFACTOR): report assembly shared between preview and remove** - `32a7770` (refactor)

**Plan metadata:** see the docs commit that lands this file

## Files Created/Modified

- `baude-core/src/removal.rs` - `RowRuntime`, `remove`, `preflight`, `kill_stage`, `DiskResults`, `disk_stage`, `owner_paths`, `disk_error`, `persist_stage`, `StageResults`, `report_from_set` taking `Option<StageResults>`, `From<ForcedRemovalError> for DiskRemovalFailure`, module-level `use crate::pty::Pty`; test fixture and helpers listed under `provides`; nine `removal::tests` cases

## Verification

- `cargo test -p baude-core removal::tests -- --test-threads=1`: 16 passed, 0 failed (host macOS, debug build)
- Full `cargo test -p baude-core` on the host: 505 passed, 0 failed, 1 ignored (the pre-existing ignore)
- `cargo clippy -p baude-core --all-targets -- -D warnings` and `cargo fmt --check` exit 0 after every task
- `git log --oneline 3c36366..HEAD` lists exactly the three task commits; `git diff --stat` over that range touches only `baude-core/src/removal.rs` (1277 insertions, 5 deletions); no file deletions, no `Cargo.toml` change
- No `sleep 30` or `perl -MPOSIX` process attributable to this module's fixtures survived the final targeted run

## Decisions Made

- `AlreadyRemoved` tolerance is matched on `ForcedRemovalError::NotRegistered` at the call site in `disk_stage`, with `owner_paths` and `disk_error` as small helpers, instead of matching the converted kind after a wrapper function
- The parked-reader fixture copies 18-08's command string verbatim, including `trap '' HUP`, and asserts per platform the way 18-08 does: Linux reports the row in `readers_parked`, macOS joins the reader because the tty is revoked at leader exit
- Task 3's condition was true, so the refactor commit was made (18-08 had found nothing to change in its own Task 3)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] LiveHandles guard leaked a child on the RED path**
- **Found during:** Task 1, inspecting `ps` after the RED run
- **Issue:** The plan's guard called `kill_and_wait` once per remaining handle. When `remove` is a stub, the `fail_next_teardown_for_test` injection on the survivor fixtures is still armed at drop time, and 18-08's `Pty::kill_and_wait` consumes the injection and bails before signalling, so the two injected runtimes ran their full 30 s
- **Fix:** The guard calls the idempotent `kill_and_wait` twice per handle, with a comment explaining why; on GREEN the first call inside `remove` consumes the injection and the guard's second call is a no-op on an extinct group
- **Files modified:** baude-core/src/removal.rs
- **Commit:** `680a371`

**2. [Rule 1 - Bug] Parked-reader holder died before setsid**
- **Found during:** Task 2, first GREEN run (`holder.pid was not written or the leader did not exit`)
- **Issue:** The plan's command string for `spawn_parked_reader_runtime` omitted the `trap '' HUP` that 18-08's shipped fixture carries. With `& exit 0` the bash leader exits immediately; the kernel's leader-exit SIGHUP reached the perl child before `POSIX::setsid` ran, so the holder died without writing its pid
- **Fix:** Command string now matches 18-08's exactly: `set +m; trap '' HUP; perl -MPOSIX -e '...' & exit 0`; the ignored HUP is inherited across the exec and the holder survives into its own session
- **Files modified:** baude-core/src/removal.rs
- **Commit:** `b175b67`

**Total deviations:** 2 auto-fixed (2 Rule 1, both inside the test fixture; no production-code or plan-shape deviation)
**Impact on plan:** Both fixes make the fixtures model the real shapes 18-08 established. Shipped signatures the dispatch warned about were used as shipped: `RemovalError::RuntimeSurvived { error: Box<SessionTeardownError> }` is constructed with `Box::new`, `remove_managed_worktree_forced(main_worktree, common_dir, path)` is called with the `PlannedRow` fields 18-01 recorded.

## Issues Encountered

- The RED verification used the filter `removal::`, which also matched `git::tests::forced_removal`, so the raw count read 30 passed rather than the 7 the plan names; the GREEN and REFACTOR runs used the plan's `removal::tests` filter and read 16 as expected
- After the full-suite run two bare `sleep 30` rows from other modules' fixtures were still on their own timers; they are not the bash-trap shape this module spawns and expire on their own, as 18-08 also observed
- `scripts/gate.sh --linux` was not run, per the dispatch rule that plan 18-07 owns the gate

## Known Stubs

None. The RED stub (`remove` returning `UnknownTarget`) was replaced in GREEN.

## Threat Flags

None beyond the plan's threat model. T-18-09 (preflight full identity), T-18-10 and T-18-11 (survivor leaves everything), T-18-21 and T-18-22 (validate before kill, single save), T-18-36 (no off-preview signal) and T-18-39 (parked reader reported, not detached) are each covered by the named test above. No new network endpoint, auth path or schema change.

## Requirements

All five declared IDs are also declared by plans not yet summarised: KILL-01 by 18-05, 18-06 and 18-07; KILL-02 by 18-06 and 18-07; KILL-03 by 18-05 and 18-07; RMV-06 by 18-05; RMV-05's only other declarer, 18-01, is complete. `requirements.ready-ids` decides which of these REQUIREMENTS.md may mark now; the rest are marked by their last declaring plan.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 05 (live main, standalone and unmanaged rows) can seed through `live_row`, `seed_live` and `spawn_trapping_runtime` and assert `paths_kept` through the shared report path
- Plan 06 (cloned-topology integration matrix) can reuse `RemovalFixture`'s managed-root and `counting_save` shapes; `remove` already prunes an `AlreadyRemoved` entry, which 18-06 asserts on the porcelain listing
- Plan 07's Linux log can name the nine tests; the parked-reader test branches per platform and expects `readers_parked == [row]` there
- The two `git.rs` helpers `forced_remove_arguments` and `removal_marker_path` are now consumed through `remove_managed_worktree_forced`, so their dead_code diagnostics are gone

---
*Phase: 18-shared-kill-then-remove-lifecycle*
*Completed: 2026-11-03*

## Self-Check: PASSED

- `baude-core/src/removal.rs` exists and carries `pub fn remove(`, `fn preflight`, `fn kill_stage`, `fn disk_stage` and `fn persist_stage`.
- Commits 680a371, b175b67 and 32a7770 are ancestors of HEAD on `gsd/phase-18-shared-kill-then-remove-lifecycle`.
- `requirements ready-ids` on this plan: RMV-05 ready and marked; KILL-01, KILL-02, KILL-03 and RMV-06 blocked by later declaring plans.
