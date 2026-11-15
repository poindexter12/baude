---
phase: 22-validation-and-v2-8-0-release
plan: 08
subsystem: re-admission suppression
tags: [suppression, admission, removal, gap-closure, tests-only]
status: complete
gap_closure: true
requires:
  - plan 22-07, the removed-primary fact and the consult in admit_repository
provides:
  - tests pinning the three explicit routes back for a removed primary, with a bite check on the lift
  - tests pinning the two ways back when the removal deleted the folder
  - store tests for the life of the fact and for the previous file format
  - session count and checkout key counter assertions on both sibling relaunch tests of plan 22-07
affects:
  - plan 22-05, which runs the full gates on the final tree
  - the README plan, which can lift the paragraph under "For the README plan"
  - Phase 19 SC2, ready for re-verification
tech-stack:
  added: []
  patterns:
    - gather observations, kill every App, delete the fixture root, then assert
key-files:
  created: []
  modified:
    - baude/src/app.rs
    - baude-core/src/suppression.rs
decisions:
  - No product code changed; every test passed on the tree plan 22-07 left
  - A deleted primary returns by putting a worktree of the default branch back at the removed path, or through removal and re-admission of the repository row
  - The session count and allocator counter pins went into the two existing sibling relaunch tests of plan 22-07, as a fourth commit
metrics:
  duration: 13m
  completed: 2026-11-14
  tasks: 3
  files: 2
actuals:
  tokens: 7424
  tasks: 3
  commits: 4
plan_head_before: 2d015eb408ad7a613a57b28554ee6a07f1f99b5f
plan_head_after: e3e7cd2c80ab5fda170f1f167b6ef06c113290f3
---

# Phase 22 Plan 08: Ways back for a removed primary Summary

Six app tests on the cloned relaunch fixture and two store tests pin READM-04 on the fix of plan 22-07: launching inside the folder, `n` and `c` each bring a removed primary back as a fresh row, `n` on a sibling does not, a deleted primary returns when its folder is back or through the repository row, and a store file written before the fact existed still loads and still suppresses. Tests only. Phase 19 SC2 is ready for re-verification.

## What was pinned

`baude/src/app.rs`, after the two tests of plan 22-07:

| Test | Shape | What it shows |
|------|-------|---------------|
| `removed_primary_returns_on_relaunch_inside_its_folder` | kept | a new App launched in the removed folder lifts the entry and admits one fresh `PrimaryDefault` row |
| `removed_primary_returns_through_open_route` | kept | after a sibling relaunch, `open_repo_session` naming a sibling leaves no primary, the entry, the fact, zero sessions and an unmoved key counter; naming the removed folder brings the primary back |
| `removed_primary_returns_through_clone_completion_route` | kept | `open_repo_session_via` with `CloneCompletion` naming the removed folder brings the primary back |
| `removed_primary_managed_returns_when_its_folder_is_back` | managed | `git worktree add <removed path> <default branch>`, then a launch inside that path: entry lifted, fact gone, one `PrimaryDefault` row at that path |
| `removed_primary_managed_returns_after_repository_readmission` | managed | removing the repository row ends the fact; a launch inside the clone then derives one new managed primary and `git worktree list` holds exactly one worktree more |
| `removed_primary_store_without_the_fact_still_loads` | kept | the real removal's store with `primary` dropped from every entry: restore completes, state valid, same repository count, folder still suppressed, no fact, kept rows keep their keys |

Each "fresh row" assertion is the same set, through one helper: entry lifted, `removed_primary` None, exactly one row at the path, role `PrimaryDefault`, the repository's only primary, key above every key recorded before the removal, a new `first_seen_order`, no `resume_id`, state valid.

`baude-core/src/suppression.rs`:

- `primary_fact_follows_its_entry`: two common dirs. `removed_primary` answers per common dir, a `lift` of another path of the same repository leaves it, a `lift` of the flagged path ends it for that repository only, `drop_children` ends it, and an entry written by `suppress` has no `primary` member in the file.
- `entries_without_the_primary_member_load`: a literal file in the previous format, one entry with `suppressed_ms` and one without. Both are suppressed, `removed_primary` is None, and both read back equal and without a `primary` member after a later `suppress_primary` of another path.

Added to the two tests of plan 22-07, both shapes: the relaunched App holds zero sessions, and `next_checkout_key` equals what the removal left. This closes the first two items plan 22-07 listed as open. The counter is the allocator's own, so a key spent with no row pushed would now fail.

## Bite check

With `self.lift_suppression(path);` at the top of `admit_repository` commented out, `cargo test -p baude -- --test-threads=1 app::tests::removed_primary_returns_` exited 101: 0 passed, 3 failed. Log: `target/gsd-22/08-bite.log`. All three failed on the same line:

```
thread 'app::tests::removed_primary_returns_through_open_route' panicked at baude/src/app.rs:11396:9:
the admission lifts the entry
```

The call was restored. `git diff HEAD -- baude/src/app.rs` then showed one hunk of 200 added lines and no removed line, the three tests and their helpers.

## For the README plan

A default checkout removed with `D` stays removed: launching baude in a sibling worktree, or opening a sibling with `n`, starts no session for that repository and shows a message saying how to bring the checkout back. If the removal kept the folder, which is the case when the default checkout is your own clone or a worktree you made, bring it back by naming that folder: launch baude inside it, open it with `n`, or finish a clone at that path with `c`. If the removal deleted the folder, which is the case when the default checkout was a worktree baude made, there are two ways back. Put a worktree of the default branch back at the removed path with `git worktree add <path> <default-branch>` and open that folder, or remove the repository row and open the repository again, which gives it a new default checkout the way a first open does. Either way the checkout returns as a new row and nothing is resumed from the one that was removed.

Two facts behind that paragraph, for whoever lifts it:

- The `c` route is pinned at the admission seam, `open_repo_session_via` with `CloneCompletion` on the existing folder. No test runs a real `git clone` into the removed path.
- In the deleted shape the message reads `default checkout was removed with its folder; remove this repository and open it again to bring it back`. It names the repository route only and does not show the removed path, so the `git worktree add` route works but is not discoverable from the message.

## Verification

Every output went to a file under `target/gsd-22/`; every exit code was read directly.

The plan's Task 3 verify command: `list=0 tests=8 run=0 core=0 fmt=0 clippy=0 untouched=0 messages=0`.

| Check | Exit | Result |
|-------|------|--------|
| `cargo fmt --all -- --check` | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | clean |
| `cargo test -p baude` | 0 | 358 passed, 0 failed, 0 ignored; 352 before this plan |
| `cargo test -p baude-core` | 0 | 568 passed, 0 failed, 1 ignored; 0 doc tests; 566 before this plan |
| plan subset `removed_ relaunch_ suppress admitting_` | 0 | 18 passed |
| `cargo test -p baude-core --lib` | 0 | 568 passed, 1 ignored |
| `git diff --quiet <base> HEAD -- bauded baude-core/src/repository.rs baude-core/src/persist.rs baude-core/src/removal.rs` | 0 | untouched |
| `scripts/check-commit-message.sh --range 2d015eb..HEAD` | 0 | the four task commits pass |

`scripts/gate.sh` was not run; the full gates on the final tree belong to plan 22-05.

The diff against the base is 651 added lines in two files, all inside `mod tests`. No state schema version bump, nothing under `bauded/`, no product code.

## Commits

| Commit | Message |
|--------|---------|
| `0413f7e` | `test(22-08): pin explicit re-admission of a removed primary` |
| `084af67` | `test(22-08): pin the return of a removed primary whose folder was deleted` |
| `1058fc4` | `test(22-08): pin the removed primary fact and older store files` |
| `e3e7cd2` | `test(22-08): pin zero sessions and an unmoved key counter on a sibling launch` |

## Deviations from Plan

### Auto-fixed Issues

None. No test failed on the tree plan 22-07 left, so no stop was needed and no product code was touched.

### Notes

- **A fourth commit the plan did not list.** The orchestrator asked for two more pins: zero sessions after a sibling launch and an unmoved checkout key counter. They are assertions added to `removed_primary_stays_removed_on_sibling_relaunch` and `removed_primary_managed_is_not_recreated_on_sibling_relaunch`, commit `e3e7cd2`. The count of `removed_primary_` tests stays at eight.
- **Helpers beyond the tests the plan named.** `common_dir_of`, `removed_primary_fact`, `PrimaryReturn` with `primary_return` and `assert_fresh_primary`, `remove_managed_primary`, `worktree_count` in `app.rs`; `read_raw` in `suppression.rs`. They keep the same observations and assertions across the route tests.
- **Task 1 carried `tdd="true"` but there was no red commit to make.** The behaviour already existed at `754665e`. The bite check stands in for red.
- The plan's verify commands print test output to the terminal. Here each step was sent to a file and its exit code echoed, as instructed.
- rustfmt reflowed the new tests before each commit.
- `STATE.md`, `ROADMAP.md` and `REQUIREMENTS.md` were left alone as instructed. No state handler was run and REL-01 is not marked here.

## Open

- The managed tests assert that removal wrote a fact for the deleted folder, not that its stored path equals the row's path. The lift succeeding once the folder is back shows the key matches.
- "By nothing else" is pinned for a sibling launch and for `n` on a sibling. Other gestures on a repository with a removed primary are not enumerated here.
- The full gates on the final tree belong to plan 22-05. These tests ran on macOS only.

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: `baude/src/app.rs` contains `fn removed_primary_returns_through_open_route`
- FOUND: `baude-core/src/suppression.rs` contains `fn entries_without_the_primary_member_load`
- FOUND: `target/gsd-22/08-bite.log`
- FOUND: commit `0413f7e`
- FOUND: commit `084af67`
- FOUND: commit `1058fc4`
- FOUND: commit `e3e7cd2`
