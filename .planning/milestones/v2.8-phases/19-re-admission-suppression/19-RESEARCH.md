# Phase 19: Re-admission Suppression - Research

**Researched:** 2026-11-03
**Domain:** Rust / baude-core lifecycle and persistence; TUI admission seam in `baude/src/app.rs`
**Confidence:** HIGH (codebase-grounded; every load-bearing claim read from source this session)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Folder-workspace binding (READM-01, READM-05)
- **D-01:** Removal deletes `folder-workspaces.json` entries whose key equals a removed row's canonical path: the repository's `observed_main_worktree`, every removed checkout's `observed_path`, and a standalone row's `canonical_path`. Keys are matched exactly with `breadcrumbs::folder_key`; nothing is matched by workspace value, so other folders bound to the same workspace keep their entries. The write goes through the existing `locked_merge_write` read-merge-write path so a parallel TUI never loses its own entry.
- **D-02:** Launching baude inside the folder afterwards re-records the binding through the unchanged `launch::start_workspace` step 5. No special case: the fresh binding is the normal one.
- **D-03:** No binding is written for a non-git standalone folder today, because `plan_launch` only derives `repo_root` for git trees. Clearing is a no-op there and the test asserts the file is byte-identical after a standalone removal.

#### Marker ownership (READM-02, READM-05)
- **D-04:** Marker ownership is cleared exactly when a repository row leaves the state. Removing a single child while the repository row remains never touches the parent's marker, because the surviving rows still own the directory.
- **D-05:** The directory is `git::worktrees_base()/<workspace>/repository-<physical_key>` using `SavedRepository::physical_key` from the removed row, never a recomputed digest, so a suffixed collision directory such as `repository-<digest>-2` is the one cleared. Legacy rows whose `physical_key` is a bare counter are resolved the same way, with the directory name taken from the field.
- **D-06:** The marker is deleted only when `marker::read_marker` returns `Valid` and its `canonical_common_dir` equals the removed repository's `observed_common_dir`. A `Missing` marker is a no-op, and a foreign, `Invalid`, or symlink marker is left untouched and reported, mirroring `write_marker`'s refusal to overwrite what it does not own. — **Reversibility:** costly — a marker deleted for the wrong owner makes the next admission of that other repository allocate a needless suffixed directory and orphans its worktrees, and the Phase 8 worktree scan refuses to clean that up by design.
- **D-07:** After the marker is gone the repository directory is removed with a non-recursive `remove_dir`. Phase 18's disk stage has already deleted the managed worktrees, so an empty directory is the expected case. A non-empty directory is left in place and named in the report; nothing recursive runs here, and nothing outside the managed root is touched. The Phase 18 `.<name>.baude-removal` sidecar is a different artefact; it is already removed on a successful forced removal and this phase does not add handling for it.
- **D-08:** Nothing is deleted inside a kept main checkout, unmanaged checkout or standalone folder. The only artefacts this phase clears live under baude's config dir and its managed worktree root.

#### Inventory sweep suppression (READM-03, READM-04)
- **D-09:** `App::admit_repository` calls `admit_existing_worktrees` on every admission of an already-known repository, including the launch-dir admission in `App::restore`, so a removed unmanaged checkout whose directory survives is re-admitted from `git worktree list` on the next relaunch inside the parent. Bindings and markers do not cover this. Phase 19 therefore adds a per-workspace suppression store: an advisory JSON file beside the breadcrumbs file, keyed by canonical path, with the same posture as `folder_workspace` and `breadcrumbs` (additive schema with `schema_version`, `locked_merge_write`, corrupt file degrades to empty and never fails a launch). The schema-locked `state-<workspace>.json` is not changed. — **Reversibility:** costly — the file is a new on-disk format under the config dir that later versions must keep reading; removing the mechanism means a migration or a documented one-time cleanup.
- **D-10:** A suppression entry is written only for a checkout row removed while its repository row remains in state. Repository-parent removal and standalone removal write no entries: nothing sweeps a repository or a standalone back, and re-admission inside the folder is then an ordinary first admission that shows the full topology, which is what "fresh row" means. When a repository row is later removed, the entries of its children are dropped as garbage.
- **D-11:** `admit_existing_worktrees` consults the store and skips a suppressed path. The consult point is one helper in `baude-core` so the daemon's admission (Phase 21) reads the same file through the same code; wiring `bauded` is Phase 21's job.
- **D-12:** An entry is lifted by an explicit admission of that exact path: `n` with the checkout's path, `c` cloning into it, or launching baude with a launch dir whose `git::repo_root` is that path. Lifting happens before the sweep runs so the fresh row is admitted in that same pass. `n` with the parent repository's root does not lift a child's entry; that is the sweep the entry exists to suppress.
- **D-13:** A fresh row carries no identity from the removed one: new `RepositoryKey`, `CheckoutKey` or `StandaloneKey` and `first_seen_order` from the monotonic counters, `resume_id: None`, no `owned_runtime`, lifecycle starting state, origin re-resolved. The deterministic `physical_key` digest of the common dir is repository identity, not row identity, and may legitimately coincide. Breadcrumb crumbs are path-keyed advisory selection memory; `Breadcrumbs::flush` already drops crumbs whose path is no longer in durable state, and the TUI flushes after removal, so no eager crumb clearing is added. A test proves the removed path's crumb is gone after the flush.

#### Stage order and failure handling
- **D-14:** Clearing runs as a fourth stage inside `removal::remove`, after `persist_stage` has committed the save. The state file is the source of truth, so an advisory-file or marker I/O failure must not turn a completed removal into an error. The stage is best-effort: outcomes land in `RemovalReport` as a list of cleared items and a list of clearing failures with path and error, and `preview_removal` names the bindings, markers and suppression entries the real run will clear so Phase 20's confirmation can show them. The crash window between the save and the clearing stage is accepted on the same terms as Phase 18's disk-to-save gap; a binding or marker left behind is residue, not a re-admission, and a missing suppression entry is repaired by removing the row again.
- **D-15:** Everything the stage touches is scoped to the removed set. The test for READM-05 builds two repositories bound to the same workspace plus a third repository with a managed worktree, removes one, and asserts the other bindings, markers, worktree directories and suppression entries are byte-identical.

#### Relaunch verification (READM-03)
- **D-16:** "Across a relaunch" is tested by building a second `App` over the same redirected config dir and state file after removal and calling `restore()`, then asserting the removed repository, checkout, worktree and standalone rows are absent and that `reconcile_teardown_recoveries`, `reconcile_activation_recoveries`, legacy reconciliation and `startup_recovery_program` produced nothing for them. Fixtures follow the repo rule: `testing::cloned_topology` and `git init` shapes, assertions on fixture output and state files, never on source text.

### Claude's Discretion
- Names of the suppression file, its structs and the `RemovalReport` fields for cleared items and clearing failures.
- Whether the clearing stage is one function or three (binding, marker, suppression), provided the stage order and the report shape above hold.
- Whether a suppression entry is pruned when its path no longer exists on disk, or kept like `folder-workspaces.json` entries are.
- Exact placement of the lift-on-explicit-admission call inside `admit_repository` and `open_new_session_modal`'s admission path, as long as it runs before `admit_existing_worktrees`.

### Deferred Ideas (OUT OF SCOPE)
- Wiring the daemon's `Manager::restore` and admission to the suppression consult helper: Phase 21, with the REST and PWA removal.
- Showing cleared bindings, markers and suppression entries in the TUI confirmation: Phase 20 consumes the report fields added here.
- An eager breadcrumb clear on removal, if the daemon path in Phase 21 turns out not to flush breadcrumbs before a TUI relaunch.
- Pruning stale `folder-workspaces.json` entries in general: outside this milestone; entries for detached volumes are legitimate history.
- `scripts/gate.sh --linux` container target dir off the bind mount: logged in Phase 18 deferred-items, still open.
- Unpackaged spike manifest at `.planning/spikes/MANIFEST.md` concerns the opencode backend and is unrelated to this phase; run `/gsd-spike --wrap-up` when that work resumes.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| READM-01 | Removing a row clears its folder-workspace binding so the launch-dir binding walk no longer resolves the removed folder to a workspace | `folder_workspace::record` is a `locked_merge_write` with `folders.insert(key, entry)`; the clear is the same call with `folders.remove(&key)`. `find_binding` walks parent dirs with `folder_key`, so only an exact-key removal is needed. See Pattern 1 and Code Example 1. |
| READM-02 | Removing a row clears baude's marker ownership for its paths so marker recovery does not restore the row | Marker is `.baude-marker.json` inside `worktrees_base()/<workspace>/repository-<physical_key>`; `read_marker` returns `Missing/Valid/Invalid`; `discover_owner` and `check_collision`/`discover_checkout_owner` are the readers. See Pattern 2 and Code Example 2. |
| READM-03 | After removal, the session-start sweep, worktree scan, marker recovery and reconcile do not re-admit the removed row, verified across a relaunch of the TUI | The only path that re-admits a surviving-directory checkout is `App::admit_existing_worktrees` (app.rs:2476), reached from `admit_repository` at 2393 and 2464. The suppression store (Pattern 3) plus the `App` restart fixture pattern already used at app.rs:9303-9352 give the relaunch test. |
| READM-04 | A removed project can be admitted again only by `n`, `c`, or launching inside the folder, with a fresh row and no stale identity | All three routes funnel through `open_repo_session_via(path, LocalAdmissionRoute::{Open,CloneCompletion,LaunchDirectory})` (app.rs:5479), which calls `admit_repository` or `admit_standalone`. The lift hook sits at the top of `admit_repository`/`admit_standalone` so it precedes both `admit_existing_worktrees` calls. Fresh keys come from `allocate_checkout_key`/`allocate_first_seen_order`. |
| READM-05 | Removal does not alter other rows' bindings, markers or worktrees, including other repositories that share a workspace | Exact-key clearing (D-01), per-row marker directory from `physical_key` (D-05), owner-compared marker deletion (D-06), non-recursive `remove_dir` (D-07). Byte-identical assertions on `folder-workspaces.json`, neighbour markers and the suppression file. |
</phase_requirements>

## Summary

Phase 18 left `removal::remove` as three stages (`kill_stage`, `disk_stage`, `persist_stage`) that end with the state file saved and `*state` replaced by the candidate. Phase 19 appends a fourth, best-effort clearing stage that runs after the save and touches exactly three artefacts, all of which live under baude's config dir or managed worktree root: the `folder-workspaces.json` entry keyed by each removed row's canonical path, the `.baude-marker.json` plus its now-empty `repository-<physical_key>` directory for each removed repository row, and a new per-workspace suppression store entry for each checkout removed while its parent repository stays. Every helper the stage needs already exists in `baude-core` and is reachable from a new core module: `breadcrumbs::folder_key`, `breadcrumbs::load_json`, `breadcrumbs::locked_merge_write` (both `pub(crate)`), `marker::read_marker`, `git::worktrees_base()` (`pub(crate)`), `workspace::active().state_file(base)` for per-workspace file naming, and `RepositoryState::remove_rows` which already returns the full `RemovedRows { repositories, checkouts, standalone_sessions }`.

The one path that can resurrect a removed row without any of those artefacts is `App::admit_existing_worktrees` (app.rs:2476), which walks `git worktree list` for an already-known repository and admits every unowned, branch-bearing worktree. That walk runs on the launch-dir admission inside `App::restore` and on `n` with the parent root, so a removed unmanaged checkout whose directory survives would be back on the next relaunch. The suppression store is consulted there and lifted at the top of `admit_repository`/`admit_standalone` for the exact path being admitted, so an explicit `n`, `c`, or launch inside the removed folder produces a fresh row in the same pass.

Two facts the planner must build around: the TUI has no call to `removal::remove` yet (Phase 20 adds it, `grep removal:: baude/src/app.rs` returns nothing), so the breadcrumb-flush proof in D-13 must drive `FolderContext::flush` directly rather than through a TUI remove key; and `App::restore` under `cfg(test)` loads the folder context with `root = None` (app.rs:1424-1427), so breadcrumb assertions belong in a `baude-core` test over `FolderContext`, not in an `App` relaunch test. The relaunch test itself follows the existing two-`App` pattern with `persistence_root_for_test` (app.rs:9303-9352) under one `TestRedirect`, which redirects the config dir to `<root>/config`, so the suppression file and `folder-workspaces.json` land where a second `App` reads them.

**Primary recommendation:** Add `baude-core/src/suppression.rs` (store + consult/lift helpers built on `breadcrumbs::{load_json, locked_merge_write, folder_key}`), add a `clear_stage(&set, &removed_rows) -> ClearingOutcome` to `removal.rs` called after `persist_stage`, extend `RemovalReport` with `cleared: Vec<ClearedItem>` and `clearing_failures: Vec<ClearingFailure>` filled by both `remove` and `preview_removal`, and wire `admit_existing_worktrees` (consult) and `admit_repository`/`admit_standalone` (lift) in `baude/src/app.rs`. TDD with `cargo test -p baude-core removal::tests -- --test-threads=1` and `cargo test -p baude app::tests::<name> -- --test-threads=1`, gate with `scripts/gate.sh`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Clear folder-workspace binding on removal | `baude-core` (`removal.rs` stage, `folder_workspace.rs` helper) | — | The file is a core-owned advisory store; TUI and daemon both call `removal::remove`. |
| Clear marker ownership and empty repository dir | `baude-core` (`removal.rs` stage) | — | Marker and managed root are core-owned; the readers (`lifecycle`, `worktree_scan`) are core. |
| Suppression store (write, consult, lift, GC) | `baude-core` (new `suppression.rs`) | `baude/src/app.rs` (call sites) | D-11: one helper so Phase 21's daemon reads the same file through the same code. |
| Skip suppressed worktrees in the inventory sweep | `baude` TUI (`App::admit_existing_worktrees`) | `bauded` (Phase 21, out of scope) | The sweep lives in the TUI admission seam today; `bauded/src/manager.rs` has its own `restore` at line 496 to be wired later. |
| Lift suppression on explicit admission | `baude` TUI (`App::admit_repository`, `App::admit_standalone`) | — | All three admission routes converge on `open_repo_session_via` which calls these two. |
| Report cleared items / failures | `baude-core` (`RemovalReport`) | `baude` (Phase 20 display), `bauded` (Phase 21 REST) | D-14: preview and real run both populate the report. |
| Relaunch verification | `baude` tests (`app::tests`) + `baude-core` tests | — | D-16 names `App::restore`; core tests cover stage behaviour and store semantics. |

## Standard Stack

### Core

No new dependencies. Everything this phase needs is already in `baude-core/Cargo.toml` and in use by the modules being extended. [VERIFIED: baude-core/src/folder_workspace.rs:1-7, baude-core/src/repository.rs:1123-1129 use `serde`, `serde_json`, `sha2` already]

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde` / `serde_json` | workspace-pinned (already a dep) | Suppression store file and `RemovalReport` serde round-trip | Same derive set as `FolderWorkspaceFile` and `RemovalReport` today |
| `std::fs` | — | `remove_dir`, `symlink_metadata`, `remove_file` | D-07 wants non-recursive `remove_dir`; `read_marker` already uses `symlink_metadata` |

### Supporting (in-repo helpers, not crates)

| Helper | Location | Purpose | When to Use |
|--------|----------|---------|-------------|
| `breadcrumbs::folder_key(&Path) -> String` | `baude-core/src/breadcrumbs.rs:116-121` | Canonical string key for advisory stores | Every key written or removed in `folder-workspaces.json` and the suppression file |
| `breadcrumbs::load_json<T>(&Path) -> (T, bool)` | `baude-core/src/breadcrumbs.rs:229-238` (`pub(crate)`) | Corrupt-tolerant load, `(default, corrupted)` | Consult path for the suppression store |
| `breadcrumbs::locked_merge_write<T,F>(root, file, update) -> io::Result<()>` | `baude-core/src/breadcrumbs.rs:248-283` (`pub(crate)`) | Advisory-locked read-merge-write, temp + rename + fsync | Every write to `folder-workspaces.json` and the suppression file |
| `marker::read_marker(&Path) -> Result<MarkerRead, io::Error>` | `baude-core/src/marker.rs:185` | Fail-closed marker read | D-06 owner check before deleting the marker |
| `git::worktrees_base() -> PathBuf` | `baude-core/src/git.rs:1789` (`pub(crate)`) | Redirect-aware managed root | D-05 directory resolution |
| `workspace::active()` / `Workspace::state_file(base)` | `baude-core/src/workspace.rs`, used at `breadcrumbs.rs:130` | Per-workspace file naming | Name the suppression file the way breadcrumbs name theirs |
| `persist::config_dir() -> PathBuf` | `baude-core/src/persist.rs:895` → `config_base` 882-891 honours `testing::config_dir_override()` | Root for both advisory files | Stage and consult paths; tests redirect it via `TestRedirect` |
| `testing::TestRedirect::new(root)` | `baude-core/src/testing.rs:128-138` | Redirects worktrees base to `<root>/data`, config dir to `<root>/config`, claude dir to `<root>/claude`, home to `<root>/home` | Every test in this phase |
| `testing::cloned_topology(root) -> ClonedTopology` | `baude-core/src/testing.rs:353` | Cloned fixture: `upstream`, `clone`, `default_worktree`, `managed_worktree`, `common_dir` | Cloned shape per the repo's fixture rule |
| `testing::git_in(dir, args) -> String` | `baude-core/src/testing.rs:311` | Git shell-out in fixtures | `git init` shape fixtures |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| New per-workspace suppression file | A field on `SavedRepository`/state file | Rejected by D-09: state file is schema-locked with `deny_unknown_fields`; not reopened. |
| `locked_merge_write` | Direct `fs::write` | Loses the lock and atomic rename; a parallel TUI could drop its own entry. D-01 locks the merge path. |
| `remove_dir` | `remove_dir_all` | D-07 forbids recursion; a non-empty directory is a report item, not a deletion. |

**Installation:** none.

## Package Legitimacy Audit

Not applicable: this phase installs no external packages. `cargo search` and `package-legitimacy check` were therefore not run. All crates referenced above are already declared in the workspace and compiled by the existing gate. [VERIFIED: no `Cargo.toml` change is required by any decision in CONTEXT.md]

## Architecture Patterns

### System Architecture Diagram

```
                      removal::remove(state, target, runtimes, save)
                                         |
          select_rows ─► preflight ─► candidate.remove_rows ─► validate_for_save
                                         |
            kill_stage ─► disk_stage ─► persist_stage  (save committed, *state = candidate)
                                         |
                      ┌──────────────────┴──────────────────┐
                      │   NEW clear_stage (best-effort)     │
                      │                                     │
   RemovedRows ──────►│ 1. bindings: for each removed path  │──► folder-workspaces.json
   (repos, checkouts, │    folder_key(p) → folders.remove   │    (locked_merge_write)
    standalones)      │ 2. markers: for each removed repo   │──► <managed>/<ws>/repository-<pk>/
                      │    read_marker == Valid && owner ==  │      .baude-marker.json  (remove_file)
                      │    observed_common_dir → remove_file │      then remove_dir (non-recursive)
                      │    then remove_dir                   │
                      │ 3. suppression:                      │──► <config>/<ws suppression file>
                      │    checkout removed, parent kept →   │    (locked_merge_write)
                      │      insert folder_key(observed_path)│
                      │    repo removed → remove children's  │
                      │      keys (GC)                       │
                      └──────────────────┬──────────────────┘
                                         ▼
                    RemovalReport { …Phase 18 fields…, cleared: Vec<ClearedItem>,
                                    clearing_failures: Vec<ClearingFailure> }

   ─────────────────────────── relaunch / rediscovery ───────────────────────────

   baude <launch_dir>
     └─ launch::start_workspace step 5 ─► find_binding walks folder_key(dir) upward
            (binding gone → falls through to configured workspace; re-records on claim)
     └─ App::restore
          ├─ reconcile_teardown/activation_recoveries, restore_standalones (state-driven; rows absent → silent)
          ├─ open_repo_session_via(launch_dir, LaunchDirectory)
          │     └─ admit_repository(path)
          │           ├─ NEW suppression::lift(folder_key(path))            ◄── explicit admission
          │           ├─ ensure_repository → marker Missing + dir gone → fresh dir, fresh marker
          │           └─ admit_existing_worktrees(repo_key, snapshot)
          │                 for record in snapshot.worktrees:
          │                    owned? skip   NEW suppressed? skip   else admit fresh row
          └─ (daemon: bauded Manager::restore, Phase 21)
```

### Recommended Project Structure

```
baude-core/src/
├── removal.rs          # + clear_stage, ClearedItem, ClearingFailure, report fields; preview populates them
├── suppression.rs      # NEW: SuppressionFile, suppress/lift/is_suppressed/drop_children, file naming
├── folder_workspace.rs # + pub fn clear(root, path) -> io::Result<bool> (mirror of record)
├── lib.rs              # + pub mod suppression;
baude/src/
└── app.rs              # admit_repository/admit_standalone: lift; admit_existing_worktrees: consult
```

### Pattern 1: Exact-key clear through the existing merge path (READM-01, READM-05)

**What:** `folder_workspace::record` is the write-side template. [VERIFIED: baude-core/src/folder_workspace.rs, `record` body]

```rust
// baude-core/src/folder_workspace.rs (verbatim, existing)
pub fn record(root: Option<&Path>, launch_dir: &Path, workspace: &str, now_ms: u64) {
    let Some(root) = root else { return };
    let key = folder_key(launch_dir);
    let entry = FolderWorkspaceEntry { workspace: workspace.to_string(), updated_ms: now_ms };
    let _ = locked_merge_write(root, FILE_NAME, |merged: &mut FolderWorkspaceFile| {
        merged.schema_version = SCHEMA_VERSION;
        merged.folders.insert(key, entry);
    });
}
```

**When to use:** the clearing stage calls a sibling `clear(root, path)` that does `merged.folders.remove(&key)` and returns whether an entry existed, so the report can say "cleared" vs "no binding". `FILE_NAME` is `"folder-workspaces.json"`, `SCHEMA_VERSION` is `1`, and the map is `folders: BTreeMap<String, FolderWorkspaceEntry>` with `FolderWorkspaceEntry { workspace: String, updated_ms: u64 }`. [VERIFIED: baude-core/src/folder_workspace.rs:9-29]

**Key derivation:** the binding was recorded with `folder_key(repo_root_path)` at `launch.rs:107-115` where `repo_root` came from `git::repo_root(launch_dir)`; `folder_key` canonicalizes when it can and falls back to the raw path. [VERIFIED: baude-core/src/launch.rs:104-115, breadcrumbs.rs:116-121] The removed row's `observed_main_worktree` / `observed_path` / `canonical_path` are `PersistedPath(Vec<u8>)` with `to_path_buf()`; pass `folder_key(&p.to_path_buf())` so a still-present folder canonicalizes identically and a deleted managed worktree falls back to the stored (already canonical) bytes. [VERIFIED: baude-core/src/repository.rs:40-53]

**Binding walk:** `find_binding` starts at `launch_dir`, checks `file.folders.get(&folder_key(current))`, and pops to the parent until it reaches canonical home. Removing the exact key is sufficient; nothing else in the walk references the removed folder. [VERIFIED: baude-core/src/folder_workspace.rs `find_binding` body]

### Pattern 2: Owner-checked marker deletion then non-recursive dir removal (READM-02)

**What:** the marker file is `dir.join(".baude-marker.json")` [VERIFIED: baude-core/src/marker.rs:89,186] with metadata

```rust
// baude-core/src/marker.rs:23-31 (verbatim)
pub struct MarkerMetadata {
    pub canonical_common_dir: Vec<u8>,
    pub scheme_version: u32,
    pub recorded_at_ms: u64,
}
// baude-core/src/marker.rs:57-63 (verbatim)
pub enum MarkerRead {
    Missing,
    Valid(MarkerMetadata),
    Invalid(InvalidReason),
}
```

The directory is built in `ensure_repository` as [VERIFIED: baude-core/src/lifecycle.rs:1696-1701]

```rust
let workspace = crate::workspace::active();
let workspace_name = workspace.name.as_str();
let target_dir = git::worktrees_base()
    .join(workspace_name)
    .join(format!("repository-{}", digest));
```

and on collision the suffixed path's `file_name` minus `repository-` becomes `physical_key` (lines 1706-1719), which is why D-05 says to use the stored field, never a recomputed digest. `SavedRepository` carries `observed_common_dir: PersistedPath`, `observed_main_worktree: PersistedPath`, `physical_key: String`. [VERIFIED: baude-core/src/repository.rs:336-343]

**Owner comparison:** `meta.canonical_common_dir == repo.observed_common_dir.as_bytes()` (both are OsStr bytes on unix; `ensure_repository` wrote the marker from `snapshot.common_dir.as_os_str().as_bytes()` at lifecycle.rs:1686-1688). [VERIFIED]

**Readers that must then see "unowned":** `worktree_scan::discover_owner` (worktree_scan.rs:862+) returns `Valid` metadata as owner, and on `Missing` falls back to `read_dir(repository_dir)` and `git::discover_repository` on the first subdirectory. That fallback is the reason D-07's `remove_dir` matters: an empty, present directory with no marker still reads as unowned only because it has no subdirectories; a removed directory is not scanned at all. `check_collision` (lifecycle.rs:1795) returns "no collision" when the directory does not exist, so the next admission of the same repository gets the un-suffixed path and a fresh marker. [VERIFIED: baude-core/src/worktree_scan.rs:862-900, lifecycle.rs:1795-1801]

**Legacy rows:** fixtures and legacy loads use `physical_key: String::new()` (repository.rs:1185, persist.rs:793). An empty `physical_key` resolves to `repository-` which will not exist; treat "directory missing" as a no-op `cleared: none` outcome rather than a failure. [VERIFIED: grep of `physical_key: String::new()` across repository.rs, persist.rs, lifecycle.rs]

### Pattern 3: Per-workspace advisory store modelled on breadcrumbs (READM-03, READM-04)

**What:** breadcrumbs name their per-workspace file with `workspace.state_file(FILE_BASE)` where `FILE_BASE = "breadcrumbs"` (breadcrumbs.rs:35,130); the store lives under `persist::config_dir()`. [VERIFIED] The suppression module should do the same with its own base, e.g. `suppressed-admissions`, keyed by `folder_key` strings, with `#[serde(default = "schema_version_default")] schema_version: u32` and `#[serde(default)] entries: BTreeMap<String, SuppressionEntry>` exactly as `FolderWorkspaceFile` is declared. Corruption handling comes free from `load_json` (returns `(T::default(), true)` on parse failure) and `locked_merge_write` (reloads under lock, so a corrupt file is replaced by one holding the caller's merge). [VERIFIED: breadcrumbs.rs:229-283]

**Consult point:** `App::admit_existing_worktrees` iterates `snapshot.worktrees`, skips `bare || detached || locked || prunable`, skips records without a branch, computes `path = PersistedPath::from_path(&record.path)`, skips when any checkout `observed_path == path` or any standalone `canonical_path == path`, then allocates `allocate_checkout_key()` and `allocate_first_seen_order()`. [VERIFIED: baude/src/app.rs:2476-2521] Insert `if suppression::is_suppressed(&record.path) { continue; }` immediately after the `owned` check.

**Lift point:** `open_repo_session_via(path, route)` (app.rs:5479) is the single local entrypoint for `LaunchDirectory` (restore, app.rs:1455), `Open` (`n` submit, app.rs:5476) and `CloneCompletion` (app.rs:5553); it calls `admit_standalone(&canonical)` for non-git folders and `admit_repository(&path)` for repositories. [VERIFIED: baude/src/app.rs:51-55, 1455, 5476-5508, 5553] `admit_repository` starts with `git::discover_repository(path)?` (2293); the lift must use the exact path being admitted (`folder_key(path)`), not `snapshot.main_worktree`, so `n` with the parent root does not lift a child's entry (D-12). Place the lift right after discovery succeeds and before the `existing_key` branch at 2360-2399, which is where both `admit_existing_worktrees` calls (2393, 2464) are reached. `admit_standalone` canonicalizes first (2005-2012); lift there too for symmetry, though D-10 means no entry will normally exist for a standalone.

### Pattern 4: Best-effort fourth stage with report fields (D-14)

**What:** `remove` today ends with `persist_stage` returning `(persistence, readers_parked)` and then `report_from_set(&set, Some(StageResults{..}))`. [VERIFIED: baude-core/src/removal.rs:396-424] Add `clear_stage` after `persist_stage`, returning a value (never `Err`) that `StageResults` carries into `report_from_set`. `preview_removal` is `select_rows` + `report_from_set(&set, None)` (320-326); it must compute the same planned clearing list from the set so Phase 20 can show it, which means the "what would be cleared" computation must be a pure function of `RemovalSet` + current state (`PlannedRow` already carries `path`, `main_worktree`, `common_dir`, `row: RowKey`) [VERIFIED: removal.rs:77-89], while the real stage uses `RemovedRows` from `candidate.remove_rows` for `physical_key`. Simplest: call `select_rows` → derive a `Vec<PlannedClearing>` from `state` (look up `SavedRepository` for repository targets to get `physical_key`) and have both paths use it.

**Report derive:** `RemovalReport` is `Clone, Debug, Deserialize, Eq, PartialEq, Serialize` and already round-trips in `preview_has_no_side_effects_and_report_round_trips_serde`; new fields must keep that derive set, so failures carry `error: String`, not `io::Error`. [VERIFIED: removal.rs:147-160 and test name list]

### Anti-Patterns to Avoid
- **Clearing bindings by workspace value:** wipes neighbours sharing the workspace (READM-05). Exact key only.
- **Recomputing the digest for the marker dir:** picks the wrong directory for a suffixed collision row. Use `physical_key`.
- **`remove_dir_all` on the repository dir:** violates D-07 and RMV-07's spirit; leaves non-empty dirs and reports them.
- **Making the clearing stage return `Err`:** the save has already committed; an error would misreport a completed removal.
- **Consulting the suppression store with `PersistedPath` equality:** the store key is a `folder_key` string; derive the key from `record.path` with `folder_key` on both sides.
- **Running the Phase 19 relaunch test through a TUI remove key:** none exists until Phase 20; call `removal::remove` directly with the `App`'s state and a save closure, or seed the post-removal state file.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Locked read-merge-write of advisory JSON | A new file lock / temp-rename routine | `breadcrumbs::locked_merge_write` | Already handles lock file, temp + rename + dir fsync, corrupt-file replacement |
| Corrupt-tolerant load | `fs::read_to_string` + `unwrap_or_default` | `breadcrumbs::load_json` | Distinguishes NotFound from corrupt and is what every sibling store uses |
| Canonical path key | `path.display().to_string()` | `breadcrumbs::folder_key` | Canonicalizes with raw-path fallback; the same function wrote the keys |
| Marker parsing and symlink/size defence | Reading the marker JSON yourself | `marker::read_marker` | Fail-closed: symlink, oversize, malformed, scheme-too-new all map to `Invalid` |
| Managed root resolution | `dirs::data_dir()` arithmetic | `git::worktrees_base()` | Redirect-aware; the test binary aborts on an unredirected real root |
| Per-workspace file naming | `format!("suppressed-{}.json", ws)` | `Workspace::state_file(base)` | Same naming rule as breadcrumbs and the state file |
| Removed-set enumeration | Re-deriving children from keys | `RepositoryState::remove_rows` → `RemovedRows` | Already partitions repositories, checkouts and standalone sessions |

**Key insight:** every artefact this phase clears was written by a core helper that already exists; the clear is the mirror of the write through the same helper, so no new I/O primitive should appear.

## Common Pitfalls

### Pitfall 1: Folder context is off in `App` tests
**What goes wrong:** a D-13 crumb test written against `App::restore` finds no breadcrumb file.
**Why it happens:** under `cfg(test)` `restore` passes `root = None` to `FolderContext::load` unless `folder_context_enabled_for_test` is set, and even then the root is `None` (app.rs:1416-1427). [VERIFIED]
**How to avoid:** prove crumb pruning in `baude-core` with `FolderContext::load(Some(root), workspace, launch_dir, now)`, insert a crumb for the removed path, call `flush(&state_after_removal, now)`, and assert the on-disk entry no longer contains it (flush retains only crumbs whose path is in `state.checkouts`/`state.standalone_sessions`, breadcrumbs.rs:198-213).
**Warning signs:** a test that asserts on `app.folder_context` being `Some`.

### Pitfall 2: No TUI removal call exists yet
**What goes wrong:** a plan task says "press the remove key, then relaunch".
**Why it happens:** `grep -n "removal::" baude/src/app.rs` returns nothing; Phase 20 adds the key. [VERIFIED]
**How to avoid:** in the `App` relaunch test, build the first `App`, admit the fixture, then call `baude_core::removal::remove(&mut app.repository_state, target, &mut Vec::new(), |file| persist::save_current_at_test(&root, "state.json", file, None, None))` (the Phase 18 test pattern at removal.rs:1460-1475), or save the post-removal state through the App's own save path, then build the second `App` with the same `persistence_root_for_test` and `restore()`.
**Warning signs:** a plan that references a key binding or `Modal::ConfirmRemove`.

### Pitfall 3: macOS temp dir symlink breaks key equality
**What goes wrong:** `folder_key` of a fixture path yields `/private/var/...` while a stored path is `/var/...`, so the clear misses.
**Why it happens:** `canonicalize` resolves `/var → /private/var`; `RemovalFixture::new` canonicalizes `root` for exactly this reason (removal.rs:1343-1347). [VERIFIED]
**How to avoid:** canonicalize fixture roots, and derive every key via `folder_key` on both write and clear sides.
**Warning signs:** a clear that passes on Linux CI and fails locally.

### Pitfall 4: `pub(crate)` helpers are invisible from `baude`
**What goes wrong:** calling `breadcrumbs::locked_merge_write` or `git::worktrees_base` from `baude/src/app.rs` fails to compile.
**Why it happens:** both are `pub(crate)` (breadcrumbs.rs:229,248; git.rs:1789). [VERIFIED]
**How to avoid:** put every consult/lift/clear function in `baude-core` (D-11 requires this anyway) and expose only `pub fn is_suppressed(path)`, `pub fn lift(path)`, etc.
**Warning signs:** a plan task that edits visibility on `breadcrumbs.rs` helpers.

### Pitfall 5: Preview and real run disagree on cleared items
**What goes wrong:** Phase 20's confirmation lists a marker that the real run does not touch, or vice versa.
**Why it happens:** `preview_removal` only has `RemovalSet`; `physical_key` and `observed_common_dir` live on `SavedRepository` in `state`.
**How to avoid:** compute the planned clearing list from `&RepositoryState` + `&RemovalSet` in one function used by both `preview_removal` and `remove` (the real stage then executes the list and records outcomes). Test: `remove_report_matches_preview_rows` already exists; add the same assertion for the cleared list.
**Warning signs:** two separate enumerations of removed paths.

### Pitfall 6: Suppression lift must precede both sweep sites
**What goes wrong:** `n` with a removed unmanaged checkout path re-admits nothing because the sweep ran first and skipped it, and the follows-branch early return at app.rs:2390-2395 bypasses the later code.
**Why it happens:** `admit_existing_worktrees` is called at 2393 (inside the `existing_key` branch) and 2464 (fresh repository). [VERIFIED]
**How to avoid:** lift immediately after `discover_repository(path)?` at the top of `admit_repository`, before `existing_key` is resolved.
**Warning signs:** the lift placed after line 2399.

### Pitfall 7: Gate's Linux lane on macOS
**What goes wrong:** `scripts/gate.sh --linux` fails with `E0463` from a Docker Desktop bind-mount race.
**Why it happens:** documented in Phase 18 deferred-items; not a code defect. [VERIFIED: .planning/phases/18-shared-kill-then-remove-lifecycle/deferred-items.md]
**How to avoid:** run the manual variant recorded in 18-07-SUMMARY: the unmodified `scripts/gate.sh` inside the `baude-gate` image as uid 1000 with `CARGO_TARGET_DIR=/tmp/linux-gate/target`, reading `exit=` from the redirected log. Phase 19 has no signal/procfs code, so the host gate is the primary evidence and the Linux lane is confirmatory.

## Code Examples

### 1. Binding clear (mirror of `record`)

```rust
// baude-core/src/folder_workspace.rs — NEW; shape mirrors `record` (verified above)
/// Remove the exact binding for `path`. Returns Ok(true) when an entry existed.
pub fn clear(root: &Path, path: &Path) -> std::io::Result<bool> {
    let key = folder_key(path);
    let mut existed = false;
    locked_merge_write(root, FILE_NAME, |merged: &mut FolderWorkspaceFile| {
        merged.schema_version = SCHEMA_VERSION;
        existed = merged.folders.remove(&key).is_some();
    })?;
    Ok(existed)
}
```

### 2. Marker clear with owner check

```rust
// baude-core/src/removal.rs — NEW stage helper; APIs verified at marker.rs:57-63,185; lifecycle.rs:1696-1701
fn clear_marker(repo: &SavedRepository) -> ClearOutcome {
    let dir = crate::git::worktrees_base()
        .join(crate::workspace::active().name.as_str())
        .join(format!("repository-{}", repo.physical_key));
    match crate::marker::read_marker(&dir) {
        Ok(crate::marker::MarkerRead::Missing) => { /* fall through to remove_dir */ }
        Ok(crate::marker::MarkerRead::Valid(meta))
            if meta.canonical_common_dir == repo.observed_common_dir.as_bytes() =>
        {
            if let Err(e) = std::fs::remove_file(dir.join(".baude-marker.json")) {
                return ClearOutcome::failed(dir, e);
            }
        }
        Ok(crate::marker::MarkerRead::Valid(_)) | Ok(crate::marker::MarkerRead::Invalid(_)) => {
            return ClearOutcome::left_in_place(dir, "marker not owned by removed repository");
        }
        Err(e) => return ClearOutcome::failed(dir, e),
    }
    match std::fs::remove_dir(&dir) {
        Ok(()) => ClearOutcome::cleared(dir),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ClearOutcome::nothing(dir),
        Err(e) => ClearOutcome::left_in_place(dir, e.to_string()), // non-empty: report, never recurse
    }
}
```

### 3. Suppression store skeleton

```rust
// baude-core/src/suppression.rs — NEW; derive set and serde attrs copied from folder_workspace.rs:16-29
pub const SCHEMA_VERSION: u32 = 1;
pub const FILE_BASE: &str = "suppressed-admissions"; // name is Claude's discretion

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SuppressionFile {
    #[serde(default = "schema_version_default")]
    pub schema_version: u32,
    #[serde(default)]
    pub entries: BTreeMap<String, SuppressionEntry>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SuppressionEntry { pub repository_common_dir: String, pub suppressed_ms: u64 }

fn file_name() -> String { crate::workspace::active().state_file(FILE_BASE) }

pub fn is_suppressed(path: &Path) -> bool {
    let root = crate::persist::config_dir();
    let (file, _) = crate::breadcrumbs::load_json::<SuppressionFile>(&root.join(file_name()));
    file.entries.contains_key(&crate::breadcrumbs::folder_key(path))
}
pub fn lift(path: &Path) -> std::io::Result<bool> { /* locked_merge_write; entries.remove(&key) */ }
pub(crate) fn suppress(path: &Path, common_dir: &Path, now_ms: u64) -> std::io::Result<()> { /* insert */ }
pub(crate) fn drop_children(common_dir: &Path) -> std::io::Result<usize> { /* GC on parent removal, D-10 */ }
```

`Workspace::state_file` is the naming helper breadcrumbs use at breadcrumbs.rs:130 (`workspace.state_file(FILE_BASE)`); confirm its exact signature in `baude-core/src/workspace.rs` when implementing. [VERIFIED: call site; signature not opened this session — treat the argument type as `[ASSUMED]` `&str`]

### 4. Two-`App` relaunch fixture (existing pattern)

```rust
// baude/src/app.rs:9303-9352 pattern (verified): same TestRedirect, same state_root, second App restores
let _redirect = baude_core::testing::TestRedirect::new(&root);
let mut app = App::new(repo.clone());
app.persistence_root_for_test = Some(state_root.clone());
app.restore();
// ... removal::remove(&mut app.repository_state, target, &mut Vec::new(), |f| save_current_at_test(&state_root, "state.json", f, None, None))
let mut restarted = App::new(repo.clone());
restarted.persistence_root_for_test = Some(state_root.clone());
restarted.restore();
assert!(restarted.repository_state.checkouts.iter().all(|c| c.observed_path != removed_path));
assert!(baude_core::lifecycle::startup_recovery_program(&restarted.repository_state).is_empty());
```

`startup_recovery_program(state: &RepositoryState) -> Vec<RecoveryStep>` with `RecoveryStep::{StopOwned, FinishRemoval, RecoverActivation, ReconcileTopology}(CheckoutKey)`. [VERIFIED: lifecycle.rs:379-383, 417]

### 5. Phase 18 save closure and state helpers for core tests

```rust
// baude-core/src/removal.rs:1460-1475 (verified)
persist::save_current_at_test(&f.state_root(), "state.json", &StateFile { schema_version: persist::SCHEMA_VERSION, state }, None, None)
persist::load_current_at(state_root, "state.json").expect("load saved state").state
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Row removal = state edit only (pre-v2.8) | Phase 18 three-stage `removal::remove` with one save | Phase 18 (2026-11-02) | Phase 19 extends with a fourth stage, not a rewrite |
| Marker left behind after removal | Owner-checked marker + dir clear | This phase | `check_collision` sees no dir → next admission is un-suffixed |

**Deprecated/outdated:** none relevant.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `Workspace::state_file` takes `&str` and returns the per-workspace file name as `String` | Code Example 3 | Compile error only; fix at implementation. |
| A2 | `git::repo_root` of a linked worktree returns that worktree's top level, so a binding keyed by the checkout's `observed_path` exists when baude was launched inside it | Pattern 1 | If it returned the main worktree instead, D-01's per-checkout key would simply find no entry (no-op), and READM-01 still holds via the repository's key. |
| A3 | Removing an entry from a `BTreeMap` and re-serializing with `to_vec_pretty` leaves untouched entries byte-identical in the pretty output | D-15 tests | If serde ordering differed, assert on parsed entries rather than raw bytes for neighbours. BTreeMap is ordered, so this is low risk. |

## Open Questions (RESOLVED)

All three questions below were settled by the plans; none remains open for the executor.

1. **Should a suppression entry be pruned when its path no longer exists on disk?** — **RESOLVED** by plan 19-02 (must_haves truth 5 and the `suppression` module doc comment): entries are kept until lifted or garbage-collected; nothing prunes an entry because its path stopped existing.
   - What we know: CONTEXT leaves this to discretion; `folder-workspaces.json` keeps stale entries by design.
   - What's unclear: whether a stale entry can ever cause harm. It cannot re-admit anything (it only suppresses) and is lifted by explicit admission.
   - Recommendation: keep entries (no pruning); drop them only on parent-repository removal (D-10 GC). Simpler and consistent with the sibling store.

2. **One stage function or three?** — **RESOLVED** by plan 19-04 Task 3: one `clear_stage` orchestrator fixes the order (bindings, markers and directories, suppression drops, suppression adds); it is split into `clear_bindings`, `clear_markers` and `update_suppression` only if it grows past roughly 80 lines, and the orchestrator keeps the order either way.
   - Recommendation: one `clear_stage` orchestrator calling three small helpers (`clear_bindings`, `clear_markers`, `update_suppression`), each returning outcomes appended to one `Vec`. Tests target the helpers; the orchestrator test proves order and best-effort behaviour.

3. **Where does `RemovedRows` reach the stage?** — **RESOLVED** by plan 19-01 (the stage slot after `persist_stage`, with `clear_stage(planned)` taking only the planned list) and plan 19-04 Task 2 (binds the `RemovedRows` return of `candidate.remove_rows` in `remove` and widens `clear_stage` to `(removed, state, planned)` the moment the markers and suppression writes first need it, so plan 01's `clippy -D warnings` gate sees no unused parameter in between).
   - What we know: `remove` builds `candidate` and calls `candidate.remove_rows(...)` discarding the return value (removal.rs:403-408).
   - Recommendation: bind the `RemovedRows` return value and pass it to `clear_stage` after `persist_stage`; it carries `physical_key` and `observed_common_dir` for repositories and `observed_path` for checkouts.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo / rustc | build, tests, gate | ✓ | cargo 1.99.1, rustc 1.99.1 | — |
| git | fixtures (`git_in`, `cloned_topology`) | ✓ | 2.54.0 (Apple Git-157) | — |
| Docker | `scripts/gate.sh --linux` | ✓ | `docker info` succeeded | Manual container variant from 18-07-SUMMARY due to bind-mount race |
| Knowledge graph | optional context | ✗ | `graphify status`: `exists: false` | Research proceeded from source directly |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** knowledge graph (not needed).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` (workspace: `baude-core`, `baude`, `bauded`) |
| Config file | none — `#[cfg(test)] mod tests` per module |
| Quick run command | `cargo test -p baude-core removal::tests -- --test-threads=1` |
| Full suite command | `scripts/gate.sh` (fmt, clippy `--all-targets -D warnings`, commit-message checks, real-roots leak check, `cargo test -- --test-threads=1` isolated) [VERIFIED: scripts/gate.sh lines 35-63] |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| READM-01 | Binding for removed repo/checkout/standalone key gone; `find_binding` no longer resolves it; standalone with no binding leaves file byte-identical | unit, `git init` + cloned fixture under `TestRedirect` | `cargo test -p baude-core removal::tests::clear_ -- --test-threads=1` and `cargo test -p baude-core folder_workspace::tests -- --test-threads=1` | ❌ Wave 0 (new tests in existing modules) |
| READM-02 | Owned marker deleted and empty `repository-<pk>` dir removed; foreign/Invalid marker left and reported; non-empty dir left and reported; `discover_owner` returns None / `check_collision` reports none | unit | `cargo test -p baude-core removal::tests::marker_ -- --test-threads=1` | ❌ Wave 0 |
| READM-03 | Second `App::restore` over same redirect shows no removed rows; `startup_recovery_program` empty; suppressed unmanaged checkout not re-admitted by `admit_existing_worktrees` | integration (two `App`s) | `cargo test -p baude app::tests::removed_ -- --test-threads=1` | ❌ Wave 0 |
| READM-04 | `n`/`Open` with removed checkout path, `CloneCompletion`, and `LaunchDirectory` admit a fresh row with new keys, `resume_id: None`; entry lifted; `n` with parent root does not lift child | integration | `cargo test -p baude app::tests::readmit_ -- --test-threads=1` | ❌ Wave 0 |
| READM-05 | Two repos bound to one workspace + third repo with managed worktree: removing one leaves others' bindings, markers, worktree dirs and suppression entries byte-identical | unit, cloned fixture | `cargo test -p baude-core removal::tests::neighbours_ -- --test-threads=1` | ❌ Wave 0 |
| D-13 crumb | `FolderContext::flush` drops the removed path's crumb | unit | `cargo test -p baude-core breadcrumbs::tests -- --test-threads=1` | ❌ Wave 0 |
| D-14 report | `preview_removal` and `remove` agree on cleared list; report serde round-trips with new fields | unit | `cargo test -p baude-core removal::tests::remove_report_matches_preview_rows -- --test-threads=1` (extend) | ✅ (extend existing) |

### Sampling Rate
- **Per task commit:** `cargo test -p baude-core removal::tests -- --test-threads=1 && cargo fmt --check` (or the `-p baude app::tests::<prefix>` filter for TUI tasks)
- **Per wave merge:** `cargo clippy --all-targets -- -D warnings && cargo test -- --test-threads=1`
- **Phase gate:** `scripts/gate.sh` exit 0 before `/gsd-verify-work`; `scripts/gate.sh --linux` via the manual container variant (confirmatory; no platform-sensitive code in this phase)

### Wave 0 Gaps
- [ ] `baude-core/src/suppression.rs` with its `#[cfg(test)] mod tests` — covers READM-03/04 store semantics (suppress, consult, lift, GC, corrupt-file degrade)
- [ ] New tests in `baude-core/src/removal.rs::tests` — READM-01, READM-02, READM-05, D-14 (fixtures: `RemovalFixture`, `cloned_topology`; use the `RemovalFixture` `_redirect` field ordering to keep the redirect alive during teardown)
- [ ] New tests in `baude-core/src/folder_workspace.rs::tests` — `clear` round trip and `find_binding` after clear
- [ ] New tests in `baude-core/src/breadcrumbs.rs::tests` — D-13 flush prune
- [ ] New tests in `baude/src/app.rs::tests` — READM-03 relaunch, READM-04 three routes (pattern: `TestRedirect::new(&root)`, `App::new(path)`, `persistence_root_for_test`, `open_repo_session_via(folder, LocalAdmissionRoute::Open)` as at app.rs:11060)
- Framework install: none

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | Paths come from durable state and `git worktree list`, not user text; keys pass through `folder_key`; marker reads use `read_marker`'s fail-closed parsing |
| V6 Cryptography | no | `physical_key` digest is identity, not security; read from the stored field, never recomputed |
| V12 Files and Resources | yes | Non-recursive `remove_dir`; delete only under `worktrees_base()` and `config_dir()`; symlink marker is `Invalid` and left alone; `remove_file` targets a fixed file name in a dir derived from stored state |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Deleting a marker owned by another repository | Tampering | D-06 owner byte comparison; `Invalid`/foreign left in place and reported |
| Recursive delete escaping the managed root | Tampering | D-07 `remove_dir` only; path built from `worktrees_base()` + workspace + `repository-<physical_key>`; test asserts kept main/unmanaged/standalone folders untouched |
| Symlinked marker or repository dir redirecting the delete | Tampering / EoP | `read_marker` rejects symlink markers (`InvalidReason::Symlink`); `remove_dir` does not follow symlinks to directories' contents; add a test with a symlinked `repository-<pk>` entry asserting nothing behind it is removed |
| Corrupt advisory file blocking launch | Denial of service | `load_json` degrades to empty; `locked_merge_write` replaces corrupt content; never `Err` out of the clearing stage |
| Parallel TUI losing its binding during clear | Tampering (race) | `locked_merge_write` advisory lock + atomic rename |
| Test leaking into real `~/.config/baude` or `~/.local/share/baude` | Tampering | `TestRedirect` on every test; gate's `assert-real-roots-untouched.sh` before/after |

## Project Constraints (from CLAUDE.md)

- Run `scripts/gate.sh` before every push; `scripts/gate.sh --linux` for platform-sensitive code (confirmatory here). Never judge a gate step by piping to `tail`/`grep` and reading `$?`.
- `main` is protected; every change lands by PR; release-please owns versions. Never nest parentheses in a commit body, squash message or PR title (`scripts/check-commit-message.sh`, enable via `git config core.hooksPath .githooks`).
- Tests: build fixtures in production shapes (cloned vs `git init`, stale runtime rows, command doubles), assert on fixture output, never grep source.
- Estate-wide CLAUDE.md (SSH CA policy) does not apply to this phase's code.
- No `.claude/skills/` or `.agents/skills/` directory exists in the repo; no project skills to load. [VERIFIED: `ls .claude/skills .agents/skills` found neither]

## Sources

### Primary (HIGH confidence)
- `baude-core/src/removal.rs` — `remove` 396-424, `preview_removal` 320-326, `RemovalReport` 147-160, `PlannedRow` 77-89, `RemovalSet` 91-94, `persist_stage` 655-691, `report_from_set` 739, `RemovalFixture` 1330-1400, save/load helpers 1460-1475, test name list
- `baude-core/src/folder_workspace.rs` — constants 9-10, `FolderWorkspaceFile`/`Entry` 16-29, `find_binding`, `plan_launch`, `record`
- `baude-core/src/breadcrumbs.rs` — `FILE_BASE` 35, `folder_key` 116-121, `FolderContext::load` 124+, `flush` 198-222, `load_json` 229-238, `locked_merge_write` 248-283
- `baude-core/src/marker.rs` — `MarkerMetadata` 23-31, `MarkerRead` 57-63, `write_marker` 88, `read_marker` 185-224, marker file name 89/186
- `baude-core/src/lifecycle.rs` — `RecoveryStep` 379-383, `startup_recovery_program` 417, `ensure_repository` 1655 and target dir 1696-1719, `check_collision` 1795, `discover_checkout_owner` 1931, `allocate_suffixed_repository_path` 2001
- `baude-core/src/repository.rs` — `PersistedPath` 40-53, `SavedRepository` 336-343, `SavedCheckout` 348-363, `SavedStandaloneSession` 453-460, `RemovedRows` 523-527, `remove_rows` 896-901, `physical_key` 1111, `compute_repository_digest` 1123
- `baude-core/src/git.rs` — `repo_root` 1758, `real_worktrees_base` 1772, `worktrees_base` 1789-1799
- `baude-core/src/worktree_scan.rs` — `discover_owner` 862-900, `scan_at` 917 (called from `baude/src/main.rs:454`, a CLI path, not session start)
- `baude-core/src/launch.rs` — `start_workspace` 66, step 5 binding record 104-115
- `baude-core/src/persist.rs` — `SCHEMA_VERSION = 3` line 17, `save_current_at_test` 473, `config_base` 882-891, `config_dir` 895
- `baude-core/src/testing.rs` — `TestRedirect::new` 107-138, overrides 196-211, `git_in` 311, `ClonedTopology` 334-345, `cloned_topology` 353
- `baude/src/app.rs` — `LocalAdmissionRoute` 51-55, `persistence_root_for_test` 599, `restore` 1352-1480, launch-dir admission 1455, flush 1864, `admit_standalone` 2005, `admit_repository` 2292-2399, `admit_existing_worktrees` 2476-2521, `reconcile_activation_recoveries` 2547, `reconcile_teardown_recoveries` 2591, `open_new_session_modal` 4800, `open_repo_session_via` 5479, `n` submit 5302/5476, clone completion 5553, restart test pattern 9303-9352
- `bauded/src/manager.rs` — `restore` 496 (Phase 21 parallel)
- `scripts/gate.sh` — steps 35-63; `.planning/phases/18-shared-kill-then-remove-lifecycle/{18-PATTERNS.md,18-VALIDATION.md,18-07-SUMMARY.md,deferred-items.md}`
- `.planning/phases/19-re-admission-suppression/19-CONTEXT.md`, `.planning/REQUIREMENTS.md:40-44`, `.planning/config.json`

### Secondary (MEDIUM confidence)
- none

### Tertiary (LOW confidence)
- none

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new crates; every helper read from source with line ranges
- Architecture: HIGH — stage order, call sites and readers all located and quoted
- Pitfalls: HIGH — each pitfall traces to a verified line (test-mode folder context, missing TUI removal call, `pub(crate)` visibility, macOS canonicalization)

**Research date:** 2026-11-03
**Valid until:** 2026-12-03 (stable internal codebase; re-verify line numbers after Phase 20 edits `app.rs`)
