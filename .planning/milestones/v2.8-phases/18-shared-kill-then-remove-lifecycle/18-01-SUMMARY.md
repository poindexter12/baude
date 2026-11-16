---
phase: 18-shared-kill-then-remove-lifecycle
plan: 01
subsystem: lifecycle
tags: [rust, lifecycle, removal, serde, tdd]

# Dependency graph
requires:
  - phase: 06-shared-lifecycle-core-refactor
    provides: RepositoryState aggregate, SavedCheckout roles, OwnedRuntime and ShellOwnership identity
provides:
  - baude-core/src/removal.rs with the shared removal vocabulary (RemovalTarget, RowKey, DiskAction, PlannedRow, RemovalSet, KilledRuntime, PersistenceOutcome, RemovalReport, DiskRemovalFailure, DiskRemovalFailureKind, RemovalError)
  - select_rows: target resolution, children-first parent-last fan-out, per-row DiskAction classification with a NotFound-only already-removed probe
  - preview_removal: side-effect-free RemovalReport that round-trips through serde_json
  - pub mod removal registered in baude-core/src/lib.rs
affects: [18-02, 18-03, 18-04, 18-05, 18-06, 18-07, 18-08, phase-20-tui, phase-21-daemon]

# Actuals (#2632) - estimateTokens scale, chars/4 over the realized diff
actuals:
  tokens: 8715
  tasks: 3
  commits: 2
plan_head_before: 73fa674543e166eefd0c833e415c1bdf3f297b51
plan_head_after: 506ee3758d443145e0b57741daf6910934d7bd2f

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "DiskAction computed once per row from persisted role and managed_by_baude, reused by preview and by the real run"
    - "Removal-owned DiskRemovalFailure so the module compiles without the git helper; plan 04 maps ForcedRemovalError into it"
    - "Drop-guarded TempTree fixture root so a failing assertion still removes its directories"

key-files:
  created:
    - baude-core/src/removal.rs
  modified:
    - baude-core/src/lib.rs

key-decisions:
  - "RemovalError::RuntimeSurvived boxes SessionTeardownError so select_rows and preview_removal pass clippy result_large_err under -D warnings; plan 04 constructs it with Box::new"
  - "The repository parent PlannedRow carries main_worktree and common_dir from its own SavedRepository so plan 04 can prune stale worktree administration without a second lookup"
  - "Unknown targets are detected per key kind: a checkout whose parent repository row is missing is also UnknownTarget rather than a panic"

patterns-established:
  - "Classification table: Main -> KeepDirectory; managed non-main -> RemoveManagedWorktree; unmanaged -> KeepDirectory; standalone and parent rows -> KeepDirectory"
  - "Only ErrorKind::NotFound downgrades a managed row to AlreadyRemoved; ENOTDIR and EACCES keep the row for the typed disk refusal"

requirements-completed: [RMV-05, RMV-04, RMV-02, RMV-03]

coverage:
  - id: D1
    description: "select_rows fans a repository target out to every child checkout in state order with the parent row last, excluding other repositories' checkouts"
    requirement: RMV-04
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#select_rows_repository_target_fans_out_children_first_parent_last"
        status: pass
    human_judgment: false
  - id: D2
    description: "Main, unmanaged and standalone rows are KeepDirectory; managed PrimaryDefault and ManagedBranch rows are RemoveManagedWorktree; a missing managed directory is AlreadyRemoved and a non-NotFound probe error keeps the row"
    requirement: RMV-02
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#select_rows_classifies_disk_action_by_role_and_management"
        status: pass
      - kind: unit
        ref: "baude-core/src/removal.rs#select_rows_marks_missing_managed_directory_already_removed"
        status: pass
      - kind: unit
        ref: "baude-core/src/removal.rs#select_rows_keeps_managed_row_when_probe_fails_without_not_found"
        status: pass
    human_judgment: false
  - id: D3
    description: "Main checkouts and standalone folders are never classified for deletion"
    requirement: RMV-03
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#select_rows_classifies_disk_action_by_role_and_management"
        status: pass
    human_judgment: false
  - id: D4
    description: "preview_removal reports the same row set the real run will use, leaves state and disk untouched, and the report round-trips through serde_json"
    requirement: RMV-05
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#preview_has_no_side_effects_and_report_round_trips_serde"
        status: pass
      - kind: unit
        ref: "baude-core/src/removal.rs#select_rows_carries_owned_runtime_identity"
        status: pass
    human_judgment: false
  - id: D5
    description: "An unknown RepositoryKey, CheckoutKey or StandaloneKey yields RemovalError::UnknownTarget from both select_rows and preview_removal"
    requirement: RMV-05
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#select_rows_unknown_target_is_typed_error"
        status: pass
    human_judgment: false

# Metrics
duration: 7 min
completed: 2026-11-03
status: complete
---

# Phase 18 Plan 01: Row Selection and Side-Effect-Free Removal Preview Summary

**New `baude-core::removal` module: typed removal vocabulary plus `select_rows` and `preview_removal`, which resolve any row kind against `RepositoryState`, classify each row's disk rule once, and produce a serde round-trippable report without signalling, running git or writing a file**

## Performance

- **Duration:** 7 min
- **Started:** 2026-11-03T22:06:24Z
- **Completed:** 2026-11-03T22:13:36Z
- **Tasks:** 3 (RED, GREEN, REFACTOR with no change needed)
- **Files modified:** 2

## Accomplishments
- `select_rows` resolves repository, checkout and standalone targets, fans a repository out to its checkouts in state order with the parent last, and classifies every row from persisted role and `managed_by_baude` alone
- Managed rows probe `symlink_metadata` and downgrade to `AlreadyRemoved` only on `NotFound`; ENOTDIR keeps the row so plan 02's helper reports a typed failure (proven with a regular file blocking the path)
- `preview_removal` emits a `RemovalReport` with `planned: true`, `persistence: Planned`, kill targets derived from the whole `OwnedRuntime` (agent pid and owned shell pid), and path buckets per `DiskAction`; the test asserts state equality, no state file, fixture directories intact, and serde equality
- `RemovalError` and `DiskRemovalFailure` are owned by `removal.rs`, so the module compiles with plan 02 absent (`crate::git::ForcedRemovalError` is never named)

## TDD Gate Compliance

- RED commit `2ec0e5a` (`test(18-01): ...`): 1 passed, 6 failed, zero `error[E` lines; every failure is the planned assertion against the `UnknownTarget` stub (semantic inspection of the captured output)
- GREEN commit `506ee37` (`feat(18-01): ...`) follows RED: 7 passed, 0 failed; `cargo clippy -p baude-core --all-targets -- -D warnings` exit 0
- REFACTOR: no commit; no duplication to extract (`disk_action_for` already private, fixtures already share `TempTree::dir`/`absent`)
- `gsd_run check tdd-red-evidence` returned `INVALID_RED` / `invalid_record` with `Unsupported report format`: the classifier has no adapter for `cargo test` text output and the only supported reporters for Rust would need a new tool or crate, which this plan's T-18-SC forbids. The plan's own RED gate (cargo exit non-zero, `6 failed` and `1 passed`, no compile errors, named test present) was applied and passed; tdd.md documents this as the known Rust gap (#4379).

## Task Commits

1. **Task 1 (RED): failing selection and preview tests, module registered** - `2ec0e5a` (test)
2. **Task 2 (GREEN): implement select_rows and preview_removal** - `506ee37` (feat)
3. **Task 3 (REFACTOR, conditional)** - no refactor needed; tests and `cargo fmt --check` re-run green, public item grep prints 9

**Plan metadata:** see the docs commit that lands this file

## Files Created/Modified
- `baude-core/src/removal.rs` - module doc (stage order, reservation hold, non-reuse of close and tombstone protocols), all shared types, `select_rows`, `preview_removal`, private `find_repository`, `checkout_row`, `disk_action_for`, `probe_disk`, `report_from_set`, and seven unit tests with a Drop-guarded `TempTree`
- `baude-core/src/lib.rs` - `pub mod removal;` between `pty` and `repository`

## Decisions Made
- Boxed `SessionTeardownError` inside `RemovalError::RuntimeSurvived` (see deviations); public function signatures are unchanged from the plan
- The parent repository row also carries `main_worktree` and `common_dir` (the plan specified them for checkout rows and `None` for standalone rows; the parent was unspecified). Harmless for preview and saves plan 04 a lookup when pruning
- `RemovalTarget::Checkout` whose parent repository row is absent from state returns `UnknownTarget` instead of panicking on the missing parent

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Boxed the `SessionTeardownError` payload of `RemovalError::RuntimeSurvived`**
- **Found during:** Task 2 (GREEN clippy gate)
- **Issue:** `cargo clippy -p baude-core --all-targets -- -D warnings` failed with `result_large_err` on `select_rows` and `preview_removal` because the plan-declared `RuntimeSurvived { error: SessionTeardownError }` makes the error at least 128 bytes
- **Fix:** `error: Box<crate::session::SessionTeardownError>`; `Error::source` returns `error.as_ref()`. No crate precedent for `#[allow(clippy::result_large_err)]`, so boxing was chosen. Plan 04 constructs the variant with `Box::new(err)`
- **Files modified:** baude-core/src/removal.rs
- **Verification:** clippy exit 0, 7 tests pass
- **Committed in:** 506ee37

**2. [Rule 3 - Blocking] Elided a needless lifetime on the test helper `row`**
- **Found during:** Task 2 (GREEN clippy gate, `--all-targets`)
- **Issue:** `needless_lifetimes` denied under `-D warnings`
- **Fix:** `fn row(set: &RemovalSet, key: RowKey) -> &PlannedRow`
- **Files modified:** baude-core/src/removal.rs
- **Verification:** clippy exit 0
- **Committed in:** 506ee37

---

**Total deviations:** 2 auto-fixed (2 blocking, both clippy under `-D warnings`)
**Impact on plan:** Plan 04 must wrap the teardown error in `Box::new` when raising `RuntimeSurvived`. No scope change; public function signatures and every other declared type match the plan.

## Issues Encountered
None beyond the clippy findings above. Full `cargo test -p baude-core`: 462 passed, 0 failed, 1 ignored.

## Known Stubs
None. `select_rows` and `preview_removal` are fully implemented; the stage functions (kill, disk, save) belong to plans 02 through 04 by design and are not stubbed here.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plans 02 (forced worktree removal in git.rs) and 03 (`RepositoryState::remove_rows`) can proceed in parallel; neither depends on this module's internals, only on its types
- Plan 04 consumes `RemovalSet`, `PlannedRow.runtime`, `main_worktree`, `common_dir`, `report_from_set`-shaped reporting, `DiskRemovalFailure` and the boxed `RuntimeSurvived`
- Test fixtures (`fixture`, `TempTree`, `checkout`, `standalone`) are save-valid builders plan 04 can reuse

---
*Phase: 18-shared-kill-then-remove-lifecycle*
*Completed: 2026-11-03*

## Self-Check: PASSED
