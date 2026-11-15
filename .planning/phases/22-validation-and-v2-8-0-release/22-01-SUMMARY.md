---
phase: 22-validation-and-v2-8-0-release
plan: 01
subsystem: removal lifecycle
tags: [removal, end-state-matrix, fixtures, tests-only, REL-01]
status: complete
requires:
  - the shared kill-then-remove lifecycle of Phase 18 and the suppression store of Phase 19
provides:
  - a 16 cell end-state matrix in baude-core that reads process, worktree, binding, marker and suppression state from the fixture before and after each removal
  - one reader, `read_end_state`, that takes paths, pids and store keys only and never sees what `remove` returned
affects:
  - plan 22-05, which runs the full gates on the final tree
  - the app and daemon halves of REL-01, which can reuse the per-kind expectations
tech-stack:
  added: []
  patterns:
    - one probe read the same way before and after the removal, with gone, kept and added each held to both reads
    - a `CELL:` line printed per passing cell so the cell count is read from the run's log
key-files:
  created: []
  modified:
    - baude-core/src/removal.rs
decisions:
  - No product code changed; all 16 cells passed on the tree as found
  - A kept expectation requires the value to be present and non-empty before the removal, so an empty read cannot pass as unchanged
  - A cell with no runtime is held to a fixture that started none and a saved state that records none, since there is no pid to observe
  - REL-01 is not marked complete; this plan delivers its core half only
metrics:
  duration: 20m
  completed: 2026-11-14
  tasks: 3
  files: 1
actuals:
  tokens: 10225
  tasks: 3
  commits: 3
plan_head_before: f8d5f0533dc08d3416730e83d78ffc565f37093a
plan_head_after: f710bcfd7f04d09d2984043b0bb994f0a1be1717
---

# Phase 22 Plan 01: End-state matrix for removal in baude-core Summary

Four table-driven tests in `baude-core/src/removal.rs` run 16 cells over every row kind, with and without a live runtime, on cloned and `git init` shapes. Each cell reads the process table, the directories, `git worktree list`, the branch snapshot, the binding file, the owned marker, the suppression store and the saved state file before the removal and again after it, and holds the two reads to the per-kind expectation. Tests only: 1069 lines added, none removed, every hunk after line 1170.

## The cells

Read from `target/gsd-22/01-cells.txt`, 16 distinct lines, 16 raw lines:

```
CELL:main-checkout/Main/external/live/cloned
CELL:main-checkout/Main/external/live/git-init
CELL:main-checkout/Main/external/none/cloned
CELL:main-checkout/Main/external/none/git-init
CELL:managed-worktree/ManagedBranch/managed/live/cloned
CELL:managed-worktree/ManagedBranch/managed/live/git-init
CELL:managed-worktree/ManagedBranch/managed/none/cloned
CELL:managed-worktree/ManagedBranch/managed/none/git-init
CELL:managed-worktree/PrimaryDefault/managed/live/cloned
CELL:managed-worktree/PrimaryDefault/managed/none/cloned
CELL:repository-parent/-/-/live/cloned
CELL:repository-parent/-/-/live/git-init
CELL:repository-parent/-/-/none/cloned
CELL:repository-parent/-/-/none/git-init
CELL:standalone/-/-/live/folder
CELL:standalone/-/-/none/folder
```

| Test | Cells | What the fixture shows |
|------|-------|------------------------|
| `end_state_matrix_managed_worktree` | 6 | the directory and its git registration go, the group is extinct, the binding goes, the neighbour's binding and suppression entry stay; the managed default checkout is deleted like any managed worktree and `main` is kept |
| `end_state_matrix_main_checkout` | 4 | the directory, its registration and every branch are kept on both shapes; the runtime and the binding go |
| `end_state_matrix_repository_parent` | 4 | both child groups extinct, managed directories gone, the main checkout and a linked worktree baude did not create kept and still registered, repository binding, owned marker and repository directory cleared, the child suppression entry dropped, the neighbour's kept |
| `end_state_matrix_standalone` | 2 | the group is extinct, the folder's entries are equal before and after, its own binding goes, the managed root holds no repository directory either time, the suppression store is byte-identical |

Every cell also asserts that the saved state file loses exactly the removed rows and that the removal saved once.

## How the reads are kept honest

- `Probe` holds paths, pids, process groups and store keys. `read_end_state` takes only a `Probe`, so no observation comes from `RemovalReport`.
- `assert_gone` requires present before and absent after. `assert_kept` requires present and non-empty before and equal after. `assert_added` requires absent before.
- Each cell seeds a neighbour folder binding and a suppression entry under another repository, so a removal that wiped a store would fail the kept reads.
- Live cells use the trapping runtime, which ignores TERM and HUP, so a passing cell shows the escalation reached the group.
- All four tests are plain `#[test]`. No tokio test was added.

## Bite checks

One expectation flipped per task, the failure confirmed, the file restored and compared byte for byte with the saved copy.

Task 1, the managed worktree expected to be kept:

```
managed-worktree/ManagedBranch/managed/live/cloned: directory .../data/baude/worktrees/managed must be unchanged by the removal
```

Task 2, the main checkout expected to be gone:

```
main-checkout/Main/external/live/cloned: directory .../clone must be gone after the removal, found Some([".git", "tracked"])
```

Task 3, the unmanaged worktree expected to be gone:

```
repository-parent/-/-/live/cloned: directory .../outside-wt must be gone after the removal, found Some([".git", "tracked"])
```

## Verification

| Check | Exit | Result |
|-------|------|--------|
| Plan gate, `removal::tests` single threaded | 0 | 53 passed, 4 matrix tests listed, 16 cells, 2 managed `PrimaryDefault` |
| Matrix tests, 10 runs in a row at default threading | 0 each | 10 of 10, 16 cells each run |
| `cargo fmt --all -- --check` | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | clean |
| `cargo test -p baude-core` | 0 | 572 passed, 0 failed, 1 ignored |
| `scripts/check-commit-message.sh --range f8d5f05..HEAD` | 0 | three commits pass |

After the 10 runs no `sleep 30` process was alive and no `baude-removal-*` directory was left in the temp root.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | fba38bd | test(22-01): read one managed worktree removal end to end from the fixture |
| 2 | 9c5043e | test(22-01): cover managed worktree and main checkout removals in ten cells |
| 3 | f710bcf | test(22-01): cover repository parent and standalone removals in the last six cells |

## Deviations from Plan

**1. [Rule 2] The Task 3 bite check was run twice.** The first flip made every child directory expected gone and failed on the main checkout, which is read first. That did not show the unmanaged read biting, so the flip was narrowed to the unmanaged worktree alone and re-run. The line above is from the second run. No committed code differs.

**2. [Rule 2] `EndState` gained the suppression store's bytes.** The standalone expectation asks for a byte-identical store, which the per-path entries of Tasks 1 and 2 could not show.

**3. State files left alone by instruction.** `STATE.md`, `ROADMAP.md` and `REQUIREMENTS.md` were not updated; the orchestrator owns them for this run.

## Deferred Issues

Found while checking for leftovers, in tests this plan did not touch, so not fixed:

- Four existing tests leave a `sleep 30` runtime behind when the suite exits: the fixtures `baude-pty-reap-kill`, `baude-pty-registration-gate`, `baude-signal-session-kill-agent` and `baude-signal-session-kill-shell`. Seen on a suite run with the matrix tests skipped. Each ends by itself within 30 seconds.
- The temp root holds about 1590 `baude-*` fixture directories from existing tests across earlier runs. None is a matrix directory.

## TDD Gate Compliance

These are characterization tests of behaviour that already shipped, so there is no RED commit. A bite check per task stands in for it: each flipped one expectation, failed on the fixture read shown above and was restored before the commit.

## Self-Check: PASSED

- FOUND: baude-core/src/removal.rs with four `end_state_matrix_` tests listed by cargo
- FOUND: fba38bd, 9c5043e, f710bcf as ancestors of HEAD
