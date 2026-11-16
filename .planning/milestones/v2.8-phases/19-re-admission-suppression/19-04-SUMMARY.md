---
phase: 19-re-admission-suppression
plan: 04
subsystem: removal
tags: [rust, removal, marker, suppression, clearing-stage, baude-core, tdd]

# Dependency graph
requires:
  - phase: 19-re-admission-suppression
    provides: "19-01 clearing stage skeleton in removal.rs with ClearedItem, ClearingFailure, planned_clearings and clear_stage for bindings; 19-02 suppression store with suppress, drop_children and load; 19-03 marker::clear_owned with MarkerOutcome, DirectoryOutcome and ClearRefusal"
  - phase: 08-workspace-identity
    provides: "SavedRepository::physical_key and the repository-<key> managed directory under git::worktrees_base()/<workspace>"
provides:
  - "ClearingKind::{Marker, RepositoryDir, SuppressionAdded, SuppressionDropped} alongside the 19-01 Binding kind"
  - "planned_clearings now emits marker, repository dir and suppression items so preview_removal names everything the real run clears"
  - "clear_stage runs marker::clear_owned once per removed repository with the stored physical_key and observed_common_dir bytes, then suppress for each child whose parent survives and drop_children for each removed repository"
  - "repository_dir(repo) -> Result<PathBuf, RepositoryDirRefusal>: empty keys resolve to a silent missing-directory no-op, keys with separators or dot components are refused and reported under the bare repository-<key> name, never joined under the managed root"
  - "Refusals and non-empty directories land in clearing_failures while remove still returns Ok and the state is saved"
affects: [19-05, 19-06, 19-07, 20-removal-confirmation]

# Actuals (#2632)
actuals:
  tokens: 12322
  tasks: 3
  commits: 3
plan_head_before: 2d87cf67316e6595f392fd7d4eef7e4d6a695c67
plan_head_after: b2a2b28ee424d77b4534cd0bc72b94039d82ed9a

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Plan and execute mirror each other per kind: plan_bindings, plan_markers and plan_suppression under planned_clearings, clear_bindings, clear_markers and update_suppression under clear_stage, so preview agreement is structural"
    - "Clearing never raises: every marker or directory outcome maps onto ClearedItem or ClearingFailure and the removal proceeds to persist_stage regardless"
    - "Path from stored identity only: the managed directory is built from SavedRepository::physical_key and never from a fresh digest of the common dir"

key-files:
  created: []
  modified:
    - baude-core/src/removal.rs

key-decisions:
  - "A legacy or empty physical_key is treated as a missing directory and produces no item and no failure, matching the plan's silent no-op; only keys that could escape the managed root are reported as failures"
  - "A refused key is reported under the bare repository-<key> name rather than a joined path so the report never shows a path the stage would not touch"
  - "Suppression entries are keyed by folder_key of the checkout's observed_path with repository_common_dir set to folder_key of the surviving parent's observed_common_dir, exactly the shape 19-02's load returns"
  - "The 19-01 test checkout_and_standalone_removals_clear_their_own_keys was extended in RED to assert the new suppression items rather than duplicated"

patterns-established:
  - "Neighbour-safety proof: a fixture with two repositories bound to one workspace plus a third holding a managed worktree, asserting byte-identical bindings, marker bytes, worktree listings and suppression files after removing one"

requirements-completed: [READM-02, READM-03, READM-05]

coverage:
  - id: D1
    description: "Removing a repository row clears its owned marker and managed directory through clear_owned using the stored physical_key, and ensure_repository afterwards allocates the un-suffixed directory with no CollisionReport"
    requirement: READM-02
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::repository_removal_clears_owned_marker_and_directory -- --test-threads=1"
        status: pass
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::repository_dir_comes_from_stored_physical_key -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D2
    description: "Removing a single checkout under a surviving repository leaves the parent marker and directory byte-identical and writes one suppression entry for the checkout"
    requirement: READM-03
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::child_removal_keeps_parent_marker_and_writes_suppression -- --test-threads=1"
        status: pass
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::repository_removal_drops_child_suppression_and_standalone_writes_none -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D3
    description: "Marker refusals and non-empty directories are reported in clearing_failures with the directory left in place; remove returns Ok and the state is saved"
    requirement: READM-02
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::marker_refusal_and_non_empty_dir_are_reported_not_raised -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D4
    description: "Removing one repository leaves neighbouring bindings, markers, managed worktree directories and suppression entries byte-identical"
    requirement: READM-05
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::removal_leaves_neighbour_bindings_markers_worktrees_and_suppressions_byte_identical -- --test-threads=1"
        status: pass
    human_judgment: false
  - id: D5
    description: "preview_removal names every marker, repository dir and suppression item the real run reports in cleared or clearing_failures"
    requirement: READM-05
    verification:
      - kind: unit
        ref: "cargo test -p baude-core removal::tests::checkout_and_standalone_removals_clear_their_own_keys -- --test-threads=1"
        status: pass
    human_judgment: false

# Metrics
duration: 19min
completed: 2026-11-04
status: complete
---

# Phase 19 Plan 04: Marker, Directory and Suppression Clearing Summary

**The removal clearing stage now releases a removed repository's marker and managed directory through `marker::clear_owned`, writes a suppression entry for a checkout removed under a surviving parent, drops child entries when the parent goes, and reports refusals instead of raising; `preview_removal` names all of it in advance.**

## Performance

- **Duration:** 19 min
- **Started:** 2026-11-04T18:23:37Z
- **Completed:** 2026-11-04T18:42:11Z
- **Tasks:** 3 (RED, GREEN, REFACTOR)
- **Files modified:** 1

## Accomplishments

- `ClearingKind` gained `Marker`, `RepositoryDir`, `SuppressionAdded` and `SuppressionDropped`; `planned_clearings` emits them so `preview_removal` agrees with the real run for every kind.
- `clear_stage` calls `marker::clear_owned` once per removed repository on `git::worktrees_base()/<workspace>/repository-<physical_key>` with the row's `observed_common_dir` bytes; `MarkerOutcome` and `DirectoryOutcome` map onto `ClearedItem` and `ClearingFailure`.
- Suppression: `suppress` for each removed checkout whose repository survives, keyed by `folder_key(observed_path)`; `drop_children` for each removed repository's common dir. Repository-parent and standalone removals write no entry.
- `repository_dir` builds the path only from the stored `physical_key`; empty keys are a silent no-op and keys containing separators or dot components are refused and reported under the bare name.
- READM-05 neighbour-safety test proves two sibling repositories and a third with a managed worktree are byte-identical after removing one.
- Production region of `removal.rs` still contains zero `remove_dir_all` calls.

## Task Commits

1. **Task 1: RED tests** - `a981de2` (test)
2. **Task 2: GREEN implementation** - `9d91197` (feat)
3. **Task 3: REFACTOR per-kind helpers** - `b2a2b28` (refactor)

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `a981de2` | `cargo test -p baude-core removal::tests -- --test-threads=1` exit 101: 42 passed, 7 failed (the 6 new tests plus the extended `checkout_and_standalone_removals_clear_their_own_keys`), all panicking on missing `Marker`, `RepositoryDir` and `Suppression*` items |
| GREEN | `9d91197` | same command exit 0: 49 passed, 0 failed |
| REFACTOR | `b2a2b28` | same command exit 0: 49 passed, 0 failed; `cargo fmt --all -- --check` exit 0; `cargo clippy -p baude-core --all-targets -- -D warnings` exit 0 |

## Test Commands and Exit Codes

| Command | Exit | Result |
|---------|------|--------|
| `cargo test -p baude-core removal::tests -- --test-threads=1` (RED) | 101 | 42 passed, 7 failed |
| `cargo test -p baude-core removal::tests -- --test-threads=1` (GREEN) | 0 | 49 passed |
| `cargo test -p baude-core removal::tests -- --test-threads=1` (REFACTOR) | 0 | 49 passed |
| `cargo fmt --all -- --check` | 0 | clean |
| `cargo clippy -p baude-core --all-targets -- -D warnings` | 0 | clean after one needless-borrow fix in a new test |
| production-region `remove_dir_all` count | 0 | matches baseline |

## Files Created/Modified

- `baude-core/src/removal.rs` - new clearing kinds, `repository_dir` guard, `plan_markers`, `plan_suppression`, `clear_markers`, `update_suppression`, six new tests and three fixture helpers (`fixture_identity`, `suppressions_on_disk`, `unix_millis` for entry timestamps)

## Decisions Made

- Empty or legacy `physical_key` is a silent missing-directory no-op; only keys that could escape the managed root become a `ClearingFailure` (D-05 plus the plan's path-component prohibition).
- Refused keys are reported under the bare `repository-<key>` name so the report never names a path under the managed root that was not touched.
- The 19-01 test covering preview agreement was extended rather than duplicated, which is why RED showed 7 failures for 6 new tests.

## Deviations from Plan

None - plan executed as written. One clippy `needless_borrow` in a new test was fixed before the GREEN commit; it was in this plan's own code, not a deviation.

## Must-Haves Self-Check

| Truth | Status | Verified by |
|-------|--------|-------------|
| Repository removal clears marker and dir via stored physical_key; ensure_repository re-allocates un-suffixed with no CollisionReport | met | `repository_removal_clears_owned_marker_and_directory`, `repository_dir_comes_from_stored_physical_key` |
| Checkout-only removal never touches the parent marker or directory | met | `child_removal_keeps_parent_marker_and_writes_suppression` |
| Refusals and non-empty dirs reported, nothing recursive, remove Ok and saved | met | `marker_refusal_and_non_empty_dir_are_reported_not_raised`, `remove_dir_all` count 0 |
| Suppression written for child under surviving parent, none for parent or standalone, dropped on parent removal | met | `repository_removal_drops_child_suppression_and_standalone_writes_none` |
| Neighbours byte-identical | met | `removal_leaves_neighbour_bindings_markers_worktrees_and_suppressions_byte_identical` |
| Preview names every kind the real run reports | met | `checkout_and_standalone_removals_clear_their_own_keys` and per-test preview assertions |
| Nothing deleted inside kept checkouts or standalone folders | met | neighbour test listings and `assert_contained` on every removed path |

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 19-05 can consume `ClearingKind::SuppressionAdded` and the on-disk entry shape to implement the read side of READM-03.
- 19-07 owns `scripts/gate.sh`; this plan ran only the per-task test, fmt and clippy commands.

---
*Phase: 19-re-admission-suppression*
*Completed: 2026-11-04*

## Self-Check: PASSED

- Commits a981de2, 9d91197, b2a2b28 are ancestors of HEAD
- baude-core/src/removal.rs modified; no files created
