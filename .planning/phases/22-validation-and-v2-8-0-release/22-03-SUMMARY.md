---
phase: 22-validation-and-v2-8-0-release
plan: 03
subsystem: TUI removal confirm
tags: [removal, tui, confirm-modal, binding, suppression, cloned-fixture, tests-only, REL-01]
status: complete
requires:
  - the PTY process group fix of plan 22-06 and the fixes of plans 22-07 and 22-08
provides:
  - one TUI test that confirms the clone's own checkout row alone and reads process, directory, state file, bindings file and suppression store from the fixture
  - one TUI test that confirms a standalone row alone and reads the same five observations from the fixture
  - a `bindings_on_disk` helper in `mod tests` of `baude/src/app.rs` that parses the bindings file from its bytes under the redirected config dir
affects:
  - plan 22-05, which runs the full gates on the final tree
tech-stack:
  added: []
  patterns:
    - the removal goes through `handle_sidebar_key` with `d_key` and `handle_modal_key` with `enter_key`, never through `baude_core::removal::remove`
    - each test seeds a neighbour binding next to the target's own, so "own key cleared" is checked against an entry that must survive
    - the suppression store is compared as optional bytes before and after, so "still absent" and "byte-identical" are one assertion
key-files:
  created: []
  modified:
    - baude/src/app.rs
decisions:
  - No product code changed; both tests pass on the tree as found, on first run
  - Neither G7 cell was already covered, so both tests were written and no covered file exists
  - Each test seeds a second binding for a neighbour folder and asserts it is untouched, which the plan did not ask for
  - The standalone test writes a note into the folder first, so the kept listing is not an empty one
  - REL-01 is not marked complete here; this plan delivers its TUI half only
metrics:
  duration: 12m
  completed: 2026-11-14
  tasks: 2
  files: 1
actuals:
  tokens: 2786
  tasks: 2
  commits: 2
plan_head_before: 98fa39b4ad10366b5abf3590e805d9f3143064b0
plan_head_after: 843ad10c4b1221d2446b1f2dfab5c2c940a7b9ef
---

# Phase 22 Plan 03: TUI single-target confirms for a main checkout and a standalone row Summary

Two `rel01_` tests in `mod tests` of `baude/src/app.rs` close gap G7. Each selects one row, presses `D`, checks the modal names exactly that row, presses Enter, and then reads the result from the process table and from files under the fixture root. Tests only: 251 lines added to `app.rs`, none removed, one hunk starting at line 15871, above the `mod tests` floor of 8368 stored in `target/gsd-22/03-floor.txt`.

## Commits

| Task | Commit | Subject |
| ---- | ------ | ------- |
| 1 | `7215c7d` | test(22-03): confirm a main checkout alone and read binding and suppression from disk |
| 2 | `843ad10` | test(22-03): confirm a standalone row alone and read binding and suppression from disk |

Base was `98fa39b`, as expected.

## G7 triage

Neither cell was covered, so no `03-main-covered.txt` or `03-standalone-covered.txt` was written.

| Neighbour, line | What it does with a main checkout or a standalone row |
| --------------- | ------------------------------------------------------ |
| `d_opens_confirm_remove_modal_with_preview_report_on_each_row_kind`, 15137 | opens the modal on both, compares the preview report, then presses Esc. Nothing is confirmed |
| `confirm_remove_managed_worktree_drops_row_in_one_save`, 15266 | confirms a managed worktree only |
| `confirm_remove_repository_parent_drops_every_child_in_one_save`, 15372 | removes the main checkout as a child of the parent, never as the target |
| `remove_project_clearing_failure_is_a_footer_notice_not_an_error`, 15805 | the only local removal test that reads the suppression store, and for a managed worktree |
| `removed_rows_stay_removed_across_relaunch`, 10977, through `remove_four_rows`, 10849 | removes a standalone row by calling `baude_core::removal::remove` directly, not through the keys, and never removes the main checkout |

No test in the neighbour ranges reads the bindings file.

## What each test witnessed

### `rel01_confirm_remove_main_checkout_reads_binding_and_suppression`, line 15889

The target is `rows.main`, the clone's own checkout. On this fixture it carries `CheckoutRole::PrimaryDefault`, which `remove_project_app` asserts, and it projects as an `External` row. The `Main` role needs the pushed shape and is covered in core by plan 22-01.

- Config dir asserted under `fx.root` before any seeding.
- Before: the runtime group answers signal zero, `fx.repo` is a directory, the binding for `fx.repo` is in `folder-workspaces.json` on disk, `is_suppressed` is false for `fx.repo`, and both the row and its managed sibling are in the state file on disk.
- The modal's target is `Checkout(rows.main)` and its `rows_removed` is exactly that one row.
- After: the modal is closed, the group is extinct inside `wait_until`, `fx.repo` is still a directory and `.git/HEAD` still reads as a ref, the row is gone from the state file and the managed sibling is still in it, the binding for `fx.repo` is gone from the file on disk while the neighbour binding for `fx.second_repo` is unchanged, and `is_suppressed` is true for `fx.repo`.
- Bite check, expecting the clone to be gone: `panicked at baude/src/app.rs:15958:9: BITE: the clone's directory is gone`.

### `rel01_confirm_remove_standalone_reads_binding_and_suppression`, line 16012

The target is `rows.standalone`, the plain `notes` folder, with the runtime `admit_standalone` spawned.

- Config dir asserted under `fx.root` before any seeding. A `todo.md` note is written into the folder first.
- Before: the runtime group answers signal zero, the sorted listing holds `todo.md`, the binding for the folder is in the file on disk, `is_suppressed` is false for the folder, the row is in the state file on disk, and the suppression store does not exist.
- The modal's target is `Standalone(rows.standalone)` and its `rows_removed` is exactly that one row.
- After: the modal is closed, the group is extinct, the folder is a directory with the same sorted listing and the same note bytes, the row is gone from the state file, the binding for the folder is gone from the file on disk while the neighbour binding for `fx.repo` is unchanged, `is_suppressed` is still false, and the suppression store is still absent.
- Bite check, expecting a suppression entry: `panicked at baude/src/app.rs:16109:9: BITE: a suppression entry exists for the folder; store existed before: false`.

The only value taken from the modal's report is the preview's `rows_removed`, used to check what the modal names before Enter. No after-state observation comes from a `RemovalReport`.

## Verification

Exit codes were read directly, with output sent to files under `target/gsd-22/`.

| Check | Exit | Result |
| ----- | ---- | ------ |
| Task 1 verify | 0 | `expect=1 list=0 tests=1 run=0` |
| Task 2 verify, the plan gate | 0 | `expect=2 list=0 tests=2 run=0 fmt=0 clippy=0 diff=0 floor=8368 hunks=0 messages=0`, 11 tests passed |
| New tests, 10 runs in a row | 0 each | 10 of 10 passed, 2 tests each |
| `cargo fmt --all -- --check` | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | clean |
| `cargo test -p baude -- --test-threads=1` | 0 | 360 passed, 0 failed, 0 ignored |
| `scripts/check-commit-message.sh --range 98fa39b..HEAD` | 0 | both task commits pass |

The text "failed to terminate exact PTY process group" did not appear in any run. `scripts/gate.sh` was not run, by instruction. The `baude` suite count went from 358 to 360.

The two bite runs each left a fixture directory behind, because a failed test never reaches its own cleanup. Both were removed by name. No `baude-admission-rel01-*` directory and no `sleep 30` process is left.

## Deviations from Plan

### Additions inside the tests

**1. [Rule 2 - Precision] A neighbour binding in each test**
- **Found during:** Tasks 1 and 2
- **Issue:** with a single seeded binding, "own key cleared" would also pass if the removal emptied the whole file
- **Fix:** each test records a second binding for another folder of the fixture and asserts that entry is unchanged afterwards
- **Files modified:** baude/src/app.rs
- **Commits:** `7215c7d`, `843ad10`

**2. [Rule 2 - Fixture realism] A note in the standalone folder**
- **Found during:** Task 2
- **Issue:** the fixture's `notes` folder is empty, so an unchanged listing would have compared two empty lists
- **Fix:** the test writes `todo.md` before recording the listing and also compares its bytes afterwards
- **Files modified:** baude/src/app.rs
- **Commit:** `843ad10`

### Process

- The state handlers were not run. `STATE.md`, `ROADMAP.md` and `REQUIREMENTS.md` were left alone by instruction, and REL-01 is not marked complete.
- The 10 consecutive runs, the workspace-wide clippy, `cargo fmt --all` and the whole `baude` suite were run on top of the plan's own gate, by instruction.

## TDD Gate Compliance

These are characterization tests of shipped behaviour. Both passed on their first run, so there is no RED commit. A bite check stands in for it in each task: one assertion was inverted, the run failed on exactly that observation, the assertion was restored and the run went green before the commit. Only the restored versions were committed.

## Known Stubs

None.

## Open

Nothing from this plan. Earlier test runs left `baude-admission-*` directories in the temp dir, such as the `containment-guard` and `identity-alpha` ones. They predate this plan and were not touched.

## Self-Check: PASSED

- FOUND: baude/src/app.rs, with `fn rel01_confirm_remove_main_checkout_reads_binding_and_suppression` and `fn rel01_confirm_remove_standalone_reads_binding_and_suppression`
- FOUND: commit `7215c7d`
- FOUND: commit `843ad10`
- FOUND: target/gsd-22/03-floor.txt holding 8368
