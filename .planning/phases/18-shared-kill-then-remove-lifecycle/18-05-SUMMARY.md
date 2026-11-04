---
phase: 18-shared-kill-then-remove-lifecycle
plan: 05
subsystem: lifecycle
tags: [rust, persistence, atomicity, reconcile, tdd]

# Dependency graph
requires:
  - phase: 18-shared-kill-then-remove-lifecycle
    provides: 18-04 removal::remove with the locked stage order, persist_stage, RemovalFixture, live_row, spawn_trapping_runtime, LiveHandles, counting_save, write_seed, load_state; 18-02 remove_managed_worktree_forced with the removal marker and the Fallback error; 18-08 Session::kill_and_wait Extinct disposition and Pty::reader_finished; persist::save_current_at_test with AtomicFailure injection; lifecycle::record_checkout_reconciliation and git::reconcile_checkout
provides:
  - nine removal::tests cases pinning every AtomicFailure branch of persist_stage, the single-save contract, SC5 crash consistency for a checkout and a repository mid-fan-out, a retryable per-child disk fault mid-fan-out, and validation-before-kill ordering
  - test helpers in removal::tests: counting_save_with (injects one AtomicFailure per call; counting_save now delegates to it), inactive_row, inactive_pair, persist_error, PinGuard with set_mode, reconcile_missing, stale_runtime
affects: [18-06, 18-07, 20-tui-remove-action, 21-daemon-and-pwa-parity]

# Actuals (#2632) - estimateTokens scale, chars/4 over the realized diff
actuals:
  tokens: 5778
  tasks: 3
  commits: 3
plan_head_before: 9915114b7c015e17c62bfe3908dcf5755fa02106
plan_head_after: 368120e879df39e776eb2f4e0d435c97127d2571

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Persistence failure modes are proven on real files through persist::save_current_at_test with one injected AtomicFailure per closure call, asserting the in-memory state against a pre-call clone, the file against its pre-call bytes, the report and the closure count together"
    - "The disk-then-save gap is exercised end to end: the failed attempt leaves the prior file, the existing startup reconcile classifies the vanished directory Missing, and a second remove call on the reconciled state reports it under already_removed"
    - "A per-child disk fault is injected inside the failing child only, as a read-only pin directory holding a file, so a sibling child and the shared managed root keep their modes and the c1-deleted, c2-refused boundary is reachable"

key-files:
  created: []
  modified:
    - baude-core/src/removal.rs

key-decisions:
  - "The refused-row assertion names the row through SaveError::validation().row() and the CheckoutKey's Debug text, because ValidationError's Display renders the key through Debug rather than through ValidationRow's Display; no production message was changed"
  - "Both Task 2 crash tests reconcile through one reconcile_missing helper, so the plan's grep proxy for record_checkout_reconciliation reads 1 while both tests exercise the call"
  - "Task 3's second repository is seeded through plan 01's repository builder on a distinct non-existent common dir, since validate() refuses a duplicate repository identity and remove never touches that repository"

patterns-established:
  - "counting_save_with(state_root, Some(AtomicFailure::X)) is the one way a removal test injects a save fault; the matching retry always uses counting_save and its own fresh counter"

requirements-completed: [RMV-06, RMV-04, KILL-03, KILL-01]

coverage:
  - id: D1
    description: "An injected rename failure returns Persist with replacement_committed false, the state equals its pre-call clone, the file bytes are unchanged, the managed directory is already gone, the closure ran once, and a retry with a working save reports the path under already_removed"
    requirement: RMV-06
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#rename_failure_keeps_prior_state_then_retry_reports_already_removed"
        status: pass
    human_judgment: false
  - id: D2
    description: "An injected sync failure on a row with a live runtime leaves the group extinct and reaped, the handle still owned with reader_finished observed true, rows and bytes intact; the retry completes, reports the runtime once and consumes the handle"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#sync_failure_keeps_prior_state_and_file_bytes"
        status: pass
    human_judgment: false
  - id: D3
    description: "An injected directory-sync failure after the rename returns Ok with SavedDegraded, commits the new state in memory, and the saved file parses without the removed row"
    requirement: RMV-06
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#directory_sync_failure_commits_state_and_reports_degraded"
        status: pass
    human_judgment: false
  - id: D4
    description: "An injected refused-row failure returns Persist naming the surviving child c2 with replacement_committed false and the prior state and file intact"
    requirement: RMV-06
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#refused_row_failure_names_row_and_keeps_prior_state"
        status: pass
    human_judgment: false
  - id: D5
    description: "The save closure runs exactly once for a repository target with two managed children; both children are deleted, the main worktree kept, and the rows are gone from state and file"
    requirement: RMV-04
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#save_closure_is_called_exactly_once_for_parent_with_children"
        status: pass
    human_judgment: false
  - id: D6
    description: "A crash between delete and save for a checkout target leaves a file that parses to the prior state with validate Ok; reconcile marks the child Protected Missing and the retry completes"
    requirement: RMV-06
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#crash_before_save_reconciles_missing"
        status: pass
    human_judgment: false
  - id: D7
    description: "A crash mid-fan-out with every child directory gone keeps the parent and both children in the file; reconcile marks each Missing and the retry reports both under already_removed with the parent's main worktree kept"
    requirement: RMV-04
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#repository_crash_mid_fan_out_keeps_parent_and_children_in_file"
        status: pass
    human_judgment: false
  - id: D8
    description: "A disk fault on the second child after the first is gone returns DiskRemoval Fallback naming c2 with state, file and counter untouched; git's half-removal is visible in worktree list and the marker, and the retry reclaims c2 through the marker"
    requirement: RMV-04
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#repository_disk_failure_mid_fan_out_leaves_state_and_file_untouched"
        status: pass
    human_judgment: false
  - id: D9
    description: "A save-invalid candidate is refused with Validation before any signal or deletion: the live group is alive, the directory present, no file written, counter 0, handle still owned"
    requirement: KILL-03
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#validation_failure_is_detected_before_any_kill_or_delete"
        status: pass
    human_judgment: false

duration: 25 min
completed: 2026-11-04
status: complete
---

# Phase 18 Plan 05: Atomic-Save Failure Injection and Half-Removal Retry Summary

**Nine tests on real files and real process doubles pin the persistence and ordering contract of `removal::remove`: every `AtomicFailure` branch has a test, the save closure runs once per call, a crash between disk delete and save leaves a file that still parses to the prior consistent state for a checkout and for a repository mid-fan-out with the startup reconcile and a retry proving recovery, a per-child disk fault mid-fan-out is retryable through plan 02's removal marker, and candidate validation refuses before anything irreversible. No production code changed.**

## Performance

- **Duration:** 25 min
- **Started:** 2026-11-03T23:48:00Z (approximate; the session was compacted during the read phase)
- **Completed:** 2026-11-04T00:13:00Z
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments

- `counting_save_with(state_root, failure)` hands one `AtomicFailure` to `persist::save_current_at_test` on every call and counts calls; plan 04's `counting_save` now delegates to it with `None`, so every removal test injects faults through one seam
- `rename_failure_keeps_prior_state_then_retry_reports_already_removed` and `sync_failure_keeps_prior_state_and_file_bytes` prove the pre-rename failures leave `*state` equal to its pre-call clone and the file equal to its pre-call bytes with the directory already gone and the counter at 1; the sync fixture carries a live trapping runtime and observes the group extinct, the leader reaped, the handle still owned with `reader_finished()` true, then the retry reports the runtime once through plan 08's `Extinct` disposition and drains the handle
- `directory_sync_failure_commits_state_and_reports_degraded` is the only `Ok` on a failed closure: `SavedDegraded`, state committed, file parses without the row
- `refused_row_failure_names_row_and_keeps_prior_state` injects `RefusedRow(ValidationRow::Checkout(c2))` while removing c1 and asserts the refusal names c2 through `SaveError::validation().row()` and the message text, with c1's directory gone and c2's present
- `save_closure_is_called_exactly_once_for_parent_with_children` pins the no-per-child-save contract on a repository with two managed children
- `crash_before_save_reconciles_missing` and `repository_crash_mid_fan_out_keeps_parent_and_children_in_file` load the file after an injected write failure, assert it equals the pre-call state and validates, run `git::reconcile_checkout` plus `lifecycle::record_checkout_reconciliation` on each vanished child (both return `Missing` and set `Protected(Missing)`), and complete the removal on the reconciled state with every gone directory under `already_removed`
- `repository_disk_failure_mid_fan_out_leaves_state_and_file_untouched` pins `<c2>/pin` at `0o555` through `PinGuard` so c1 is deleted and c2 fails with `DiskRemovalFailureKind::Fallback`; `git worktree list` no longer lists c2 while `removal_marker_path(c2)` exists; state, bytes and counter untouched; after the guard restores the mode the retry reports c1 under `already_removed`, c2 under `paths_deleted`, and the marker is gone
- `validation_failure_is_detected_before_any_kill_or_delete` seeds a second repository whose `Inactive` child still records a runtime (`validate()` Ok, `validate_for_save()` Err, asserted as the premise) and proves a `Validation` refusal leaves the target's group alive, its directory present, the state unchanged, no file written and the handle owned

## TDD Gate Compliance

| Gate | Evidence |
| ---- | -------- |
| RED | Before any edit, the plan's own `fails_when` was applied per task: `cargo test -p baude-core removal::tests::<name> -- --test-threads=1` for `refused_row_failure_names_row_and_keeps_prior_state`, `repository_crash_mid_fan_out_keeps_parent_and_children_in_file` and `validation_failure_is_detected_before_any_kill_or_delete` each printed `running 0 tests` with `506 filtered out`, so the output lacked every named test. These tests pin behavior plans 04, 02 and 08 already shipped; the plan anticipates this with "if any test is red, the fix belongs in ..." and no production fix was needed. |
| GREEN | Task 1 commit `4c937aa`: `cargo test -p baude-core removal::tests -- --test-threads=1` 21 passed, 0 failed in 127.11 s. Task 2 commit `187981d`: the three new tests alone 3 passed in 33.62 s, then the filter 24 passed, 0 failed in 156.41 s. Task 3 commit `368120e`: the single test 1 passed in 5.10 s, then the filter 25 passed, 0 failed in 154.19 s. `cargo clippy -p baude-core --tests -- -D warnings` exit 0 and `cargo fmt -p baude-core -- --check` exit 0 before each commit. |
| REFACTOR | No refactor commit: the only production-adjacent change is `counting_save` delegating to `counting_save_with`, landed inside the Task 1 test commit. |

`gsd_run check tdd-red-evidence` has no adapter for `cargo test` text and reports `INVALID_RED` / `unreadable_record` on it, the gap plans 18-01 through 18-04 and 18-08 recorded. The plan's own RED gate was applied instead.

## Task Commits

1. **Task 1: Every AtomicFailure branch and the single-save proof** - `4c937aa` (test)
2. **Task 2: Crash and disk failure between delete and save, for a checkout and a repository mid-fan-out** - `187981d` (test)
3. **Task 3: Candidate validation refuses before any kill or delete** - `368120e` (test)

**Plan metadata:** see the docs commit that lands this file

## Files Created/Modified

- `baude-core/src/removal.rs` - nine `removal::tests` cases and the helpers `counting_save_with`, `inactive_row`, `inactive_pair`, `persist_error`, `PinGuard`, `set_mode`, `reconcile_missing`, `stale_runtime`; `counting_save` delegates to `counting_save_with`; test-module imports of `persist::AtomicFailure`, `repository::ValidationRow`, `git::ReconciliationUnavailable` and `lifecycle`

## Verification

- `cargo test -p baude-core removal::tests -- --test-threads=1`: 25 passed, 0 failed (host macOS, debug build, 154.19 s)
- `cargo clippy -p baude-core --tests -- -D warnings` and `cargo fmt -p baude-core -- --check` exit 0 after every task
- Acceptance greps on the final tree: five Task 1 test names 5; injected variants 4; `reader_finished()` 2; three Task 2 test names 3; `PermissionsExt` 1; `impl Drop for` 5; Task 3 test name 1; `RemovalError::Validation` 2; `record_checkout_reconciliation` 1 (see Deviations)
- `git log --oneline 9915114..HEAD` lists exactly the three task commits; `git diff --stat` over that range touches only `baude-core/src/removal.rs` (533 insertions, 8 deletions); no file deletions, no `Cargo.toml` change
- Full workspace suite: see the line appended below after the background run completed

## Decisions Made

- The refused-row message check uses the key's Debug rendering plus the typed `validation().row()` accessor, since `ValidationError`'s Display is `invalid repository state: {self:?}`; changing a public error message to match `ValidationRow`'s Display is outside this plan's test-only scope
- Both crash tests share `reconcile_missing`, which asserts the `Missing` classification, the `Ok(Some(UnavailableCause::Missing))` record result and the `Protected(Missing)` lifecycle in one place
- `PinGuard::new` creates `pin/held` and pins the mode; its Drop restores `0o755` unconditionally so a failed assertion still lets `RemovalFixture` remove its root

## Deviations from Plan

### Plan-shape adjustments

**1. Refused-row message assertion matches the shipped Display**
- **Found during:** Task 1, reading `ValidationError`'s Display before writing the test
- **Issue:** The behavior text expected `error.to_string()` to contain the row "as `ValidationRow`'s Display renders it" (`checkout N`); the shipped `ValidationError` Display renders `ContradictoryLifecycle(CheckoutKey(N))`
- **Fix:** Assert `error.validation().and_then(ValidationError::row) == Some(ValidationRow::Checkout(c2))` and that the message contains `format!("{c2:?}")`; no production change
- **Files modified:** baude-core/src/removal.rs
- **Commit:** `4c937aa`

**2. `record_checkout_reconciliation` grep proxy reads 1, not 2**
- **Found during:** Task 2 acceptance greps
- **Issue:** The plan counts source lines naming the function; both tests call it through the `reconcile_missing` helper, which is the shape that keeps the two reconcile assertions identical
- **Fix:** None; the substance (both tests exercise the call and assert its result) holds and the helper is not padded with a second mention
- **Files modified:** none
- **Commit:** `187981d`

**Total deviations:** 2 plan-shape adjustments, 0 auto-fixed bugs, 0 production changes

**Impact on plan:** None on behavior. Every truth in `must_haves` is asserted on fixture output. Shipped signatures the dispatch warned about were used as shipped.

## Issues Encountered

- The session was compacted mid-read; the plan, the shipped helpers and the persist seams were re-read before any edit, and the ledger file for this plan did not exist yet at that point, so the measured `plan_head_before` is the dispatch HEAD `9915114`
- The removal suite now takes about 155 s single-threaded because the parked-reader fixture and the live-runtime fixtures each wait on real signals; the plan's 600 s cap was never approached

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: baude-core/src/removal.rs
- FOUND: .planning/phases/18-shared-kill-then-remove-lifecycle/18-05-SUMMARY.md
- FOUND: 4c937aa, 187981d, 368120e (ancestors of HEAD)
