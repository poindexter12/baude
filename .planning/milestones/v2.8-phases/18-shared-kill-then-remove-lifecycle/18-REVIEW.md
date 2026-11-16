---
phase: 18-shared-kill-then-remove-lifecycle
reviewed: 2026-11-04T01:27:01Z
depth: standard
files_reviewed: 7
files_reviewed_list:
  - baude-core/src/removal.rs
  - baude-core/src/git.rs
  - baude-core/src/repository.rs
  - baude-core/src/pty.rs
  - baude-core/src/session.rs
  - baude-core/src/testing.rs
  - baude-core/src/lib.rs
findings:
  critical: 1
  warning: 5
  info: 5
  total: 11
status: fixed
---

# Phase 18: Code Review Report

**Reviewed:** 2026-11-04T01:27:01Z
**Depth:** standard
**Files Reviewed:** 7
**Status:** fixed (CR-01, WR-01 to WR-05 fixed; IN-01 to IN-05 open, out of fix scope)

## Summary

Reviewed the production side of `git diff 73fa674..HEAD` for the seven files in scope: the new `removal` module, the forced managed-worktree removal in `git.rs`, `RepositoryState::remove_rows`, the reworked `Pty::kill_and_wait` with its pure `group_disposition` rule, `Session::kill_and_wait`, and the `cloned_topology` fixture. Test modules were read only where a production claim depended on them.

The stage order, the one atomic save, the typed error surface and the preflight identity checks are sound. Path containment resolves both sides through the nearest existing ancestor, refuses a symlink at the target, and rejects `..` after a missing ancestor; `remove_dir_all` is only ever called on a path already proven strictly inside the resolved managed root. The reader-thread lock ordering is correct: nothing waits on the child mutex until the group is extinct or proven foreign, polling reaps through `try_lock`, and the error arm never takes the lock. The test-only `revoke(2)` helper is `#[cfg(test)]` inside `impl Pty` and unreachable from production.

The one Critical is collateral damage: both fallback arms of `remove_managed_worktree_forced` run repository-wide `git worktree prune`, which drops the administrative directory of every other registered worktree whose directory is temporarily absent, a state the startup reconcile explicitly supports as "unavailable". Verified empirically that `git worktree remove --force --force` on a missing directory drops only that entry, so a per-entry path exists. The Warnings cover a pgid-reuse hole in the new `Ok(None)` disposition, the fallback overriding git's submodule refusal, an unremovable repository once its main worktree is gone, a symlinked managed root refusing every removal, and the `AlreadyRemoved` arm in `removal.rs` mislabelling two real outcomes.

## Critical Issues

### CR-01: Repository-wide `git worktree prune` destroys other rows' worktree administration

**File:** `baude-core/src/git.rs:3281`, `baude-core/src/git.rs:3293`, `baude-core/src/git.rs:3316-3319`, `baude-core/src/git.rs:3339`
**Status:** fixed in 40642a7 — fallback arms no longer run `git worktree prune`; the missing-directory arm calls `git worktree remove --force --force -- <path>` on the single entry, and the unclean fallback deletes only `<common>/worktrees/<id>` after proving its `gitdir` file points at the target. Fixture tests keep a sibling worktree's admin entry while its directory is absent.

**Issue:** `ForcedStep::PruneOnly` and `ForcedStep::Fallback` both finish with `prune_worktrees(main_worktree)`, which is `git worktree prune` with no path argument. Prune is repository-wide: it deletes `<common>/worktrees/<id>` for every registered worktree whose directory is not currently present, not just the target. A sibling checkout on an unmounted external volume or a disconnected network share, which the startup reconcile deliberately keeps as an "unavailable" row, loses its worktree HEAD, index, reflog and lock state and cannot be re-attached when the volume returns. The `(None, false)` arm (`PruneOnly`) is the worst case: nothing is registered and nothing is on disk for the target, so the prune can only affect other rows, yet it runs on every `DiskAction::AlreadyRemoved` row that git has already dropped. Verified on a scratch repository with two worktrees deleted from disk: `git worktree remove --force --force -- <missing path>` removes only that entry (3 entries to 2), then `git worktree prune` removes the other (2 to 1).
**Fix:**
```rust
// (None, false): nothing registered, nothing on disk. There is nothing of ours to clean.
(None, false) => ForcedStep::Nothing,

// (Some(_), false): registered, directory gone. Git removes a missing worktree per entry.
(Some(_), false) => {
    let output = run_worktree_command(
        main_worktree,
        forced_remove_arguments().into_iter().chain([canonical.as_os_str()]),
    )
    .map_err(ForcedRemovalError::CommandStart)?;
    if output.status.success() { ForcedStep::GitRemoved } else { ForcedStep::Fallback(stderr_of(&output)) }
}

// Fallback: after remove_dir_all, retry the per-entry remove before any prune, and
// reach for prune only when the entry still shows in the inventory. If prune must
// run, document in the report that it is repository-wide, or drop it entirely and
// surface Postcondition so the caller can decide.
```
Add a test with a second registered worktree whose directory is absent, remove the target, and assert the sibling's `<common>/worktrees/<id>` still exists and `git worktree list` still names it.

## Warnings

### WR-01: `Ok(None)` disposition signals a live group whose membership is never proven ours

**File:** `baude-core/src/pty.rs:956`, `baude-core/src/pty.rs:839`
**Status:** fixed in 7c7ecab — when the recorded leader is gone or its pid was reused elsewhere, the live members of the recorded group are enumerated and a member in the recorded session must prove the group is ours; a live group with no such member is `ForeignOwner`, an enumeration error is `Unverifiable`. Table test plus a live `group_members` test on a real child group.

**Issue:** `group_disposition` returns `Signal` when the recorded leader pid no longer exists but `kill(-pgid, 0)` succeeds. The comment assumes the survivors are descendants, but a process group id is reusable once the group is empty: after the whole group dies, a new same-user process that gets the old leader pid and calls `setsid()` or `setpgid(0, 0)` owns a group with exactly that id. The pre-phase code returned `Ok` on identity mismatch without signalling; the new rule sends SIGTERM then SIGKILL to that stranger's whole group. The `ForeignOwner` arm only catches the case where the reused pid is observable and sits in the group; it does not catch a reused pgid whose leader is a different pid, nor a reused leader pid that is unobservable. The `EPERM` branch of `process_group_exists` already shows the authors know the id can belong to someone else.
**Fix:** Before signalling on `Ok(None)`, enumerate the group's members (meta.rs already walks `/proc`; macOS has `proc_listpids(PROC_PGRP_ONLY, pgid)`) and require each member's session id to equal `recorded.session` and its start time to be no earlier than `recorded.start_time`; otherwise return `ForeignOwner`. At minimum, table-test the reused-pgid case so the decision is explicit rather than implied.

### WR-02: Fallback overrides git's submodule refusal and prunes submodule gitdirs

**File:** `baude-core/src/git.rs:3320-3339`
**Status:** fixed in b925c73 — before any fallback, the admin entry's `modules/` directory is scanned for absorbed submodule gitdirs and the removal is refused with a typed `Submodules` kind. Fixture test with a real absorbed submodule in a managed worktree.

**Issue:** `git worktree remove` refuses, even with `--force --force`, when the worktree contains submodules ("working trees containing submodules cannot be moved or removed"). That refusal lands in `ForcedStep::Fallback`, which `remove_dir_all`s the directory and then prunes `<common>/worktrees/<id>`, where a linked worktree's submodule gitdirs live under `modules/`. Unpushed submodule commits are gone with no typed refusal. `foreign_repository_guard` inspects only the top-level `.git`, so nested repositories further down are not seen either.
**Fix:** Inspect `git_stderr` for git's submodule refusal (or run `git -C <path> submodule status` before the removal) and return a new `ForcedRemovalError::Submodules { path }` instead of falling back. Reserve the fallback for refusals the user explicitly waived (unclean tree, lock, cwd held).

### WR-03: A repository whose main worktree is gone can never remove a managed child, and the kill has already happened

**File:** `baude-core/src/git.rs:2990-2997`, `baude-core/src/removal.rs:411-412`
**Status:** fixed in 0ddf485 — every forced-removal git command now runs with `--git-dir=<common>` so a missing main checkout is no longer fatal, and `remove` probes each owner common dir via `git rev-parse --git-common-dir` before any kill, failing typed and leaving the runtime alive when the repository is unusable. Fixture tests: main checkout gone, and a live runtime surviving a refused removal.

**Issue:** Every arm of `remove_managed_worktree_forced` starts with `registered_linked_worktree`, which runs `git -C <main_worktree> rev-parse`. When the main checkout directory has been deleted outside baude, `git -C` fails, `discovery_to_forced` maps it to `Uninspectable`, and `disk_stage` aborts. By then `kill_stage` has already stopped every runtime in the set. The user sees all sessions killed, no rows dropped, and no way to ever remove the repository through this path. Nothing in `select_rows` or `preflight` detects the missing main before the kill.
**Fix:** Run the inventory, remove and prune commands with `--git-dir=<expected_common_dir>` (the common dir is already canonicalized and identity-checked) instead of `-C <main_worktree>`, so a vanished main checkout does not block cleanup of children inside the managed root. Additionally, probe `main_worktree` and `common_dir` in `preflight` and return a typed error before the kill stage when either is missing, so the all-or-nothing promise covers this case.

### WR-04: A symlinked managed root refuses every forced removal with a misleading error

**File:** `baude-core/src/git.rs:2955-2956`
**Status:** fixed in ff192e9 — a missing managed path is re-rooted onto the managed root's canonical path before the lies-inside check, so a symlinked managed root is accepted; an existing path keeps its own symlink check. Fixture test with a symlinked worktrees root.

**Issue:** `managed_root_contains` resolves the root with `canonicalize_through_missing`, which returns `Symlink { path }` when the path itself is a symlink. `real_worktrees_base()` is `$XDG_DATA_HOME/baude/worktrees`; relocating that directory to another disk via a symlink is routine. Worktree creation and discovery follow the link fine, and `observed_path` is recorded canonical, so every row of such a user is refused with "refusing forced removal of <root>: path is a symlink", naming the root rather than the target. The symlink refusal protects the target from being deleted through a link; applying it to the root buys nothing because the target is compared after both sides resolve.
**Fix:** Resolve the root with plain `canonicalize()` (falling back to the missing-ancestor walk only when it does not exist) and keep the symlink refusal for the target and its ancestors below the root. Add a test with the managed root as a symlink.

### WR-05: `AlreadyRemoved` arm mislabels two real outcomes

**File:** `baude-core/src/removal.rs:574-581`
**Status:** fixed in 89271fe — both managed arms report by what the forced helper did: `deleted` only when a directory was deleted, `already_removed` for `NotRegistered` while the path is absent, and a typed `NotRegistered` failure when an unregistered directory sits at the row's path. Fixture tests for a reappeared-and-deleted directory and a stray one.

**Issue:** The arm treats `Ok(_)` and `Err(NotRegistered)` identically and pushes the path to `already_removed`. Two reachable cases are wrong: (a) the directory reappeared between `select_rows` and `disk_stage` with a matching removal marker, in which case `remove_managed_worktree_forced` takes the fallback and deletes it, but the report says nothing was deleted; (b) the directory reappeared without a marker, `NotRegistered` is returned, the row is dropped from state, and a stray directory now sits inside the managed root with no row and no report line. `RemovalReport` is the Phase 21 REST contract (RMV-05), so these labels will be served to users.
**Fix:** Match on `Ok(ForcedRemoval { method, .. })`: `GitRemove` and `PruneFallback` with a present-before-call directory go to `deleted`; only a confirmed-absent directory goes to `already_removed`. Treat `NotRegistered` with a present directory as `DiskRemoval` error rather than success, since a directory is now at the row's path.

## Info

### IN-01: `hang_up_slave_for_test` dereferences `ptsname` without a null check and relies on a non-reentrant static buffer

**File:** `baude-core/src/pty.rs:634`
**Issue:** `libc::ptsname` returns NULL on failure, and `CStr::from_ptr(NULL)` is undefined behavior rather than a test panic. The returned buffer is static and not thread-safe; `cargo test` runs tests on parallel threads, so two PTY tests calling it concurrently can read each other's name. Test-only, cannot reach production, but a wrong slave path would `revoke` another test's terminal and hang that test.
**Fix:** Check for null and panic with a message; use `ptsname_r` via a local `extern "C"` declaration (available on macOS) so each call gets its own buffer.

### IN-02: `GroupDisposition` is `pub` with only a `pub(crate)` producer

**File:** `baude-core/src/pty.rs:925`, `baude-core/src/pty.rs:945`
**Issue:** The enum is exported from the crate while `group_disposition` is crate-private, so no external caller can obtain a value. Dead public API surface.
**Fix:** `pub(crate) enum GroupDisposition`.

### IN-03: `SessionTeardownError::agent_pid` is `Option<u32>` but is always `Some` now

**File:** `baude-core/src/session.rs:516`, `baude-core/src/session.rs:537`
**Issue:** The phase changed the constructor to read the immutable recorded identity, so `agent_pid` is never `None`; the `Option` forces every consumer (and the removal error path) to handle a case that cannot occur.
**Fix:** Make `agent_pid: u32`, or drop it in favor of `agent_identity.pid` since `agent_identity` is also always `Some`.

### IN-04: Bounded reader join blocks the caller's thread for up to 2 s per Pty

**File:** `baude-core/src/pty.rs:896-910`
**Issue:** `join_reader` sleeps in 10 ms steps up to 2000 ms whenever an out-of-group process holds the slave open (a `setsid`/`nohup` background job in the shell PTY is enough). A parent removal with N such rows blocks `remove`, and therefore the TUI thread, for up to 4N seconds. Documented as bounded, and no grace knob is allowed by the phase decisions, but the cost compounds across the set.
**Fix:** Join once after the whole kill stage rather than per Pty (collect handles, then wait on all with one shared deadline), or skip the sleep loop when `is_finished()` is already true at the first check.

### IN-05: The `RepositoryReservations` hold is a documentation-only contract

**File:** `baude-core/src/removal.rs:19-23`, `baude-core/src/removal.rs:394-399`
**Issue:** Correctness of the half-committed window between the disk stage and the save depends on callers holding the reservation for every repository in the set, but `remove` neither takes the reservation nor checks it. Phases 20 and 21 can call it without one and nothing fails.
**Fix:** Take `&lifecycle::RepositoryReservations` (or a held-guard type) as a parameter of `remove` and `select_rows`, so the hold is a compile-time requirement.

---

_Reviewed: 2026-11-04T01:27:01Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
