# Phase 19: Re-admission Suppression - Pattern Map

**Mapped:** 2026-11-03
**Files analyzed:** 7 (1 new, 6 modified)
**Analogs found:** 7 / 7

All analog paths below are git-tracked source (`git ls-files` verified for `baude-core/src/{removal,folder_workspace,breadcrumbs,marker,lib}.rs` and `baude/src/app.rs`). No mirror or gitignored paths are named.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/suppression.rs` (NEW) | store / advisory persistence | file-I/O (read-merge-write JSON) | `baude-core/src/folder_workspace.rs` (whole file, 486 lines) | exact |
| `baude-core/src/folder_workspace.rs` (+ `clear`) | store | file-I/O | own `record` at lines 186-197 | exact |
| `baude-core/src/removal.rs` (+ `clear_stage`, report fields) | service / lifecycle stage | batch (per-row best-effort I/O) | own `disk_stage` 591-622, `persist_stage` 655-691, `report_from_set` 739-785 | exact |
| `baude-core/src/lib.rs` (+ `pub mod suppression;`) | config | — | lines 8-27 module list | exact |
| `baude/src/app.rs` (`admit_repository`, `admit_standalone`, `admit_existing_worktrees`) | controller / admission seam | request-response | own lines 2005-2016, 2292-2399, 2476-2525 | exact |
| `baude-core/src/removal.rs::tests` (new READM-01/02/05 tests) | test | — | `RemovalFixture` 1336-1413, `seed_cloned` 2949-2976, `remove_report_matches_preview_rows` 2236-2265 | exact |
| `baude/src/app.rs::tests` (relaunch + three-route tests) | test | — | two-`App` restart pattern 9303-9331, `open_repo_session_via` Open test 11040-11075 | exact |

Supporting analogs read for shared patterns: `baude-core/src/breadcrumbs.rs` 114-121, 198-222, 229-283, 468-496; `baude-core/src/marker.rs` 22-64, 185-226; `baude-core/src/lifecycle.rs` 1684-1721; `baude-core/src/repository.rs` 40-56, 523-527, 896-901; `baude-core/src/persist.rs` 882-897; `baude-core/src/testing.rs` 107-138, 334-360.

## Pattern Assignments

### `baude-core/src/suppression.rs` (NEW store, file-I/O)

**Analog:** `baude-core/src/folder_workspace.rs` — copy its module shape nearly verbatim, swapping the workspace-agnostic `FILE_NAME` for a per-workspace `workspace.state_file(FILE_BASE)` name as breadcrumbs do.

**Module doc + imports pattern** (`folder_workspace.rs` lines 18-31):
```rust
//! Same durability posture as breadcrumbs: advisory UI memory, not lifecycle
//! state. Writes are read-merge-write under the shared advisory-lock helper
//! so parallel TUIs launched from different folders never clobber each other;
//! a missing or corrupt file degrades to "no memory" and never fails a
//! launch. Entries are never pruned automatically — a folder on a detached
//! volume is legitimate history.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::breadcrumbs::{folder_key, load_json, locked_merge_write};
```

**File struct pattern** (`folder_workspace.rs` lines 33-57) — keep `Default` derive, `schema_version_default`, no `deny_unknown_fields`:
```rust
pub const SCHEMA_VERSION: u32 = 1;
pub const FILE_NAME: &str = "folder-workspaces.json";

fn schema_version_default() -> u32 {
    SCHEMA_VERSION
}

/// Whole-file shape. No `deny_unknown_fields`: additive evolution is expected
/// and an older baude reading a newer file must not treat it as corrupt.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FolderWorkspaceFile {
    #[serde(default = "schema_version_default")]
    pub schema_version: u32,
    /// Keyed by the canonical launch directory (lossy UTF-8).
    #[serde(default)]
    pub folders: BTreeMap<String, FolderWorkspaceEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FolderWorkspaceEntry {
    pub workspace: String,
    #[serde(default)]
    pub updated_ms: u64,
}
```

**Per-workspace file naming pattern** (`breadcrumbs.rs` lines 34-35, 130; `workspace.rs` 87-89):
```rust
/// Base name; the on-disk file is `breadcrumbs-<workspace>.json`.
pub const FILE_BASE: &str = "breadcrumbs";
// ...
let file = workspace.state_file(FILE_BASE);   // Workspace::state_file(&self, base: &str) -> String  => "{base}-{name}.json"
```
Root is `crate::persist::config_dir()` (`persist.rs` 893-897), which honours `testing::config_dir_override()` so `TestRedirect` lands the file under `<root>/config`.

**Read (consult) pattern** (`folder_workspace.rs` lines 84-85):
```rust
let (file, _) = load_json::<FolderWorkspaceFile>(&root.join(FILE_NAME));
// ...
if let Some(entry) = file.folders.get(&folder_key(&current)) { ... }
```

**Write (suppress / lift / GC) pattern** (`folder_workspace.rs` lines 186-197) — the lift is this with `remove(&key)`; GC is a `retain` over entries:
```rust
pub fn record(root: Option<&Path>, launch_dir: &Path, workspace: &str, now_ms: u64) {
    let Some(root) = root else { return };
    let key = folder_key(launch_dir);
    let entry = FolderWorkspaceEntry {
        workspace: workspace.to_string(),
        updated_ms: now_ms,
    };
    let _ = locked_merge_write(root, FILE_NAME, |merged: &mut FolderWorkspaceFile| {
        merged.schema_version = SCHEMA_VERSION;
        merged.folders.insert(key, entry);
    });
}
```
Note `locked_merge_write` and `load_json` are `pub(crate)` (`breadcrumbs.rs` 229, 248), so every suppression function lives in `baude-core` and exposes only `pub fn is_suppressed(&Path) -> bool`, `pub fn lift(&Path) -> io::Result<bool>`, and crate-visible `suppress`/`drop_children` for `removal.rs`.

**Test pattern** (`folder_workspace.rs` lines 206-238, 270-284) — scratch dir + `TestRedirect`, round trip preserving neighbours, corrupt file degrades and heals:
```rust
fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "baude-folder-workspace-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn records_and_recalls_one_folder_preserving_others() {
    let _redirect = TestRedirect::new(scratch("roundtrip-root"));
    let dir = scratch("roundtrip");
    record(Some(&dir), Path::new("/mem/api"), "opencode", 10);
    record(Some(&dir), Path::new("/mem/web"), "work", 20);
    record(Some(&dir), Path::new("/mem/api"), "oss", 30);
    // ...
    let (on_disk, corrupted) = load_json::<FolderWorkspaceFile>(&dir.join(FILE_NAME));
    assert!(!corrupted);
    assert_eq!(on_disk.schema_version, SCHEMA_VERSION);
    assert_eq!(on_disk.folders.len(), 2);
}

#[test]
fn corrupt_file_degrades_to_no_memory_with_one_note_and_heals_on_record() {
    let _redirect = TestRedirect::new(scratch("corrupt-root"));
    let dir = scratch("corrupt");
    std::fs::write(dir.join(FILE_NAME), b"{not json").unwrap();
    // ... read returns empty; a record() afterwards heals the file
}
```

---

### `baude-core/src/folder_workspace.rs` (+ `pub fn clear`)

**Analog:** own `record` (lines 186-197 above). The clear is the same `locked_merge_write` closure with `merged.folders.remove(&key).is_some()` captured into a local so the caller learns whether a binding existed; return `io::Result<bool>` instead of swallowing the error so `removal.rs` can report a failure. Keep `merged.schema_version = SCHEMA_VERSION;` in the closure (a corrupt file is replaced by a fresh one). Test alongside `find_binding_returns_none_when_no_binding_exists` (line 362) by recording two keys, clearing one, asserting `find_binding` still resolves the other and the file's remaining entry is unchanged.

---

### `baude-core/src/removal.rs` (+ fourth stage, report fields, planned clearing list)

**Analog:** own stage functions and report assembly.

**Stage result struct + per-row loop pattern** (`removal.rs` lines 577-622) — copy the `DiskResults` shape for a `ClearResults { cleared: Vec<ClearedItem>, failures: Vec<ClearingFailure> }`, but unlike `disk_stage` never `return Err`; push a failure and continue:
```rust
struct DiskResults {
    deleted: Vec<PathBuf>,
    kept: Vec<PathBuf>,
    already_removed: Vec<PathBuf>,
}

fn disk_stage(set: &RemovalSet) -> Result<DiskResults, RemovalError> {
    let mut results = DiskResults { deleted: Vec::new(), kept: Vec::new(), already_removed: Vec::new() };
    for row in &set.rows {
        let Some(path) = &row.path else { continue; };
        match row.disk {
            DiskAction::KeepDirectory => results.kept.push(path.clone()),
            DiskAction::RemoveManagedWorktree | DiskAction::AlreadyRemoved => {
                let common_dir = owner_common_dir(row, path)?;
                match crate::git::remove_managed_worktree_forced(common_dir, path) {
                    Ok(removal) if removal.deleted_directory => results.deleted.push(path.clone()),
                    Ok(_) => results.already_removed.push(path.clone()),
                    Err(crate::git::ForcedRemovalError::NotRegistered { .. })
                        if matches!(std::fs::symlink_metadata(path),
                                    Err(ref error) if error.kind() == ErrorKind::NotFound) =>
                    { results.already_removed.push(path.clone()) }
                    Err(error) => return Err(disk_error(row, path, error.into())),
                }
            }
        }
    }
    Ok(results)
}
```

**Stage order in `remove`** (`removal.rs` lines 396-425) — bind the discarded `RemovedRows` from `candidate.remove_rows(...)` (line 405) and add `clear_stage` after line 415:
```rust
    let set = select_rows(state, target)?;
    preflight(&set, runtimes)?;
    let mut candidate = state.clone();
    candidate.remove_rows(&set.repository_keys(), &set.checkout_keys(), &set.standalone_keys());
    candidate.validate_for_save().map_err(RemovalError::Validation)?;
    let killed = kill_stage(&set, runtimes)?;
    let disk = disk_stage(&set)?;
    let (persistence, readers_parked) = persist_stage(state, candidate, &set, runtimes, save)?;
    Ok(report_from_set(&set, Some(StageResults { killed, disk, readers_parked, persistence })))
```
`RemovedRows` (`repository.rs` 523-527) is `{ repositories: Vec<SavedRepository>, checkouts: Vec<SavedCheckout>, standalone_sessions: Vec<SavedStandaloneSession> }`; `SavedRepository` carries `physical_key: String`, `observed_common_dir: PersistedPath`, `observed_main_worktree: PersistedPath`. Because `remove_rows` consumes `candidate`, compute the planned clearing list from `state` + `set` BEFORE the stages (so `preview_removal` at lines 320-326 can call the same function), then execute it after `persist_stage`.

**`StageResults` carrier** (`removal.rs` lines 429-434) — add a `clearing: ClearResults` field:
```rust
struct StageResults {
    killed: Vec<KilledRuntime>,
    disk: DiskResults,
    readers_parked: Vec<RowKey>,
    persistence: PersistenceOutcome,
}
```

**Report shape and derive set** (`removal.rs` lines 145-160) — new fields must keep `Clone, Debug, Deserialize, Eq, PartialEq, Serialize`, so failures carry `error: String`:
```rust
/// What a removal will do (`planned == true`) or did.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemovalReport {
    pub target: RemovalTarget,
    pub planned: bool,
    pub rows_removed: Vec<RowKey>,
    pub runtimes_killed: Vec<KilledRuntime>,
    pub paths_deleted: Vec<PathBuf>,
    pub paths_kept: Vec<PathBuf>,
    pub already_removed: Vec<PathBuf>,
    /// ... Always empty in a preview; filled only by the real run.
    pub readers_parked: Vec<RowKey>,
    pub persistence: PersistenceOutcome,
}
```
Typed-failure precedent for a kind + detail pair (`removal.rs` lines 164-191): `DiskRemovalFailureKind` is `#[serde(rename_all = "snake_case")]` enum, `DiskRemovalFailure { kind, detail: String }` with a `Display` impl writing `"{:?}: {}"`. Model `ClearedItem { kind: ClearedKind /* Binding | Marker | RepositoryDir | Suppression */, path: PathBuf }` and `ClearingFailure { kind, path, error: String }` the same way.

**Two-path report assembly** (`removal.rs` lines 739-785) — the preview branch derives from the set, the real branch copies stage output; mirror that for the clearing fields:
```rust
fn report_from_set(set: &RemovalSet, stages: Option<StageResults>) -> RemovalReport {
    let rows_removed = set.rows.iter().map(|row| row.row).collect();
    if let Some(stages) = stages {
        return RemovalReport {
            target: set.target,
            planned: false,
            rows_removed,
            runtimes_killed: stages.killed,
            paths_deleted: stages.disk.deleted,
            paths_kept: stages.disk.kept,
            already_removed: stages.disk.already_removed,
            readers_parked: stages.readers_parked,
            persistence: stages.persistence,
        };
    }
    let mut report = RemovalReport { target: set.target, planned: true, rows_removed, /* empty vecs */ persistence: PersistenceOutcome::Planned };
    for row in &set.rows {
        if let Some(path) = &row.path {
            match row.disk {
                DiskAction::KeepDirectory => report.paths_kept.push(path.clone()),
                DiskAction::RemoveManagedWorktree => report.paths_deleted.push(path.clone()),
                DiskAction::AlreadyRemoved => report.already_removed.push(path.clone()),
            }
        }
    }
    report
}
```
Since the planned clearing list needs `physical_key` from `state` and `report_from_set` only sees the set, pass the precomputed `Vec<PlannedClearing>` as a third argument (or embed it in `StageResults` for the real run and a new parameter for preview).

**Marker owner check pattern** — the delete mirrors the write site. Marker dir construction (`lifecycle.rs` lines 1697-1701, use `repo.physical_key` instead of `digest`):
```rust
let workspace = crate::workspace::active();
let workspace_name = workspace.name.as_str();
let target_dir = git::worktrees_base()
    .join(workspace_name)
    .join(format!("repository-{}", digest));
```
Owner bytes were written from `snapshot.common_dir.as_os_str().as_bytes()` (`lifecycle.rs` 1686-1688), so compare `meta.canonical_common_dir == repo.observed_common_dir.as_bytes()` (`repository.rs` 53-55). Read with `marker::read_marker(&dir)` (`marker.rs` 185-226), which returns:
```rust
pub enum MarkerRead {
    Missing,                 // -> fall through to remove_dir
    Valid(MarkerMetadata),   // -> remove_file(dir.join(".baude-marker.json")) only if owner matches
    Invalid(InvalidReason),  // Symlink | Oversized | Malformed | SchemeTooNew -> leave, report
}
```
The marker file name is the literal `".baude-marker.json"` (`marker.rs` 89, 186). Then `std::fs::remove_dir(&dir)`: `NotFound` is a no-op, any other error (non-empty) is a reported left-in-place item.

**Core test fixture pattern** (`removal.rs` lines 1336-1363) — canonicalized root, redirect declared before root, `git init` repo:
```rust
struct RemovalFixture {
    _redirect: crate::testing::TestRedirect,
    root: PathBuf,
}

impl RemovalFixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir()
            .join(format!("baude-removal-live-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        let root = fs::canonicalize(&root).expect("canonicalize fixture root");
        let redirect = crate::testing::TestRedirect::new(&root);
        let fixture = Self { _redirect: redirect, root };
        git_ok(&fixture.root, &["init", "-q", "-b", "main", "repo"]);
        // ... user.name/email, one commit, create managed_root() and state_root()
        fixture
    }
    fn managed_root(&self) -> PathBuf { crate::git::worktrees_base() }
    fn state_root(&self) -> PathBuf { self.root.join("state") }
}
```
Cloned seed (`removal.rs` 2949-2976) uses `cloned_topology(&f.root)` and pushes a `SavedRepository` with `physical_key: String::new()`; READM-02 tests need a real `physical_key` and marker, so seed the repository via `lifecycle::ensure_repository` or write the marker with `marker::write_marker` into `worktrees_base()/<ws>/repository-<key>` and set `physical_key` to match. Bindings are seeded with `folder_workspace::record(Some(&persist::config_dir()), &path, &workspace.name, now)`.

Preview/real agreement test (`removal.rs` 2236-2265) — extend with the new fields:
```rust
let preview = preview_removal(&state, RemovalTarget::Repository(k)).expect("preview");
let (save, calls) = counting_save(&f.state_root());
let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save).expect("remove repository");
assert_eq!(preview.rows_removed, report.rows_removed);
assert_eq!(report.persistence, PersistenceOutcome::Saved);
assert_eq!(calls.get(), 1);
```
Byte-identity assertions for READM-05 follow `write_seed` (`removal.rs` 1466-1479): `fs::read(path)` before and after, `assert_eq!` on the `Vec<u8>`.

---

### `baude-core/src/lib.rs` (+ `pub mod suppression;`)

**Analog:** lines 8-27, alphabetical `pub mod` list. Insert `pub mod suppression;` between `pub mod session;` (23) and `pub mod testing;` (25).

---

### `baude/src/app.rs` (admission seam: lift + consult)

**Analog:** own admission functions.

**Lift point in `admit_repository`** (lines 2292-2300) — insert `let _ = baude_core::suppression::lift(path);` immediately after discovery succeeds, before `existing_key` (2360) so it precedes both `admit_existing_worktrees` calls (2393 and 2464):
```rust
pub fn admit_repository(&mut self, path: &Path) -> Result<Option<u64>> {
    let snapshot = git::discover_repository(path)?;
    // Admission is the reconciliation seam: resolve origin once here and
    // retain only its browser-addressable identity for gesture-time hints.
    let origin = git::origin_target(&snapshot.main_worktree).map(|target| RepositoryOrigin { ... });
```
Use `path` (what the user named), not `snapshot.main_worktree`, so `n` on the parent root does not lift a child (D-12).

**Lift point in `admit_standalone`** (lines 2005-2012) — after canonicalize, before the lookup:
```rust
pub fn admit_standalone(&mut self, path: &Path) -> Result<Option<u64>> {
    let canonical = path
        .canonicalize()
        .map_err(|error| anyhow::anyhow!("folder unavailable: {error}"))?;
    if !canonical.is_dir() {
        anyhow::bail!("not a directory: {}", canonical.display());
    }
    let persisted = PersistedPath::from_path(&canonical);
```

**Consult point in `admit_existing_worktrees`** (lines 2487-2507) — add `if baude_core::suppression::is_suppressed(&record.path) { continue; }` right after the `owned` skip:
```rust
for record in &snapshot.worktrees {
    if record.bare || record.detached || record.locked || record.prunable {
        continue;
    }
    let Some(branch_ref) = record.branch.as_deref() else { continue; };
    let path = PersistedPath::from_path(&record.path);
    let owned = self.repository_state.checkouts.iter().any(|checkout| checkout.observed_path == path)
        || self.repository_state.standalone_sessions.iter().any(|standalone| standalone.canonical_path == path);
    if owned {
        continue;
    }
    // <- suppression consult goes here
    let key = self.repository_state.allocate_checkout_key()?;
    let first_seen_order = self.repository_state.allocate_first_seen_order()?;
```
All three explicit routes converge on `open_repo_session_via(path, LocalAdmissionRoute::{LaunchDirectory, Open, CloneCompletion})` (lines 51-55, 1455, 5479-5510), which calls `admit_standalone` for known standalone paths and `admit_repository` otherwise, so the two lift sites cover `n`, `c` and launch-dir.

**TUI relaunch test pattern** (lines 9303-9331) — one `TestRedirect` (inside `admission_repo`, line 7873), shared `state_root`, second `App` calls `restore()`:
```rust
let state_file = baude_core::workspace::active().state_file("state");
let persisted = baude_core::persist::load_current_at(&state_root, &state_file).unwrap();
assert_eq!(persisted.state, app.repository_state);

let mut restarted = App::new(repo.clone());
restarted.remote = None;
restarted.persistence_root_for_test = Some(state_root.clone());
restarted.spawn_error_for_test = Some("pty unavailable after restart".into());
restarted.restore();

assert_eq!(restarted.repository_state, persisted.state);
std::fs::remove_dir_all(root).unwrap();
```
Between the two `App`s call `baude_core::removal::remove(&mut app.repository_state, target, &mut Vec::new(), |file| baude_core::persist::save_current_at_test(&state_root, &state_file, file, None, None))` (no TUI remove key exists until Phase 20). Then assert on `restarted.repository_state` rows and `baude_core::lifecycle::startup_recovery_program(&restarted.repository_state).is_empty()` (`lifecycle.rs` 417).

**Explicit-route test pattern** (lines 11040-11060):
```rust
let mut app = App::new(folder.clone());
app.remote = None;  // (or a dead RemotePoller as at 11047 when exercising the daemon fallthrough)
app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
app.persistence_root_for_test = Some(state_root.clone());

app.open_repo_session_via(folder.clone(), LocalAdmissionRoute::Open);
```
Use `admission_repo_cloned` (line 7935) for a cloned shape and `admission_repo` (7873) for `git init`.

---

## Shared Patterns

### Canonical path key
**Source:** `baude-core/src/breadcrumbs.rs` lines 114-121
**Apply to:** every key written to or removed from `folder-workspaces.json` and the suppression file, on both the record and clear sides
```rust
pub fn folder_key(launch_dir: &Path) -> String {
    let canonical = launch_dir
        .canonicalize()
        .unwrap_or_else(|_| launch_dir.to_path_buf());
    canonical.to_string_lossy().into_owned()
}
```
`PersistedPath::to_path_buf()` (`repository.rs` 48-51) turns a stored row path into the `&Path` this wants.

### Corrupt-tolerant load and locked read-merge-write
**Source:** `baude-core/src/breadcrumbs.rs` lines 229-283 (both `pub(crate)`)
**Apply to:** `suppression.rs` and `folder_workspace::clear`; never call `fs::write` on either file
```rust
pub(crate) fn load_json<T: Default + serde::de::DeserializeOwned>(path: &Path) -> (T, bool) {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(file) => (file, false),
            Err(_) => (T::default(), true),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (T::default(), false),
        Err(_) => (T::default(), true),
    }
}

pub(crate) fn locked_merge_write<T, F>(root: &Path, file: &str, update: F) -> std::io::Result<()>
where T: Default + serde::de::DeserializeOwned + Serialize, F: FnOnce(&mut T),
{
    std::fs::create_dir_all(root)?;
    let destination = root.join(file);
    let lock_path = root.join(format!(".{file}.lock"));
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&lock_path)?;
    lock.lock()?;
    let result = (|| {
        let (mut merged, _) = load_json::<T>(&destination);
        update(&mut merged);
        let bytes = serde_json::to_vec_pretty(&merged).map_err(|e| std::io::Error::other(e.to_string()))?;
        let temporary = root.join(format!(".{file}.tmp-{}", std::process::id()));
        // write, sync_all, rename, fsync dir
        Ok(())
    })();
    let _ = lock.unlock();
    result
}
```
`to_vec_pretty` over a `BTreeMap` keeps untouched entries byte-stable, which is what the READM-05 byte-identity assertions rely on.

### Best-effort stage that reports instead of failing
**Source:** `baude-core/src/breadcrumbs.rs` lines 195-222 (`FolderContext::flush`) and the D-14 decision
**Apply to:** `clear_stage` and every helper it calls
```rust
/// Prune entries that no longer resolve in durable state, then merge this
/// folder's entry back into the shared file. Best-effort: an I/O failure
/// keeps the entry dirty for a later retry and never surfaces as an error.
pub fn flush(&mut self, state: &RepositoryState, now_ms: u64) {
    // ...
    if save_folder(root, &self.file, &self.folder_key, &self.entry).is_ok() {
        self.dirty = false;
    }
}
```
`flush` is also the D-13 crumb-prune proof point; the existing test `flush_prunes_entries_missing_from_durable_state_and_coalesces` (`breadcrumbs.rs` 468-496) is the template: `FolderContext::load(Some(dir), &ws, launch_dir, 0)`, `record_checkout`, `maybe_flush(&state, t)`, reload and assert `!contains_checkout(removed)`.

### Fail-closed marker read
**Source:** `baude-core/src/marker.rs` lines 185-226
**Apply to:** marker clearing in `removal.rs`; never parse the marker JSON directly. Symlink markers come back `Invalid(InvalidReason::Symlink)` via `symlink_metadata`, so the delete never follows a link.

### Test redirect and fixture realism
**Source:** `baude-core/src/testing.rs` lines 128-138 (`TestRedirect::new`: worktrees base `<root>/data`, config dir `<root>/config`, home `<root>/home`), 334-360 (`ClonedTopology`, `cloned_topology` panics without an active redirect)
**Apply to:** every new test. Canonicalize fixture roots (macOS `/var` to `/private/var`) as `RemovalFixture::new` does, keep the redirect alive across teardown by declaring it before the root, and assert on files and `RepositoryState`, never on source text.

## No Analog Found

None. Every new or modified file has an in-crate analog of the same role and data flow. The one genuinely new behaviour, consulting a suppression set inside the inventory sweep, is a one-line `continue` in an existing loop whose skip conditions (lines 2488-2507) are the pattern.

## Metadata

**Analog search scope:** `baude-core/src/{removal,folder_workspace,breadcrumbs,marker,lifecycle,repository,persist,testing,workspace,lib}.rs`, `baude/src/app.rs`
**Files scanned:** 11
**Pattern extraction date:** 2026-11-03
