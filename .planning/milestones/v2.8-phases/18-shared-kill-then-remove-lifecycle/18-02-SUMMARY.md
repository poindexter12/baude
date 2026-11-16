---
phase: 18-shared-kill-then-remove-lifecycle
plan: 02
subsystem: git
tags: [rust, git, worktree, filesystem, tdd]

# Dependency graph
requires:
  - phase: 18-shared-kill-then-remove-lifecycle
    plan: 01
    provides: DiskAction::RemoveManagedWorktree rows that plan 04 feeds into this helper
  - phase: 06-shared-lifecycle-core-refactor
    provides: worktrees_base, worktree_inventory, WorktreeRecord, RepositoryDiscoveryError
provides:
  - remove_managed_worktree_forced in baude-core/src/git.rs with containment, identity re-proof, git worktree remove --force --force, a guarded remove_dir_all plus prune fallback, and a refs/heads postcondition
  - ForcedRemoval, ForcedRemovalMethod, ForcedRemovalError public types
  - removal marker written beside the target before git runs so a half-completed removal is retryable
  - run_worktree_command shared by the verified remove, forced remove, prune and unlock paths
affects: [18-04, 18-05, 18-06, phase-20-tui, phase-21-daemon]

# Actuals (#2632) - estimateTokens scale, chars/4 over the realized diff
actuals:
  tokens: 14931
  tasks: 3
  commits: 3
plan_head_before: 0200d5ae24b92ce59cf08e5137cf459dee8ccf29
plan_head_after: b2e52033845addbde02b07bbc93d6db2f5ef1b59

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Containment through canonicalize_through_missing: canonicalize the nearest existing ancestor and re-join the remainder, so a wiped managed root or missing repository directory still proves the path is under worktrees_base"
    - "Inspection failure is a refusal, never an authorization: an unreadable, malformed or symlinked .git refuses with Uninspectable before remove_dir_all"
    - "Removal marker through symlink_metadata plus File::create_new, matching marker.rs write_marker and read_marker"
    - "One run_worktree_command helper builds every git -C <main> worktree invocation"

key-files:
  created: []
  modified:
    - baude-core/src/git.rs

key-decisions:
  - "The .git shape guard runs after registration and before git worktree remove is spawned, not only before the fallback, because git follows a symlinked gitfile"
  - "A bare repository at the registered path is refused with ForeignRepository by its HEAD, objects and refs signature or a trimmed bare = true config line; a partial signature is no repository and the registration proof carries the deletion"
  - "A missing directory is accepted and pruned so plan 04's AlreadyRemoved rows still clean git administration"
  - "The removal marker is the only way an unregistered directory inside the root is reclaimed, and only when its two lines name this common dir"

patterns-established:
  - "Forced path bypasses exactly the cleanliness and lock checks of the verified path; common dir, main worktree, registration and repository identity are re-proved"
  - "Every refusal test asserts the directory still exists, the inventory is unchanged and branch_snapshot is unchanged"

requirements-completed: [RMV-01, RMV-07]

coverage:
  - id: T1
    description: "Unclean managed worktree inside the root is removed and refs/heads OIDs are unchanged"
    requirement: RMV-01
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#inside_root_unclean_tree_is_removed_and_branch_kept"
        status: pass
    human_judgment: false
  - id: T2
    description: "Locked worktree removed with --force --force in one call"
    requirement: RMV-01
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#locked_worktree_inside_root_is_removed"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#forced_arguments_never_touch_branches"
        status: pass
    human_judgment: false
  - id: T3
    description: "Git refusal inside the root falls back to delete plus prune"
    requirement: RMV-01
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#git_refusal_inside_root_falls_back_to_prune_and_delete"
        status: pass
    human_judgment: false
  - id: T4
    description: "Outside root, root itself, ancestor inside root, symlink inside root all refuse untouched"
    requirement: RMV-07
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#outside_root_is_refused_and_untouched"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#managed_root_itself_is_refused"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#ancestor_directory_inside_root_is_refused"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#symlinked_path_inside_root_is_refused"
        status: pass
    human_judgment: false
  - id: T5
    description: "Common dir mismatch, main worktree, unregistered directory and replacement repository refuse"
    requirement: RMV-07
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#common_dir_mismatch_is_refused"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#main_worktree_inside_root_is_refused"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#unregistered_directory_inside_root_is_refused"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#replacement_repository_at_registered_path_is_refused"
        status: pass
    human_judgment: false
  - id: T6
    description: "Missing directory is accepted and the administrative entry pruned"
    requirement: RMV-01
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#missing_directory_inside_root_prunes_admin_entry"
        status: pass
    human_judgment: false
  - id: T7
    description: "Half-completed removal is retryable through the marker; foreign marker is refused"
    requirement: RMV-01
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#partial_git_removal_leaves_retryable_orphan"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#unregistered_directory_with_foreign_marker_is_refused"
        status: pass
    human_judgment: false
  - id: T8
    description: "Uninspectable .git refuses before remove_dir_all"
    requirement: RMV-07
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#uninspectable_git_identity_is_refused"
        status: pass
    human_judgment: false
  - id: T9
    description: "Bare repository at the registered path refuses; partial signatures do not"
    requirement: RMV-07
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#replacement_bare_repository_at_registered_path_is_refused"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#lone_head_file_at_registered_path_is_not_a_bare_signature"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#non_bare_config_file_at_registered_path_is_not_a_bare_signature"
        status: pass
    human_judgment: false
  - id: T10
    description: "Symlinked gitfile refuses before git runs and no marker is written"
    requirement: RMV-07
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#symlinked_gitfile_at_registered_path_is_refused"
        status: pass
    human_judgment: false
  - id: T11
    description: "Missing repository directory and wiped managed root still prune through canonicalize_through_missing"
    requirement: RMV-01
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#missing_directory_and_repository_dir_inside_root_prunes_admin_entry"
        status: pass
      - kind: unit
        ref: "baude-core/src/git.rs#wiped_managed_root_still_prunes_admin_entry"
        status: pass
    human_judgment: false
  - id: T12
    description: "Symlink at the marker path is refused before git runs"
    requirement: RMV-07
    verification:
      - kind: unit
        ref: "baude-core/src/git.rs#symlinked_removal_marker_is_refused_before_git_runs"
        status: pass
    human_judgment: false

metrics:
  duration: "~35 min"
  completed: 2026-11-03
status: complete
---

# Phase 18 Plan 02: Forced Managed-Worktree Removal Summary

Forced removal of a managed worktree that re-proves containment under worktrees_base and git identity before `git worktree remove --force --force`, falls back to a guarded `remove_dir_all` plus prune, and refuses whenever the path cannot be shown to be the registered managed worktree.

## What Was Built

- `remove_managed_worktree_forced(path, expected_common_dir)` in `baude-core/src/git.rs` returning `ForcedRemoval { method: GitRemove | PruneFallback, .. }` or a typed `ForcedRemovalError`.
- Containment (`managed_root_contains`) resolves both the managed root and the target through `canonicalize_through_missing`, so a missing directory, a missing repository directory, or a wiped managed root still proves the path and is pruned rather than failing canonicalization. The root itself, ancestors, paths outside the root and symlinks inside it are refused.
- Identity re-proof: the main worktree's common dir must match `expected_common_dir`, the path must be a registered linked worktree of that common dir (or carry a matching removal marker from a prior half-completed attempt), and the directory's `.git` must still describe that worktree. A symlinked, directory, malformed or unreadable `.git` refuses with `Uninspectable`; a bare repository signature refuses with `ForeignRepository`. These shape checks run before git is spawned.
- Removal marker beside the target, written with `symlink_metadata` plus `File::create_new`, read back only as a small regular file whose two lines match; a symlink at the marker path refuses before git runs.
- Fallback: when git refuses a registered path inside the root, the guard re-checks the directory, deletes it, and runs `git worktree prune`; the inventory no longer lists it.
- Postcondition: `refs/heads` names and OIDs are snapshotted before and compared after; drift is a `Postcondition` error.
- Refactor: `run_worktree_command` now builds every `git -C <main> worktree ...` invocation for the verified remove, forced remove, prune and unlock paths.

## Tasks and Commits

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 | RED: forced_removal test module against the new signatures | 1e7bdbe | baude-core/src/git.rs |
| 2 | GREEN: containment, identity re-proof, forced remove, guarded fallback, postcondition | 5bf7f43 | baude-core/src/git.rs |
| 3 | REFACTOR: share the worktree command runner with the verified path | b2e5203 | baude-core/src/git.rs |

Task 3 was conditional on duplicated `Command` construction; four sites (verified remove, forced remove, prune, unlock) built the same `git -C <main>` command, so it applied.

## Verification

- RED (`cargo test -p baude-core --lib tests::forced_removal`): exit 101, no `error[E` lines, 22 failed on the stub's `Postcondition` assertions, 1 passed (`forced_arguments_never_touch_branches`, pinned against the real five-element vector).
- GREEN (same target): 23 passed, 0 failed.
- `cargo clippy -p baude-core --all-targets -- -D warnings`: clean on the first run after GREEN and again after the refactor.
- Full suite `cargo test -p baude-core`: 485 passed, 0 failed, 1 ignored.
- All plan acceptance greps passed. Criterion H (`"branch"`, `"-D"`, `"-d"` occurrences) rose from 13 to 14; the extra occurrence sits inside `mod tests`, and the production count is the single pre-existing `label.push_str("branch")`.

## TDD Gate Compliance

| Gate | Evidence |
| ---- | -------- |
| RED | Commit 1e7bdbe. Test target exit 101, zero compile errors, 22 failed / 1 passed. |
| GREEN | Commit 5bf7f43. Test target exit 0, 23 passed. Only production changed between RED and GREEN (472 lines added, 4 removed: the stub body). |
| REFACTOR | Commit b2e5203. Test target and full suite still green, clippy clean. |

`gsd_run check tdd-red-evidence` has no cargo adapter: it returns `INVALID_RED` with reason `unreadable_record` because a `cargo test` log is not a JSON report, the same gap plan 18-01 recorded. The plan's own RED gate (exit code, no `error[E`, assertion-only failures) was applied instead.

## Deviations from Plan

None in behavior; the plan executed as written. Two notes for the record:

- The plan's prose names the RED failure count as eighteen in one place and twenty-two in another; the behavior list yields 22 failing tests plus the one pinned argument test, which is what landed.
- No clippy-driven shape change was needed, unlike 18-01.

## Known Stubs

None. The stub `Postcondition { detail: "unimplemented" }` from the RED commit was replaced in GREEN.

## Threat Flags

None beyond the plan's threat model. The helper only widens deletion authority inside `worktrees_base()` and every widening is gated by a refusal test.

## Self-Check: PASSED

- `baude-core/src/git.rs` exists and carries `remove_managed_worktree_forced`.
- Commits 1e7bdbe, 5bf7f43, b2e5203 are ancestors of HEAD on `gsd/phase-18-shared-kill-then-remove-lifecycle`.
