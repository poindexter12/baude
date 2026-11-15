---
phase: 18-shared-kill-then-remove-lifecycle
plan: 06
subsystem: lifecycle
tags: [rust, git, fixtures, integration, tdd]

# Dependency graph
requires:
  - phase: 18-shared-kill-then-remove-lifecycle
    provides: 18-01 select_rows disk classes and the checkout, repository and standalone builders; 18-02 remove_managed_worktree_forced with the prune fallback for already-removed rows; 18-03 RepositoryState::remove_rows; 18-04 removal::remove stage order, RemovalFixture, spawn_trapping_runtime, seed_live, handle_for, LiveHandles, counting_save, worktree_list, group_is_extinct, assert_reaped_promptly; 18-08 Session::kill_and_wait group extinction; testing::TestRedirect and git::worktrees_base
provides:
  - testing::git_in, testing::ClonedTopology and testing::cloned_topology, a cloned repository fixture with an upstream, a clone on chore/workspace, a default worktree on main and a managed worktree on feature/kept, both linked worktrees under the redirected managed root
  - eleven removal::tests cases proving the keep rules for main, standalone and unmanaged rows with and without live runtimes, already-removed rows in both lifecycles, unclean cwd-held managed deletion on the cloned and git init fixtures, and repository fan-out with every disk class in one call
  - test helpers in removal::tests: branch_snapshot over for-each-ref, ClonedRows and seed_cloned, outside_worktree, notes_folder, seed_notes, no_handles, make_unclean
affects: [18-07, 20-tui-remove-action, 21-daemon-and-pwa-parity]

# Actuals (#2632) - estimateTokens scale, chars/4 over the realized diff
actuals:
  tokens: 8210
  tasks: 3
  commits: 3
plan_head_before: 10a6e436b65afe947ce6824da81d4f5796c010a0
plan_head_after: ba34462e26d20abbb06e6d1e460bc83279822265

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Branch integrity is pinned by comparing sorted refname and OID lines from git for-each-ref before and after a removal, never git branch --list, whose + and * decorations change when a linked worktree disappears"
    - "The cloned fixture is seeded through plan 01's builders with one Main, one PrimaryDefault and one ManagedBranch row per repository and the clone's discovered common dir, and every seed asserts validate_for_save before the call"
    - "Every live runtime a test holds sits in a LiveHandles guard so a failed assertion still stops it; the handle must be consumed by remove, so handles.0.is_empty is asserted on every kill path"

key-files:
  created: []
  modified:
    - baude-core/src/testing.rs
    - baude-core/src/removal.rs

key-decisions:
  - "cloned_topology asserts an active TestRedirect before touching the managed root, so a caller that forgets the redirect fails at the fixture rather than writing into the real managed root"
  - "The git init fixture's tracked file is tracked.txt while the cloned topology's is tracked; each test asserts on its own fixture's real file rather than a shared name"
  - "ClonedTopology was dropped from the Task 1 import and re-added in Task 2, since clippy under -D warnings refuses an unused import and the struct is only named by Task 2's seed helper"

patterns-established:
  - "seed_cloned(state, &topology) -> ClonedRows { k, m, d, b } is the one way a removal test seeds the cloned fixture; outside_worktree adds the unmanaged linked worktree on feature/outside when a test needs the fourth disk class"

requirements-completed: [RMV-01, RMV-02, RMV-03, RMV-04, RMV-07, KILL-01, KILL-02]

coverage:
  - id: D1
    description: "cloned_topology places both linked worktrees under the redirected managed root, the clone sits on chore/workspace, the default worktree on main and the managed worktree on feature/kept, and git worktree list in the clone names both"
    requirement: RMV-01
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#cloned_topology_places_worktrees_under_managed_root"
        status: pass
    human_judgment: false
  - id: D2
    description: "Removing the Main row on the cloned fixture keeps the clone directory and its tracked file, reports it under paths_kept, leaves the other rows and the repository, validates, and leaves the for-each-ref snapshot identical"
    requirement: RMV-02
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#main_checkout_keeps_directory_and_branches"
        status: pass
    human_judgment: false
  - id: D3
    description: "Removing an Inactive standalone row keeps the folder and its file, reports it under paths_kept and drops the row from state and file"
    requirement: RMV-03
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#standalone_keeps_folder"
        status: pass
    human_judgment: false
  - id: D4
    description: "A managed row whose directory is gone without a prune is removed with the path under already_removed and nothing under paths_deleted, the porcelain entry pruned and feature/kept resolving to the same OID, both as Inactive and as Protected(Missing)"
    requirement: RMV-01
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#already_removed_managed_directory_drops_row"
        status: pass
    human_judgment: false
  - id: D5
    description: "Removing an unmanaged linked worktree outside the managed root keeps its directory and porcelain registration, reports it under paths_kept, and leaves the snapshot including feature/outside identical"
    requirement: RMV-07
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#unmanaged_checkout_keeps_directory"
        status: pass
    human_judgment: false
  - id: D6
    description: "A Main row carrying a live trapping runtime is killed to group extinction and reaped, its handle consumed, the clone kept and the snapshot identical"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#live_main_checkout_runtime_is_killed_and_directory_kept"
        status: pass
    human_judgment: false
  - id: D7
    description: "A Running standalone row with a full OwnedRuntime is killed to group extinction and reaped, its handle consumed, the folder and file kept and the row gone from state and file"
    requirement: KILL-02
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#live_standalone_runtime_is_killed_and_folder_kept"
        status: pass
    human_judgment: false
  - id: D8
    description: "A live runtime on an unmanaged checkout is killed to group extinction while the directory, its tracked file and its porcelain registration survive"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#live_unmanaged_checkout_runtime_is_killed_and_directory_kept"
        status: pass
    human_judgment: false
  - id: D9
    description: "On a git init fixture, removing the Main row keeps the repository and branches and does not touch the managed row's live runtime; removing the unclean, cwd-held managed worktree then deletes it with the group extinct, feature/a at the same OID and the registration gone"
    requirement: RMV-02
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#git_init_unclean_cwd_held_managed_is_deleted_and_main_kept"
        status: pass
    human_judgment: false
  - id: D10
    description: "On the cloned fixture, an unclean managed worktree held as cwd by a live runtime is deleted, the group extinct, the registration gone and the snapshot identical"
    requirement: RMV-07
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#managed_worktree_unclean_and_cwd_held_is_deleted"
        status: pass
    human_judgment: false
  - id: D11
    description: "Removing the repository parent removes main, default, managed and unmanaged children before the parent, keeps the clone and the unmanaged directory with its registration, deletes both managed worktrees in stage order, extinguishes the default worktree's runtime, leaves no row with the repository key, validates, round-trips the file and leaves all four branches resolving with one save"
    requirement: RMV-04
    verification:
      - kind: unit
        ref: "baude-core/src/removal.rs#repository_removes_children_then_parent"
        status: pass
    human_judgment: false

duration: 25 min
completed: 2026-11-04
status: complete
---

# Phase 18 Plan 06: Integration Matrix Over the Cloned Fixture Summary

Eleven `removal::tests` cases on a new `testing::cloned_topology` fixture prove the keep rules for main, standalone and unmanaged rows with and without live runtimes, already-removed rows in both lifecycles, unclean cwd-held managed deletion on cloned and `git init` repositories, and repository fan-out with every disk class, with branch integrity pinned by `for-each-ref` snapshots.

## Performance

- **Duration:** about 25 min
- **Started:** 2026-11-04T00:19:24Z
- **Completed:** 2026-11-04T00:44:32Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- `testing::git_in(dir, args)` runs git in a directory and returns trimmed stdout, panicking with stderr on failure; `testing::ClonedTopology { upstream, clone, default_worktree, managed_worktree, common_dir }` and `testing::cloned_topology(root)` build an upstream, a clone switched to `chore/workspace`, a linked worktree `default` on `main` and a linked worktree `managed` on `feature/kept`, both under `git::worktrees_base()`; the fixture asserts an active `TestRedirect` first and `cloned_topology_places_worktrees_under_managed_root` pins placement, branches and the porcelain list. `lifecycle.rs` is untouched.
- `branch_snapshot` returns the sorted `refname objectname` lines from `git for-each-ref refs/heads`; every keep-rule and fan-out test asserts the snapshot is identical before and after the removal, so a branch deletion or OID move anywhere in the clone fails the test regardless of which worktree disappeared.
- `main_checkout_keeps_directory_and_branches`, `standalone_keeps_folder` and `unmanaged_checkout_keeps_directory` prove the three keep classes report under `paths_kept` with nothing deleted, the row gone, the siblings present and (for the unmanaged linked worktree on `feature/outside`) the porcelain registration intact.
- `already_removed_managed_directory_drops_row` deletes the managed directory without a prune, then proves both an `Inactive` row and a `Protected(Missing)` row (set through `set_lifecycle`, health asserted `Unavailable(Missing)`) are dropped from state and file with the path under `already_removed`, the porcelain entry pruned and `feature/kept` at the same OID.
- `live_main_checkout_runtime_is_killed_and_directory_kept`, `live_standalone_runtime_is_killed_and_folder_kept` and `live_unmanaged_checkout_runtime_is_killed_and_directory_kept` hold a trapping runtime in a `LiveHandles` guard, observe `group_is_extinct`, `assert_reaped_promptly`, one `runtimes_killed` entry, the consumed handle, and the directory (and for the unmanaged row, the registration) kept with the snapshot identical.
- `git_init_unclean_cwd_held_managed_is_deleted_and_main_kept` and `managed_worktree_unclean_and_cwd_held_is_deleted` make the managed worktree unclean (untracked file plus modified tracked file, `status --porcelain` asserted non-empty) and spawn the runtime with that cwd; the first proves on the `git init` fixture that removing Main leaves the managed runtime alive and its handle unconsumed, then the managed removal deletes the directory with the group extinct and `feature/a` unchanged; the second proves the same deletion on the cloned fixture.
- `repository_removes_children_then_parent` seeds Main, a live PrimaryDefault, ManagedBranch and an unmanaged child in one repository and proves `rows_removed` ends with the parent with all four children before it, `paths_kept == [clone, outside]` and `paths_deleted == [default, managed]` in stage order, the unmanaged registration alive and both managed registrations gone, the group extinct, no row left with the repository key, the file equal to the state and all four branches resolving, with one save.

## TDD Gate Compliance

| Gate | Evidence |
| ---- | -------- |
| RED | Before any edit, the plan's own `fails_when` was applied: `cargo test -p baude-core removal::tests::main_checkout_keeps_directory_and_branches -- --test-threads=1` and `... removal::tests::repository_removes_children_then_parent ...` each printed `running 0 tests` with `515 filtered out`, so the output lacked every named test, and `crate::testing::cloned_topology` did not resolve before Task 1. These tests pin behavior plans 01, 02, 04 and 08 already shipped; the plan anticipates this with "if red, fix the production stage that is wrong, never the assertion" and no production fix was needed. |
| GREEN | Task 1 commit `936ab7e`: the placement test passed alone on first run (its timing was lost to a compaction; it passes in both later filter runs). Task 2 commit `e92b9e4`: `cargo test -p baude-core removal::tests -- --test-threads=1` 30 passed, 0 failed in 280.47 s. Task 3 commit `ba34462`: 36 passed, 0 failed in 129.93 s. `cargo clippy -p baude-core --all-targets -- -D warnings` exit 0 and `cargo fmt -p baude-core -- --check` exit 0 before each commit. |
| REFACTOR | No refactor commit: nothing production-side changed; the Task 1 import of `ClonedTopology` was trimmed to satisfy clippy and restored in Task 2 where it is used. |

`gsd_run check tdd-red-evidence` has no adapter for `cargo test` text and reports `INVALID_RED` / `unreadable_record` on it, the gap plans 18-01 through 18-05 and 18-08 recorded. The plan's own RED gate was applied instead.

## Task Commits

1. **Task 1: Cloned topology fixture helper in testing.rs** - `936ab7e` (test)
2. **Task 2: Keep rules and already-removed rows on the cloned fixture** - `e92b9e4` (test)
3. **Task 3: Live keep rows, unclean cwd-held deletion and parent fan-out with every disk class** - `ba34462` (test)

**Plan metadata:** see the docs commit that lands this file

## Files Created/Modified

- `baude-core/src/testing.rs` - `git_in`, `ClonedTopology`, `cloned_topology` under the existing test-support cfg (111 insertions)
- `baude-core/src/removal.rs` - eleven `removal::tests` cases and the helpers `branch_snapshot`, `ClonedRows`, `seed_cloned`, `outside_worktree`, `notes_folder`, `seed_notes`, `no_handles`, `make_unclean`; test-module imports of `testing::{cloned_topology, git_in, ClonedTopology}` and `repository::CheckoutHealth` (707 insertions)

## Verification

- `cargo test -p baude-core removal::tests -- --test-threads=1`: 36 passed, 0 failed (host macOS, debug build, 129.93 s)
- `cargo clippy -p baude-core --all-targets -- -D warnings` and `cargo fmt -p baude-core -- --check` exit 0 after every task
- Acceptance greps on the final tree: four Task 2 test names 4; `for-each-ref` present; `branch_snapshot` body contains no `"branch"` argument; six Task 3 test names 6
- `git log --oneline 10a6e43..HEAD` lists exactly the three task commits; `git diff --stat` over that range touches only `baude-core/src/testing.rs` and `baude-core/src/removal.rs` (818 insertions, 0 deletions); `lifecycle.rs` untouched; no file deletions, no `Cargo.toml` change
- Full crate suite `cargo test -p baude-core -- --test-threads=1`: 525 passed, 0 failed, 1 ignored in 496.10 s; the doctest target reports 0 tests (exit 0)

## Decisions Made

- `cloned_topology` asserts the redirect is active before creating anything under the managed root, so a misuse fails at the fixture rather than leaking into the real managed root
- Rows are seeded through plan 01's builders with the clone's discovered common dir, one role of each kind, and `validate_for_save` asserted before every call, so every test starts from a save-valid state and a refusal would be the removal's, not the seed's
- Each fixture's real tracked file name is asserted (`tracked.txt` on the `git init` fixture, `tracked` on the cloned one) rather than normalising the fixtures to one name

## Deviations from Plan

### Plan-shape adjustments

**1. `ClonedTopology` import deferred to Task 2**
- **Found during:** Task 1 clippy run
- **Issue:** The plan's Task 1 import line names `ClonedTopology`, but only Task 2's `seed_cloned` uses the struct; clippy under `-D warnings` refused the unused import
- **Fix:** Task 1 imports `cloned_topology` and `git_in` only; Task 2 restores the full import where the struct is first named (Rule 3, blocking clippy)
- **Files modified:** baude-core/src/removal.rs
- **Commit:** `936ab7e`, `e92b9e4`

**Total deviations:** 1 plan-shape adjustment, 0 auto-fixed bugs, 0 production changes

**Impact on plan:** None on behavior. Every truth in `must_haves` is asserted on fixture output. Shipped signatures the dispatch warned about were used as shipped.

## Issues Encountered

- The session was compacted during Task 2's test run; the plan's Task 3 text, the shipped live-runtime helpers and the fixture constructors were re-read before any further edit, and the ledger file for this plan was created from the dispatch HEAD `10a6e43`
- The removal suite under `--test-threads=1` took 280 s on the Task 2 run while another cargo build held the machine, and 130 s on the Task 3 run; the plan's 600 s cap was never approached

## Known Stubs

None.

## Self-Check: PASSED

- Files present: baude-core/src/testing.rs, baude-core/src/removal.rs, 18-06-SUMMARY.md
- Commits reachable from HEAD: 936ab7e, e92b9e4, ba34462
- Commit count measured from the ledger base 10a6e43: 3
