---
phase: 19-re-admission-suppression
plan: 01
subsystem: removal
tags: [rust, removal, folder-workspace, tracer, tdd, bindings]

# Dependency graph
requires:
  - phase: 18-shared-kill-then-remove-lifecycle
    provides: "removal::remove with the select, kill, disk and persist stages, RemovalReport, preview_removal and report_from_set"
  - phase: 13-folder-workspace-memory
    provides: "folder_workspace::record and find_binding over folder-workspaces.json through breadcrumbs::locked_merge_write"
provides:
  - "folder_workspace::clear(root, folder) -> io::Result<bool>: exact folder_key removal through the existing merge lock, no write when the key or file is absent"
  - "ClearingKind, ClearedItem, ClearingFailure and the RemovalReport fields cleared and clearing_failures"
  - "planned_clearings(state, set): one Binding per removed folder, children first, shared folders listed once"
  - "clear_stage(planned) as the fourth stage of removal::remove, after persist_stage, never Err"
  - "preview_removal reports the planned clearing list so the real run's cleared and failures are a subset of it"
affects: [19-02, 19-03, 19-04, 19-07, 20-removal-confirmation]

# Actuals (#2632)
actuals:
  tokens: 6389
  tasks: 3
  commits: 4
plan_head_before: bfc409cf454b72c9c8cfd0f7ad425bbe4522d991
plan_head_after: fd4a84e6ec0949f32e90e46c73c71f3373a6c486

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Post-commit best-effort stage: clearing runs after the save, returns a results struct instead of Result, and maps every I/O error into the report"
    - "Preview parity by construction: one pure planned_clearings function feeds both report_from_set arms"

key-files:
  created: []
  modified:
    - baude-core/src/folder_workspace.rs
    - baude-core/src/removal.rs

key-decisions:
  - "planned_clearings lists a folder shared by two rows once: a main checkout and its repository name the same folder, the stage clears it once, so the preview and the real run agree without a second attempt"
  - "clear pre-checks the key outside the merge lock; the check-then-write gap is accepted for an advisory file because it is what keeps an absent file absent and an untouched file byte-identical"
  - "Task 2 needed no production change: all four expansion tests passed against the Task 1 stage, committed as one test commit as the plan allowed"
  - "Task 3 refactor skipped: planned_clearings already had one call per entry point and report_from_set already took the list for both arms; only the agreement assertion was added"

patterns-established:
  - "ClearingKind declares all five phase-19 kinds now so plans 03 and 04 add no serde-visible shape change"
  - "Failure injection for locked_merge_write: a directory at .<file>.lock makes the lock open fail after the state save committed"

requirements-completed: [READM-01, READM-05]

coverage:
  - id: D1
    description: "A removed repository's main-worktree binding is gone from folder-workspaces.json, the neighbour sharing the workspace value is byte-identical, preview and real run name the same cleared item"
    requirement: READM-01
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::clear_stage_removes_repository_binding_and_reports_it -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D2
    description: "Checkout rows clear observed_path and standalone rows clear canonical_path by exact folder_key match; neighbours untouched"
    requirement: READM-05
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::checkout_and_standalone_removals_clear_their_own_keys -- --test-threads=1"
        status: pass
      - kind: unit
        ref: "cargo test -p baude-core folder_workspace::tests::clear_removes_only_the_named_key -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D3
    description: "A removal with no recorded binding leaves the bindings file absent or byte-identical"
    requirement: READM-05
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::standalone_without_binding_leaves_bindings_file_byte_identical -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D4
    description: "A binding I/O failure lands in clearing_failures while remove returns Ok with the save committed"
    requirement: READM-01
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::binding_clear_failure_is_reported_not_raised -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D5
    description: "Real run cleared plus clearing_failures is a subset of the preview's cleared; RemovalReport still round-trips serde"
    requirement: READM-01
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::remove_report_matches_preview_rows -- --test-threads=1"
        status: pass
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::preview_has_no_side_effects_and_report_round_trips_serde -- --test-threads=1"
        status: pass
    human_judgment: false

# Metrics
duration: 35min
completed: 2026-11-04
status: complete
---

# Phase 19 Plan 01: Binding clear as the fourth removal stage Summary

**Repository, checkout and standalone removals now forget their exact `folder-workspaces.json` entry in a fourth stage after the save commits, report what they cleared in `RemovalReport.cleared`, and the preview names the same list.**

## Performance

- **Duration:** about 35 min
- **Started:** 2026-11-04T17:25:00Z (approximate; first commit 17:45Z)
- **Completed:** 2026-11-04T18:02:00Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- `folder_workspace::clear` removes one `folder_key` entry through the same `locked_merge_write` path `record` uses, pre-checking the key so an absent file stays absent and an untouched file keeps its bytes (D-01, D-03).
- `removal::remove` gained `planned_clearings` before the stages and `clear_stage` after `persist_stage`; the stage never returns `Err` and maps every I/O failure into `ClearingFailure` so a stuck advisory file cannot fail a removal whose row is already saved out (D-14).
- `preview_removal` passes the planned list into `report_from_set`, so Phase 20's confirmation can show what the real run will clear; the agreement test proves the real run's cleared plus failed pairs are a subset of the preview.
- Five new tests cover the repository tracer, checkout and standalone keys, the no-binding no-op, failure injection, and the unit behaviour of `clear`.

## Task Commits

1. **Task 1 RED: failing binding-clear tracer test** - `0d83881` (test)
2. **Task 1 GREEN: clear the folder-workspace binding as a fourth removal stage** - `2521628` (feat)
3. **Task 2: cover checkout, standalone and failure paths** - `7712107` (test; no production change needed)
4. **Task 3: assert the real run clears a subset of the preview list** - `fd4a84e` (test; no refactor needed)

**Plan metadata:** recorded in the final docs commit.

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `0d83881` | `cargo test -p baude-core removal::tests::clear_stage_removes_repository_binding_and_reports_it` exit 101: E0422 `ClearedItem`, E0433 `ClearingKind`, E0609 no field `cleared` / `clearing_failures` on `RemovalReport` |
| GREEN | `2521628` | same command exit 0, `1 passed`; `preview_has_no_side_effects_and_report_round_trips_serde` exit 0, `1 passed` |
| REFACTOR | skipped by design | Task 3's refactor condition did not hold; only the agreement assertion was added in `fd4a84e` |

Task 2 (`tdd="true"`): the four tests were written first; `binding_clear_failure_is_reported_not_raised` failed on its first run because of a fixture collision (see Issues), the other three passed immediately against Task 1's implementation, which the plan explicitly accepted. No GREEN production commit was needed.

## Verification Results

| Command | Exit | Result |
|---------|------|--------|
| `cargo test -p baude-core removal::tests::clear_stage_removes_repository_binding_and_reports_it -- --test-threads=1` | 0 | 1 passed |
| `cargo test -p baude-core removal::tests::preview_has_no_side_effects_and_report_round_trips_serde -- --test-threads=1` | 0 | 1 passed |
| `cargo test -p baude-core removal::tests -- --test-threads=1` | 0 | 43 passed, 0 failed |
| `cargo test -p baude-core folder_workspace::tests -- --test-threads=1` | 0 | 15 passed, 0 failed |
| `cargo fmt --check` | 0 | no diff |
| `cargo clippy -p baude-core --all-targets -- -D warnings` | 0 | clean |
| `cargo build --workspace --all-targets` | 0 | no other `RemovalReport {` literal exists outside `removal.rs` |
| `grep -n 'remove_dir_all\|fs::write(' baude-core/src/folder_workspace.rs` | n/a | only the two pre-existing test-module calls; none added by this plan |

## Files Created/Modified

- `baude-core/src/folder_workspace.rs` - `pub fn clear(root: Option<&Path>, folder: &Path) -> io::Result<bool>` beside `record`; unit test `clear_removes_only_the_named_key`.
- `baude-core/src/removal.rs` - `ClearingKind` (all five variants), `ClearedItem`, `ClearingFailure`, `RemovalReport.cleared` and `.clearing_failures`, `planned_clearings`, `ClearingResults`, `clear_stage`, `StageResults.clearing`, `report_from_set(set, planned, stages)`; four new tests and the extended `remove_report_matches_preview_rows`.

## Decisions Made

- **Shared folders listed once in `planned_clearings`.** The plan's literal "one Binding per row" produced the main-worktree path twice for a repository removal (once from the Main checkout row's `observed_path`, once from the repository row's `main_worktree`), while the real run clears it once and the second attempt returns `Ok(false)`. Deduplicating by `(kind, path)` in first-seen order keeps the plan's own GREEN assertion (`preview.cleared == report.cleared`) and the must-have subset truth without a pointless second attempt.
- **`clear_stage` takes only the planned list**, as the plan directs, so `clippy -D warnings` sees no unused parameter; plan 04 widens it when the stage first reads `removed` and `state`.
- **All five `ClearingKind` variants declared now** with explicit no-op arms in `clear_stage`, so plans 03 and 04 add behaviour without changing the serde shape.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `planned_clearings` emitted a duplicate item for a repository removal**
- **Found during:** Task 1 (GREEN)
- **Issue:** Following the action text literally, the Main checkout row and the repository row both mapped to `f.repo()`, so the preview listed two items while the real run cleared one; the plan's own test failed on `preview.cleared == report.cleared`.
- **Fix:** `planned_clearings` skips an item equal to one already planned, keeping `set.rows` order for the first occurrence.
- **Files modified:** `baude-core/src/removal.rs`
- **Verification:** `clear_stage_removes_repository_binding_and_reports_it` and `remove_report_matches_preview_rows` pass; the latter pins the order (main checkout folder first, managed child second).
- **Committed in:** `2521628`

---

**Total deviations:** 1 auto-fixed (Rule 1)
**Impact on plan:** Required for the plan's own acceptance criterion; no scope change.

## Issues Encountered

- `binding_clear_failure_is_reported_not_raised` first failed with `AlreadyExists` because `folder_workspace::record` leaves its `.folder-workspaces.json.lock` file behind and the fixture tried to create a directory at that path. The fixture now removes the leftover lock file before occupying the path with a directory; the implementation was already correct.
- `clippy -D warnings` rejected `assert_eq!(…, true/false)` in the new `folder_workspace` unit test (`bool_assert_comparison`); switched to `assert!` / `assert!(!…)`.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None. `ClearingKind::Marker`, `RepositoryDir`, `SuppressionAdded` and `SuppressionDropped` are declared with explicit no-op arms in `clear_stage` by plan design; plans 03 and 04 give them behaviour. They are not produced by anything in this plan.

## Next Phase Readiness

- Plan 03 (marker) and plan 04 (suppression and `RemovedRows`) can add kinds to `planned_clearings` and arms to `clear_stage` without touching the stage order or the report shape.
- Plan 04 widens `clear_stage` to `(removed, state, planned)` when it binds `candidate.remove_rows`'s return; the call site in `remove` is the only one.
- Phase 20's confirmation can read `preview_removal(...).cleared` directly.
- The full `scripts/gate.sh` run is owned by plan 19-07, as directed; this plan ran the per-crate test, fmt and clippy gates only.

---
*Phase: 19-re-admission-suppression*
*Completed: 2026-11-04*

## Self-Check: PASSED

- Files present: baude-core/src/folder_workspace.rs, baude-core/src/removal.rs, 19-01-SUMMARY.md
- Commits on HEAD: 0d83881, 2521628, 7712107, fd4a84e
