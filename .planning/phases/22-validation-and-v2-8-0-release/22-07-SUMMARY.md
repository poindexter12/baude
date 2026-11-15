---
phase: 22-validation-and-v2-8-0-release
plan: 07
subsystem: re-admission suppression
tags: [suppression, admission, removal, gap-closure, tdd]
status: complete
gap_closure: true
requires:
  - phase 19 suppression store and the removal stage that writes it
  - plan 22-06, PTY group extinction, landed
provides:
  - the removed-primary fact on a suppression entry, its writer and its repository-scoped query
  - the consult in admit_repository ahead of primary derivation
  - test helpers remove_primary and worktree_list for plan 22-08
affects:
  - plan 22-08, which pins the routes back and older store files on this fix
  - plan 22-05, which runs the full gates on the final tree
tech-stack:
  added: []
  patterns:
    - additive serde member, defaulted on read and skipped on write when false
key-files:
  created: []
  modified:
    - baude-core/src/suppression.rs
    - baude-core/src/removal.rs
    - baude/src/app.rs
decisions:
  - The removed-primary fact lives on the suppression entry removal already writes, as a primary flag, not in the state file
  - admit_repository consults the store only when the repository has no PrimaryDefault row, after the lift and before any key is allocated
  - A sibling admission of a repository whose primary was removed derives nothing and starts no session
metrics:
  duration: 12m
  completed: 2026-11-14
  tasks: 1
  files: 3
actuals:
  tokens: 4284
  tasks: 1
  commits: 2
plan_head_before: 10b5bb6a2173a24cfe3f71232b3817d8d74a6c0d
plan_head_after: 754665e5cc58fb4916d32471d12f9bcb34585e5b
---

# Phase 22 Plan 07: Removed primary stays removed on a sibling relaunch Summary

A `primary` flag on the suppression entry that removal already writes, read by `admit_repository` before it derives a primary, so a primary checkout removed with `D` no longer returns when baude is launched inside a sibling worktree, whether its folder was kept or was a managed worktree the removal deleted.

## What changed

- `baude-core/src/suppression.rs`: `SuppressionEntry` gains `pub primary: bool`, `serde(default)` and left out of the file when false. New `suppress_primary` with the parameters of `suppress`, and `removed_primary(root, common_dir) -> Option<PathBuf>`, which returns the path of the first flagged entry of that repository and never errors. `suppress` keeps its signature; both writers share a private `write_entry`.
- `baude-core/src/removal.rs`, `update_suppression`: a removed `PrimaryDefault` row is written through `suppress_primary`, every other row through `suppress`. The report and `ClearingKind` are unchanged.
- `baude/src/app.rs`, `admit_repository`: when the repository has no `PrimaryDefault` row and `suppression::removed_primary` names a path, the admission sweeps the kept worktrees, validates, saves, sets a message and returns `Ok(None)`. No checkout key is allocated and `ensure_default_worktree` is never reached. The consult sits after `lift_suppression`, so naming the removed folder itself still derives in the same pass.

## What a user sees

A launch inside a sibling of a removed primary starts no session. Measured on the fixture after the fix, both shapes: zero live sessions in the relaunched App, the kept sibling rows present under their old keys, and one of two messages.

- Folder kept: `default checkout <path> was removed; open that folder to bring it back`
- Folder deleted with the removal: `default checkout was removed with its folder; remove this repository and open it again to bring it back`

In the deleted shape the launch folder can be the clone itself, which is a `Main` row and a sibling of the managed primary. It too starts no session until the repository row is removed and admitted again.

## TDD Gate Compliance

- RED: `7a1b0e6` `test(22-07): reproduce a removed primary returning on a sibling relaunch`. `cargo test -p baude -- --test-threads=1 app::tests::removed_primary_` exited 101, 0 passed and 2 failed, each on its first assertion. Log: `target/gsd-22/07-red.log`.
  - `removed_primary_stays_removed_on_sibling_relaunch`, `baude/src/app.rs:11199:9`: `removed primary checkout came back on a sibling relaunch: Some((CheckoutKey(4), PrimaryDefault, 5))`
  - `removed_primary_managed_is_not_recreated_on_sibling_relaunch`, `baude/src/app.rs:11272:9`: `assertion left == right failed: git worktree list changed across a sibling relaunch`. The right side gained `worktree .../data/baude/worktrees/removed-primary-managed/repository-1/primary-5` on `refs/heads/main`.
- GREEN: `754665e` `fix(22-07): remember a removed primary and skip its derivation`. The same command exited 0, 2 passed. Log: `target/gsd-22/07-green.log`.
- REFACTOR: none needed.

## Verification

The plan's verify command, each step's output sent to a file under `target/gsd-22/`, each exit code read directly:

`list=0 tests=2 run=0 core=0 fmt=0 clippy=0 untouched=0 messages=0`, overall exit 0.

| Check | Exit | Result |
|-------|------|--------|
| `cargo fmt --all -- --check` | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | clean |
| `cargo test -p baude` | 0 | 352 passed, 0 failed, 0 ignored |
| `cargo test -p baude-core` | 0 | 566 passed, 0 failed, 1 ignored; 0 doc tests |
| plan subset `removed_ relaunch_ suppress admitting_` | 0 | 12 passed |
| `git diff --quiet <base> HEAD -- bauded baude-core/src/repository.rs baude-core/src/persist.rs` | 0 | untouched |
| `scripts/check-commit-message.sh --range <base>..HEAD` | 0 | both task commits pass |

No state schema version bump, no new member in a state struct, nothing under `bauded/`, no change to a type the daemon sends the TUI.

An entry without the flag still loads. `suppress` writes no `primary` key, so the files the existing store tests write and read back are in the pre-flag shape, and they compare equal to entries with `primary: false`.

## For plan 22-08

Line numbers in `baude/src/app.rs` at `754665e`:

| Item | Line |
|------|------|
| consult `suppression::removed_primary` in `admit_repository` | 2634 |
| `fn worktree_list(repo: &Path) -> String` | 11094 |
| `fn remove_primary_of(fx, app: &mut App) -> (RemovedIdentity, PathBuf)` | 11112 |
| `fn remove_primary(fx) -> (App, RemovedIdentity, PathBuf)` | 11159 |
| `struct CheckoutsSeen` and `fn checkouts_seen(app: &App)` | 11169, 11175 |
| `removed_primary_stays_removed_on_sibling_relaunch` | 11200 |
| `removed_primary_managed_is_not_recreated_on_sibling_relaunch` | 11266 |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Two test helpers beyond the two the plan named**
- **Found during:** Task 1, red
- **Issue:** The managed test must record `managed_by_baude` and the role of the row at `fx.repo` before the removal, but `remove_primary` as specified builds the App and removes the row in one call, and its return carries neither.
- **Fix:** `remove_primary` keeps the planned signature and delegates to `remove_primary_of(fx, &mut app)`, which the managed test calls on its own App after `git switch -c feature/work`. `CheckoutsSeen` and `checkouts_seen` gather the highest key, the `PrimaryDefault` rows and the kept rows once, so both tests take the same observations before and after the relaunch.
- **Files modified:** `baude/src/app.rs`
- **Commit:** `7a1b0e6`

### Notes

- The plan anchors name HEAD `9303552`; the base was `10b5bb6`, a docs commit on top. Every line anchor held.
- rustfmt reflowed the new tests before the red commit, so the red command was run again on the formatted tree. `target/gsd-22/07-red.log` is that run and matches the red commit line for line.
- The session count and the two full messages under "What a user sees" came from a temporary `eprintln!` in the two tests, run once and reverted with `git checkout -- baude/src/app.rs`. Log: `target/gsd-22/07-probe.log`. Nothing from it is committed.
- The removal in both tests succeeded; no "failed to terminate exact PTY process group" failure appeared.

## Open

- The tests read "highest checkout key" as the maximum key over checkout rows, not the allocator's counter. The consult returns before `allocate_checkout_key`, so no key is spent, but a key allocated with no row pushed would not show in this measure. Plan 22-08 can pin the counter if that matters.
- The tests do not assert the session count. It was observed as zero in both shapes; plan 22-08 pins the routes and can pin it.
- The managed test asserts nothing about the store entry for the deleted path. The repository-scoped query answering after the folder is gone is what the green result shows.
- `STATE.md`, `ROADMAP.md` and `REQUIREMENTS.md` were left alone as instructed; REL-01 is not marked here.
- The full gates on the final tree belong to plan 22-05.

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: `baude-core/src/suppression.rs` contains `pub fn removed_primary`
- FOUND: `baude-core/src/removal.rs` contains `suppress_primary`
- FOUND: `baude/src/app.rs` contains `fn removed_primary_managed_is_not_recreated_on_sibling_relaunch`
- FOUND: commit `7a1b0e6`
- FOUND: commit `754665e`
