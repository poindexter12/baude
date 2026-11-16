---
phase: 19-re-admission-suppression
plan: 06
subsystem: tui-relaunch-proof
tags: [rust, tui, relaunch, breadcrumbs, proof]
requires: [19-04, 19-05]
provides:
  - app::tests::removed_rows_stay_removed_across_relaunch
  - app::tests::relaunch_inside_removed_checkout_readmits_it_fresh
  - breadcrumbs::tests::flush_drops_crumb_of_removed_path
affects: []
tech-stack:
  added: []
  patterns: [two-App restart over a redirected state root, live RowRuntime handles handed to removal::remove]
key-files:
  created: []
  modified: [baude/src/app.rs, baude-core/src/breadcrumbs.rs]
decisions:
  - "Fixture uses two unmanaged linked worktrees instead of a managed one: the cloned fixture does not create managed worktrees and the create path was out of budget; both directories survive removal, which is the dangerous sweep case"
  - "The second repository's primary handle is optional: admit_repository returned no runtime for a remote-less git init repo, so the Repository removal runs with whatever handles the App actually holds"
metrics:
  duration: two executor sessions
  completed: 2026-11-04
status: complete
actuals:
  tokens: 6000
  tasks: 3
  commits: 3
plan_head_before: 3245a82
plan_head_after: da360f5
---

# Phase 19 Plan 06: Relaunch and breadcrumb proof Summary

Two-App relaunch tests prove removed repository, checkout and standalone rows stay gone, recover nothing, and that relaunching inside a removed checkout lifts its entry and admits a fresh identity; a breadcrumbs test proves flush drops the removed path's crumb.

## Commits

- 0bdd4c6 test(19-06): prove flush drops the removed path's crumb (Task 3, first executor)
- b32b445 test(19-06): prove removed rows stay removed across a relaunch (Task 1)
- da360f5 test(19-06): prove relaunch inside a removed checkout admits it fresh (Task 2)

## Verification

| Check | Exit |
|---|---|
| cargo test -p baude app::tests::removed_rows_stay_removed_across_relaunch, 1 passed | 0 |
| cargo test -p baude app::tests::relaunch_inside_removed_checkout_readmits_it_fresh, 1 passed | 0 |
| cargo fmt --all -- --check | 0 |
| cargo clippy -p baude --all-targets -- -D warnings | 0 |
| grep -c FolderContext baude-core/src/removal.rs prints 0 | kept |

scripts/gate.sh is owned by plan 19-07 and was not run here.

## Must-haves self-check

- Removed repository, unmanaged checkout with surviving directory, linked worktree and standalone absent after a second App restore: covered by removed_rows_stay_removed_across_relaunch.
- startup_recovery_program empty and both reconcile methods a no-op on the restarted state: covered by the same test.
- Relaunch inside the removed checkout admits it with new CheckoutKey, new first_seen_order, resume_id None, runtime if any owned by the new row, entry lifted: covered by relaunch_inside_removed_checkout_readmits_it_fresh.
- flush drops the removed crumb with no eager clearing in removal: covered by flush_drops_crumb_of_removed_path; FolderContext count in removal.rs is 0.
- Fixtures are cloned_topology and git init shapes under a TestRedirect with persistence_root_for_test; assertions are on state and fixture output.

## Deviations from Plan

1. [Fixture] No managed worktree row. admission_repo_cloned does not create managed worktrees and building one through the create path was out of context budget, so the fixture adds two unmanaged linked worktrees via git worktree add and removes both as Checkout targets. Both directories survive on disk, which is the sweep case the test exists to prove.
2. [Fixture] The second repository is a remote-less git init repo and admit_repository returned Ok with no runtime for it, so the Repository removal supplies a RowRuntime handle only when a running primary exists.
3. [Task 3, first executor] The plan's post-removal flush is gated on dirty, which only record_checkout and record_standalone set, so a flush straight after removal::remove would not rewrite the file. The test mirrors the TUI's real order: after removing the whole repository it records the surviving repository's main checkout with a later timestamp, then flushes, asserting the removed crumb is gone. No production code was added.
4. [Process] 19-06 was executed by two executors because of context limits: the first landed Task 3 as 0bdd4c6, the second landed Tasks 1 and 2 and this summary.

No production code changed; no fix commits were needed.

## Known Stubs

None.
