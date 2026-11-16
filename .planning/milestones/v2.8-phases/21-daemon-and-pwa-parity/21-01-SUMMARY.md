---
phase: 21-daemon-and-pwa-parity
plan: 01
subsystem: api
tags: [rust, axum, bauded, removal, rest, persistence]

requires:
  - phase: 18-shared-kill-then-remove-lifecycle
    provides: baude_core::removal preview_removal and remove, the kill then delete then persist stages
  - phase: 20
    provides: the TUI confirm path in baude/src/app.rs that this daemon wrapper mirrors
provides:
  - POST /removals/preview and POST /removals on the bauded router, checkout rows only
  - Manager::preview_removal, Manager::remove_rows and RemoveRowsError
  - merge_live_runtime_metadata, the single survivor-metadata merge used by every save
  - api test fixtures removal_fixture and removal_fixture_two_checkouts
affects: [21-02, 21-03, 21-05, 21-07, 21-08, 21-09, 21-10]

tech-stack:
  added: []
  patterns:
    - "Removal save closure: snapshot survivor overrides before the core call, merge into a copy of core's candidate, never mutate the candidate"
    - "Removal fixtures use an argument-tolerant agent command and a ps liveness probe"

key-files:
  created: []
  modified:
    - bauded/src/manager.rs
    - bauded/src/api.rs

key-decisions:
  - "RemoveRowsError::Drift carries Box<RemovalReport>, not a bare report, because clippy result_large_err rejects the unboxed 240 byte variant under -D warnings; the 409 wire body is unchanged"
  - "removal_fixture and removal_fixture_two_checkouts are async fn, because admission is real HTTP through the router"
  - "removal_fixture_two_checkouts gives its repository a real bare origin with push -u, because the daemon refuses a managed worktree in a git init only repository"
  - "Preview holds no repository reservation; the global manager mutex already serializes it"

requirements-completed: []
requirements-advanced: [API-01]

duration: 11min
completed: 2026-11-11
status: complete
actuals:
  tokens: 10844
  tasks: 3
  commits: 3
plan_head_before: c7605d49ba94d5dde083a2f9160045049bfab12f
plan_head_after: 79e4f45ed0123e39744951f48882bfc0764efc9d
---

# Phase 21 Plan 01: Removal preview and execute over REST Summary

**`POST /removals/preview` and `POST /removals` on bauded for checkout rows, wrapping the Phase 18 core removal with a save closure that merges live survivor metadata so a removal never regresses another row's persisted state**

## Performance

- **Duration:** 11 min
- **Started:** 2026-11-11T20:48:00Z
- **Completed:** 2026-11-11T20:59:00Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- A checkout row can be previewed and removed over REST: the runtime is killed, the row leaves state and `runtime_checkouts`, and one save commits it.
- The HIGH #1 seam is closed and proven: the removal save writes core's candidate plus live survivor metadata, through the same merge function `state_for_save` uses.
- The HIGH #2 fixture defect is closed: every removal fixture runs a runtime that is observably alive before the removal and observably gone after.
- Two concurrent executes of one previewed report commit exactly once.

## RED gate

Recorded before any implementation existed, with `todo!()` bodies in `Manager::preview_removal` and `Manager::remove_rows`:

```
cargo test -p bauded api::tests::removals_ -- --test-threads=1 > /tmp/baude-21-01-red.log 2>&1
RED=101  REACHED=0

thread 'api::tests::removals_execute_drops_a_checkout_row_in_one_save' panicked at bauded/src/manager.rs:842:9:
not yet implemented
thread 'api::tests::removals_unknown_target_is_404' panicked at bauded/src/manager.rs:842:9:
not yet implemented
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 107 filtered out
```

Following the Phase 20 convention, RED is the first failing run, not a separate commit. Tasks 2 and 3 are test-only tasks over the Task 1 implementation. The Task 2 survivor test was additionally mutation-checked: with the merge call removed from the save closure it fails with `the removal save regressed the survivor's archived flag`, and passes with the merge restored.

## Task Commits

1. **Task 1: one checkout row removed end to end over REST** - `7980fe1` (feat)
2. **Task 2: survivor metadata, persistence bookkeeping, fresh restore** - `7e06d19` (test)
3. **Task 3: concurrent removals of one row, and the crate gate** - `79e4f45` (test)

## What later plans need

### Production surface, `bauded/src/manager.rs`

```rust
pub enum RemoveRowsError {
    Drift(Box<RemovalReport>),
    Removal(RemovalError),
    Blocked(persist::SaveError),
    Busy(RepositoryKey),
}
pub fn preview_removal(&self, target: RemovalTarget) -> Result<RemovalReport, RemovalError>
pub fn remove_rows(&mut self, previewed: &RemovalReport) -> Result<RemovalReport, RemoveRowsError>
pub fn repository_path(&self, repository: RepositoryKey) -> Option<PathBuf>
fn merge_live_runtime_metadata(state: &mut RepositoryState, overrides: &[(CheckoutKey, RetainedSessionState)])
fn survivor_runtime_overrides(&self) -> Vec<(CheckoutKey, RetainedSessionState)>
fn removal_target_repository(&self, target: RemovalTarget) -> Option<RepositoryKey>
fn collect_row_runtimes(&mut self, report: &RemovalReport) -> Vec<RowRuntime>
fn restore_row_runtimes(&mut self, runtimes: Vec<RowRuntime>)
```

`merge_live_runtime_metadata` is a free function at module level in `manager.rs`, directly above `lock`. `state_for_save` is now two lines: clone, then call it with `survivor_runtime_overrides()`.

`collect_row_runtimes` handles `RowKey::Checkout` only. `RowKey::Repository` and `RowKey::Standalone` are skipped; 21-10 adds the standalone registry and must extend this function and the `runtime_checkouts` cleanup loop in `remove_rows`. `removal_target_repository` returns `None` for a standalone target, so no reservation is taken for one.

### Save closure shape

A `move` closure capturing the owned override snapshot, `self.persist`, and under `cfg(test)` clones of `persistence_target_for_test` and `atomic_failure_for_test`. It returns `Ok(())` without touching disk when persistence is off; otherwise it clones `file.state`, calls `merge_live_runtime_metadata` on the clone, wraps it in `StateFile::new`, and writes that through `persist::save_current_at_test` when the test target is set, else `persist::save_for_workspace_status(STATE_BASE, workspace::active(), ..)`. It never mutates `file`, because core assigns the candidate to the live state after the save.

### Unreserved preview

Preview holds no reservation because the caller holds the global manager mutex, `lock()` at `bauded/src/manager.rs:106` at planning time, which already serializes it against every mutation, and core's `remove` at `baude-core/src/removal.rs:461` re-selects its own rows instead of trusting a preview.

### HTTP status mapping, `removal_error` in `bauded/src/api.rs`

| Cause | Status | Body |
|---|---|---|
| success | 200 | `RemovalReport` JSON |
| `Drift` | 409 | fresh `RemovalReport` JSON, the one JSON error body |
| `Removal(UnknownTarget)` | 404 | plain text, the error `Display` |
| `Removal(Persist)` | 503 | plain text |
| `Blocked` | 503 | plain text, `persistence is blocked after a state load failure` |
| `Busy` | 503 | plain text, `repository busy: <path>` |
| `RuntimeSurvived`, `DiskRemoval`, `RuntimeHandleMissing`, `RuntimeIdentityMismatch`, `Validation` | 500 | plain text |

Preview maps `UnknownTarget` to 404 and anything else to 500, both plain text. A malformed body is axum's own `Json` rejection. `persistence` on a saved real run serializes as `{"outcome":"saved"}`, pinned by test.

### Test seams added

- `Manager::checkout_key_for_runtime_for_test(&self, id: u64) -> Option<CheckoutKey>`, `#[cfg(test)] pub(crate)`
- `Manager::restore_at` widened from private to `pub(crate)`, still `#[cfg(test)]` only

### Test fixtures, inline `mod tests` of `bauded/src/api.rs`

```rust
const REMOVAL_AGENT_CMD: &str = "sh -c \"sleep 30\"";

fn removal_setup(label: &str) -> (PathBuf, FixtureRepo, Workspace, Shared, Router)

async fn removal_fixture(label: &str)
    -> (PathBuf, FixtureRepo, Workspace, Shared, Router, u64, CheckoutKey)

struct TwoCheckoutFixture {
    root: PathBuf, repo: FixtureRepo, workspace: Workspace, shared: Shared, app: Router,
    main_id: u64, main_key: CheckoutKey,
    worktree_id: u64, worktree_key: CheckoutKey, worktree_path: PathBuf,
}
async fn removal_fixture_two_checkouts(label: &str) -> TwoCheckoutFixture

async fn admit_removal_session(shared: &Shared, app: &Router, repo: &Path, worktree: Option<&str>)
    -> (u64, CheckoutKey, serde_json::Value)
async fn preview_report(app: &Router, key: CheckoutKey) -> serde_json::Value
fn previewed_agent_pid(report: &serde_json::Value, key: CheckoutKey) -> u64
async fn execute_previewed(app: &Router, key: CheckoutKey) -> axum::response::Response
fn removal_state_file(root: &Path, workspace: &Workspace) -> PathBuf
fn removal_teardown(shared: &Shared, root: &Path)
fn pid_is_alive(pid: u64) -> bool
fn wait_for_pid_gone(pid: u64) -> bool
```

Both fixture helpers are `async fn` and must be awaited. `removal_fixture` carries `#[must_use]`; bind the `FixtureRepo` to a named local, since it owns the redirect that contains the fixture. The fixture root is `temp_dir()/bauded-removal-<label>-<pid>`, so each test needs a unique label. Every test ends with `removal_teardown`.

The two-checkout repository is `initialized_repo` plus a bare `origin.git` under the fixture root with `remote add origin` and `push -u origin main`. The main checkout is admitted by `POST /sessions {"repo": ..}`, the managed worktree by `POST /sessions {"repo": .., "worktree": "topic"}`. `worktree_path` is the `cwd` the created `SessionInfo` reports. The helper asserts the two keys differ, the worktree path exists and differs from the repository path, and both runtimes are alive under `ps`. It asserts nothing about `role` or branch.

### Fixture command and liveness probe

- Command: `sh -c "sleep 30"`. The daemon appends a permission flag to every spawn; a bare `sleep 30` rejects it and exits, while the shell wrapper takes it as `$0`.
- Probe: `ps -o pid= -p <pid>` exit status, polled for the after case with a 5 s deadline and a 25 ms sleep. The pid comes from the preview report's `runtimes_killed[0].agent_pid`.

### Concurrency observation, for 21-03

The concurrency test observed `[200, 404]`. The loser's body was `removal target Checkout(CheckoutKey(1)) is not in the saved state`. The loser finds no repository for the gone checkout, takes no reservation, and fails the fresh preview with `UnknownTarget`, so 21-03 inherits the 404 arm for a repeated execute. The test still accepts `[200, 409]`.

## Observed test counts

- `cargo test -p bauded api::tests::removals_ -- --test-threads=1`: 7 passed, 107 filtered out
- `cargo test -p bauded`: 114 passed, 0 failed, up from 107 before this plan
- `cargo fmt --all --check`: exit 0
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Boxed the drift payload**
- **Found during:** Task 1
- **Issue:** `Drift(RemovalReport)` as planned fails `clippy::result_large_err` under `-D warnings`; the variant is at least 240 bytes.
- **Fix:** `Drift(Box<RemovalReport>)`. `removal_error` dereferences it, so the 409 JSON body is unchanged.
- **Files modified:** bauded/src/manager.rs, bauded/src/api.rs
- **Commit:** 7980fe1

**2. [Rule 3 - Blocking] Two-checkout fixture needs a real remote**
- **Found during:** Task 2
- **Issue:** The plan built the two-checkout fixture on `initialized_repo` alone. `POST /sessions` with `worktree` answered 400 there: `resolve repository default: no verified local remote HEAD was found`. A `git init` only repository cannot host a managed worktree.
- **Fix:** The fixture adds a bare `origin.git` and runs `push -u origin main`, the shape the manager's own worktree fixtures use. No assertion was weakened and the pinned composition is unchanged.
- **Files modified:** bauded/src/api.rs
- **Commit:** 7e06d19

**3. [Signature] Fixture helpers are async**
- The plan wrote `fn removal_fixture` and `fn removal_fixture_two_checkouts`. Both are `async fn`, because they admit sessions through the router.

**4. [Addition] `Manager::repository_path`**
- Not named in the plan. The plan requires the busy body to be `repository busy: <path>`, and `RemoveRowsError::Busy` carries only a key, so the handler resolves the path through this accessor.

**5. [Addition] `removal_setup` and small test helpers**
- `removal_setup`, `execute_previewed`, `removal_state_file`, `removal_teardown` were added so both fixtures share one body of setup, as the plan asked.

No production change was needed in Task 3.

**6. [Bookkeeping] API-01 left pending**
- The plan frontmatter lists `requirements: [API-01]`, but API-01 also covers repository, worktree and standalone rows and is shared with 21-03, 21-06, 21-08, 21-09, 21-10 and 21-11. This plan delivers checkout rows only, so API-01 is not marked complete in REQUIREMENTS.md.

## TDD Gate Compliance

- RED: recorded run, `RED=101`, log carries `not yet implemented`. Not a separate commit, per the plan and the Phase 20 convention.
- GREEN: `7980fe1` (feat).
- REFACTOR: none needed.

## Issues Encountered

None beyond the two blocking fixes above.

## Known Stubs

None.

## Next Phase Readiness

- 21-02, 21-09 and 21-10 can consume `removal_fixture_two_checkouts` unchanged, remembering it is async and its repository has an `origin` remote.
- 21-03 owns the untested arms of `removal_error`: `Drift` 409, `Busy` 503, `Blocked` 503, `SavedDegraded`, and the 500 family. Only 200, 404 and the `Persist` 503 are exercised here.
- 21-10 must extend `collect_row_runtimes` and the cleanup loop for standalone rows.

## Self-Check: PASSED

- FOUND: bauded/src/manager.rs, bauded/src/api.rs
- FOUND: 7980fe1, 7e06d19, 79e4f45 as ancestors of HEAD
