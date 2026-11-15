# Phase 18: Shared Kill-Then-Remove Lifecycle - Pattern Map

**Mapped:** 2026-11-02
**Files analyzed:** 5 (1 new, 4 modified)
**Analogs found:** 5 / 5

All analog paths below are git-tracked source (`git ls-files` verified for every file named). No `.gsd/capabilities` mirrors exist in this repo.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/removal.rs` (NEW) | service (lifecycle operation module) | batch / transform (select rows, kill, delete, one save) | `baude-core/src/lifecycle.rs` (removal protocol at 673-1000, `LifecycleOutcome` 2784, fixtures 2924-2990, 3908, 4286-4330) + `baude-core/src/session.rs:478-523` | role-match (same tier, different protocol: one save instead of two) |
| `baude-core/src/git.rs` (ADD `remove_managed_worktree_forced` + `ForcedRemovalError`) | utility (git shell-out) | request-response (Command → postcondition proof) | same file: `remove_verified_worktree_with_post_remove_hook` 2681-2715, `verify_removal_postconditions` 2622-2679, `RemoveVerifiedError` 2049-2053 | exact |
| `baude-core/src/repository.rs` (ADD `RepositoryState::remove_rows`) | model (aggregate mutation) | CRUD (batch delete) | same file: `remove_standalone_session` 870-879, `validate` 894-962 | exact |
| `baude-core/src/lib.rs` (ADD `pub mod removal;`) | config | — | same file lines 7-26 | exact |
| `baude-core/src/removal.rs` `#[cfg(test)] mod tests` | test | integration (real PTY children, real git fixtures) | `baude-core/src/session.rs:1440-1492`, `baude-core/src/pty.rs:1338-1396`, `baude-core/src/lifecycle.rs:2924-2990, 3908-3920, 4286-4330` | exact |

Files explicitly NOT modified this phase: `pty.rs`, `session.rs`, `persist.rs`, `baude/src/app.rs` (Phase 20), `bauded/src/manager.rs` (Phase 21), `lifecycle.rs` types.

## Pattern Assignments

### `baude-core/src/removal.rs` (service, batch)

**Analog A: kill stage** — `baude-core/src/session.rs:478-523`. Call verbatim; do not re-implement. The error type to wrap:

```rust
// session.rs:478
pub fn kill_and_wait(&mut self) -> std::result::Result<(), SessionTeardownError> {
    // Attempt each owned process independently. A partially successful
    // stop remains retryable because Pty::kill_and_wait is idempotent.
    let claude = self.claude.kill_and_wait().err();
    let shell = self.shell.as_mut().and_then(|shell| shell.kill_and_wait().err());
    match (claude, shell) {
        (None, None) => Ok(()),
        (claude, shell) => { /* ... */ Err(SessionTeardownError { agent_pid, shell_pid,
            agent_identity, shell_identity, agent_stopped, shell_stopped, detail: failures.join("; ") }) }
    }
}

// session.rs:516-523
#[derive(Debug)]
pub struct SessionTeardownError {
    pub agent_pid: Option<u32>,
    pub shell_pid: Option<u32>,
    pub agent_identity: Option<ProcessIdentity>,
    pub shell_identity: Option<ProcessIdentity>,
    pub agent_stopped: bool,
    pub shell_stopped: bool,
    pub detail: String,
}
```

Underlying primitive (`pty.rs:744-791`) is idempotent: early `Ok` on already-exited child or identity mismatch (pid reuse guard, 766-772), then `signal_group(SIGTERM)` → `wait_for_group_extinction` (50 × 10 ms, 830-838) → `SIGKILL` → wait → `child.wait()` reap. Debug-only injection `fail_next_teardown_for_test(detail)` at 794-802 is consumed at 745-753; use it for the KILL-03 survivor test.

**Analog B: typed error / outcome enums** — `baude-core/src/lifecycle.rs:2782-2800` and `git.rs:2049-2053`. The codebase uses plain enums with struct variants carrying keys, pids and paths, plus a hand-written `fmt::Display`; no `thiserror`.

```rust
// lifecycle.rs:2782-2784
/// Shared lifecycle meaning returned by local and daemon runtime owners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleOutcome {
    Created { checkout: CheckoutKey, runtime: Option<u64> },
    ...
}

// git.rs:2048-2053
#[derive(Debug)]
pub enum RemoveVerifiedError {
    CommandStart(std::io::Error),
    GitRefused { status: Option<i32>, stderr: String },
    Postcondition(RemovalPostconditionFailure),
}
impl fmt::Display for RemoveVerifiedError { /* match self, write! per variant */ }
```

For the report struct add `#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]` exactly as `repository.rs:336-337` and `persist.rs:19-24` do (serde already imported there as `use serde::{Deserialize, Serialize};`). The error enum stays `Debug + Display` only (it carries `SessionTeardownError`, which is not serde).

**Analog C: single save with commit-on-success** — `baude/src/app.rs:3870-3899` (pattern only; core takes a closure, callers decide). Shape to mirror in core:

```rust
// app.rs:3870-3899
let before_deletion = self.repository_state.clone();
let outcome = lifecycle::commit_removed_checkout(&mut self.repository_state, &confirmation, &removal)?;
match self.save_durable_status() {
    Ok(()) => { self.persistence_dirty = false; Ok(outcome) }
    Err(error) if !error.replacement_committed() => {
        self.repository_state = before_deletion;          // rename never happened: roll back memory
        ...
    }
    Err(error) => { self.persistence_dirty = true; ... }  // file replaced, dir fsync failed: keep new state
}
```

Closure signature the callers can satisfy (`app.rs:1944-1958`, `manager.rs:720-728` both end in `persist::save_for_workspace_status(..., &StateFile) -> Result<(), SaveError>` or `save_current_at_test` under test):

```rust
save: impl FnOnce(&persist::StateFile) -> Result<(), persist::SaveError>
// persist.rs:17-24
pub const SCHEMA_VERSION: u32 = 3;
pub struct StateFile { pub schema_version: u32, pub state: RepositoryState }
// persist.rs:278-280
pub fn replacement_committed(&self) -> bool
```

**Module doc / imports pattern** — `lifecycle.rs` and `git.rs` open with `//!` doc, `use std::...` block, blank line, `use anyhow::...; use serde::...;`, blank line, `use crate::...` (see `persist.rs:1-15`, `repository.rs:1-9`). Follow that order.

---

### `baude-core/src/git.rs` — `remove_managed_worktree_forced` (utility, request-response)

**Analog:** same file, `remove_verified_worktree_with_post_remove_hook` at 2681-2704. Copy the Command shape and add `--force` (recommend `--force --force` inside the managed root for locked trees, per RESEARCH Open Question 1):

```rust
// git.rs:2614-2620 (argument helper; write a forced_remove_arguments() beside it)
fn verified_remove_arguments() -> [&'static OsStr; 3] {
    [OsStr::new("worktree"), OsStr::new("remove"), OsStr::new("--")]
}

// git.rs:2686-2700
let output = Command::new("git")
    .arg("-C")
    .arg(&target.main_worktree)
    .args(arguments)
    .arg(&target.path)
    .output()
    .map_err(RemoveVerifiedError::CommandStart)?;
if !output.status.success() {
    return Err(RemoveVerifiedError::GitRefused {
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    });
}
```

**Containment pattern** — `git.rs:1787-1797` (`pub(crate) fn worktrees_base()`), honors `crate::testing::worktrees_base_override()` and calls `assert_contained` on the real root. Canonicalize the target (or its parent when the leaf is gone, Pitfall 4) and require `starts_with(worktrees_base())` before any `remove_dir_all`.

**Postcondition pattern** — `git.rs:2622-2679` `verify_removal_postconditions`: re-`discover_repository(main_worktree)`, assert `common_dir`/`main_worktree` unchanged, assert path absent from `fresh.worktrees`, `symlink_metadata` is `NotFound`, and `removal_oid(main, "<branch_ref>^{commit}")` equals the recorded OID (branch never deleted). Reuse directly if a `VerifiedRemovalTarget` (private fields `path, common_dir, main_worktree, branch_ref, branch_oid`, 1979-1985) can be built from `discover_repository` inside the file; otherwise write a forced-variant in the same style.

**Argument guard test pattern** — `git.rs:4302-4304` and 4508-4511: tests assert forbidden tokens never appear. For the forced path assert `"delete"`, `"-d"`, `"-D"`, `"-B"`, `"branch"` are absent even though `"--force"` and `"prune"` are now allowed.

```rust
// git.rs:4302
for forbidden in ["--force", "-B", "fetch", "delete", "remove"] {
    assert!(!new.iter().chain(&existing).any(|arg| arg == forbidden));
}
```

---

### `baude-core/src/repository.rs` — `RepositoryState::remove_rows` (model, CRUD)

**Analog:** same file, 870-879. Generalize to checkouts by key, repository by key, standalone by key, returning removed rows for the report:

```rust
// repository.rs:870-879
pub fn remove_standalone_session(&mut self, key: StandaloneKey) -> Option<SavedStandaloneSession> {
    let index = self.standalone_sessions.iter().position(|session| session.key == key)?;
    Some(self.standalone_sessions.remove(index))
}
```

Row facts the selector reads (all `pub`): `SavedCheckout { key, repository_key, role: CheckoutRole, managed_by_baude, observed_path: PersistedPath, observed_branch, .. }` (348-365), `owned_runtime()` (402, 486), `CheckoutRole { Main, PrimaryDefault, ManagedBranch }` (284-288), `RepositoryState { repositories, checkouts, standalone_sessions, .. }` (510-518).

**Validation to run on the clone before save** — `validate()` 894-962; the check that makes parent-last ordering mandatory:

```rust
// repository.rs:940-944
if !repository_keys.contains(&checkout.repository_key) {
    return Err(ValidationError::DanglingRepositoryKey(checkout.repository_key));
}
```

---

### `baude-core/src/lib.rs` (config)

**Analog:** lines 7-26, alphabetical `pub mod` list. Insert `pub mod removal;` between `pub mod pty;` and `pub mod repository;`.

---

### `baude-core/src/removal.rs` tests (test, integration)

**Fixture isolation** — `lifecycle.rs:2924-2970` `LifecycleFixture` (private to lifecycle; copy or lift into `testing.rs`). Key details to preserve: canonicalized temp root, `TestRedirect::new(&root)` acquired BEFORE `workspace::override_for_test`, fields held as struct members so drop order restores identity under the live redirect.

**Git topology** — `lifecycle.rs:4290-4330` `cloned_topology` (upstream `git init -q -b main` → clone → `worktree add <default> main` → `worktree add -b feature/kept <managed>`, each path as `discover_repository` reports it) and `git_in` at 3908-3920:

```rust
// lifecycle.rs:3908
fn git_in(dir: &Path, args: &[&str]) {
    let output = Command::new("git").args(args).current_dir(dir).output().expect("run git");
    assert!(output.status.success(), "git {args:?} in {}: {}", dir.display(),
        String::from_utf8_lossy(&output.stderr));
}
```

**Live-runtime double** — `pty.rs:1375-1396` and `session.rs:1440-1492`: spawn `Pty::spawn(Some("trap '' TERM HUP; sleep 30"), cwd, 5, 40)`, sleep 200 ms so the trap arms, build a `Session { claude: agent, shell: Some(shell) | None, ... }` literal (full field list at session.rs:1464-1484), then assert extinction on fixture output:

```rust
// pty.rs:1341-1350
fn assert_reaped_promptly(pid: u32) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while is_zombie(pid) {
        assert!(std::time::Instant::now() < deadline, "pid {pid} is still a zombie after 5s; ...");
        std::thread::sleep(Duration::from_millis(20));
    }
}
```

`assert_reaped_promptly`/`is_zombie` are private to `pty::tests`; `assert_reaped` is private to `session::tests`. Copy one into `removal::tests` (or lift to `testing.rs`). Group extinction: `unsafe { libc::kill(-pgid, 0) } == -1` with `ESRCH`.

**Persistence injection** — `persist::save_current_at_test(root, file, &state, Some(AtomicFailure::Rename|Write), None)` (variants at persist.rs:242-250) for RMV-06 and SC5; compare file bytes before/after for the "untouched" assertions.

## Shared Patterns

### Typed errors, no strings
**Source:** `git.rs:2048-2053`, `session.rs:516-523`
**Apply to:** `RemovalError`, `ForcedRemovalError`. Struct-variant enums with pids/paths/keys, `#[derive(Debug)]`, manual `impl fmt::Display`, `impl std::error::Error`.

### Serde derives on anything REST will reuse
**Source:** `persist.rs:19-24`, `repository.rs:336-337`
**Apply to:** `RemovalTarget`, `RemovalReport`, `RowKey`. `#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]`; consider `#[serde(deny_unknown_fields)]` as the persisted types do.

### Never pipe the gate
**Source:** `scripts/gate.sh`, repo CLAUDE.md
**Apply to:** every task's verify step: `scripts/gate.sh` and `scripts/gate.sh --linux` unpiped.

### Anti-patterns (do not copy)
- `lifecycle::destructive_teardown` (lifecycle.rs:984-1000) mutates state via `mark_teardown_pending`; violates KILL-03.
- `revoke_removal_authority` / `RemovalTombstone` two-save protocol (app.rs:3802-3821, 3877); violates RMV-06.
- `Pty::kill()` (session.rs:377-384) best-effort; use `kill_and_wait` only.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| — | — | — | Every file has an in-tree analog; no gaps. |

## Metadata

**Analog search scope:** `baude-core/src/{pty,session,git,persist,repository,lifecycle,testing,lib,meta}.rs`, `baude/src/app.rs`, `bauded/src/manager.rs`
**Files scanned:** 11 (targeted sed ranges; no whole-file reads)
**Pattern extraction date:** 2026-11-02
