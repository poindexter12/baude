# Phase 6: Safe Managed Worktree Lifecycle - Pattern Map

**Mapped:** 2026-08-30
**Files analyzed:** 11 new/modified files
**Analogs found:** 11 / 11 (the new lifecycle module has a composite local analog)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `baude-core/src/lifecycle.rs` (new) | service / model | event-driven + transform | `baude/src/app.rs:30-94,739-857` plus `baude-core/src/repository.rs:141-231` | composite role/data-flow match |
| `baude-core/src/git.rs` | service / utility | request-response + file-I/O + transform | same file, admission APIs at `8-357,421-463,770-943` | exact extension seam |
| `baude-core/src/repository.rs` | model / store | transform | same file, durable aggregate at `81-127,191-351` | exact extension seam |
| `baude-core/src/persist.rs` | service | file-I/O + transform | same file, commit-stage API at `89-130,310-384` | exact extension seam |
| `baude-core/src/lib.rs` | config / module registry | transform | same file | exact |
| `baude-core/src/backend/mod.rs` | provider | request-response + transform | same file, `SpawnPlan`/`Backend` at `46-105` | exact extension seam |
| `baude-core/src/backend/claude.rs` | provider | process-spawn plan transform | same file, `spawn_plan` at `33-56` | exact extension seam |
| `baude-core/src/backend/opencode.rs` | provider | process-spawn plan transform | same file, composition at `90-101,135-153` | exact extension seam |
| `baude-core/src/pty.rs` | service | streaming + process-I/O | same file, `Pty::spawn` at `31-64` | exact extension seam |
| `baude/src/app.rs` and `baude/src/ui.rs` | controller / store + component | event-driven + request-response | Phase 5 admission and existing worktree modal in the same files | exact extension seam |
| `bauded/src/manager.rs` | service / store | request-response + file-I/O + process-I/O | Phase 5 Manager transaction/reconciliation methods in the same file | exact extension seam |

`bauded/src/api.rs` should remain API-compatible in this phase. Its existing create/delete/restart handlers (`api.rs:146-173,251-269`) should continue delegating to Manager; modify it only if typed Manager outcomes require status mapping or tests to preserve that compatibility. Do not add Phase 8 hierarchy or worktree-removal endpoints.

Tests remain inline under `#[cfg(test)]`; there is no separate Rust test tree.

## Pattern Assignments

### `baude-core/src/lifecycle.rs` (new service/model, event-driven + transform)

**Composite analog:** pure dispatch and ordered orchestration in `baude/src/app.rs`, typed aggregate behavior in `baude-core/src/repository.rs`, and commit-stage handling in `bauded/src/manager.rs`.

**Imports/type style** (`baude-core/src/repository.rs:3-12`):

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RepositoryKey(u64);
```

Keep lifecycle requests, blockers, plans, reservations, and outcomes UI-free. Prefer inspectable enums/newtypes over booleans or formatted errors. The module should own the shared meaning and ordering of create/activate/close/reopen/remove; App and Manager own effects only.

**Pure decision pattern** (`baude/src/app.rs:30-55`):

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimaryDispatch {
    Focus(u64),
    Restart(u64),
    Spawn,
    Idle,
}

fn primary_dispatch(active_intent: bool, runtime: Option<(u64, bool)>) -> PrimaryDispatch {
    match runtime {
        Some((id, false)) => PrimaryDispatch::Focus(id),
        Some((id, true)) => PrimaryDispatch::Restart(id),
        None if active_intent => PrimaryDispatch::Spawn,
        None => PrimaryDispatch::Idle,
    }
}
```

Generalize this to lifecycle plans/outcomes keyed by `RepositoryKey` and `CheckoutKey`. Plans should encode a fixed sequence such as reconcile, validate, save intent, stop, Git mutate, verify, spawn/focus; adapters must not reorder steps.

**Reservation analogs:** `App.runtime_checkouts: HashMap<CheckoutKey, u64>` (`app.rs:343-345`) and the daemon's poison-recovering lock (`manager.rs:69-73`). Use explicit per-repository mutation reservations even though current owners are serialized. Same-checkout concurrent reopen should return/focus the reserved runtime; conflicting same-repository mutation should wait or return a typed `Busy` outcome. Release on every return path (an RAII guard is preferred).

**Ordered-effect test pattern** (`app.rs:2431-2461`):

```rust
commit_then_spawn(
    &mut (),
    |_| { events.borrow_mut().push("save"); Ok::<_, &'static str>(()) },
    |_| { events.borrow_mut().push("spawn"); Ok::<_, &'static str>(23) },
).unwrap();
assert_eq!(*events.borrow(), ["save", "spawn"]);
```

Add table-driven lifecycle vectors for all transitions and execute the same vectors through App and Manager adapters. Assert identical domain outcome, aggregate delta, planned Git effects, reservation behavior, and no-force invariant.

---

### `baude-core/src/git.rs` (service/utility, request-response + file-I/O)

**Analog:** Phase 5's typed byte-safe discovery and verified default-worktree creation in this file.

**Typed subprocess boundary** (`git.rs:29-48,96-115`):

```rust
pub enum RepositoryDiscoveryError {
    Canonicalize { path: PathBuf, source: std::io::Error },
    CommandStart { operation: &'static str, source: std::io::Error },
    GitCommand { operation: &'static str, status: Option<i32>, stderr: String },
    MalformedTopology(String),
    InvalidPathOutput(&'static str),
    SelectedWorktreeMissing(PathBuf),
}

let output = Command::new("git")
    .arg("-C").arg(repo).args(args).output()
    .map_err(|source| RepositoryDiscoveryError::CommandStart { operation, source })?;
if !output.status.success() {
    return Err(RepositoryDiscoveryError::GitCommand { /* status + stderr */ });
}
```

Copy argv-only execution, typed command-start/nonzero errors, status inspection, and diagnostic-only lossy stderr. Add an injectable/recording executor for lifecycle commands so malformed output, process failure, and races can be deterministic in tests.

**Inventory parser/fail-closed pattern** (`git.rs:193-272`): parse NUL-delimited machine output, require explicit terminators and known fields, and reject malformed/unknown records. Extend `WorktreeRecord` rather than creating a second weaker inventory parser if lock/prunable reasons or HEAD OIDs are needed.

**Rediscover/postcondition pattern** (`git.rs:916-942`):

```rust
let fresh = discover_repository(managed_path).map_err(EnsureDefaultWorktreeError::Discovery)?;
if fresh.common_dir != snapshot.common_dir {
    return Err(EnsureDefaultWorktreeError::Verification(
        "repository common directory changed".into(),
    ));
}
if fresh.selected_worktree.path != canonical_managed { /* typed failure */ }
if fresh.selected_worktree.branch.as_deref() != Some(default.local_ref.as_str()) { /* failure */ }
```

Apply this after every add/remove. Creation must validate the literal with Git, classify exact `refs/heads/<name>` as new/existing-local/remote-only, reuse an occupied same-repository inventory record, and create a new branch from the freshly verified default ref/OID. Existing local activation must not reset it. Candidate paths combine durable repository/checkout identity with a bounded branch slug and are rejected if either `symlink_metadata` or fresh inventory reports ownership.

**Command form:** retain exact argv and end-of-options conventions from `verify_commit` (`git.rs:589-605`) and `clone_repo` (`1097-1112`). Never use `--force`, `-B`, inferred remote checkout, branch deletion, prune/repair/clean/reset/stash, a shell string, or recursive deletion.

**Unsafe seams to replace, not copy** (`git.rs:999-1023,1115-1124`):

```rust
if dir.exists() { return Ok(dir); }                 // unsafe reuse
let new_branch = git(repo, &["worktree", "add", &dir_str, "-b", branch]);
// ... speculative retry ...

pub fn is_dirty(worktree: &Path) -> bool {
    git(worktree, &["status", "--porcelain"])
        .map(|s| !s.is_empty())
        .unwrap_or(false)                            // error becomes clean
}
```

Replace with `Result<RemovalSafety, InspectionError>`. Parse `status --porcelain=v2 -z --untracked-files=all --ignore-submodules=none --ignored=matching`; classify tracked, untracked, ignored, conflict, submodule, malformed, and unknown records. Run recursive submodule status; any recorded submodule blocks non-force removal. Plain `git worktree remove -- <path>` is the only deletion authority.

**Tests:** extend the existing unique-temp `GitFixture` (`git.rs:1138-1285`). Preserve its configured identity, argv helper, and `Drop` cleanup. Add nested lifecycle modules for branch classification/activation, path collision, removal preflight, remove postconditions, malformed output, injected failures, and forbidden argv. Include the complete real-Git status/submodule/topology matrix from `06-RESEARCH.md:487-509`.

---

### `baude-core/src/repository.rs` and `baude-core/src/persist.rs` (durable model + file-I/O)

**Analog:** current strict aggregate and atomic replacement contracts.

**Retained child shape** (`repository.rs:81-117`):

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedSessionState {
    pub name: String,
    pub cwd: PersistedPath,
    pub repo_root: PersistedPath,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub shell_open: bool,
    pub archived: bool,
    pub archived_by_user: bool,
}

pub struct SavedCheckout {
    pub key: CheckoutKey,
    pub repository_key: RepositoryKey,
    pub role: CheckoutRole,
    pub managed_by_baude: bool,
    pub observed_path: PersistedPath,
    pub observed_branch: Option<String>,
    pub first_seen_order: u64,
    pub active_intent: bool,
    pub session: RetainedSessionState,
    pub health: CheckoutHealth,
}
```

Add an optional opaque resume/session ID to retained session state with an explicit serde compatibility default. Closing snapshots it with name, branch, shell/archive settings and order unchanged, then flips only `active_intent=false`. Successful worktree removal deletes only the exact child/runtime association; never remove the repository parent or local branch. External occupied worktrees stay `managed_by_baude=false`.

Update validation to protect any new invariant without making path/name runtime identity. Reuse monotonic allocation and no-mutation-on-failure (`repository.rs:191-229,544-553`). Add schema round-trip, missing-field compatibility, opaque-ID preservation, retained-close, child-only removal, and parent-preservation tests. Update all fixture struct literals in repository, persistence, App, and Manager tests.

**Commit boundary** (`persist.rs:97-117,343-384`):

```rust
pub struct SaveError {
    replacement_committed: bool,
    source: anyhow::Error,
}

pub fn replacement_committed(&self) -> bool {
    self.replacement_committed
}

std::fs::rename(&temporary, &destination)?;
replacement_committed = true;
// directory sync follows
```

Before replacement failure permits restoring the in-memory snapshot and keeping the runtime. After replacement failure means memory/effects must follow the replacement while reporting dirty durability. After Git removal, a final save failure cannot be represented as full rollback: retain unavailable recovery context when replacement did not commit, or follow child deletion when it did. Never recreate a removed worktree automatically.

---

### `baude-core/src/lib.rs`

**Analog:** flat alphabetical module exports (`lib.rs:8-18`). Add `pub mod lifecycle;` between `hook` and `meta`; retain the crate's UI-free boundary.

---

### `baude-core/src/backend/{mod,claude,opencode}.rs` and `baude-core/src/pty.rs`

**Analog:** existing shared spawn-plan provider and exact command-plan tests.

Replace the boolean resume argument (`backend/mod.rs:74-81`) with a typed mode such as `Fresh | ContinueLatest | ResumeId(String)`. Keep the ID opaque.

**Existing provider seam** (`backend/mod.rs:46-54,74-81`):

```rust
pub struct SpawnPlan {
    pub cmd: String,
    pub server_port: Option<u16>,
}

fn spawn_plan(&self, resolved_cmd: &str, event_url: Option<&str>, resume: bool) -> SpawnPlan;
```

Do not interpolate a persisted resume ID into `cmd`: `Pty::spawn` executes a shell command (`pty.rs:47-59`). Extend `SpawnPlan`/PTY spawning to carry a fixed environment variable or direct argv safely, following the existing `CommandBuilder::env` pattern:

```rust
let mut cmd = CommandBuilder::new(&shell);
cmd.args(["-il", "-c", c]);
cmd.cwd(cwd);
cmd.env("TERM", "xterm-256color");
cmd.env("COLORTERM", "truecolor");
```

Claude maps targeted resume to `--resume` with the opaque value; OpenCode maps it to `--session`. `ContinueLatest` retains current `--continue` behavior and is used only when no ID was ever observed. Preserve Claude's exported event URL/fallback semantics (`claude.rs:33-55`) and OpenCode's pinned port/prompt config (`opencode.rs:90-101,135-153`).

Extend existing exact-string tests (`claude.rs:124-162`, `opencode.rs:301-329`) with fresh/latest/targeted modes, both backends, hostile-looking opaque IDs, and assertions that the ID is carried as data rather than shell syntax.

---

### `baude/src/app.rs` and `baude/src/ui.rs` (local adapter and confirmation component)

**Analog:** Phase 5's App aggregate/runtime adapter.

**Core/App ownership pattern** (`app.rs:310-355`): App owns sessions, `RepositoryState`, checkout-to-runtime map, persistence status, focus/modal state, and test effect seams. Add repository reservations and stop/save-stage/spawn hooks here only as adapter/test machinery; shared decisions belong in core.

**Reconcile before dispatch** (`app.rs:739-797`):

```rust
if !self.reconcile_primary(checkout_key) {
    self.save_durable()?;
    return Ok(None);
}
let runtime = self.runtime_checkouts.get(&checkout_key).and_then(/* live/exited */);
match primary_dispatch(checkout.active_intent, runtime) {
    PrimaryDispatch::Focus(id) => { /* select/focus */ }
    PrimaryDispatch::Restart(id) => { /* targeted resume */ }
    PrimaryDispatch::Spawn => {
        let id = commit_then_spawn(self, |app| app.save_durable(), |app| app.add_session(/*...*/))?;
        self.runtime_checkouts.insert(checkout_key, id);
    }
    PrimaryDispatch::Idle => { /* no process */ }
}
```

Generalize `ensure_primary`/`reconcile_primary` to all retained checkouts and use core lifecycle plans. Runtime ownership remains by `CheckoutKey`, never cwd/name. Reopen: reserve, fresh reconcile, persist active intent, then focus/restart/resume/spawn exactly one runtime. Close: snapshot live metadata including `meta.session_id`, persist inactive intent, then stop/remove runtime; on precommit save failure leave the live runtime and state untouched.

**Current unsafe removal to replace** (`app.rs:1622-1647`): it calls `remove_session` before `is_dirty`, losing the running agent before inspection. New flow is preflight #1 while live, explicit confirmation, stop immediately before preflight #2, plain Git remove, verify, child-only save. If #2/Git fails, compensate by focus/resume/spawn exactly one runtime with active intent/context retained. A blocked first preflight must be byte-for-byte non-mutating.

Keep modal presentation local. The existing component (`ui.rs:1046-1064`) is the analog, but separate close/keep from destructive removal clearly and render typed blockers/actionable degraded results. Do not add Phase 7 hierarchy rendering. Do not route local handlers directly to `git::create_worktree`, `git::is_dirty`, or `git::remove_worktree`.

**Tests:** extend `repository_admission_tests` (`app.rs:2265-2535`) and its injected persistence/spawn seams. Add close snapshot/order, pre/post-commit saves, absent/live/exited reopen, duplicate reopen, moved/branch-changed block, first/second preflight, stop failure, Git refusal compensation, post-remove degraded persistence, and modal no-mutation cancellation. Use harmless commands only.

---

### `bauded/src/manager.rs` (daemon adapter, request-response + file/process-I/O)

**Analog:** current reconciliation, transaction, and process owner.

**Owner and error boundary** (`manager.rs:38-67,75-99`): retain typed `MutationError`, `RepositoryState`, `runtime_checkouts`, persistence dirty/blocked reporting, and `Arc<Mutex<Manager>>` ownership. Lifecycle domain outcomes should remain inspectable through this boundary rather than being flattened to strings too early.

**Reconciliation pattern** (`manager.rs:399-444`): fresh `git::reconcile_checkout`, then update checkout/repository health and refuse dispatch when facts changed. Generalize this from restart/restore to create/activate/reopen/remove.

**Commit-aware rollback pattern** (`manager.rs:593-607,873-912`):

```rust
if let Err(error) = self.save_checked() {
    if !error.replacement_committed() {
        self.repository_state = state_before;
    }
    return Err(MutationError::Persistence(error));
}
let id = self.spawn(/* ... */)?;
self.runtime_checkouts.insert(checkout_key, id);
```

Reuse this distinction, but replace current `remove`: DELETE/close must retain checkout and parent with inactive intent; safe physical worktree removal is a separate lifecycle action and must never prune an unused parent. Manager create must stop calling legacy `git::create_worktree` (`manager.rs:559-565`) and consume the same core classification/allocation/transition as App. Restart/reopen must use the retained targeted resume ID instead of boolean latest-only resume.

**Tests:** extend Manager's inline module and persistence fixtures (`manager.rs:1453-1748`). Keep unique temp roots, `sleep`/`true` stubs, explicit cleanup, and `AtomicFailure::{Rename,DirectorySync}` matrices. Add the same adapter contract vectors as App, reservation/race tests, close retention, managed/external removal authorization, double-preflight compensation, parent/branch preservation, and no duplicate runtime.

## Shared Core → App/Manager Data Flow

```text
surface request (App modal/key or existing Manager method)
  -> resolve durable RepositoryKey / CheckoutKey
  -> core repository reservation
  -> fresh Git discovery/reconciliation from repository main/retained path
  -> core typed decision/plan
  -> adapter executes fixed effects
       CREATE/ACTIVATE: classify exact ref -> inventory/path checks -> Git add/reuse
                        -> postconditions -> save active child -> focus/resume/spawn
       CLOSE:           snapshot runtime/resume ID -> save inactive intent -> stop runtime
       REOPEN:          reconcile -> save active intent -> focus/restart/resume/spawn one
       REMOVE:          preflight #1 -> confirmation -> stop -> preflight #2
                        -> plain Git remove -> postconditions -> delete child/save
                        -> compensate runtime on any pre-remove failure
  -> typed LifecycleOutcome + adapter-specific message/runtime ID
  -> release reservation on every path
```

Git facts, durable intent, and runtime effects remain separate authorities. Core decides meaning/order; `git.rs` proves topology/status/ref facts; `RepositoryState` stores intent/context; App/Manager perform PTY/focus/persistence effects.

## Shared Safety Patterns

### Fail Closed on Unknown State

Every Git start/nonzero/decode/parse/topology/status/submodule error blocks mutation. Never use `.ok()`, `.unwrap_or(false)`, default-clean behavior, stale preflight results, or path existence alone to authorize add/remove.

### Persist Before Runtime Effect

Reopen/create persist active intent before spawn. Close persists inactive intent before stop. Removal leaves old active intent/context as recovery state until Git removal and postconditions succeed. Use `SaveError::replacement_committed()` to decide rollback; do not claim Git + JSON are one transaction.

### Preserve User Work and Ownership

- Only exact `managed_by_baude=true`, linked, non-main, available checkout records may be physically removed.
- External occupied worktrees can be registered/focused but remain unmanaged.
- Tracked, staged, unstaged, deleted, untracked, ignored, conflicted, submodule, locked, prunable, malformed, and indeterminate states block removal.
- Preserve local branch, repository parent, sibling checkouts, retained metadata, and first-seen order.
- No force, recursive deletion, branch deletion, automatic prune/repair/unlock/stash/reset/clean, or remote-only branch activation.

### Shell/Argument Safety

Git always uses `Command` argv and exact refs/paths with an option terminator where supported. Resume IDs are opaque process data passed through environment/direct argv, never shell-interpolated. Branch slugs are display/path components only; Git validates branch grammar and durable keys provide uniqueness.

### Race/TOCTOU Safety

Serialize create/activate/reopen/remove per repository, rediscover immediately before mutation, repeat removal preflight after stopping the agent, and let plain non-force Git removal be the final safeguard. Never cache the first preflight across confirmation.

## Test Pattern Map

| Concern | Existing test analog | Phase 6 additions |
|---|---|---|
| Pure lifecycle decisions/order | `app.rs:2417-2461` | table vectors, reservations, idempotence, identical App/Manager outcomes |
| Real Git facts/mutations | `git.rs:1138-1645` | ref matrix, occupied branch reuse, collision matrix, status/submodule/topology blockers, postconditions |
| Aggregate/schema | `repository.rs:354-555`; persistence inline tests | resume-ID compatibility, close retention, child-only delete, parent/branch preservation |
| Commit-stage rollback | `manager.rs:1676-1748`; `persist.rs:265-384` | close/reopen/remove at pre/post replacement and post-Git boundaries |
| Backend plans | `claude.rs:124-235`; `opencode.rs:301-329` | fresh/latest/targeted ID for both backends; hostile ID cannot become shell syntax |
| Local adapter | `app.rs:2265-2535` | stop/save/spawn hooks, double preflight, compensation, modal cancellation |
| Daemon adapter | `manager.rs:1453-1878` | same contract vectors, repeated/concurrent reopen, compatibility create/delete/restart |

Focused commands from research:

```text
cargo test -p baude-core git::tests::lifecycle -- --nocapture
cargo test -p baude-core lifecycle::tests -- --nocapture
cargo test lifecycle_close
cargo test lifecycle_reopen
cargo test lifecycle_remove_clean
```

Phase gate: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.

## Patterns That Must Not Be Copied Unchanged

| Source | Unsafe/obsolete behavior | Replacement |
|---|---|---|
| `git.rs:999-1023` | directory reuse + speculative `-b` fallback from caller checkout | exact ref class, default-base OID, inventory/disk checks, explicit add, postconditions |
| `git.rs:1115-1119` | Git error becomes clean | result-valued fail-closed preflight |
| `app.rs:1622-1647` | kill/forget before cleanliness check | live preflight, confirm, stop, second preflight, compensate |
| `app.rs:1723-1748` | direct Git create then direct spawn | shared lifecycle plan and save-before-spawn |
| `manager.rs:559-565` | daemon direct legacy worktree helper | same core create/activate semantics as App |
| `manager.rs:873-912` | deletes checkout and now-unused repository on session close | retained inactive child; parent always preserved |
| backend boolean `resume` | latest-directory conversation only | retained opaque targeted ID, latest fallback only when absent |

## No Analog Found

No single existing file implements the full lifecycle transaction or double-preflight compensation. `baude-core/src/lifecycle.rs` must compose the established typed aggregate, Git verification, commit-stage persistence, and checkout-key runtime patterns above; use `06-RESEARCH.md` for the new state-machine details rather than inventing an App-only or Manager-only protocol.

## Metadata

**Analog search scope:** `baude-core/src`, `baude/src`, `bauded/src`, Phase 5 summaries/pattern map, and existing inline tests  
**Primary analogs:** `baude-core/src/{git,repository,persist,backend,pty,lib}.rs`, `baude/src/{app,ui}.rs`, `bauded/src/{manager,api}.rs`  
**Pattern extraction date:** 2026-08-30
