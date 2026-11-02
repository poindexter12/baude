# Phase 18: Shared Kill-Then-Remove Lifecycle - Context

**Gathered:** 2026-11-02
**Status:** Ready for planning

<domain>
## Phase Boundary

`baude-core` gains one removal operation usable for any row kind (repository parent, main checkout, managed worktree, unmanaged checkout, standalone folder). It kills the owned runtime first, removes managed worktree directories from disk, never deletes a main checkout, standalone folder or local branch, and persists the removed row set atomically. The TUI (Phase 20) and daemon (Phase 21) call this one path. Re-admission suppression (bindings, markers) is Phase 19, not this phase.

Requirements: KILL-01, KILL-02, KILL-03, RMV-01, RMV-02, RMV-03, RMV-04, RMV-05, RMV-06, RMV-07.

</domain>

<decisions>
## Implementation Decisions

### Kill Semantics
- The kill unit per row is the whole session: `Session::kill_and_wait` stops the agent PTY and the shell PTY. It already escalates SIGTERM to SIGKILL per exact process group and reaps the child (KILL-01, KILL-02).
- No new grace timer or config knob. Reuse the existing `wait_for_group_extinction` bound in `baude-core/src/pty.rs`.
- A runtime that survives SIGKILL returns a structured error carrying the surviving pids. The row and the state file are untouched; the caller surfaces the error (KILL-03).
- Parent removal is all-or-nothing at the kill stage: if any child's kill fails, abort before any disk or state change. Nothing is dropped and the error names the child.

### Disk Removal Rules
- Managed worktree children are deleted after the kill with the force path (`git worktree remove --force`), bypassing the clean-tree preflight in `remove_verified_worktree`. The user asked for removal explicitly, so unclean trees and a runtime that held the tree as cwd do not block it (RMV-01).
- If `git worktree remove` fails and the path is inside baude's managed worktree root, fall back to `git worktree prune` plus deleting the directory. Never delete anything outside the managed root.
- Main checkout, unmanaged checkout and standalone rows drop the row and its runtime only. The directory and every local branch are untouched (RMV-02, RMV-03, RMV-07). Managed worktree branches are also never deleted.
- Baude's own markers and breadcrumbs inside a kept folder are left in place in this phase. Clearing ownership is Phase 19.
- Removing a row whose worktree directory is already gone succeeds as already removed. The row drops and the report records that no path was deleted.

### Operation Shape and Atomicity
- One public entry point in `baude-core` takes a removal target (repository, checkout, worktree or standalone row) and returns a report listing rows removed, runtimes killed and paths deleted, or a typed error. Phase 21's REST response reuses this report (RMV-05).
- Stage order: kill every targeted runtime, then delete worktree directories, then one atomic state save for the whole row set through the existing temp-file-plus-rename path in `persist.rs` (RMV-06). No per-child persistence.
- The gap between disk delete and the state save is accepted: the prior state is intact in the file and the existing startup reconcile marks a missing worktree unavailable. No intent journal.
- A side-effect-free preview function computes the same report before acting, so the Phase 20 TUI and Phase 21 PWA confirmations describe exactly what will happen.

### Claude's Discretion
- Exact names of the target enum, report struct and error type.
- Test process doubles for live-runtime fixtures, as long as they spawn a real process group and tests assert its extinction on fixture output, per the repo's fixture-realism rule.
- Whether repository parent removal reuses the existing `RepositoryState` child removal helpers or adds a dedicated batch method, provided the single atomic save holds.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Pty::kill_and_wait` (`baude-core/src/pty.rs`): verifies process identity, SIGTERM then SIGKILL on the exact process group, waits for extinction, reaps the child, bails with a descriptive error if the group survives.
- `Session::kill_and_wait` (`baude-core/src/session.rs`): stops agent and shell independently, returns `SessionTeardownError` with pids and per-process failures; idempotent and retryable.
- `git::remove_verified_worktree` and `remove_verified_worktree_with_post_remove_hook` (`baude-core/src/git.rs`): current managed worktree removal with clean-tree preflight; `lifecycle.rs` tests already use `git worktree remove --force`.
- `persist.rs` `save`, `save_for_workspace`, `save_current_at`: atomic temp-file-plus-rename state writes.
- `RepositoryState::remove_standalone_session` (`baude-core/src/repository.rs`): existing single-row removal to generalize from.
- `testing.rs` fixture helpers; `meta.rs` tests already spawn and SIGKILL real children.

### Established Patterns
- Lifecycle changes flow through `LifecycleEngine` with `persist_lifecycle` and `apply_lifecycle_effect` effects, keeping state mutation separate from side effects.
- Errors are typed structs or enums carrying pids and paths rather than strings.
- Tests build cloned and `git init` fixtures and assert on fixture output, never by grepping source.

### Integration Points
- TUI keys `w`, `x`, `X` in `baude/src/app.rs` near lines 331 to 372 are the existing close and worktree lifecycle entry points the Phase 20 remove action will sit beside.
- `bauded` will expose the operation over REST in Phase 21; the report struct must be serializable.
- `scripts/gate.sh --linux` is the required check for the signal and procfs paths.

</code_context>

<specifics>
## Specific Ideas

- Compose from existing kill, close and managed-worktree removal stages rather than a parallel code path (RMV-05 wording).
- Removal must never delete a main checkout, a standalone folder or a local branch, with a test for each on cloned and `git init` fixtures (RMV-07).

</specifics>

<deferred>
## Deferred Ideas

- Clearing folder-workspace bindings and marker ownership on removal: Phase 19.
- Any confirmation UI or REST endpoint: Phases 20 and 21.

</deferred>
