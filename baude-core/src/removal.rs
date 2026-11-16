//! Shared kill-then-remove lifecycle for every durable row kind.
//!
//! One removal operation serves the repository parent, the main checkout,
//! managed worktree children, unmanaged checkouts and standalone folders.
//! The stage order is fixed: kill every owned runtime in the set, then
//! remove managed worktree directories from disk, then persist the removed
//! row set with one atomic save. Nothing here deletes a main checkout, a
//! standalone folder or a local branch.
//!
//! This module owns the vocabulary (`RemovalTarget`, `RowKey`, `DiskAction`,
//! `PlannedRow`, `RemovalSet`, `RemovalReport`, `RemovalError`) and the two
//! pure functions every caller needs first: `select_rows` resolves a target
//! against `RepositoryState` and classifies each row's disk rule once, and
//! `preview_removal` turns that set into the report the real run will later
//! fill in. Preview never signals a process, never runs git and never writes
//! a file; its filesystem access is read-only: a `symlink_metadata` probe
//! that labels a managed directory as already removed when it is `NotFound`,
//! the same probe on a removed repository's `repository-<key>` directory,
//! and a read of the suppression store, so it can name every marker,
//! directory and entry the clearing stage will touch.
//!
//! Callers hold `lifecycle::RepositoryReservations` for every repository in
//! the set from before `select_rows` until `remove` has returned, including
//! after the atomic save, so no concurrent reconcile or launch can re-admit
//! a row the removal is about to drop. The hold and the post-success
//! `Session` ownership rules are defined with `remove` in plan 04.
//!
//! The two existing protocols in `lifecycle.rs` and `app.rs` for closing a
//! runtime and for tombstoned removal are intentionally not reused: they
//! persist twice (tombstone, then drop) and are specific to one row kind,
//! while this path persists the whole removed set once.

use std::fmt;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::marker::{DirectoryOutcome, MarkerOutcome};
use crate::pty::Pty;
use crate::repository::{
    CheckoutKey, CheckoutRole, OwnedRuntime, RemovedRows, RepositoryKey, RepositoryState,
    SavedCheckout, SavedRepository, ShellOwnership, StandaloneKey, ValidationError,
};

/// What the caller asked to remove.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalTarget {
    Repository(RepositoryKey),
    Checkout(CheckoutKey),
    Standalone(StandaloneKey),
}

/// One durable row named by its key.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKey {
    Repository(RepositoryKey),
    Checkout(CheckoutKey),
    Standalone(StandaloneKey),
}

/// The disk rule computed once per row and reused by preview and by the
/// real run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskAction {
    /// Main checkouts, unmanaged checkouts, standalone folders and the
    /// repository parent row: the directory is never touched.
    KeepDirectory,
    /// A managed worktree child: removed with the force path after the kill.
    RemoveManagedWorktree,
    /// A managed worktree child whose directory is already gone. Reporting
    /// label only; the real run still prunes stale worktree administration.
    AlreadyRemoved,
}

/// One row of a planned removal set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlannedRow {
    pub row: RowKey,
    pub path: Option<PathBuf>,
    pub disk: DiskAction,
    /// The whole persisted runtime so the kill stage can name every process.
    pub runtime: Option<OwnedRuntime>,
    /// The owning repository's main worktree; `None` for standalone rows.
    pub main_worktree: Option<PathBuf>,
    /// The owning repository's common directory; `None` for standalone rows.
    pub common_dir: Option<PathBuf>,
}

/// The rows a target resolves to, children first and the parent last.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemovalSet {
    pub target: RemovalTarget,
    pub rows: Vec<PlannedRow>,
}

impl RemovalSet {
    pub fn checkout_keys(&self) -> Vec<CheckoutKey> {
        self.rows
            .iter()
            .filter_map(|row| match row.row {
                RowKey::Checkout(key) => Some(key),
                _ => None,
            })
            .collect()
    }

    pub fn repository_keys(&self) -> Vec<RepositoryKey> {
        self.rows
            .iter()
            .filter_map(|row| match row.row {
                RowKey::Repository(key) => Some(key),
                _ => None,
            })
            .collect()
    }

    pub fn standalone_keys(&self) -> Vec<StandaloneKey> {
        self.rows
            .iter()
            .filter_map(|row| match row.row {
                RowKey::Standalone(key) => Some(key),
                _ => None,
            })
            .collect()
    }
}

/// One runtime the kill stage will stop (preview) or stopped (real run).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KilledRuntime {
    pub row: RowKey,
    pub agent_pid: u32,
    pub shell_pid: Option<u32>,
}

/// Whether the removed row set reached the state file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum PersistenceOutcome {
    Planned,
    Saved,
    SavedDegraded { detail: String },
}

/// What the clearing stage forgets on behalf of a removed row.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearingKind {
    /// The folder-to-workspace entry in `folder-workspaces.json`.
    Binding,
    Marker,
    RepositoryDir,
    SuppressionAdded,
    SuppressionDropped,
}

/// One thing the clearing stage will forget (preview) or forgot (real run).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClearedItem {
    pub kind: ClearingKind,
    pub path: PathBuf,
}

/// One planned clearing the real run could not perform. Never fails the
/// removal: the row is already saved out of the state file by the time the
/// clearing stage runs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClearingFailure {
    pub kind: ClearingKind,
    pub path: PathBuf,
    pub error: String,
}

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
    /// Rows whose PTY reader thread was still parked behind a process
    /// outside the recorded group when the handle was dropped. Always empty
    /// in a preview; filled only by the real run.
    pub readers_parked: Vec<RowKey>,
    pub persistence: PersistenceOutcome,
    /// In a preview, every clearing the real run will attempt; in the real
    /// run, the ones that removed something.
    pub cleared: Vec<ClearedItem>,
    /// Clearings the real run attempted and could not perform. Always empty
    /// in a preview.
    pub clearing_failures: Vec<ClearingFailure>,
}

/// Why a managed worktree could not be removed from disk. Owned here so this
/// module compiles without the git helper; the helper's error maps into it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskRemovalFailureKind {
    OutsideManagedRoot,
    Symlink,
    IdentityMismatch,
    MainWorktree,
    NotRegistered,
    ForeignRepository,
    Submodules,
    CommandStart,
    Fallback,
    Uninspectable,
    Marker,
    Postcondition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiskRemovalFailure {
    pub kind: DiskRemovalFailureKind,
    pub detail: String,
}

impl fmt::Display for DiskRemovalFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

/// Every way a removal can stop. Each arm names the row or target it is
/// about, so a parent removal that fails on a child names the child.
#[derive(Debug)]
pub enum RemovalError {
    UnknownTarget(RemovalTarget),
    RuntimeHandleMissing(RowKey),
    RuntimeIdentityMismatch {
        row: RowKey,
        detail: String,
    },
    RuntimeSurvived {
        row: RowKey,
        error: Box<crate::session::SessionTeardownError>,
    },
    DiskRemoval {
        row: RowKey,
        path: PathBuf,
        error: DiskRemovalFailure,
    },
    Validation(ValidationError),
    Persist {
        error: crate::persist::SaveError,
    },
}

impl fmt::Display for RemovalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTarget(target) => {
                write!(f, "removal target {target:?} is not in the saved state")
            }
            Self::RuntimeHandleMissing(row) => {
                write!(
                    f,
                    "row {row:?} has a persisted runtime but no live handle was supplied"
                )
            }
            Self::RuntimeIdentityMismatch { row, detail } => {
                write!(f, "row {row:?}: supplied runtime handle does not match the persisted runtime: {detail}")
            }
            Self::RuntimeSurvived { row, error } => {
                write!(f, "row {row:?}: runtime survived the kill stage: {error}")
            }
            Self::DiskRemoval { row, path, error } => {
                write!(
                    f,
                    "row {row:?}: could not remove {}: {error}",
                    path.display()
                )
            }
            Self::Validation(error) => write!(f, "removed state failed validation: {error}"),
            Self::Persist { error } => write!(f, "could not persist the removed row set: {error}"),
        }
    }
}

impl std::error::Error for RemovalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RuntimeSurvived { error, .. } => Some(error.as_ref()),
            Self::Persist { error } => Some(error),
            Self::Validation(error) => Some(error),
            _ => None,
        }
    }
}

/// Resolve `target` against `state` and classify every row's disk rule.
///
/// A repository target fans out to every checkout whose `repository_key`
/// matches, in state order, followed by the parent row. Classification is
/// pure (role and `managed_by_baude` only); the one probe is a
/// `symlink_metadata` on rows classified `RemoveManagedWorktree`, which
/// downgrades to `AlreadyRemoved` solely on `NotFound`.
pub fn select_rows(
    state: &RepositoryState,
    target: RemovalTarget,
) -> Result<RemovalSet, RemovalError> {
    let rows = match target {
        RemovalTarget::Repository(key) => {
            let parent = find_repository(state, key).ok_or(RemovalError::UnknownTarget(target))?;
            let mut rows: Vec<PlannedRow> = state
                .checkouts
                .iter()
                .filter(|checkout| checkout.repository_key == key)
                .map(|checkout| checkout_row(checkout, parent))
                .collect();
            rows.push(PlannedRow {
                row: RowKey::Repository(key),
                path: None,
                disk: DiskAction::KeepDirectory,
                runtime: None,
                main_worktree: Some(parent.observed_main_worktree.to_path_buf()),
                common_dir: Some(parent.observed_common_dir.to_path_buf()),
            });
            rows
        }
        RemovalTarget::Checkout(key) => {
            let checkout = state
                .checkouts
                .iter()
                .find(|checkout| checkout.key == key)
                .ok_or(RemovalError::UnknownTarget(target))?;
            let parent = find_repository(state, checkout.repository_key)
                .ok_or(RemovalError::UnknownTarget(target))?;
            vec![checkout_row(checkout, parent)]
        }
        RemovalTarget::Standalone(key) => {
            let standalone = state
                .standalone_sessions
                .iter()
                .find(|standalone| standalone.key == key)
                .ok_or(RemovalError::UnknownTarget(target))?;
            vec![PlannedRow {
                row: RowKey::Standalone(key),
                path: Some(standalone.canonical_path.to_path_buf()),
                disk: DiskAction::KeepDirectory,
                runtime: standalone.owned_runtime().cloned(),
                main_worktree: None,
                common_dir: None,
            }]
        }
    };
    Ok(RemovalSet { target, rows })
}

/// Describe what removing `target` would do without doing any of it.
pub fn preview_removal(
    state: &RepositoryState,
    target: RemovalTarget,
) -> Result<RemovalReport, RemovalError> {
    let set = select_rows(state, target)?;
    let planned = planned_clearings(state, &set);
    Ok(report_from_set(&set, planned, None))
}

/// One live runtime handed to `remove`, owned for the call. The caller moves
/// the `Session` out of its registry; on `Ok` the handles for removed rows
/// are consumed and dropped here, on `Err` every handle stays in the Vec.
pub struct RowRuntime {
    pub row: RowKey,
    pub session: crate::session::Session,
}

impl From<crate::git::ForcedRemovalError> for DiskRemovalFailure {
    fn from(error: crate::git::ForcedRemovalError) -> Self {
        use crate::git::ForcedRemovalError as E;
        let kind = match &error {
            E::OutsideManagedRoot { .. } => DiskRemovalFailureKind::OutsideManagedRoot,
            E::Symlink { .. } => DiskRemovalFailureKind::Symlink,
            E::IdentityMismatch { .. } => DiskRemovalFailureKind::IdentityMismatch,
            E::MainWorktree { .. } => DiskRemovalFailureKind::MainWorktree,
            E::NotRegistered { .. } => DiskRemovalFailureKind::NotRegistered,
            E::ForeignRepository { .. } => DiskRemovalFailureKind::ForeignRepository,
            E::Submodules { .. } => DiskRemovalFailureKind::Submodules,
            E::CommandStart(_) => DiskRemovalFailureKind::CommandStart,
            E::Fallback { .. } => DiskRemovalFailureKind::Fallback,
            E::Uninspectable { .. } => DiskRemovalFailureKind::Uninspectable,
            E::Marker { .. } => DiskRemovalFailureKind::Marker,
            E::Postcondition { .. } => DiskRemovalFailureKind::Postcondition,
        };
        Self {
            kind,
            detail: error.to_string(),
        }
    }
}

/// Remove `target` from the saved state: stop its runtimes, remove the
/// managed worktree directories it owns, then save the whole removed row set
/// once.
///
/// Stages, in this order: `select_rows`, `preflight` (every supplied handle
/// must match the persisted runtime of its row in full, one handle per row,
/// no handle for a row without a persisted runtime), candidate construction
/// with `validate_for_save` so a validation refusal never follows an
/// irreversible effect, the kill stage (one `Session::kill_and_wait` per live
/// row in `set.rows` order, aborting on the first survivor), the disk stage
/// (forced removal of every managed worktree directory, pruning the git
/// administrative entry of one that is already gone), and one `save` of the
/// candidate. The in-memory state is replaced and the handles for removed
/// rows are consumed only after that save returns `Ok` or reports that the
/// replacement file was committed.
///
/// The kill unit is the whole `Session`: the agent and, when the persisted
/// ownership says so, the shell. Callers hold `lifecycle::RepositoryReservations`
/// for every repository in the set from before `select_rows` until this
/// function has returned, including after the save, so a concurrent reopen
/// cannot observe the half-committed window between the disk stage and the
/// save; a crash in that window is covered by the startup reconcile, which
/// re-selects a deleted directory as `DiskAction::AlreadyRemoved`.
///
/// On `Ok` the handles whose rows were removed have been drained from
/// `runtimes` and dropped here, so the caller must drop its own registry
/// entries for those rows. `RemovalReport::readers_parked` names the rows
/// whose PTY reader thread was still blocked behind a process outside the
/// recorded group when the handle dropped; that thread exits when the holder
/// closes the terminal. On `Err` the caller keeps every handle and `*state`
/// is untouched; after a parent all-or-nothing abort some of those handles
/// may already be dead, which the existing exit detection reconciles.
///
/// This is deliberately not the close protocol or the tombstoned-removal
/// protocol the TUI uses: a kill failure leaves the row exactly as it was,
/// with no pending lifecycle written.
///
/// `remove` is the composition of three public stages, run back to back:
/// [`prepare`] (selection, planned clearings, preflight and candidate
/// validation, with no runtime, disk or file touched), [`stop_and_delete`]
/// (the kill and disk stages, reading no `RepositoryState`) and [`commit`]
/// (the one save and the clearing stage). A caller that runs them apart, for
/// instance to release a lock during the slow kill and delete, must hold an
/// exclusion across the window that keeps every row of the set from being
/// added to, reopened or removed by anyone else until `commit` returns.
pub fn remove(
    state: &mut RepositoryState,
    target: RemovalTarget,
    runtimes: &mut Vec<RowRuntime>,
    save: impl FnOnce(&crate::persist::StateFile) -> Result<(), crate::persist::SaveError>,
) -> Result<RemovalReport, RemovalError> {
    let prepared = prepare(state, target, runtimes)?;
    let effects = stop_and_delete(&prepared, runtimes)?;
    commit(state, prepared, effects, runtimes, save)
}

/// What [`prepare`] resolved: the selected rows and every clearing the
/// clearing stage will attempt, both derived from the state `prepare` read.
/// It deliberately carries no save candidate: [`commit`] builds that from
/// the state it is handed, so a change made to other rows between the two
/// stages is saved rather than reverted.
pub struct PreparedRemoval {
    set: RemovalSet,
    planned: Vec<ClearedItem>,
}

/// What [`stop_and_delete`] did: the runtimes it stopped and the paths it
/// deleted, kept or found already gone.
pub struct StageEffects {
    killed: Vec<KilledRuntime>,
    disk: DiskResults,
}

/// Resolve `target` and refuse everything that can be refused before an
/// irreversible effect: `select_rows`, the planned clearings, `preflight`,
/// then a candidate with the set's rows removed run through
/// `validate_for_save`. Touches no runtime, deletes nothing and writes no
/// file; its only filesystem access is the read-only probes of
/// `select_rows`, `planned_clearings` and `preflight`.
pub fn prepare(
    state: &RepositoryState,
    target: RemovalTarget,
    runtimes: &[RowRuntime],
) -> Result<PreparedRemoval, RemovalError> {
    let set = select_rows(state, target)?;
    let planned = planned_clearings(state, &set);
    preflight(&set, runtimes)?;
    candidate_without(state, &set)?;
    Ok(PreparedRemoval { set, planned })
}

/// A clone of `state` with the set's rows removed, refused unless it passes
/// `validate_for_save`, together with the rows the clone dropped.
fn candidate_without(
    state: &RepositoryState,
    set: &RemovalSet,
) -> Result<(RepositoryState, RemovedRows), RemovalError> {
    let mut candidate = state.clone();
    let removed = candidate.remove_rows(
        &set.repository_keys(),
        &set.checkout_keys(),
        &set.standalone_keys(),
    );
    candidate
        .validate_for_save()
        .map_err(RemovalError::Validation)?;
    Ok((candidate, removed))
}

/// The kill stage, then the disk stage, for the prepared set. Reads no
/// `RepositoryState`, so it may run without the lock that guards the state,
/// provided the caller's exclusion keeps the set's rows unchanged until
/// [`commit`]. An `Err` names the row it stopped on; the state and the file
/// are untouched and every handle stays in `runtimes`.
pub fn stop_and_delete(
    prepared: &PreparedRemoval,
    runtimes: &mut [RowRuntime],
) -> Result<StageEffects, RemovalError> {
    let killed = kill_stage(&prepared.set, runtimes)?;
    let disk = disk_stage(&prepared.set)?;
    Ok(StageEffects { killed, disk })
}

/// Save the removal once and run the clearing stage. The save candidate is
/// rebuilt here from `*state` as it is now, minus the prepared rows, and
/// validated again, so a row another caller admitted or changed since
/// [`prepare`] is saved rather than reverted to the state `prepare` read.
/// `*state` is replaced and the removed rows' handles are drained from
/// `runtimes` only after the save returns `Ok` or reports a committed
/// replacement; on every `Err` `*state` and `runtimes` are untouched.
pub fn commit(
    state: &mut RepositoryState,
    prepared: PreparedRemoval,
    effects: StageEffects,
    runtimes: &mut Vec<RowRuntime>,
    save: impl FnOnce(&crate::persist::StateFile) -> Result<(), crate::persist::SaveError>,
) -> Result<RemovalReport, RemovalError> {
    let PreparedRemoval { set, planned } = prepared;
    let (candidate, removed) = candidate_without(state, &set)?;
    let (persistence, readers_parked) = persist_stage(state, candidate, &set, runtimes, save)?;
    let clearing = clear_stage(&removed, state, &planned);
    Ok(report_from_set(
        &set,
        planned,
        Some(StageResults {
            killed: effects.killed,
            disk: effects.disk,
            readers_parked,
            persistence,
            clearing,
        }),
    ))
}

/// What the real run's stages produced, handed to `report_from_set` in place
/// of the preview's derivation from the planned set.
struct StageResults {
    killed: Vec<KilledRuntime>,
    disk: DiskResults,
    readers_parked: Vec<RowKey>,
    persistence: PersistenceOutcome,
    clearing: ClearingResults,
}

/// What the clearing stage removed and what it could not.
struct ClearingResults {
    cleared: Vec<ClearedItem>,
    failures: Vec<ClearingFailure>,
}

/// Everything the clearing stage will attempt for `set`, derived from the
/// state before the removal so the preview and the real run name the same
/// items, in the stage's own order: bindings, then markers and managed
/// directories, then suppression drops, then suppression adds.
///
/// One `Binding` per row: a repository row names its main worktree, a
/// checkout row its observed path, a standalone row its canonical path. A
/// folder two rows share, such as a main checkout and its repository, is
/// listed once, because the stage clears it once. One `Marker` and one
/// `RepositoryDir` per repository row whose stored directory exists (D-04,
/// D-05); a refused key is still named as a `Marker`, because the real run
/// will report it. One `SuppressionDropped` per repository row that has
/// children in the store, and one `SuppressionAdded` per checkout row whose
/// parent repository survives the removal (D-10).
fn planned_clearings(state: &RepositoryState, set: &RemovalSet) -> Vec<ClearedItem> {
    let mut planned: Vec<ClearedItem> = Vec::new();
    plan_bindings(state, set, &mut planned);
    let repository_keys = set.repository_keys();
    let repositories: Vec<&SavedRepository> = repository_keys
        .iter()
        .filter_map(|key| find_repository(state, *key))
        .collect();
    plan_markers(&repositories, &mut planned);
    plan_suppression(state, set, &repository_keys, &repositories, &mut planned);
    planned
}

fn plan_bindings(state: &RepositoryState, set: &RemovalSet, planned: &mut Vec<ClearedItem>) {
    for row in &set.rows {
        let path = match row.row {
            RowKey::Repository(key) => row.main_worktree.clone().or_else(|| {
                find_repository(state, key)
                    .map(|repository| repository.observed_main_worktree.to_path_buf())
            }),
            RowKey::Checkout(_) | RowKey::Standalone(_) => row.path.clone(),
        };
        let Some(path) = path else { continue };
        let item = ClearedItem {
            kind: ClearingKind::Binding,
            path,
        };
        if !planned.contains(&item) {
            planned.push(item);
        }
    }
}

/// A `Marker` and `RepositoryDir` pair per repository row whose stored
/// directory exists; a refused key is named as a `Marker` alone, because the
/// real run will report it under that name.
fn plan_markers(repositories: &[&SavedRepository], planned: &mut Vec<ClearedItem>) {
    for repository in repositories {
        match repository_dir(repository) {
            Ok(dir) if std::fs::symlink_metadata(&dir).is_ok() => {
                planned.push(ClearedItem {
                    kind: ClearingKind::Marker,
                    path: dir.clone(),
                });
                planned.push(ClearedItem {
                    kind: ClearingKind::RepositoryDir,
                    path: dir,
                });
            }
            Ok(_) | Err(RepositoryDirRefusal::LegacyKey) => {}
            Err(RepositoryDirRefusal::NotSingleComponent) => planned.push(ClearedItem {
                kind: ClearingKind::Marker,
                path: refused_key_path(repository),
            }),
        }
    }
}

/// A `SuppressionDropped` per repository row with children in the store,
/// then a `SuppressionAdded` per checkout row whose parent survives. The
/// store is read once, and only when a repository row is in the set.
fn plan_suppression(
    state: &RepositoryState,
    set: &RemovalSet,
    repository_keys: &[RepositoryKey],
    repositories: &[&SavedRepository],
    planned: &mut Vec<ClearedItem>,
) {
    if !repositories.is_empty() {
        let store = crate::breadcrumbs::load_json::<crate::suppression::SuppressionFile>(
            &crate::suppression::file_path(&crate::persist::config_dir()),
        )
        .0;
        for repository in repositories {
            let common_dir = repository.observed_common_dir.to_path_buf();
            let common_key = crate::breadcrumbs::folder_key(&common_dir);
            if store
                .entries
                .values()
                .any(|entry| entry.repository_common_dir == common_key)
            {
                planned.push(ClearedItem {
                    kind: ClearingKind::SuppressionDropped,
                    path: common_dir,
                });
            }
        }
    }
    for row in &set.rows {
        let RowKey::Checkout(key) = row.row else {
            continue;
        };
        let parent_removed = state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == key)
            .is_none_or(|checkout| repository_keys.contains(&checkout.repository_key));
        if parent_removed {
            continue;
        }
        let Some(path) = row.path.clone() else {
            continue;
        };
        planned.push(ClearedItem {
            kind: ClearingKind::SuppressionAdded,
            path,
        });
    }
}

/// Why a repository row has no managed directory for the stage to clear.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepositoryDirRefusal {
    /// A row saved before digests carries an empty `physical_key`; it never
    /// owned a `repository-<key>` directory, so the stage has nothing to do.
    LegacyKey,
    /// The key holds a path separator or is `.` or `..`; joined under the
    /// managed root it could name a path outside it, so it is reported and
    /// never joined (T-19-10).
    NotSingleComponent,
}

/// The managed directory a repository row owns:
/// `worktrees_base()/<workspace>/repository-<physical_key>`, built from the
/// stored key and never from a recomputed digest of the common dir, so a
/// suffixed collision directory is the one cleared (D-05).
fn repository_dir(repo: &SavedRepository) -> Result<PathBuf, RepositoryDirRefusal> {
    let key = repo.physical_key.as_str();
    if key.is_empty() {
        return Err(RepositoryDirRefusal::LegacyKey);
    }
    if key == "." || key == ".." || key.chars().any(|c| c == '/' || c == '\\') {
        return Err(RepositoryDirRefusal::NotSingleComponent);
    }
    Ok(crate::git::worktrees_base()
        .join(crate::workspace::active().name.as_str())
        .join(format!("repository-{key}")))
}

/// The path a refused key is reported under: the bare directory name, never
/// joined under the managed root.
fn refused_key_path(repo: &SavedRepository) -> PathBuf {
    PathBuf::from(format!("repository-{}", repo.physical_key))
}

/// Unix milliseconds for a suppression entry's `suppressed_ms`.
fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// The fourth stage: forget the advisory records that would re-admit a row
/// the save just committed. Runs after `persist_stage`, so it never returns
/// an error and never touches the state file; a refusal or I/O failure on
/// one item is reported as a `ClearingFailure` and the remaining items are
/// still tried (D-14). Order: bindings, markers and managed directories,
/// suppression drops, suppression adds. `removed` is what the save dropped,
/// so markers and entries are keyed by the rows' stored fields; `state` is
/// the surviving state, where a removed checkout's parent is looked up.
fn clear_stage(
    removed: &RemovedRows,
    state: &RepositoryState,
    planned: &[ClearedItem],
) -> ClearingResults {
    let mut results = ClearingResults {
        cleared: Vec::new(),
        failures: Vec::new(),
    };
    let config_dir = crate::persist::config_dir();
    clear_bindings(&config_dir, planned, &mut results);
    clear_markers(removed, &mut results);
    update_suppression(&config_dir, removed, state, &mut results);
    results
}

fn clear_bindings(config_dir: &Path, planned: &[ClearedItem], results: &mut ClearingResults) {
    for item in planned
        .iter()
        .filter(|item| item.kind == ClearingKind::Binding)
    {
        match crate::folder_workspace::clear(Some(config_dir), &item.path) {
            Ok(true) => results.cleared.push(item.clone()),
            Ok(false) => {}
            Err(error) => results.failures.push(ClearingFailure {
                kind: item.kind,
                path: item.path.clone(),
                error: error.to_string(),
            }),
        }
    }
}

/// One `marker::clear_owned` per removed repository row, keyed by the row's
/// stored `physical_key` and `observed_common_dir` (D-04, D-05). Nothing
/// recursive runs: a directory that still holds entries is reported and
/// left in place (D-07).
fn clear_markers(removed: &RemovedRows, results: &mut ClearingResults) {
    for repository in &removed.repositories {
        let dir = match repository_dir(repository) {
            Ok(dir) => dir,
            Err(RepositoryDirRefusal::LegacyKey) => continue,
            Err(RepositoryDirRefusal::NotSingleComponent) => {
                results.failures.push(ClearingFailure {
                    kind: ClearingKind::Marker,
                    path: refused_key_path(repository),
                    error: "physical key is not a single path component".to_string(),
                });
                continue;
            }
        };
        let clear =
            match crate::marker::clear_owned(&dir, repository.observed_common_dir.as_bytes()) {
                Ok(clear) => clear,
                Err(error) => {
                    results.failures.push(ClearingFailure {
                        kind: ClearingKind::Marker,
                        path: dir,
                        error: error.to_string(),
                    });
                    continue;
                }
            };
        match clear.marker {
            MarkerOutcome::Removed => results.cleared.push(ClearedItem {
                kind: ClearingKind::Marker,
                path: dir.clone(),
            }),
            MarkerOutcome::Missing => {}
            MarkerOutcome::Refused(refusal) => results.failures.push(ClearingFailure {
                kind: ClearingKind::Marker,
                path: dir.clone(),
                error: format!("marker refused: {refusal:?}"),
            }),
        }
        match clear.directory {
            DirectoryOutcome::Removed => results.cleared.push(ClearedItem {
                kind: ClearingKind::RepositoryDir,
                path: dir,
            }),
            DirectoryOutcome::NotEmpty => results.failures.push(ClearingFailure {
                kind: ClearingKind::RepositoryDir,
                path: dir,
                error: "directory not empty, left in place".to_string(),
            }),
            DirectoryOutcome::Missing | DirectoryOutcome::Untouched => {}
        }
    }
}

/// Garbage-collect the entries of every removed repository's children, then
/// record every removed checkout whose parent survives (D-10). Repository
/// parents and standalone rows write no entry of their own.
fn update_suppression(
    config_dir: &Path,
    removed: &RemovedRows,
    state: &RepositoryState,
    results: &mut ClearingResults,
) {
    for repository in &removed.repositories {
        let common_dir = repository.observed_common_dir.to_path_buf();
        match crate::suppression::drop_children(config_dir, &common_dir) {
            Ok(0) => {}
            Ok(_) => results.cleared.push(ClearedItem {
                kind: ClearingKind::SuppressionDropped,
                path: common_dir,
            }),
            Err(error) => results.failures.push(ClearingFailure {
                kind: ClearingKind::SuppressionDropped,
                path: common_dir,
                error: error.to_string(),
            }),
        }
    }
    let now_ms = unix_millis();
    for checkout in &removed.checkouts {
        if removed
            .repositories
            .iter()
            .any(|repository| repository.key == checkout.repository_key)
        {
            continue;
        }
        let path = checkout.observed_path.to_path_buf();
        let Some(parent) = find_repository(state, checkout.repository_key) else {
            results.failures.push(ClearingFailure {
                kind: ClearingKind::SuppressionAdded,
                path,
                error: "surviving parent repository row not found".to_string(),
            });
            continue;
        };
        let common_dir = parent.observed_common_dir.to_path_buf();
        // A removed primary is remembered as such, so the next admission of
        // this repository through a sibling derives no new one.
        let written = if checkout.role == CheckoutRole::PrimaryDefault {
            crate::suppression::suppress_primary(config_dir, &path, &common_dir, now_ms)
        } else {
            crate::suppression::suppress(config_dir, &path, &common_dir, now_ms)
        };
        match written {
            Ok(()) => results.cleared.push(ClearedItem {
                kind: ClearingKind::SuppressionAdded,
                path,
            }),
            Err(error) => results.failures.push(ClearingFailure {
                kind: ClearingKind::SuppressionAdded,
                path,
                error: error.to_string(),
            }),
        }
    }
}

/// A row whose disk stage will need git must have its owning repository's
/// common dir on disk now, so a repository deleted outside baude is refused
/// before the kill stage instead of after every runtime is already dead.
fn probe_owner_common_dir(row: &PlannedRow) -> Result<(), RemovalError> {
    if row.disk == DiskAction::KeepDirectory {
        return Ok(());
    }
    let Some(path) = &row.path else {
        return Ok(());
    };
    let common_dir = owner_common_dir(row, path)?;
    match std::fs::symlink_metadata(common_dir) {
        Ok(_) => Ok(()),
        Err(error) => Err(disk_error(
            row,
            path,
            DiskRemovalFailure {
                kind: DiskRemovalFailureKind::Uninspectable,
                detail: format!(
                    "owning repository common dir {} is not on disk: {error}",
                    common_dir.display()
                ),
            },
        )),
    }
}

/// Verify every handle against the persisted runtime of its row before any
/// signal is sent: exactly one handle per live row, full `ProcessIdentity`
/// equality for the agent, exact `ShellOwnership` agreement, and no handle
/// at all for a row without a persisted runtime.
fn preflight(set: &RemovalSet, runtimes: &[RowRuntime]) -> Result<(), RemovalError> {
    for row in &set.rows {
        probe_owner_common_dir(row)?;
        let handles: Vec<&RowRuntime> = runtimes
            .iter()
            .filter(|handle| handle.row == row.row)
            .collect();
        if handles.len() > 1 {
            return Err(RemovalError::RuntimeIdentityMismatch {
                row: row.row,
                detail: format!("duplicate handles supplied: {} for one row", handles.len()),
            });
        }
        let Some(runtime) = &row.runtime else {
            if !handles.is_empty() {
                return Err(RemovalError::RuntimeIdentityMismatch {
                    row: row.row,
                    detail: "a handle was supplied for a row with no persisted runtime".to_string(),
                });
            }
            continue;
        };
        let Some(handle) = handles.first() else {
            return Err(RemovalError::RuntimeHandleMissing(row.row));
        };
        let found = handle.session.claude.process_identity();
        if found != &runtime.agent {
            return Err(RemovalError::RuntimeIdentityMismatch {
                row: row.row,
                detail: format!(
                    "agent identity differs: expected pid {} start_time {} pgid {} sid {}, found pid {} start_time {} pgid {} sid {}",
                    runtime.agent.pid,
                    runtime.agent.start_time,
                    runtime.agent.process_group,
                    runtime.agent.session,
                    found.pid,
                    found.start_time,
                    found.process_group,
                    found.session,
                ),
            });
        }
        let live_shell = handle.session.shell.as_ref().map(Pty::process_identity);
        match (&runtime.shell, live_shell) {
            (ShellOwnership::Closed, None) => {}
            (ShellOwnership::Closed, Some(found)) => {
                return Err(RemovalError::RuntimeIdentityMismatch {
                    row: row.row,
                    detail: format!(
                        "persisted shell ownership is closed but the handle carries a live shell pid {}",
                        found.pid
                    ),
                });
            }
            (ShellOwnership::Owned(expected), Some(found)) if found == expected => {}
            (ShellOwnership::Owned(expected), found) => {
                return Err(RemovalError::RuntimeIdentityMismatch {
                    row: row.row,
                    detail: format!(
                        "shell identity differs: expected pid {} start_time {}, found {}",
                        expected.pid,
                        expected.start_time,
                        found.map_or_else(
                            || "no shell".to_string(),
                            |f| format!("pid {} start_time {}", f.pid, f.start_time)
                        ),
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Stop every live row in `set.rows` order through its verified handle,
/// aborting on the first survivor so disk and state stay untouched.
fn kill_stage(
    set: &RemovalSet,
    runtimes: &mut [RowRuntime],
) -> Result<Vec<KilledRuntime>, RemovalError> {
    let mut killed = Vec::new();
    for row in &set.rows {
        if row.runtime.is_none() {
            continue;
        }
        let handle = runtimes
            .iter_mut()
            .find(|handle| handle.row == row.row)
            .ok_or(RemovalError::RuntimeHandleMissing(row.row))?;
        handle
            .session
            .kill_and_wait()
            .map_err(|error| RemovalError::RuntimeSurvived {
                row: row.row,
                error: Box::new(error),
            })?;
        killed.push(KilledRuntime {
            row: row.row,
            agent_pid: handle.session.claude.process_identity().pid,
            shell_pid: handle
                .session
                .shell
                .as_ref()
                .map(|shell| shell.process_identity().pid),
        });
    }
    Ok(killed)
}

/// Paths the disk stage deleted, kept and found already gone.
struct DiskResults {
    deleted: Vec<PathBuf>,
    kept: Vec<PathBuf>,
    already_removed: Vec<PathBuf>,
}

/// Apply each row's `DiskAction`. A managed directory that is already gone
/// still goes through the forced helper so its own administrative entry is
/// dropped. The report follows what the helper did, not what `select_rows`
/// planned: a directory that reappeared since the plan and was deleted is
/// reported as deleted, and one that reappeared unregistered and without a
/// removal marker is a `NotRegistered` failure rather than a silent drop,
/// since a directory now sits at the row's path. `NotRegistered` with the
/// path still absent means there was nothing left to clean.
fn disk_stage(set: &RemovalSet) -> Result<DiskResults, RemovalError> {
    let mut results = DiskResults {
        deleted: Vec::new(),
        kept: Vec::new(),
        already_removed: Vec::new(),
    };
    for row in &set.rows {
        let Some(path) = &row.path else {
            continue;
        };
        match row.disk {
            DiskAction::KeepDirectory => results.kept.push(path.clone()),
            DiskAction::RemoveManagedWorktree | DiskAction::AlreadyRemoved => {
                let common_dir = owner_common_dir(row, path)?;
                match crate::git::remove_managed_worktree_forced(common_dir, path) {
                    Ok(removal) if removal.deleted_directory => results.deleted.push(path.clone()),
                    Ok(_) => results.already_removed.push(path.clone()),
                    Err(crate::git::ForcedRemovalError::NotRegistered { .. })
                        if matches!(
                            std::fs::symlink_metadata(path),
                            Err(ref error) if error.kind() == ErrorKind::NotFound
                        ) =>
                    {
                        results.already_removed.push(path.clone())
                    }
                    Err(error) => return Err(disk_error(row, path, error.into())),
                }
            }
        }
    }
    Ok(results)
}

/// The owning repository's common directory, which `select_rows` records on
/// every checkout row. The main worktree is deliberately not consulted: the
/// forced helper runs against the common dir so a vanished main checkout
/// does not strand its managed children.
fn owner_common_dir<'a>(row: &'a PlannedRow, path: &Path) -> Result<&'a Path, RemovalError> {
    match &row.common_dir {
        Some(common_dir) => Ok(common_dir),
        None => Err(disk_error(
            row,
            path,
            DiskRemovalFailure {
                kind: DiskRemovalFailureKind::NotRegistered,
                detail: "managed row carries no owning repository".to_string(),
            },
        )),
    }
}

fn disk_error(row: &PlannedRow, path: &Path, error: DiskRemovalFailure) -> RemovalError {
    RemovalError::DiskRemoval {
        row: row.row,
        path: path.to_path_buf(),
        error,
    }
}

/// Save the candidate once. Commits `*state` and consumes the removed rows'
/// handles on `Ok` or on a committed replacement; otherwise leaves both
/// untouched. The parked-reader sweep runs while the handles are still
/// owned, so a reader left behind an out-of-group slave holder is reported
/// rather than detached silently.
fn persist_stage(
    state: &mut RepositoryState,
    candidate: RepositoryState,
    set: &RemovalSet,
    runtimes: &mut Vec<RowRuntime>,
    save: impl FnOnce(&crate::persist::StateFile) -> Result<(), crate::persist::SaveError>,
) -> Result<(PersistenceOutcome, Vec<RowKey>), RemovalError> {
    let file = crate::persist::StateFile {
        schema_version: crate::persist::SCHEMA_VERSION,
        state: candidate,
    };
    let persistence = match save(&file) {
        Ok(()) => PersistenceOutcome::Saved,
        Err(error) if error.replacement_committed() => PersistenceOutcome::SavedDegraded {
            detail: error.to_string(),
        },
        Err(error) => return Err(RemovalError::Persist { error }),
    };
    *state = file.state;
    let removed_rows: Vec<RowKey> = set.rows.iter().map(|row| row.row).collect();
    let readers_parked: Vec<RowKey> = runtimes
        .iter()
        .filter(|handle| removed_rows.contains(&handle.row))
        .filter(|handle| {
            let joined = handle.session.claude.reader_finished()
                && handle
                    .session
                    .shell
                    .as_ref()
                    .is_none_or(Pty::reader_finished);
            !joined
        })
        .map(|handle| handle.row)
        .collect();
    runtimes.retain(|handle| !removed_rows.contains(&handle.row));
    Ok((persistence, readers_parked))
}

fn find_repository(state: &RepositoryState, key: RepositoryKey) -> Option<&SavedRepository> {
    state
        .repositories
        .iter()
        .find(|repository| repository.key == key)
}

fn checkout_row(checkout: &SavedCheckout, parent: &SavedRepository) -> PlannedRow {
    let path = checkout.observed_path.to_path_buf();
    let disk = probe_disk(disk_action_for(checkout), &path);
    PlannedRow {
        row: RowKey::Checkout(checkout.key),
        path: Some(path),
        disk,
        runtime: checkout.owned_runtime().cloned(),
        main_worktree: Some(parent.observed_main_worktree.to_path_buf()),
        common_dir: Some(parent.observed_common_dir.to_path_buf()),
    }
}

/// The per-kind disk rule from the persisted row alone: a main checkout and
/// an unmanaged checkout are kept; a managed non-main checkout is removed.
fn disk_action_for(checkout: &SavedCheckout) -> DiskAction {
    match (checkout.role, checkout.managed_by_baude) {
        (CheckoutRole::Main, _) | (_, false) => DiskAction::KeepDirectory,
        (_, true) => DiskAction::RemoveManagedWorktree,
    }
}

/// Downgrade a managed removal to `AlreadyRemoved` only when the directory
/// is `NotFound`. Any other inspection error keeps the row so the disk stage
/// reports it as a typed failure instead of silently dropping the row.
fn probe_disk(action: DiskAction, path: &Path) -> DiskAction {
    if action != DiskAction::RemoveManagedWorktree {
        return action;
    }
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => DiskAction::AlreadyRemoved,
        _ => DiskAction::RemoveManagedWorktree,
    }
}

/// One assembly path for both the preview and the real run. With `stages`
/// absent the report is a plan derived from the selected rows; with them
/// present the runtime, path and persistence fields are what the stages
/// actually did. `planned` is what the clearing stage will attempt; the
/// preview reports it as `cleared`, the real run reports what the stage did.
fn report_from_set(
    set: &RemovalSet,
    planned: Vec<ClearedItem>,
    stages: Option<StageResults>,
) -> RemovalReport {
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
            cleared: stages.clearing.cleared,
            clearing_failures: stages.clearing.failures,
        };
    }
    let mut report = RemovalReport {
        target: set.target,
        planned: true,
        rows_removed,
        runtimes_killed: Vec::new(),
        paths_deleted: Vec::new(),
        paths_kept: Vec::new(),
        already_removed: Vec::new(),
        readers_parked: Vec::new(),
        persistence: PersistenceOutcome::Planned,
        cleared: planned,
        clearing_failures: Vec::new(),
    };
    for row in &set.rows {
        if let Some(runtime) = &row.runtime {
            report.runtimes_killed.push(KilledRuntime {
                row: row.row,
                agent_pid: runtime.agent.pid,
                shell_pid: match &runtime.shell {
                    ShellOwnership::Owned(identity) => Some(identity.pid),
                    ShellOwnership::Closed => None,
                },
            });
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        CheckoutLifecycle, PersistedPath, ProcessIdentity, RepositoryHealth, RetainedSessionState,
        RetainedStandaloneSessionState, RuntimeGeneration, SavedStandaloneSession,
        StandaloneLifecycle, UnavailableCause,
    };
    use std::fs;

    /// The workspace identity every fixture here pins, so the stage's
    /// per-workspace artefacts (`repository-<key>` directories, the
    /// suppression store) resolve under the fixture's redirect instead of
    /// aborting the test.
    const WORKSPACE: &str = "removal-ws";

    /// Acquired AFTER the fixture's `TestRedirect` and declared BEFORE it in
    /// every fixture struct, so identity is restored before the root it was
    /// resolved against.
    fn fixture_identity() -> crate::testing::TestRedirect {
        crate::workspace::override_for_test(
            &crate::persist::Config {
                workspace: Some(WORKSPACE.to_string()),
                ..crate::persist::Config::default()
            },
            None,
        )
    }

    /// Per-test fixture root under the temp dir, removed on drop so a failing
    /// assertion still cleans up. Redirected and given an identity because
    /// `preview_removal` reads the suppression store under the config dir.
    struct TempTree {
        _identity: crate::testing::TestRedirect,
        _redirect: crate::testing::TestRedirect,
        /// Declared last so the tree is removed after both guards drop.
        root: crate::testing::FixtureRoot,
    }

    impl TempTree {
        fn new(label: &str) -> Self {
            let root = crate::testing::FixtureRoot::new(&format!("baude-removal-{label}"));
            let redirect = crate::testing::TestRedirect::new(&root);
            let identity = fixture_identity();
            Self {
                _identity: identity,
                _redirect: redirect,
                root,
            }
        }

        /// A directory that exists on disk.
        fn dir(&self, name: &str) -> PathBuf {
            let path = self.root.join(name);
            fs::create_dir_all(&path).expect("create fixture dir");
            path
        }

        /// A path deliberately left absent.
        fn absent(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }

        /// A directory that only the test may populate with a state file.
        fn state_dir(&self) -> PathBuf {
            self.dir("state")
        }
    }

    fn persisted(path: &Path) -> PersistedPath {
        PersistedPath::from_path(path)
    }

    fn repository(state: &mut RepositoryState, main: &Path) -> RepositoryKey {
        let key = state.allocate_repository_key().expect("repository key");
        let order = state.allocate_first_seen_order().expect("order");
        state.repositories.push(SavedRepository {
            key,
            observed_common_dir: persisted(&main.join(".git")),
            observed_main_worktree: persisted(main),
            first_seen_order: order,
            health: RepositoryHealth::Available,
            physical_key: String::new(),
        });
        key
    }

    fn checkout(
        state: &mut RepositoryState,
        repository_key: RepositoryKey,
        role: CheckoutRole,
        managed_by_baude: bool,
        observed_path: &Path,
    ) -> CheckoutKey {
        let main = state
            .repositories
            .iter()
            .find(|repository| repository.key == repository_key)
            .expect("parent repository")
            .observed_main_worktree
            .to_path_buf();
        let key = state.allocate_checkout_key().expect("checkout key");
        let order = state.allocate_first_seen_order().expect("order");
        state.checkouts.push(SavedCheckout::new(
            key,
            repository_key,
            role,
            managed_by_baude,
            persisted(observed_path),
            Some("main".into()),
            order,
            CheckoutLifecycle::Inactive,
            RetainedSessionState {
                name: "repo".into(),
                cwd: persisted(observed_path),
                repo_root: persisted(&main),
                branch: Some("main".into()),
                is_worktree: observed_path != main,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
        ));
        key
    }

    fn standalone(state: &mut RepositoryState, canonical_path: &Path) -> StandaloneKey {
        let key = state.allocate_standalone_key().expect("standalone key");
        let order = state.allocate_first_seen_order().expect("order");
        state.standalone_sessions.push(SavedStandaloneSession::new(
            key,
            persisted(canonical_path),
            order,
            StandaloneLifecycle::Inactive,
            None,
            RetainedStandaloneSessionState {
                name: "standalone".into(),
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
                ever_launched: false,
            },
        ));
        key
    }

    fn identity(pid: u32) -> ProcessIdentity {
        ProcessIdentity {
            pid,
            start_time: 7,
            process_group: pid as i32,
            session: pid as i32,
        }
    }

    fn checkout_mut(state: &mut RepositoryState, key: CheckoutKey) -> &mut SavedCheckout {
        state
            .checkouts
            .iter_mut()
            .find(|checkout| checkout.key == key)
            .expect("checkout row")
    }

    /// Repository k with four checkouts (Main, managed PrimaryDefault, managed
    /// ManagedBranch, unmanaged ManagedBranch), repository j with one checkout,
    /// and one standalone folder. Every directory exists.
    struct Fixture {
        tree: TempTree,
        state: RepositoryState,
        k: RepositoryKey,
        c1: CheckoutKey,
        c2: CheckoutKey,
        c3: CheckoutKey,
        c4: CheckoutKey,
        j_checkout: CheckoutKey,
        s: StandaloneKey,
    }

    fn fixture(label: &str) -> Fixture {
        let tree = TempTree::new(label);
        let mut state = RepositoryState::default();
        let k_main = tree.dir("k");
        let k = repository(&mut state, &k_main);
        let c1 = checkout(&mut state, k, CheckoutRole::Main, false, &k_main);
        let c2 = checkout(
            &mut state,
            k,
            CheckoutRole::PrimaryDefault,
            true,
            &tree.dir("k-default"),
        );
        let c3 = checkout(
            &mut state,
            k,
            CheckoutRole::ManagedBranch,
            true,
            &tree.dir("k-feature"),
        );
        let c4 = checkout(
            &mut state,
            k,
            CheckoutRole::ManagedBranch,
            false,
            &tree.dir("k-unmanaged"),
        );
        let j_main = tree.dir("j");
        let j = repository(&mut state, &j_main);
        let j_checkout = checkout(&mut state, j, CheckoutRole::Main, false, &j_main);
        let s = standalone(&mut state, &tree.dir("folder"));
        state.validate().expect("fixture state validates");
        Fixture {
            tree,
            state,
            k,
            c1,
            c2,
            c3,
            c4,
            j_checkout,
            s,
        }
    }

    fn row(set: &RemovalSet, key: RowKey) -> &PlannedRow {
        set.rows
            .iter()
            .find(|row| row.row == key)
            .unwrap_or_else(|| panic!("row {key:?} missing from {:?}", set.rows))
    }

    #[test]
    fn select_rows_repository_target_fans_out_children_first_parent_last() {
        let f = fixture("fanout");
        let set = select_rows(&f.state, RemovalTarget::Repository(f.k)).expect("select");
        let keys: Vec<RowKey> = set.rows.iter().map(|row| row.row).collect();
        assert_eq!(
            keys,
            vec![
                RowKey::Checkout(f.c1),
                RowKey::Checkout(f.c2),
                RowKey::Checkout(f.c3),
                RowKey::Checkout(f.c4),
                RowKey::Repository(f.k),
            ]
        );
        assert!(!keys.contains(&RowKey::Checkout(f.j_checkout)));
        assert_eq!(set.target, RemovalTarget::Repository(f.k));
        assert_eq!(set.checkout_keys(), vec![f.c1, f.c2, f.c3, f.c4]);
        assert_eq!(set.repository_keys(), vec![f.k]);
        assert!(set.standalone_keys().is_empty());
    }

    #[test]
    fn select_rows_classifies_disk_action_by_role_and_management() {
        let f = fixture("classify");
        let set = select_rows(&f.state, RemovalTarget::Repository(f.k)).expect("select");
        assert_eq!(
            row(&set, RowKey::Checkout(f.c1)).disk,
            DiskAction::KeepDirectory
        );
        assert_eq!(
            row(&set, RowKey::Checkout(f.c2)).disk,
            DiskAction::RemoveManagedWorktree
        );
        assert_eq!(
            row(&set, RowKey::Checkout(f.c3)).disk,
            DiskAction::RemoveManagedWorktree
        );
        assert_eq!(
            row(&set, RowKey::Checkout(f.c4)).disk,
            DiskAction::KeepDirectory
        );
        let parent = row(&set, RowKey::Repository(f.k));
        assert_eq!(parent.disk, DiskAction::KeepDirectory);
        assert_eq!(parent.path, None);

        let set = select_rows(&f.state, RemovalTarget::Standalone(f.s)).expect("select");
        assert_eq!(set.rows.len(), 1);
        let standalone = &set.rows[0];
        assert_eq!(standalone.row, RowKey::Standalone(f.s));
        assert_eq!(standalone.disk, DiskAction::KeepDirectory);
        assert_eq!(standalone.path, Some(f.tree.root.join("folder")));
        assert_eq!(set.standalone_keys(), vec![f.s]);
        assert!(set.checkout_keys().is_empty());
        assert!(set.repository_keys().is_empty());
    }

    #[test]
    fn select_rows_marks_missing_managed_directory_already_removed() {
        let tree = TempTree::new("missing");
        let mut state = RepositoryState::default();
        let k = repository(&mut state, &tree.dir("k"));
        let gone = tree.absent("k-gone");
        let c3 = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &gone);
        state.validate().expect("fixture state validates");
        assert!(!gone.exists());

        let set = select_rows(&state, RemovalTarget::Checkout(c3)).expect("select");
        assert_eq!(set.rows.len(), 1);
        assert_eq!(set.rows[0].disk, DiskAction::AlreadyRemoved);
        assert_eq!(set.rows[0].path, Some(gone.clone()));

        checkout_mut(&mut state, c3)
            .set_lifecycle(CheckoutLifecycle::Protected(UnavailableCause::Missing));
        state.validate().expect("protected state validates");
        let set = select_rows(&state, RemovalTarget::Checkout(c3)).expect("select");
        assert_eq!(set.rows[0].disk, DiskAction::AlreadyRemoved);

        let report = preview_removal(&state, RemovalTarget::Checkout(c3)).expect("preview");
        assert_eq!(report.already_removed, vec![gone]);
        assert!(report.paths_deleted.is_empty());
    }

    #[test]
    fn select_rows_keeps_managed_row_when_probe_fails_without_not_found() {
        let tree = TempTree::new("enotdir");
        let mut state = RepositoryState::default();
        let k = repository(&mut state, &tree.dir("k"));
        let blocker = tree.root.join("not-a-dir");
        fs::write(&blocker, b"regular file").expect("write blocker");
        let under_file = blocker.join("wt");
        let c3 = checkout(
            &mut state,
            k,
            CheckoutRole::ManagedBranch,
            true,
            &under_file,
        );
        state.validate().expect("fixture state validates");
        let probe = fs::symlink_metadata(&under_file).expect_err("probe must fail");
        assert_ne!(
            probe.kind(),
            ErrorKind::NotFound,
            "fixture must not be NotFound"
        );

        let set = select_rows(&state, RemovalTarget::Checkout(c3)).expect("select");
        assert_eq!(set.rows[0].disk, DiskAction::RemoveManagedWorktree);

        let report = preview_removal(&state, RemovalTarget::Checkout(c3)).expect("preview");
        assert_eq!(report.paths_deleted, vec![under_file]);
        assert!(report.already_removed.is_empty());
    }

    #[test]
    fn select_rows_unknown_target_is_typed_error() {
        let f = fixture("unknown");
        let mut probe = RepositoryState::default();
        let mut unknown_repository = probe.allocate_repository_key().expect("key");
        let mut unknown_checkout = probe.allocate_checkout_key().expect("key");
        let mut unknown_standalone = probe.allocate_standalone_key().expect("key");
        for _ in 0..16 {
            unknown_repository = probe.allocate_repository_key().expect("key");
            unknown_checkout = probe.allocate_checkout_key().expect("key");
            unknown_standalone = probe.allocate_standalone_key().expect("key");
        }
        assert!(!f
            .state
            .repositories
            .iter()
            .any(|r| r.key == unknown_repository));
        assert!(!f.state.checkouts.iter().any(|c| c.key == unknown_checkout));
        assert!(!f
            .state
            .standalone_sessions
            .iter()
            .any(|s| s.key == unknown_standalone));

        for target in [
            RemovalTarget::Repository(unknown_repository),
            RemovalTarget::Checkout(unknown_checkout),
            RemovalTarget::Standalone(unknown_standalone),
        ] {
            match select_rows(&f.state, target) {
                Err(RemovalError::UnknownTarget(reported)) => assert_eq!(reported, target),
                other => panic!("expected UnknownTarget for {target:?}, got {other:?}"),
            }
            match preview_removal(&f.state, target) {
                Err(RemovalError::UnknownTarget(reported)) => assert_eq!(reported, target),
                other => panic!("expected UnknownTarget for {target:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn select_rows_carries_owned_runtime_identity() {
        let mut f = fixture("runtime");
        let closed_shell = OwnedRuntime {
            generation: RuntimeGeneration::initial(),
            agent: identity(4242),
            shell: ShellOwnership::Closed,
        };
        let owned_shell = OwnedRuntime {
            generation: RuntimeGeneration::initial(),
            agent: identity(5151),
            shell: ShellOwnership::Owned(identity(4343)),
        };
        {
            let c2 = checkout_mut(&mut f.state, f.c2);
            c2.set_lifecycle(CheckoutLifecycle::Running(RuntimeGeneration::initial()));
            c2.owned_runtime = Some(closed_shell.clone());
        }
        {
            let c3 = checkout_mut(&mut f.state, f.c3);
            c3.set_lifecycle(CheckoutLifecycle::Running(RuntimeGeneration::initial()));
            c3.owned_runtime = Some(owned_shell.clone());
        }
        f.state.validate().expect("runtime state validates");
        let parent = f
            .state
            .repositories
            .iter()
            .find(|repository| repository.key == f.k)
            .expect("parent")
            .clone();

        let set = select_rows(&f.state, RemovalTarget::Repository(f.k)).expect("select");
        assert_eq!(
            row(&set, RowKey::Checkout(f.c2)).runtime,
            Some(closed_shell.clone())
        );
        assert_eq!(
            row(&set, RowKey::Checkout(f.c3)).runtime,
            Some(owned_shell.clone())
        );
        assert_eq!(row(&set, RowKey::Checkout(f.c1)).runtime, None);
        assert_eq!(row(&set, RowKey::Checkout(f.c4)).runtime, None);
        assert_eq!(row(&set, RowKey::Repository(f.k)).runtime, None);
        for key in [f.c1, f.c2, f.c3, f.c4] {
            let planned = row(&set, RowKey::Checkout(key));
            assert_eq!(
                planned.main_worktree,
                Some(parent.observed_main_worktree.to_path_buf())
            );
            assert_eq!(
                planned.common_dir,
                Some(parent.observed_common_dir.to_path_buf())
            );
        }

        let report = preview_removal(&f.state, RemovalTarget::Repository(f.k)).expect("preview");
        assert_eq!(
            report.runtimes_killed,
            vec![
                KilledRuntime {
                    row: RowKey::Checkout(f.c2),
                    agent_pid: 4242,
                    shell_pid: None,
                },
                KilledRuntime {
                    row: RowKey::Checkout(f.c3),
                    agent_pid: 5151,
                    shell_pid: Some(4343),
                },
            ]
        );

        let set = select_rows(&f.state, RemovalTarget::Standalone(f.s)).expect("select");
        assert_eq!(set.rows[0].runtime, None);
        assert_eq!(set.rows[0].main_worktree, None);
        assert_eq!(set.rows[0].common_dir, None);
    }

    #[test]
    fn preview_has_no_side_effects_and_report_round_trips_serde() {
        let mut f = fixture("preview");
        {
            let c3 = checkout_mut(&mut f.state, f.c3);
            c3.set_lifecycle(CheckoutLifecycle::Running(RuntimeGeneration::initial()));
            c3.owned_runtime = Some(OwnedRuntime {
                generation: RuntimeGeneration::initial(),
                agent: identity(6161),
                shell: ShellOwnership::Owned(identity(6262)),
            });
        }
        f.state.validate().expect("fixture state validates");
        let state_dir = f.tree.state_dir();
        let before = f.state.clone();
        let fixture_dirs = ["k", "k-default", "k-feature", "k-unmanaged", "j", "folder"]
            .map(|name| f.tree.root.join(name));

        let report = preview_removal(&f.state, RemovalTarget::Repository(f.k)).expect("preview");

        assert!(report.planned);
        assert_eq!(report.target, RemovalTarget::Repository(f.k));
        assert_eq!(report.persistence, PersistenceOutcome::Planned);
        assert_eq!(
            report.rows_removed,
            vec![
                RowKey::Checkout(f.c1),
                RowKey::Checkout(f.c2),
                RowKey::Checkout(f.c3),
                RowKey::Checkout(f.c4),
                RowKey::Repository(f.k),
            ]
        );
        assert_eq!(
            report.runtimes_killed,
            vec![KilledRuntime {
                row: RowKey::Checkout(f.c3),
                agent_pid: 6161,
                shell_pid: Some(6262),
            }]
        );
        assert_eq!(
            report.paths_deleted,
            vec![f.tree.root.join("k-default"), f.tree.root.join("k-feature")]
        );
        assert_eq!(
            report.paths_kept,
            vec![f.tree.root.join("k"), f.tree.root.join("k-unmanaged")]
        );
        assert!(report.already_removed.is_empty());
        assert!(report.readers_parked.is_empty());

        assert_eq!(f.state, before, "preview must not mutate state");
        assert!(!state_dir.join("state.json").exists());
        assert!(fs::read_dir(&state_dir)
            .expect("state dir")
            .next()
            .is_none());
        for dir in &fixture_dirs {
            assert!(dir.is_dir(), "{} must still exist", dir.display());
        }

        let json = serde_json::to_string(&report).expect("serialize");
        let decoded: RemovalReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, report);
    }

    // ---- live-runtime orchestration fixture (plan 04) ----

    use crate::meta::ClaudeMeta;
    use crate::persist::{self, SaveError, StateFile};
    use crate::session::Session;
    use crate::testing::{holder_pgid, spawn_parked_reader_runtime};
    use std::cell::Cell;
    use std::process::Command;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    /// Job control off so the sleeper stays in the recorded group under the
    /// interactive gate shell; TERM and HUP ignored so SIGKILL escalation is
    /// exercised.
    const TRAPPING_COMMAND: &str = "set +m; trap '' TERM HUP; sleep 30";

    fn git_ok(cwd: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {args:?} in {}: {}",
            cwd.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Per-test root under the temp dir with a real repository, a managed
    /// root under the redirect and a state directory. Canonicalized because
    /// Git reports the resolved form of a worktree path and the managed-root
    /// containment check compares canonical paths. The guards are fields
    /// declared before `_root`, so they are restored before the tree is
    /// removed; that removal resolves no redirected path.
    struct RemovalFixture {
        _identity: crate::testing::TestRedirect,
        _redirect: crate::testing::TestRedirect,
        /// The canonical form of `_root`, the path every case is handed.
        root: PathBuf,
        /// Declared last so the tree is removed after both guards drop.
        _root: crate::testing::FixtureRoot,
    }

    impl RemovalFixture {
        fn new(label: &str) -> Self {
            let fixture_root =
                crate::testing::FixtureRoot::new(&format!("baude-removal-live-{label}"));
            let root = fs::canonicalize(&fixture_root).expect("canonicalize fixture root");
            let redirect = crate::testing::TestRedirect::new(&root);
            let identity = fixture_identity();
            let fixture = Self {
                _identity: identity,
                _redirect: redirect,
                root,
                _root: fixture_root,
            };
            git_ok(&fixture.root, &["init", "-q", "-b", "main", "repo"]);
            let repo = fixture.repo();
            git_ok(&repo, &["config", "user.name", "Baude Test"]);
            git_ok(&repo, &["config", "user.email", "baude@example.invalid"]);
            fs::write(repo.join("tracked.txt"), b"fixture\n").expect("write tracked file");
            git_ok(&repo, &["add", "tracked.txt"]);
            git_ok(&repo, &["commit", "-q", "-m", "fixture"]);
            fs::create_dir_all(fixture.managed_root()).expect("create managed root");
            fs::create_dir_all(fixture.state_root()).expect("create state root");
            fixture
        }

        fn repo(&self) -> PathBuf {
            self.root.join("repo")
        }

        fn managed_root(&self) -> PathBuf {
            crate::git::worktrees_base()
        }

        fn managed_worktree(&self, name: &str, branch: &str) -> PathBuf {
            let path = self.managed_root().join(name);
            git_ok(
                &self.repo(),
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    branch,
                    path.to_str().expect("utf-8 worktree path"),
                ],
            );
            fs::canonicalize(&path).expect("canonicalize managed worktree")
        }

        fn state_root(&self) -> PathBuf {
            self.root.join("state")
        }

        fn state_path(&self) -> PathBuf {
            self.state_root().join("state.json")
        }

        /// The common dir's path on disk, usable after the repository is gone.
        fn common_dir_path(&self) -> PathBuf {
            self.repo().join(".git")
        }

        fn common_dir(&self) -> PathBuf {
            crate::git::discover_repository(&self.repo())
                .expect("discover repository")
                .common_dir
        }
    }

    fn seed_repository(state: &mut RepositoryState, f: &RemovalFixture) -> RepositoryKey {
        let key = state.allocate_repository_key().expect("repository key");
        let order = state.allocate_first_seen_order().expect("order");
        state.repositories.push(SavedRepository {
            key,
            observed_common_dir: persisted(&f.common_dir()),
            observed_main_worktree: persisted(&f.repo()),
            first_seen_order: order,
            health: RepositoryHealth::Available,
            physical_key: String::new(),
        });
        key
    }

    /// Put `key` in the save-valid live shape: `Running` with the full
    /// persisted runtime taken from the real process identities.
    fn seed_live(state: &mut RepositoryState, key: CheckoutKey, agent: &Pty, shell: Option<&Pty>) {
        let checkout = checkout_mut(state, key);
        checkout.set_lifecycle(CheckoutLifecycle::Running(RuntimeGeneration::initial()));
        checkout.set_owned_runtime(Some(OwnedRuntime {
            generation: RuntimeGeneration::initial(),
            agent: agent.process_identity().clone(),
            shell: shell.map_or(ShellOwnership::Closed, |shell| {
                ShellOwnership::Owned(shell.process_identity().clone())
            }),
        }));
    }

    /// A fresh state with the repository, its main checkout and one managed
    /// child at `name` whose runtime is a live trapping double.
    fn live_row(f: &RemovalFixture, name: &str) -> (RepositoryState, CheckoutKey, PathBuf, Pty) {
        let managed = f.managed_worktree(name, &format!("feature/{name}"));
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        let key = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed);
        let pty = spawn_trapping_runtime(&managed);
        seed_live(&mut state, key, &pty, None);
        (state, key, managed, pty)
    }

    fn has_checkout(state: &RepositoryState, key: CheckoutKey) -> bool {
        state.checkouts.iter().any(|checkout| checkout.key == key)
    }

    fn load_state(state_root: &Path) -> RepositoryState {
        persist::load_current_at(state_root, "state.json")
            .expect("load saved state")
            .state
    }

    fn write_seed(f: &RemovalFixture, state: &RepositoryState) -> Vec<u8> {
        persist::save_current_at_test(
            &f.state_root(),
            "state.json",
            &StateFile {
                schema_version: persist::SCHEMA_VERSION,
                state: state.clone(),
            },
            None,
            None,
        )
        .expect("seed save");
        fs::read(f.state_path()).expect("read seed bytes")
    }

    fn proc_state(pid: u32) -> String {
        let out = Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn wait_for(pid: u32, pred: impl Fn(&str) -> bool, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let state = proc_state(pid);
            if pred(&state) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for pid {pid} to be {what}; last state {state:?}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn assert_reaped_promptly(pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while proc_state(pid).starts_with('Z') {
            assert!(
                Instant::now() < deadline,
                "pid {pid} is still a zombie after 5s"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn group_is_extinct(pgid: i32) -> bool {
        // SAFETY: signal zero is a liveness probe only.
        let failed = unsafe { libc::kill(-pgid, 0) == -1 };
        failed && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }

    fn group_is_alive(pgid: i32) -> bool {
        // SAFETY: signal zero is a liveness probe only.
        unsafe { libc::kill(-pgid, 0) == 0 }
    }

    /// One `ps -eo pid=,pgid=,comm=` read: true when a row carries `pgid` and
    /// the command name `comm` (bare or as the last path component).
    fn descendant_in_group(pgid: i32, comm: &str) -> bool {
        let out = Command::new("ps")
            .args(["-eo", "pid=,pgid=,comm="])
            .output()
            .expect("ps");
        let suffix = format!("/{comm}");
        String::from_utf8_lossy(&out.stdout).lines().any(|line| {
            let mut fields = line.split_whitespace();
            let _pid = fields.next();
            let row_pgid = fields.next().and_then(|field| field.parse::<i32>().ok());
            let name = fields.next().unwrap_or("");
            row_pgid == Some(pgid) && (name == comm || name.ends_with(&suffix))
        })
    }

    /// A TERM-trapping runtime whose `sleep` is proven, from `ps`, to sit in
    /// the recorded process group before the caller may signal anything.
    fn spawn_trapping_runtime(cwd: &Path) -> Pty {
        let pty = Pty::spawn(Some(TRAPPING_COMMAND), cwd, 5, 40).expect("spawn trapping runtime");
        std::thread::sleep(Duration::from_millis(200));
        let pid = pty.process_identity().pid;
        wait_for(pid, |s| !s.is_empty() && !s.starts_with('T'), "running");
        let pgid = pty.process_identity().process_group;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !descendant_in_group(pgid, "sleep") {
            assert!(
                Instant::now() < deadline,
                "no sleep descendant joined recorded group {pgid} within 5s"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        pty
    }

    fn session_for(pty: Pty, shell: Option<Pty>, cwd: &Path, repo: &Path) -> Session {
        Session {
            id: 1,
            name: "removal-test".to_string(),
            cwd: cwd.to_path_buf(),
            repo_root: repo.to_path_buf(),
            branch: None,
            is_worktree: cwd != repo,
            claude: pty,
            shell_open: shell.is_some(),
            shell,
            spawn_unix_ms: 0,
            targeted_resume: false,
            meta: ClaudeMeta::default(),
            archived: false,
            archived_by_user: false,
            was_busy: false,
            unarchived_at_ms: None,
            pending_permission: None,
            permission_decision: None,
            child_suspended: false,
            pane_focus_shell: false,
            poll_meta_calls_for_test: Cell::new(0),
        }
    }

    /// Owns every handle a test holds; stops whatever `remove` did not
    /// consume, including during unwinding, so no 30 s child outlives a
    /// failed assertion. Called twice because an injected teardown failure
    /// that `remove` never reached is consumed by the first call without
    /// signalling anything.
    struct LiveHandles(Vec<RowRuntime>);

    impl Drop for LiveHandles {
        fn drop(&mut self) {
            for handle in &mut self.0 {
                let _ = handle.session.kill_and_wait();
                let _ = handle.session.kill_and_wait();
            }
        }
    }

    fn handle_for(
        row: RowKey,
        pty: Pty,
        shell: Option<Pty>,
        cwd: &Path,
        repo: &Path,
    ) -> RowRuntime {
        RowRuntime {
            row,
            session: session_for(pty, shell, cwd, repo),
        }
    }

    fn worktree_list(repo: &Path) -> String {
        git_ok(repo, &["worktree", "list", "--porcelain"])
    }

    fn counting_save(
        state_root: &Path,
    ) -> (
        impl FnOnce(&StateFile) -> Result<(), SaveError>,
        Rc<Cell<u32>>,
    ) {
        counting_save_with(state_root, None)
    }

    fn mismatch_row(error: RemovalError, needle: &str) -> RowKey {
        match error {
            RemovalError::RuntimeIdentityMismatch { row, detail } => {
                assert!(
                    detail.contains(needle),
                    "detail {detail:?} must mention {needle:?}"
                );
                row
            }
            other => panic!("expected RuntimeIdentityMismatch, got {other:?}"),
        }
    }

    // ---- live-runtime orchestration tests (plan 04) ----

    /// A repository deleted outside baude is refused before the kill stage:
    /// the runtime keeps running, no handle is consumed, nothing is saved.
    #[test]
    fn vanished_owner_common_dir_is_refused_before_the_kill() {
        let f = RemovalFixture::new("owner-gone");
        let (mut state, key, managed, pty) = live_row(&f, "wt-live");
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        state.validate_for_save().expect("seed is save-valid");
        let before = state.clone();
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(key),
            pty,
            None,
            &managed,
            &f.repo(),
        )]);
        let (save, calls) = counting_save(&f.state_root());
        fs::remove_dir_all(f.repo()).expect("delete the main checkout and its common dir");
        assert!(!f.common_dir_path().exists());

        let error = remove(
            &mut state,
            RemovalTarget::Checkout(key),
            &mut handles.0,
            save,
        )
        .expect_err("a vanished owning repository is refused");

        match error {
            RemovalError::DiskRemoval { row, path, error } => {
                assert_eq!(row, RowKey::Checkout(key));
                assert_eq!(path, managed);
                assert_eq!(error.kind, DiskRemovalFailureKind::Uninspectable, "{error}");
            }
            other => panic!("expected DiskRemoval, got {other:?}"),
        }
        assert!(!group_is_extinct(pgid), "group {pgid} must still be alive");
        assert!(
            crate::session::inspect_process_identity(pid)
                .expect("inspect")
                .is_some(),
            "leader {pid} must still be running"
        );
        assert_eq!(handles.0.len(), 1, "no handle is consumed");
        assert_eq!(state, before);
        assert_eq!(calls.get(), 0, "nothing is saved");
        assert!(managed.exists(), "the managed directory survives");
    }

    /// A directory that reappeared at an already-removed row's path after the
    /// plan is deleted and reported as deleted, not as already removed.
    #[test]
    fn reappeared_directory_at_already_removed_row_is_reported_deleted() {
        let f = RemovalFixture::new("reappeared");
        let (state, _k, c1, _c2, c1_dir, _c2_dir) = inactive_pair(&f);
        fs::remove_dir_all(&c1_dir).expect("directory goes missing");
        let set = select_rows(&state, RemovalTarget::Checkout(c1)).expect("select");
        assert_eq!(set.rows[0].disk, DiskAction::AlreadyRemoved);
        fs::create_dir_all(&c1_dir).expect("volume returns before the disk stage");

        let results = disk_stage(&set).expect("disk stage");

        assert_eq!(results.deleted, vec![c1_dir.clone()]);
        assert!(results.already_removed.is_empty());
        assert!(!c1_dir.exists(), "the reappeared directory is deleted");
        assert!(!worktree_list(&f.repo()).contains(c1_dir.to_str().expect("utf-8")));
    }

    /// A directory that reappeared unregistered and without a marker is a
    /// typed failure: the row must not be dropped while a stray directory
    /// sits inside the managed root at its path.
    #[test]
    fn reappeared_unregistered_directory_is_a_typed_failure() {
        let f = RemovalFixture::new("reappeared-stray");
        let (mut state, _k, c1, _c2, c1_dir, _c2_dir) = inactive_pair(&f);
        fs::remove_dir_all(&c1_dir).expect("directory goes missing");
        let set = select_rows(&state, RemovalTarget::Checkout(c1)).expect("select");
        assert_eq!(set.rows[0].disk, DiskAction::AlreadyRemoved);
        git_ok(
            &f.repo(),
            &[
                "worktree",
                "remove",
                "--force",
                "--",
                c1_dir.to_str().expect("utf-8"),
            ],
        );
        fs::create_dir_all(&c1_dir).expect("a stray directory appears at the path");
        let before = state.clone();
        let (save, calls) = counting_save(&f.state_root());

        let error = remove(
            &mut state,
            RemovalTarget::Checkout(c1),
            &mut Vec::new(),
            save,
        )
        .expect_err("a stray directory at the row's path is refused");

        match error {
            RemovalError::DiskRemoval { row, path, error } => {
                assert_eq!(row, RowKey::Checkout(c1));
                assert_eq!(path, c1_dir);
                assert_eq!(error.kind, DiskRemovalFailureKind::NotRegistered, "{error}");
            }
            other => panic!("expected DiskRemoval, got {other:?}"),
        }
        assert!(c1_dir.exists(), "the stray directory survives");
        assert_eq!(state, before);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn live_runtime_group_is_extinct_after_remove() {
        let f = RemovalFixture::new("extinct");
        let (mut state, key, managed, pty) = live_row(&f, "wt-live");
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(key),
            pty,
            None,
            &managed,
            &f.repo(),
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Checkout(key),
            &mut handles.0,
            save,
        )
        .expect("remove");

        assert!(group_is_extinct(pgid), "group {pgid} must be extinct");
        assert_reaped_promptly(pid);
        assert!(!managed.exists(), "managed directory must be deleted");
        assert!(!has_checkout(&state, key));
        assert!(!has_checkout(&load_state(&f.state_root()), key));
        assert_eq!(
            report.runtimes_killed,
            vec![KilledRuntime {
                row: RowKey::Checkout(key),
                agent_pid: pid,
                shell_pid: None,
            }]
        );
        assert_eq!(report.paths_deleted, vec![managed.clone()]);
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert!(!report.planned);
        assert_eq!(calls.get(), 1);
        assert!(handles.0.is_empty(), "the consumed handle must be gone");
    }

    #[test]
    fn agent_and_shell_are_both_extinct_after_remove() {
        let f = RemovalFixture::new("shell");
        let (mut state, key, managed, agent) = live_row(&f, "wt-shell");
        let shell = spawn_trapping_runtime(&managed);
        let agent_pid = agent.process_identity().pid;
        let agent_pgid = agent.process_identity().process_group;
        let shell_pid = shell.process_identity().pid;
        let shell_pgid = shell.process_identity().process_group;
        seed_live(&mut state, key, &agent, Some(&shell));
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(key),
            agent,
            Some(shell),
            &managed,
            &f.repo(),
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Checkout(key),
            &mut handles.0,
            save,
        )
        .expect("remove");

        assert!(group_is_extinct(agent_pgid), "agent group must be extinct");
        assert!(group_is_extinct(shell_pgid), "shell group must be extinct");
        assert_reaped_promptly(agent_pid);
        assert_reaped_promptly(shell_pid);
        assert_eq!(report.runtimes_killed.len(), 1);
        assert_eq!(report.runtimes_killed[0].agent_pid, agent_pid);
        assert_eq!(report.runtimes_killed[0].shell_pid, Some(shell_pid));
        assert_eq!(calls.get(), 1);
        assert!(handles.0.is_empty());
    }

    #[test]
    fn surviving_runtime_leaves_row_and_disk() {
        let f = RemovalFixture::new("survivor");
        let (mut state, key, managed, pty) = live_row(&f, "wt-survivor");
        let pid = pty.process_identity().pid;
        state.validate_for_save().expect("seed is save-valid");
        let before_bytes = write_seed(&f, &state);
        let before = state.clone();
        pty.fail_next_teardown_for_test("injected: group survived SIGKILL");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(key),
            pty,
            None,
            &managed,
            &f.repo(),
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let error = remove(
            &mut state,
            RemovalTarget::Checkout(key),
            &mut handles.0,
            save,
        )
        .expect_err("injected survivor must fail the kill stage");

        match error {
            RemovalError::RuntimeSurvived { row, error } => {
                assert_eq!(row, RowKey::Checkout(key));
                assert_eq!(error.agent_pid, Some(pid));
            }
            other => panic!("expected RuntimeSurvived, got {other:?}"),
        }
        assert_eq!(state, before, "state must be untouched");
        assert_eq!(fs::read(f.state_path()).expect("read"), before_bytes);
        assert!(managed.exists(), "managed directory must survive");
        assert_eq!(calls.get(), 0);
        assert_eq!(handles.0.len(), 1, "the caller still owns the handle");
    }

    #[test]
    fn parent_kill_failure_aborts_before_disk_and_state() {
        let f = RemovalFixture::new("parent");
        let managed1 = f.managed_worktree("wt-c1", "feature/c1");
        let managed2 = f.managed_worktree("wt-c2", "feature/c2");
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        let c1 = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed1);
        let c2 = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed2);
        let pty1 = spawn_trapping_runtime(&managed1);
        let pty2 = spawn_trapping_runtime(&managed2);
        let c1_pgid = pty1.process_identity().process_group;
        seed_live(&mut state, c1, &pty1, None);
        seed_live(&mut state, c2, &pty2, None);
        state.validate_for_save().expect("seed is save-valid");
        let before_bytes = write_seed(&f, &state);
        let before = state.clone();
        pty2.fail_next_teardown_for_test("injected: c2 survived");
        let mut handles = LiveHandles(vec![
            handle_for(RowKey::Checkout(c1), pty1, None, &managed1, &f.repo()),
            handle_for(RowKey::Checkout(c2), pty2, None, &managed2, &f.repo()),
        ]);
        let (save, calls) = counting_save(&f.state_root());

        let error = remove(
            &mut state,
            RemovalTarget::Repository(k),
            &mut handles.0,
            save,
        )
        .expect_err("c2's injected failure must abort the parent removal");

        match error {
            RemovalError::RuntimeSurvived { row, .. } => assert_eq!(row, RowKey::Checkout(c2)),
            other => panic!("expected RuntimeSurvived, got {other:?}"),
        }
        assert!(managed1.exists());
        assert!(managed2.exists());
        assert_eq!(state, before);
        assert_eq!(fs::read(f.state_path()).expect("read"), before_bytes);
        assert_eq!(calls.get(), 0);
        assert_eq!(handles.0.len(), 2);
        assert!(
            group_is_extinct(c1_pgid),
            "kill_stage walks rows in order, so c1 was stopped before c2 failed"
        );
    }

    #[test]
    fn missing_runtime_handle_is_typed_error_before_any_kill() {
        let f = RemovalFixture::new("missing-handle");
        let (mut state, key, managed, pty) = live_row(&f, "wt-nohandle");
        let pgid = pty.process_identity().process_group;
        state.validate_for_save().expect("seed is save-valid");
        let before = state.clone();
        let _owner = LiveHandles(vec![handle_for(
            RowKey::Checkout(key),
            pty,
            None,
            &managed,
            &f.repo(),
        )]);
        let mut empty: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save(&f.state_root());

        let error = remove(&mut state, RemovalTarget::Checkout(key), &mut empty, save)
            .expect_err("a live row without a handle must be refused");

        match error {
            RemovalError::RuntimeHandleMissing(row) => assert_eq!(row, RowKey::Checkout(key)),
            other => panic!("expected RuntimeHandleMissing, got {other:?}"),
        }
        assert!(group_is_alive(pgid), "nothing may be signalled");
        assert_eq!(state, before);
        assert_eq!(calls.get(), 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn mismatched_runtime_identity_is_refused_before_any_kill() {
        let f = RemovalFixture::new("mismatch");

        // Part A: same pid, different start_time in the persisted identity.
        {
            let (mut state, key, managed, pty) = live_row(&f, "wt-a");
            let pgid = pty.process_identity().process_group;
            {
                let row = checkout_mut(&mut state, key);
                let mut runtime = row.owned_runtime().cloned().expect("live runtime");
                runtime.agent.start_time += 1;
                row.set_owned_runtime(Some(runtime));
            }
            state.validate_for_save().expect("seed is save-valid");
            let before = state.clone();
            let mut handles = LiveHandles(vec![handle_for(
                RowKey::Checkout(key),
                pty,
                None,
                &managed,
                &f.repo(),
            )]);
            let (save, calls) = counting_save(&f.state_root());
            let error = remove(
                &mut state,
                RemovalTarget::Checkout(key),
                &mut handles.0,
                save,
            )
            .expect_err("start_time drift must be refused");
            assert_eq!(mismatch_row(error, "start_time"), RowKey::Checkout(key));
            assert!(group_is_alive(pgid), "nothing may be signalled");
            assert_eq!(state, before);
            assert_eq!(calls.get(), 0);
            assert_eq!(handles.0.len(), 1);
        }

        // Part B: persisted shell Owned by a process the handle does not carry.
        {
            let (mut state, key, managed, pty) = live_row(&f, "wt-b");
            let pgid = pty.process_identity().process_group;
            let stray = spawn_trapping_runtime(&managed);
            let stray_pgid = stray.process_identity().process_group;
            seed_live(&mut state, key, &pty, Some(&stray));
            state.validate_for_save().expect("seed is save-valid");
            let before = state.clone();
            let _stray_owner = LiveHandles(vec![handle_for(
                RowKey::Checkout(key),
                stray,
                None,
                &managed,
                &f.repo(),
            )]);
            let mut handles = LiveHandles(vec![handle_for(
                RowKey::Checkout(key),
                pty,
                None,
                &managed,
                &f.repo(),
            )]);
            let (save, calls) = counting_save(&f.state_root());
            let error = remove(
                &mut state,
                RemovalTarget::Checkout(key),
                &mut handles.0,
                save,
            )
            .expect_err("missing shell must be refused");
            assert_eq!(mismatch_row(error, "shell"), RowKey::Checkout(key));
            assert!(group_is_alive(pgid));
            assert!(group_is_alive(stray_pgid));
            assert_eq!(state, before);
            assert_eq!(calls.get(), 0);
            assert_eq!(handles.0.len(), 1);
        }

        // Part C: two handles for the same row.
        {
            let (mut state, key, managed, pty) = live_row(&f, "wt-c");
            let pgid = pty.process_identity().process_group;
            let twin = spawn_trapping_runtime(&managed);
            let twin_pgid = twin.process_identity().process_group;
            state.validate_for_save().expect("seed is save-valid");
            let before = state.clone();
            let mut handles = LiveHandles(vec![
                handle_for(RowKey::Checkout(key), pty, None, &managed, &f.repo()),
                handle_for(RowKey::Checkout(key), twin, None, &managed, &f.repo()),
            ]);
            let (save, calls) = counting_save(&f.state_root());
            let error = remove(
                &mut state,
                RemovalTarget::Checkout(key),
                &mut handles.0,
                save,
            )
            .expect_err("duplicate handles must be refused");
            assert_eq!(mismatch_row(error, "duplicate"), RowKey::Checkout(key));
            assert!(group_is_alive(pgid));
            assert!(group_is_alive(twin_pgid));
            assert_eq!(state, before);
            assert_eq!(calls.get(), 0);
            assert_eq!(handles.0.len(), 2);
        }

        // Part D: a handle for a row with no persisted runtime.
        {
            let (mut state, key, managed, pty) = live_row(&f, "wt-d");
            let pgid = pty.process_identity().process_group;
            {
                let row = checkout_mut(&mut state, key);
                row.set_lifecycle(CheckoutLifecycle::Inactive);
                row.set_owned_runtime(None);
            }
            state.validate_for_save().expect("seed is save-valid");
            let before = state.clone();
            let mut handles = LiveHandles(vec![handle_for(
                RowKey::Checkout(key),
                pty,
                None,
                &managed,
                &f.repo(),
            )]);
            let (save, calls) = counting_save(&f.state_root());
            let error = remove(
                &mut state,
                RemovalTarget::Checkout(key),
                &mut handles.0,
                save,
            )
            .expect_err("an unrecorded runtime must be refused");
            assert_eq!(
                mismatch_row(error, "no persisted runtime"),
                RowKey::Checkout(key)
            );
            assert!(
                group_is_alive(pgid),
                "an unrecorded runtime is never killed"
            );
            assert_eq!(state, before);
            assert_eq!(calls.get(), 0);
            assert_eq!(handles.0.len(), 1);
        }

        // Part E: persisted shell Closed while the handle carries a live shell.
        {
            let (mut state, key, managed, pty) = live_row(&f, "wt-e");
            let pgid = pty.process_identity().process_group;
            let shell = spawn_trapping_runtime(&managed);
            let shell_pgid = shell.process_identity().process_group;
            state.validate_for_save().expect("seed is save-valid");
            let before = state.clone();
            let mut handles = LiveHandles(vec![handle_for(
                RowKey::Checkout(key),
                pty,
                Some(shell),
                &managed,
                &f.repo(),
            )]);
            let (save, calls) = counting_save(&f.state_root());
            let error = remove(
                &mut state,
                RemovalTarget::Checkout(key),
                &mut handles.0,
                save,
            )
            .expect_err("a shell the state calls closed must be refused");
            assert_eq!(mismatch_row(error, "shell"), RowKey::Checkout(key));
            assert!(group_is_alive(pgid));
            assert!(group_is_alive(shell_pgid));
            assert_eq!(state, before);
            assert_eq!(calls.get(), 0);
            assert_eq!(handles.0.len(), 1);
        }
    }

    #[test]
    fn already_removed_managed_row_prunes_git_registration() {
        let f = RemovalFixture::new("already-removed");
        let wt = f.managed_worktree("wt-gone", "feature/gone");
        let oid_before = git_ok(&f.repo(), &["rev-parse", "feature/gone"]);
        fs::remove_dir_all(&wt).expect("remove worktree directory without prune");
        let wt_text = wt.to_str().expect("utf-8");
        assert!(
            worktree_list(&f.repo()).contains(wt_text),
            "git must still list the removed directory"
        );
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        let key = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &wt);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Checkout(key), &mut handles, save)
            .expect("already-removed row is pruned");

        assert_eq!(report.already_removed, vec![wt.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert!(
            !worktree_list(&f.repo()).contains(wt_text),
            "the administrative entry must be pruned"
        );
        assert_eq!(
            git_ok(&f.repo(), &["rev-parse", "feature/gone"]),
            oid_before
        );
        assert!(!has_checkout(&state, key));
        assert!(!has_checkout(&load_state(&f.state_root()), key));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn remove_report_matches_preview_rows() {
        let f = RemovalFixture::new("preview-match");
        let managed = f.managed_worktree("wt-child", "feature/child");
        let folder = f.root.join("folder");
        fs::create_dir_all(&folder).expect("create standalone folder");
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed);
        let s = standalone(&mut state, &folder);
        state.validate_for_save().expect("seed is save-valid");
        // Only the main worktree has a binding, so the real run clears a
        // strict subset of what the preview names. Rows run children first,
        // so the main checkout's folder leads and the managed child follows.
        folder_workspace::record(
            Some(&crate::persist::config_dir()),
            &f.repo(),
            "repo-ws",
            10,
        );
        let preview = preview_removal(&state, RemovalTarget::Repository(k)).expect("preview");
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository");

        assert_eq!(preview.rows_removed, report.rows_removed);
        assert!(preview.clearing_failures.is_empty());
        assert_eq!(
            preview.cleared,
            vec![
                ClearedItem {
                    kind: ClearingKind::Binding,
                    path: f.repo(),
                },
                ClearedItem {
                    kind: ClearingKind::Binding,
                    path: managed.clone(),
                },
            ]
        );
        assert_eq!(
            report.cleared,
            vec![ClearedItem {
                kind: ClearingKind::Binding,
                path: f.repo(),
            }]
        );
        assert!(report.clearing_failures.is_empty());
        for (kind, path) in report
            .cleared
            .iter()
            .map(|item| (item.kind, &item.path))
            .chain(
                report
                    .clearing_failures
                    .iter()
                    .map(|failure| (failure.kind, &failure.path)),
            )
        {
            assert!(
                preview
                    .cleared
                    .iter()
                    .any(|item| item.kind == kind && &item.path == path),
                "real run cleared {kind:?} {} that the preview did not name",
                path.display()
            );
        }
        assert!(!state.repositories.iter().any(|r| r.key == k));
        assert!(!state.checkouts.iter().any(|c| c.repository_key == k));
        assert_eq!(state.standalone_sessions.len(), 1);
        assert_eq!(state.standalone_sessions[0].key, s);
        assert_eq!(report.paths_kept, vec![f.repo()]);
        assert_eq!(report.paths_deleted, vec![managed.clone()]);
        assert!(!managed.exists());
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert!(report.readers_parked.is_empty());
        assert_eq!(calls.get(), 1);
    }

    /// A process that left the session with setsid but kept the slave
    /// terminal open is outside the kill unit. Linux leaves the reader parked
    /// behind it, so the row is reported in `readers_parked`; macOS revokes
    /// the controlling tty at leader exit, so the reader is joined and the
    /// list is empty. Either way removal completes and the holder is cleaned
    /// up by the test.
    #[test]
    fn reader_parked_on_out_of_group_slave_holder_is_reported() {
        let f = RemovalFixture::new("parked");
        let managed = f.managed_worktree("wt-parked", "feature/parked");
        let holder_pid_path = f.root.join("holder.pid");
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        let key = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed);
        let (pty, holder) = spawn_parked_reader_runtime(&managed, &holder_pid_path);
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        seed_live(&mut state, key, &pty, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(key),
            pty,
            None,
            &managed,
            &f.repo(),
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let started = Instant::now();
        let report = remove(
            &mut state,
            RemovalTarget::Checkout(key),
            &mut handles.0,
            save,
        )
        .expect("remove");
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_secs(6),
            "remove blocked behind the parked reader for {elapsed:?}"
        );
        assert!(group_is_extinct(pgid));
        assert!(!managed.exists());
        assert!(!has_checkout(&state, key));
        assert!(!has_checkout(&load_state(&f.state_root()), key));
        assert_eq!(calls.get(), 1);
        assert!(handles.0.is_empty());
        if cfg!(target_os = "macos") {
            assert!(
                report.readers_parked.is_empty(),
                "macOS revokes the tty at leader exit, so the reader is joined"
            );
        } else {
            assert_eq!(report.readers_parked, vec![RowKey::Checkout(key)]);
        }
        assert_eq!(report.runtimes_killed[0].agent_pid, pid);

        let holder_pid = holder.pid();
        drop(holder);
        let deadline = Instant::now() + Duration::from_secs(2);
        while holder_pgid(holder_pid).is_some() {
            assert!(
                Instant::now() < deadline,
                "holder {holder_pid} did not exit after SIGKILL"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // ---- persistence failure modes (plan 05) ----

    use crate::persist::AtomicFailure;
    use crate::repository::ValidationRow;

    /// `counting_save` with one `AtomicFailure` injected into every call, so
    /// a test can prove how `remove` treats each branch of the atomic save.
    fn counting_save_with(
        state_root: &Path,
        failure: Option<AtomicFailure>,
    ) -> (
        impl FnOnce(&StateFile) -> Result<(), SaveError>,
        Rc<Cell<u32>>,
    ) {
        let calls = Rc::new(Cell::new(0));
        let counter = Rc::clone(&calls);
        let root = state_root.to_path_buf();
        let save = move |file: &StateFile| {
            counter.set(counter.get() + 1);
            persist::save_current_at_test(&root, "state.json", file, failure, None)
        };
        (save, calls)
    }

    /// Repository k with its main checkout and one managed child at `name`
    /// seeded `Inactive` with no runtime: the save-valid shape of a row that
    /// has never launched.
    fn inactive_row(
        f: &RemovalFixture,
        name: &str,
    ) -> (RepositoryState, RepositoryKey, CheckoutKey, PathBuf) {
        let managed = f.managed_worktree(name, &format!("feature/{name}"));
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        let key = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed);
        state.validate_for_save().expect("seed is save-valid");
        (state, k, key, managed)
    }

    /// Repository k with its main checkout and two managed children, c1
    /// pushed before c2 so `set.rows` walks c1 first.
    fn inactive_pair(
        f: &RemovalFixture,
    ) -> (
        RepositoryState,
        RepositoryKey,
        CheckoutKey,
        CheckoutKey,
        PathBuf,
        PathBuf,
    ) {
        let c1_dir = f.managed_worktree("wt-c1", "feature/one");
        let c2_dir = f.managed_worktree("wt-c2", "feature/two");
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        let c1 = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &c1_dir);
        let c2 = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &c2_dir);
        state.validate_for_save().expect("seed is save-valid");
        (state, k, c1, c2, c1_dir, c2_dir)
    }

    fn persist_error(error: RemovalError) -> SaveError {
        match error {
            RemovalError::Persist { error } => error,
            other => panic!("expected Persist, got {other:?}"),
        }
    }

    /// The retry can report `already_removed` at all because the first
    /// attempt's disk stage completed before the save failed: either
    /// `git worktree remove --force --force` dropped the administrative entry
    /// together with the directory, or plan 02's fallback deleted the
    /// directory and pruned the entry; on the retry the forced helper finds
    /// no record and no directory and only prunes. A marker-write failure is
    /// not a third case: it returns `Marker` before git runs, the row stays
    /// registered with its directory intact and the first `remove` fails at
    /// the disk stage, so this fixture would never reach its save failure.
    #[test]
    fn rename_failure_keeps_prior_state_then_retry_reports_already_removed() {
        let f = RemovalFixture::new("rename-failure");
        let (mut state, _k, key, managed) = inactive_row(&f, "wt-rename");
        let before_bytes = write_seed(&f, &state);
        let before = state.clone();
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save_with(&f.state_root(), Some(AtomicFailure::Rename));

        let error = remove(&mut state, RemovalTarget::Checkout(key), &mut handles, save)
            .expect_err("the injected rename failure must surface");

        assert!(!persist_error(error).replacement_committed());
        assert_eq!(state, before);
        assert_eq!(fs::read(f.state_path()).expect("read"), before_bytes);
        assert!(
            !managed.exists(),
            "the disk stage completed before the save failed"
        );
        assert_eq!(calls.get(), 1, "one attempt, no internal retry");

        let (save, calls) = counting_save(&f.state_root());
        let report = remove(&mut state, RemovalTarget::Checkout(key), &mut handles, save)
            .expect("retry with a working save");

        assert_eq!(report.already_removed, vec![managed.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert!(!has_checkout(&state, key));
        assert!(!has_checkout(&load_state(&f.state_root()), key));
        assert_eq!(calls.get(), 1);
    }

    /// The one pre-rename failure with a live runtime: the kill stage has
    /// run to completion when the save fails, so the restored row still says
    /// Running while its process is gone. The handle stays owned by the
    /// caller and the retry completes through plan 08's `Extinct`
    /// disposition without a second signal.
    #[test]
    fn sync_failure_keeps_prior_state_and_file_bytes() {
        let f = RemovalFixture::new("sync-failure");
        let (mut state, key, managed, pty) = live_row(&f, "wt-sync");
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        state.validate_for_save().expect("seed is save-valid");
        let before_bytes = write_seed(&f, &state);
        let before = state.clone();
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(key),
            pty,
            None,
            &managed,
            &f.repo(),
        )]);
        let (save, calls) = counting_save_with(&f.state_root(), Some(AtomicFailure::Sync));

        let error = remove(
            &mut state,
            RemovalTarget::Checkout(key),
            &mut handles.0,
            save,
        )
        .expect_err("the injected sync failure must surface");

        assert!(!persist_error(error).replacement_committed());
        assert!(
            group_is_extinct(pgid),
            "the kill stage ran to completion before the save failed"
        );
        assert_reaped_promptly(pid);
        assert_eq!(
            handles.0.len(),
            1,
            "an error never drains the caller's handles"
        );
        assert_eq!(handles.0[0].row, RowKey::Checkout(key));
        assert!(
            handles.0[0].session.claude.reader_finished(),
            "nothing outside the group held the terminal, so the reader was joined"
        );
        assert_eq!(state, before);
        assert_eq!(fs::read(f.state_path()).expect("read"), before_bytes);
        assert!(!managed.exists());
        assert_eq!(calls.get(), 1);

        let (save, calls) = counting_save(&f.state_root());
        let report = remove(
            &mut state,
            RemovalTarget::Checkout(key),
            &mut handles.0,
            save,
        )
        .expect("retry with a working save");

        assert_eq!(report.already_removed, vec![managed.clone()]);
        assert_eq!(report.runtimes_killed.len(), 1);
        assert_eq!(report.runtimes_killed[0].agent_pid, pid);
        assert!(report.readers_parked.is_empty());
        assert!(handles.0.is_empty(), "the retry consumes the handle");
        assert!(!has_checkout(&state, key));
        assert!(!has_checkout(&load_state(&f.state_root()), key));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn directory_sync_failure_commits_state_and_reports_degraded() {
        let f = RemovalFixture::new("dirsync-failure");
        let (mut state, _k, key, managed) = inactive_row(&f, "wt-dirsync");
        write_seed(&f, &state);
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save_with(&f.state_root(), Some(AtomicFailure::DirectorySync));

        let report = remove(&mut state, RemovalTarget::Checkout(key), &mut handles, save)
            .expect("a committed replacement is a degraded success");

        assert!(matches!(
            report.persistence,
            PersistenceOutcome::SavedDegraded { .. }
        ));
        assert!(!has_checkout(&state, key));
        assert_eq!(load_state(&f.state_root()), state);
        assert!(!managed.exists());
        assert_eq!(report.paths_deleted, vec![managed.clone()]);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn refused_row_failure_names_row_and_keeps_prior_state() {
        let f = RemovalFixture::new("refused-row");
        let (mut state, _k, c1, c2, c1_dir, c2_dir) = inactive_pair(&f);
        let before_bytes = write_seed(&f, &state);
        let before = state.clone();
        let mut handles: Vec<RowRuntime> = Vec::new();
        let refused = ValidationRow::Checkout(c2);
        let (save, calls) =
            counting_save_with(&f.state_root(), Some(AtomicFailure::RefusedRow(refused)));

        let error = remove(&mut state, RemovalTarget::Checkout(c1), &mut handles, save)
            .expect_err("the refused row must surface");

        let error = persist_error(error);
        assert!(!error.replacement_committed());
        assert_eq!(
            error.validation().and_then(ValidationError::row),
            Some(refused),
            "the refusal names the survivor, not the target"
        );
        let message = error.to_string();
        assert!(
            message.contains(&format!("{c2:?}")),
            "message {message:?} must name the refused row"
        );
        assert_eq!(state, before);
        assert_eq!(fs::read(f.state_path()).expect("read"), before_bytes);
        assert!(!c1_dir.exists());
        assert!(c2_dir.exists());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn save_closure_is_called_exactly_once_for_parent_with_children() {
        let f = RemovalFixture::new("single-save");
        let (mut state, k, _c1, _c2, c1_dir, c2_dir) = inactive_pair(&f);
        write_seed(&f, &state);
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository");

        assert_eq!(calls.get(), 1, "one save for the parent and both children");
        assert!(!state.repositories.iter().any(|r| r.key == k));
        assert!(!state.checkouts.iter().any(|c| c.repository_key == k));
        assert!(!c1_dir.exists());
        assert!(!c2_dir.exists());
        assert!(f.repo().exists(), "the main worktree is kept");
        assert_eq!(report.paths_kept, vec![f.repo()]);
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert_eq!(load_state(&f.state_root()), state);
    }

    // ---- stages run apart (plan 22.1-08) ----

    /// A save closure that keeps a copy of the `StateFile` it was handed and
    /// writes it under `state_root`, plus a count of its calls.
    fn recording_save(
        state_root: &Path,
    ) -> (
        impl FnOnce(&StateFile) -> Result<(), SaveError>,
        Rc<std::cell::RefCell<Option<RepositoryState>>>,
        Rc<Cell<u32>>,
    ) {
        let received = Rc::new(std::cell::RefCell::new(None));
        let calls = Rc::new(Cell::new(0));
        let sink = Rc::clone(&received);
        let counter = Rc::clone(&calls);
        let root = state_root.to_path_buf();
        let save = move |file: &StateFile| {
            counter.set(counter.get() + 1);
            *sink.borrow_mut() = Some(file.state.clone());
            persist::save_current_at_test(&root, "state.json", file, None, None)
        };
        (save, received, calls)
    }

    fn has_repository(state: &RepositoryState, key: RepositoryKey) -> bool {
        state
            .repositories
            .iter()
            .any(|repository| repository.key == key)
    }

    /// A repository admitted while the kill and delete ran without the lock
    /// survives the commit: the save describes the state `commit` is handed
    /// minus the removed rows, not a candidate built before the window.
    #[test]
    fn commit_rederives_the_candidate_from_current_state() {
        let f = RemovalFixture::new("rederive");
        let (mut state, _a, a_checkout, managed) = inactive_row(&f, "wt-rederive");
        write_seed(&f, &state);
        let mut handles: Vec<RowRuntime> = Vec::new();

        let prepared =
            prepare(&state, RemovalTarget::Checkout(a_checkout), &handles).expect("prepare");
        let effects = stop_and_delete(&prepared, &mut handles).expect("stop and delete");
        assert!(
            !managed.exists(),
            "the disk stage deleted A's managed worktree"
        );

        let b_main = f.root.join("b");
        fs::create_dir_all(&b_main).expect("create repository B's main worktree");
        let b = repository(&mut state, &b_main);
        let b_checkout = checkout(&mut state, b, CheckoutRole::Main, false, &b_main);
        state
            .validate_for_save()
            .expect("the state with B admitted is save-valid");
        let (save, received, calls) = recording_save(&f.state_root());

        commit(&mut state, prepared, effects, &mut handles, save).expect("commit");

        assert!(
            has_repository(&state, b),
            "repository B, admitted between stop_and_delete and commit, is missing from the state after commit"
        );
        assert!(
            has_checkout(&state, b_checkout),
            "B's checkout is missing from the state after commit"
        );
        assert!(!has_checkout(&state, a_checkout));
        assert_eq!(calls.get(), 1, "one save");
        let saved = received
            .borrow_mut()
            .take()
            .expect("the save closure received a StateFile");
        assert!(
            has_repository(&saved, b),
            "repository B is missing from the saved StateFile"
        );
        assert!(has_checkout(&saved, b_checkout));
        assert!(!has_checkout(&saved, a_checkout));
        assert_eq!(saved, state);
        assert_eq!(load_state(&f.state_root()), state);
    }

    /// A checkout added to the target repository after `prepare` would be
    /// orphaned by a save of the prepared rows. `commit` refuses with
    /// `SelectionChanged`, naming both row lists, and leaves the state, the
    /// file and the save closure untouched. The assertions read the error's
    /// Debug name, Display and source, the forms a caller sees.
    #[test]
    fn commit_refuses_when_the_selected_rows_changed() {
        let f = RemovalFixture::new("selection-changed");
        let (mut state, k, _child, managed) = inactive_row(&f, "wt-selection");
        let before_bytes = write_seed(&f, &state);
        let mut handles: Vec<RowRuntime> = Vec::new();
        let target = RemovalTarget::Repository(k);

        let prepared = prepare(&state, target, &handles).expect("prepare");
        let expected: Vec<RowKey> = prepared.set.rows.iter().map(|row| row.row).collect();
        let effects = stop_and_delete(&prepared, &mut handles).expect("stop and delete");
        assert!(
            !managed.exists(),
            "the disk stage deleted the managed child"
        );

        let late = f.root.join("late");
        fs::create_dir_all(&late).expect("create the late checkout");
        checkout(&mut state, k, CheckoutRole::ManagedBranch, false, &late);
        state
            .validate_for_save()
            .expect("the state with the late checkout is save-valid");
        let found: Vec<RowKey> = select_rows(&state, target)
            .expect("the target still resolves")
            .rows
            .iter()
            .map(|row| row.row)
            .collect();
        assert_ne!(found, expected, "the fixture changed the selection");
        let before = state.clone();
        let (save, calls) = counting_save(&f.state_root());

        let error = commit(&mut state, prepared, effects, &mut handles, save)
            .expect_err("commit must refuse a changed selection");

        let debug = format!("{error:?}");
        assert!(
            debug.starts_with("SelectionChanged"),
            "commit must refuse with SelectionChanged, got {debug}"
        );
        let message = error.to_string();
        assert!(
            message.contains(&format!("{expected:?}")),
            "message {message:?} must name the prepared rows {expected:?}"
        );
        assert!(
            message.contains(&format!("{found:?}")),
            "message {message:?} must name the rows the target selects now {found:?}"
        );
        assert!(
            std::error::Error::source(&error).is_none(),
            "SelectionChanged wraps no other error"
        );
        assert_eq!(state, before, "the state is untouched");
        assert_eq!(calls.get(), 0, "the save closure is never called");
        assert_eq!(fs::read(f.state_path()).expect("read"), before_bytes);
    }

    // ---- crash and disk failure between delete and save (plan 05) ----

    use crate::git::ReconciliationUnavailable;
    use crate::lifecycle;

    /// Pins `<child>/pin` read-only with `pin/held` inside so neither git nor
    /// the fallback can unlink it; restores `0o755` on drop so the retry can
    /// reclaim the child and the fixture root can be removed even after a
    /// failed assertion. The fault lives inside one child only: the managed
    /// root and every sibling keep their modes.
    struct PinGuard(PathBuf);

    impl PinGuard {
        fn new(child: &Path) -> Self {
            let pin = child.join("pin");
            fs::create_dir(&pin).expect("create pin");
            fs::write(pin.join("held"), b"held\n").expect("write held");
            set_mode(&pin, 0o555);
            Self(pin)
        }
    }

    impl Drop for PinGuard {
        fn drop(&mut self) {
            set_mode(&self.0, 0o755);
        }
    }

    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set fixture mode");
    }

    /// The existing startup reconcile applied to one managed child whose
    /// directory vanished: classified `Missing`, recorded as
    /// `Protected(Missing)`.
    fn reconcile_missing(
        state: &mut RepositoryState,
        f: &RemovalFixture,
        key: CheckoutKey,
        path: &Path,
        branch: &str,
    ) {
        let outcome =
            crate::git::reconcile_checkout(&f.common_dir(), path, Some(branch)).map(|_| ());
        assert!(
            matches!(outcome, Err(ReconciliationUnavailable::Missing { .. })),
            "expected Missing, got {outcome:?}"
        );
        assert_eq!(
            lifecycle::record_checkout_reconciliation(state, key, &outcome),
            Ok(Some(UnavailableCause::Missing))
        );
        assert_eq!(
            checkout_mut(state, key).lifecycle(),
            &CheckoutLifecycle::Protected(UnavailableCause::Missing)
        );
    }

    /// SC5 for a checkout target: a crash between the disk delete and the
    /// save leaves the prior file, which still parses and validates; the
    /// startup reconcile marks the vanished child Missing and the retry
    /// completes the removal.
    #[test]
    fn crash_before_save_reconciles_missing() {
        let f = RemovalFixture::new("crash-checkout");
        let (mut state, _k, c, managed) = inactive_row(&f, "x");
        write_seed(&f, &state);
        let before = state.clone();
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save_with(&f.state_root(), Some(AtomicFailure::Write));

        let error = remove(&mut state, RemovalTarget::Checkout(c), &mut handles, save)
            .expect_err("the injected write failure must surface");

        assert!(!persist_error(error).replacement_committed());
        assert!(!managed.exists());
        assert_eq!(calls.get(), 1);

        let mut loaded = load_state(&f.state_root());
        assert_eq!(
            loaded, before,
            "parent and child are both still in the file"
        );
        assert_eq!(loaded.validate(), Ok(()));
        reconcile_missing(&mut loaded, &f, c, &managed, "feature/x");
        assert_eq!(loaded.validate(), Ok(()));

        let (save, calls) = counting_save(&f.state_root());
        let report = remove(&mut loaded, RemovalTarget::Checkout(c), &mut handles, save)
            .expect("retry on the reconciled state");

        assert_eq!(report.already_removed, vec![managed.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert!(!has_checkout(&loaded, c));
        assert!(!has_checkout(&load_state(&f.state_root()), c));
        assert_eq!(calls.get(), 1);
    }

    /// SC5 for a repository target mid-fan-out: every child directory is
    /// gone when the save fails, yet the file still holds the parent and
    /// both children, so no child is ever orphaned from its parent.
    #[test]
    fn repository_crash_mid_fan_out_keeps_parent_and_children_in_file() {
        let f = RemovalFixture::new("crash-repository");
        let (mut state, k, c1, c2, c1_dir, c2_dir) = inactive_pair(&f);
        write_seed(&f, &state);
        let before = state.clone();
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save_with(&f.state_root(), Some(AtomicFailure::Write));

        let error = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect_err("the injected write failure must surface");

        assert!(!persist_error(error).replacement_committed());
        assert!(!c1_dir.exists(), "the disk stage completed for every row");
        assert!(!c2_dir.exists());
        assert!(f.repo().exists());
        assert_eq!(calls.get(), 1);

        let mut loaded = load_state(&f.state_root());
        assert_eq!(loaded, before);
        assert!(loaded.repositories.iter().any(|r| r.key == k));
        assert!(has_checkout(&loaded, c1));
        assert!(has_checkout(&loaded, c2));
        assert_eq!(loaded.validate(), Ok(()));
        reconcile_missing(&mut loaded, &f, c1, &c1_dir, "feature/one");
        reconcile_missing(&mut loaded, &f, c2, &c2_dir, "feature/two");
        assert_eq!(loaded.validate(), Ok(()));

        let (save, calls) = counting_save(&f.state_root());
        let report = remove(
            &mut loaded,
            RemovalTarget::Repository(k),
            &mut handles,
            save,
        )
        .expect("retry on the reconciled state");

        assert_eq!(report.already_removed, vec![c1_dir.clone(), c2_dir.clone()]);
        assert_eq!(report.paths_kept, vec![f.repo()]);
        assert!(report.paths_deleted.is_empty());
        assert!(!loaded.checkouts.iter().any(|c| c.repository_key == k));
        assert!(!loaded.repositories.iter().any(|r| r.key == k));
        assert_eq!(load_state(&f.state_root()), loaded);
        assert_eq!(calls.get(), 1);
    }

    /// A per-child disk fault on the second child after the first is gone:
    /// git drops c2's administrative entry but cannot unlink `pin/held`, the
    /// guarded fallback fails on the same entry, and `remove` returns before
    /// the save with state and file untouched. The retry re-selects c1 as
    /// already removed and reclaims c2 through plan 02's removal marker.
    #[test]
    fn repository_disk_failure_mid_fan_out_leaves_state_and_file_untouched() {
        let f = RemovalFixture::new("disk-failure");
        let (mut state, k, _c1, c2, c1_dir, c2_dir) = inactive_pair(&f);
        let before_bytes = write_seed(&f, &state);
        let before = state.clone();
        let guard = PinGuard::new(&c2_dir);
        let marker = crate::git::removal_marker_path(&c2_dir);
        let c2_text = c2_dir.to_str().expect("utf-8");
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save(&f.state_root());

        let error = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect_err("c2 cannot be deleted while pin is read-only");

        match error {
            RemovalError::DiskRemoval { row, path, error } => {
                assert_eq!(row, RowKey::Checkout(c2));
                assert_eq!(path, c2_dir);
                assert_eq!(error.kind, DiskRemovalFailureKind::Fallback, "{error}");
            }
            other => panic!("expected DiskRemoval, got {other:?}"),
        }
        assert!(
            !c1_dir.exists(),
            "the first child was deleted before c2 failed"
        );
        assert!(c2_dir.exists());
        assert!(
            !worktree_list(&f.repo()).contains(c2_text),
            "git dropped the administrative entry despite the failed unlink"
        );
        assert!(marker.exists(), "the removal marker persists for the retry");
        assert_eq!(state, before);
        assert_eq!(fs::read(f.state_path()).expect("read"), before_bytes);
        assert_eq!(calls.get(), 0);

        drop(guard);
        let (save, calls) = counting_save(&f.state_root());
        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("retry after the cause is cleared");

        assert_eq!(report.already_removed, vec![c1_dir.clone()]);
        assert_eq!(report.paths_deleted, vec![c2_dir.clone()]);
        assert_eq!(report.paths_kept, vec![f.repo()]);
        assert!(!c2_dir.exists());
        assert!(!marker.exists());
        assert!(!state.checkouts.iter().any(|c| c.repository_key == k));
        assert!(!state.repositories.iter().any(|r| r.key == k));
        assert_eq!(load_state(&f.state_root()), state);
        assert_eq!(calls.get(), 1);
    }

    // ---- candidate validation precedes kill and delete (plan 05) ----

    /// The shape an older state file can carry: an `Inactive` row that still
    /// records a runtime. `validate()` accepts it and `validate_for_save()`
    /// refuses it, which is what makes it the right premise for proving the
    /// refusal arrives before anything irreversible.
    fn stale_runtime() -> OwnedRuntime {
        OwnedRuntime {
            generation: RuntimeGeneration::initial(),
            agent: ProcessIdentity {
                pid: 100,
                start_time: 200,
                process_group: 100,
                session: 100,
            },
            shell: ShellOwnership::Closed,
        }
    }

    #[test]
    fn validation_failure_is_detected_before_any_kill_or_delete() {
        let f = RemovalFixture::new("validation-first");
        let (mut state, t, t_dir, pty) = live_row(&f, "wt-t");
        let pgid = pty.process_identity().process_group;
        let other_main = f.root.join("other");
        let j = repository(&mut state, &other_main);
        let u = checkout(
            &mut state,
            j,
            CheckoutRole::ManagedBranch,
            true,
            &other_main.join("u"),
        );
        checkout_mut(&mut state, u).set_owned_runtime(Some(stale_runtime()));
        assert!(state.validate().is_ok(), "validate accepts the stale shape");
        assert!(
            state.validate_for_save().is_err(),
            "validate_for_save refuses the stale shape"
        );
        let before = state.clone();
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(t),
            pty,
            None,
            &t_dir,
            &f.repo(),
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let error = remove(&mut state, RemovalTarget::Checkout(t), &mut handles.0, save)
            .expect_err("a save-invalid candidate must be refused");

        assert!(
            matches!(error, RemovalError::Validation(_)),
            "expected Validation, got {error:?}"
        );
        assert!(group_is_alive(pgid), "group {pgid} was never signalled");
        assert!(t_dir.exists(), "the directory was never deleted");
        assert_eq!(state, before);
        assert!(
            !f.state_path().exists(),
            "the save closure was never called, so no file was ever written"
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(handles.0.len(), 1);
    }

    // ---- integration matrix over the cloned fixture (plan 06) ----

    use crate::testing::{cloned_topology, git_in, ClonedTopology};

    #[test]
    fn cloned_topology_places_worktrees_under_managed_root() {
        let f = RemovalFixture::new("cloned-placement");
        let t = cloned_topology(&f.root);
        let base = fs::canonicalize(crate::git::worktrees_base()).expect("canonical managed root");
        assert!(
            t.default_worktree.starts_with(&base),
            "{} must sit under {}",
            t.default_worktree.display(),
            base.display()
        );
        assert!(
            t.managed_worktree.starts_with(&base),
            "{} must sit under {}",
            t.managed_worktree.display(),
            base.display()
        );
        assert!(
            !t.clone.starts_with(&base),
            "the clone {} must stay outside the managed root",
            t.clone.display()
        );
        let refs = git_in(
            &t.clone,
            &["for-each-ref", "--format=%(refname)", "refs/heads"],
        );
        for branch in [
            "refs/heads/main",
            "refs/heads/chore/workspace",
            "refs/heads/feature/kept",
        ] {
            assert!(refs.contains(branch), "{branch} missing from {refs:?}");
        }
        let listed = git_in(&t.clone, &["worktree", "list", "--porcelain"])
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count();
        assert_eq!(
            listed, 3,
            "clone, default and managed worktrees must be listed"
        );
    }

    use crate::repository::CheckoutHealth;

    /// Sorted `refname objectname` lines for every local branch. Never `git
    /// branch --list`: its `+` and `*` decorations legitimately change when a
    /// linked worktree disappears, which is exactly what these tests do.
    fn branch_snapshot(repo: &Path) -> Vec<String> {
        let mut lines: Vec<String> = git_in(
            repo,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/heads",
            ],
        )
        .lines()
        .map(str::to_string)
        .collect();
        lines.sort();
        lines
    }

    /// The rows the cloned topology seeds: repository `k`, Main `m` at the
    /// clone, managed PrimaryDefault `d` and managed ManagedBranch `b`.
    struct ClonedRows {
        k: RepositoryKey,
        m: CheckoutKey,
        d: CheckoutKey,
        b: CheckoutKey,
    }

    /// Seed the topology through plan 01's builders: one role of each kind,
    /// distinct paths, the common dir discovery reported for the clone.
    fn seed_cloned(state: &mut RepositoryState, t: &ClonedTopology) -> ClonedRows {
        let k = state.allocate_repository_key().expect("repository key");
        let order = state.allocate_first_seen_order().expect("order");
        state.repositories.push(SavedRepository {
            key: k,
            observed_common_dir: persisted(&t.common_dir),
            observed_main_worktree: persisted(&t.clone),
            first_seen_order: order,
            health: RepositoryHealth::Available,
            physical_key: String::new(),
        });
        let m = checkout(state, k, CheckoutRole::Main, false, &t.clone);
        let d = checkout(
            state,
            k,
            CheckoutRole::PrimaryDefault,
            true,
            &t.default_worktree,
        );
        let b = checkout(
            state,
            k,
            CheckoutRole::ManagedBranch,
            true,
            &t.managed_worktree,
        );
        ClonedRows { k, m, d, b }
    }

    /// A second linked worktree of the clone, outside the managed root, on
    /// `feature/outside`: the shape of a checkout baude did not create.
    fn outside_worktree(f: &RemovalFixture, t: &ClonedTopology) -> PathBuf {
        let path = f.root.join("outside-wt");
        git_in(
            &t.clone,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature/outside",
                path.to_str().expect("utf-8 outside path"),
            ],
        );
        fs::canonicalize(&path).expect("canonicalize outside worktree")
    }

    /// A standalone folder with one file, and its row in `state`.
    fn notes_folder(f: &RemovalFixture) -> PathBuf {
        let notes = f.root.join("notes");
        fs::create_dir_all(&notes).expect("create notes folder");
        fs::write(notes.join("todo.md"), b"- keep me\n").expect("write notes file");
        notes
    }

    fn seed_notes(
        state: &mut RepositoryState,
        notes: &Path,
        lifecycle: StandaloneLifecycle,
        runtime: Option<OwnedRuntime>,
    ) -> StandaloneKey {
        let key = state.allocate_standalone_key().expect("standalone key");
        let order = state.allocate_first_seen_order().expect("order");
        state.standalone_sessions.push(SavedStandaloneSession::new(
            key,
            persisted(notes),
            order,
            lifecycle,
            runtime,
            RetainedStandaloneSessionState {
                name: "notes".into(),
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
                ever_launched: false,
            },
        ));
        key
    }

    fn no_handles() -> Vec<RowRuntime> {
        Vec::new()
    }

    #[test]
    fn main_checkout_keeps_directory_and_branches() {
        let f = RemovalFixture::new("cloned-main-kept");
        let t = cloned_topology(&f.root);
        let before = branch_snapshot(&t.clone);
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Checkout(rows.m),
            &mut handles,
            save,
        )
        .expect("remove main checkout");

        assert!(t.clone.exists(), "the main checkout must stay on disk");
        assert!(t.clone.join("tracked").exists());
        assert_eq!(report.paths_kept, vec![t.clone.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert!(!has_checkout(&state, rows.m));
        assert!(has_checkout(&state, rows.d));
        assert!(has_checkout(&state, rows.b));
        assert!(state.repositories.iter().any(|r| r.key == rows.k));
        state.validate().expect("state validates after removal");
        assert_eq!(branch_snapshot(&t.clone), before);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn standalone_keeps_folder() {
        let f = RemovalFixture::new("standalone-kept");
        let notes = notes_folder(&f);
        let mut state = RepositoryState::default();
        let key = seed_notes(&mut state, &notes, StandaloneLifecycle::Inactive, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Standalone(key),
            &mut handles,
            save,
        )
        .expect("remove standalone");

        assert!(notes.exists(), "the standalone folder must stay on disk");
        assert!(notes.join("todo.md").exists());
        assert_eq!(report.paths_kept, vec![notes.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert!(state.standalone_sessions.is_empty());
        assert!(load_state(&f.state_root()).standalone_sessions.is_empty());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn already_removed_managed_directory_drops_row() {
        let f = RemovalFixture::new("cloned-already-removed");
        let t = cloned_topology(&f.root);
        let oid_before = git_in(&t.clone, &["rev-parse", "feature/kept"]);
        fs::remove_dir_all(&t.managed_worktree).expect("remove managed directory without prune");
        let wt_text = t.managed_worktree.to_str().expect("utf-8");

        // Part A: an Inactive row whose directory is gone.
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());
        let report = remove(
            &mut state,
            RemovalTarget::Checkout(rows.b),
            &mut handles,
            save,
        )
        .expect("already-removed inactive row");
        assert_eq!(report.already_removed, vec![t.managed_worktree.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert!(!has_checkout(&state, rows.b));
        assert!(!has_checkout(&load_state(&f.state_root()), rows.b));
        assert!(
            !worktree_list(&t.clone).contains(wt_text),
            "the administrative entry must be pruned"
        );
        assert_eq!(git_in(&t.clone, &["rev-parse", "feature/kept"]), oid_before);
        assert_eq!(calls.get(), 1);

        // Part B: the same row already Protected(Missing), on a fresh state.
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        checkout_mut(&mut state, rows.b)
            .set_lifecycle(CheckoutLifecycle::Protected(UnavailableCause::Missing));
        assert_eq!(
            checkout_mut(&mut state, rows.b).health(),
            &CheckoutHealth::Unavailable(UnavailableCause::Missing)
        );
        state
            .validate_for_save()
            .expect("protected seed is save-valid");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());
        let report = remove(
            &mut state,
            RemovalTarget::Checkout(rows.b),
            &mut handles,
            save,
        )
        .expect("already-removed protected row");
        assert_eq!(report.already_removed, vec![t.managed_worktree.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert!(!has_checkout(&state, rows.b));
        assert!(!has_checkout(&load_state(&f.state_root()), rows.b));
        assert!(!worktree_list(&t.clone).contains(wt_text));
        assert_eq!(git_in(&t.clone, &["rev-parse", "feature/kept"]), oid_before);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn unmanaged_checkout_keeps_directory() {
        let f = RemovalFixture::new("cloned-unmanaged-kept");
        let t = cloned_topology(&f.root);
        let outside = outside_worktree(&f, &t);
        let outside_text = outside.to_str().expect("utf-8");
        let before = branch_snapshot(&t.clone);
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        let x = checkout(
            &mut state,
            rows.k,
            CheckoutRole::ManagedBranch,
            false,
            &outside,
        );
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Checkout(x), &mut handles, save)
            .expect("remove unmanaged checkout");

        assert!(outside.exists(), "the unmanaged checkout must stay on disk");
        assert!(outside.join("tracked").exists());
        assert_eq!(report.paths_kept, vec![outside.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert!(
            worktree_list(&t.clone).contains(outside_text),
            "nothing may be pruned for an unmanaged checkout"
        );
        assert!(!has_checkout(&state, x));
        assert!(has_checkout(&state, rows.m));
        assert!(has_checkout(&state, rows.d));
        assert!(has_checkout(&state, rows.b));
        assert_eq!(branch_snapshot(&t.clone), before);
        assert!(before
            .iter()
            .any(|line| line.starts_with("refs/heads/feature/outside ")));
        assert_eq!(calls.get(), 1);
    }

    // ---- live keep rows, unclean cwd-held deletion, parent fan-out (plan 06) ----

    /// Make a worktree unclean the way a working session leaves it: one
    /// untracked file and one modified tracked file.
    fn make_unclean(worktree: &Path, tracked: &str) {
        fs::write(worktree.join("scratch.txt"), b"untracked\n").expect("write untracked file");
        fs::write(worktree.join(tracked), b"modified\n").expect("modify tracked file");
        assert!(
            !git_in(worktree, &["status", "--porcelain"]).is_empty(),
            "worktree must report as unclean"
        );
    }

    #[test]
    fn live_main_checkout_runtime_is_killed_and_directory_kept() {
        let f = RemovalFixture::new("cloned-live-main");
        let t = cloned_topology(&f.root);
        let before = branch_snapshot(&t.clone);
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        let pty = spawn_trapping_runtime(&t.clone);
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        seed_live(&mut state, rows.m, &pty, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(rows.m),
            pty,
            None,
            &t.clone,
            &t.clone,
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Checkout(rows.m),
            &mut handles.0,
            save,
        )
        .expect("remove live main checkout");

        assert!(group_is_extinct(pgid), "group {pgid} must be extinct");
        assert_reaped_promptly(pid);
        assert!(t.clone.exists(), "the main checkout must stay on disk");
        assert!(t.clone.join("tracked").exists());
        assert_eq!(report.paths_kept, vec![t.clone.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert_eq!(report.runtimes_killed.len(), 1);
        assert_eq!(report.runtimes_killed[0].agent_pid, pid);
        assert!(handles.0.is_empty(), "the consumed handle must be gone");
        assert!(!has_checkout(&state, rows.m));
        assert_eq!(branch_snapshot(&t.clone), before);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn live_standalone_runtime_is_killed_and_folder_kept() {
        let f = RemovalFixture::new("standalone-live");
        let notes = notes_folder(&f);
        let pty = spawn_trapping_runtime(&notes);
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        let runtime = OwnedRuntime {
            generation: RuntimeGeneration::initial(),
            agent: pty.process_identity().clone(),
            shell: ShellOwnership::Closed,
        };
        let mut state = RepositoryState::default();
        let key = seed_notes(
            &mut state,
            &notes,
            StandaloneLifecycle::Running(RuntimeGeneration::initial()),
            Some(runtime),
        );
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Standalone(key),
            pty,
            None,
            &notes,
            &notes,
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Standalone(key),
            &mut handles.0,
            save,
        )
        .expect("remove live standalone");

        assert!(group_is_extinct(pgid), "group {pgid} must be extinct");
        assert_reaped_promptly(pid);
        assert!(notes.exists(), "the standalone folder must stay on disk");
        assert!(notes.join("todo.md").exists());
        assert_eq!(report.paths_kept, vec![notes.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert_eq!(report.runtimes_killed.len(), 1);
        assert!(handles.0.is_empty(), "the consumed handle must be gone");
        assert!(state.standalone_sessions.is_empty());
        assert!(load_state(&f.state_root()).standalone_sessions.is_empty());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn live_unmanaged_checkout_runtime_is_killed_and_directory_kept() {
        let f = RemovalFixture::new("cloned-live-unmanaged");
        let t = cloned_topology(&f.root);
        let outside = outside_worktree(&f, &t);
        let outside_text = outside.to_str().expect("utf-8");
        let before = branch_snapshot(&t.clone);
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        let x = checkout(
            &mut state,
            rows.k,
            CheckoutRole::ManagedBranch,
            false,
            &outside,
        );
        let pty = spawn_trapping_runtime(&outside);
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        seed_live(&mut state, x, &pty, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(x),
            pty,
            None,
            &outside,
            &t.clone,
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Checkout(x), &mut handles.0, save)
            .expect("remove live unmanaged checkout");

        assert!(group_is_extinct(pgid), "group {pgid} must be extinct");
        assert_reaped_promptly(pid);
        assert!(outside.exists(), "the unmanaged checkout must stay on disk");
        assert!(outside.join("tracked").exists());
        assert_eq!(report.paths_kept, vec![outside.clone()]);
        assert!(report.paths_deleted.is_empty());
        assert_eq!(report.runtimes_killed.len(), 1);
        assert_eq!(report.runtimes_killed[0].agent_pid, pid);
        assert!(
            worktree_list(&t.clone).contains(outside_text),
            "the unmanaged registration must survive"
        );
        assert!(handles.0.is_empty(), "the consumed handle must be gone");
        assert!(!has_checkout(&state, x));
        assert!(has_checkout(&state, rows.m));
        assert!(has_checkout(&state, rows.d));
        assert!(has_checkout(&state, rows.b));
        assert_eq!(branch_snapshot(&t.clone), before);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn git_init_unclean_cwd_held_managed_is_deleted_and_main_kept() {
        let f = RemovalFixture::new("init-unclean-held");
        let wt_a = f.managed_worktree("wt-a", "feature/a");
        let wt_text = wt_a.to_str().expect("utf-8");
        make_unclean(&wt_a, "tracked.txt");
        let oid_before = git_ok(&f.repo(), &["rev-parse", "feature/a"]);
        let before = branch_snapshot(&f.repo());
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        let main = checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        let managed = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &wt_a);
        let pty = spawn_trapping_runtime(&wt_a);
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        seed_live(&mut state, managed, &pty, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(managed),
            pty,
            None,
            &wt_a,
            &f.repo(),
        )]);

        let (save, calls) = counting_save(&f.state_root());
        let report = remove(
            &mut state,
            RemovalTarget::Checkout(main),
            &mut handles.0,
            save,
        )
        .expect("remove main checkout");
        assert!(f.repo().exists(), "the main checkout must stay on disk");
        assert!(f.repo().join("tracked.txt").exists());
        assert_eq!(report.paths_kept, vec![f.repo()]);
        assert!(report.paths_deleted.is_empty());
        assert!(report.runtimes_killed.is_empty());
        assert!(
            group_is_alive(pgid),
            "the managed runtime must survive the main removal"
        );
        assert_eq!(
            handles.0.len(),
            1,
            "the managed handle must not be consumed"
        );
        assert_eq!(branch_snapshot(&f.repo()), before);
        assert_eq!(calls.get(), 1);

        let (save, calls) = counting_save(&f.state_root());
        let report = remove(
            &mut state,
            RemovalTarget::Checkout(managed),
            &mut handles.0,
            save,
        )
        .expect("remove unclean cwd-held managed checkout");
        assert!(!wt_a.exists(), "the managed directory must be deleted");
        assert!(group_is_extinct(pgid), "group {pgid} must be extinct");
        assert_reaped_promptly(pid);
        assert_eq!(report.paths_deleted, vec![wt_a.clone()]);
        assert_eq!(report.runtimes_killed.len(), 1);
        assert_eq!(git_ok(&f.repo(), &["rev-parse", "feature/a"]), oid_before);
        assert!(
            !worktree_list(&f.repo()).contains(wt_text),
            "the managed registration must be gone"
        );
        assert!(handles.0.is_empty());
        assert!(!has_checkout(&state, managed));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn managed_worktree_unclean_and_cwd_held_is_deleted() {
        let f = RemovalFixture::new("cloned-unclean-held");
        let t = cloned_topology(&f.root);
        let wt_text = t.managed_worktree.to_str().expect("utf-8");
        make_unclean(&t.managed_worktree, "tracked");
        let before = branch_snapshot(&t.clone);
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        let pty = spawn_trapping_runtime(&t.managed_worktree);
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        seed_live(&mut state, rows.b, &pty, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(rows.b),
            pty,
            None,
            &t.managed_worktree,
            &t.clone,
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Checkout(rows.b),
            &mut handles.0,
            save,
        )
        .expect("remove unclean cwd-held managed checkout");

        assert!(
            !t.managed_worktree.exists(),
            "the managed directory must be deleted"
        );
        assert!(group_is_extinct(pgid), "group {pgid} must be extinct");
        assert_reaped_promptly(pid);
        assert_eq!(report.paths_deleted, vec![t.managed_worktree.clone()]);
        assert!(report.paths_kept.is_empty());
        assert_eq!(report.runtimes_killed.len(), 1);
        assert!(
            !worktree_list(&t.clone).contains(wt_text),
            "the managed registration must be gone"
        );
        assert!(handles.0.is_empty());
        assert!(!has_checkout(&state, rows.b));
        assert_eq!(branch_snapshot(&t.clone), before);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn repository_removes_children_then_parent() {
        let f = RemovalFixture::new("cloned-fan-out");
        let t = cloned_topology(&f.root);
        let outside = outside_worktree(&f, &t);
        let outside_text = outside.to_str().expect("utf-8");
        let default_text = t.default_worktree.to_str().expect("utf-8");
        let managed_text = t.managed_worktree.to_str().expect("utf-8");
        let before = branch_snapshot(&t.clone);
        assert_eq!(
            before.len(),
            4,
            "main, chore/workspace, feature/kept, feature/outside"
        );
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        let x = checkout(
            &mut state,
            rows.k,
            CheckoutRole::ManagedBranch,
            false,
            &outside,
        );
        let pty = spawn_trapping_runtime(&t.default_worktree);
        let pid = pty.process_identity().pid;
        let pgid = pty.process_identity().process_group;
        seed_live(&mut state, rows.d, &pty, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = LiveHandles(vec![handle_for(
            RowKey::Checkout(rows.d),
            pty,
            None,
            &t.default_worktree,
            &t.clone,
        )]);
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Repository(rows.k),
            &mut handles.0,
            save,
        )
        .expect("remove repository");

        assert_eq!(
            report.rows_removed.last(),
            Some(&RowKey::Repository(rows.k))
        );
        let children = &report.rows_removed[..report.rows_removed.len() - 1];
        for child in [rows.m, rows.d, rows.b, x] {
            assert!(
                children.contains(&RowKey::Checkout(child)),
                "{child:?} must be removed before the parent"
            );
        }
        assert!(t.clone.exists(), "the main checkout must stay on disk");
        assert!(outside.exists(), "the unmanaged checkout must stay on disk");
        assert!(outside.join("tracked").exists());
        assert!(
            !t.default_worktree.exists(),
            "the default worktree must be deleted"
        );
        assert!(
            !t.managed_worktree.exists(),
            "the managed worktree must be deleted"
        );
        assert_eq!(report.paths_kept, vec![t.clone.clone(), outside.clone()]);
        assert_eq!(
            report.paths_deleted,
            vec![t.default_worktree.clone(), t.managed_worktree.clone()]
        );
        let list = worktree_list(&t.clone);
        assert!(
            list.contains(outside_text),
            "the unmanaged registration must survive"
        );
        assert!(
            !list.contains(default_text),
            "the default registration must be gone"
        );
        assert!(
            !list.contains(managed_text),
            "the managed registration must be gone"
        );
        assert!(group_is_extinct(pgid), "group {pgid} must be extinct");
        assert_reaped_promptly(pid);
        assert_eq!(report.runtimes_killed.len(), 1);
        assert!(handles.0.is_empty());
        assert!(!state.repositories.iter().any(|r| r.key == rows.k));
        assert!(!state.checkouts.iter().any(|c| c.repository_key == rows.k));
        state.validate().expect("state validates after fan-out");
        assert_eq!(load_state(&f.state_root()), state);
        assert_eq!(branch_snapshot(&t.clone), before);
        assert_eq!(calls.get(), 1);
    }
    // ---- binding clear as the fourth stage (phase 19 plan 01) ----

    use crate::breadcrumbs::{folder_key, load_json};
    use crate::folder_workspace::{self, FolderWorkspaceFile, FILE_NAME as BINDINGS_FILE};

    /// The one workspace-agnostic bindings file under the redirected config
    /// dir, read the way `find_binding` reads it.
    fn bindings_on_disk() -> FolderWorkspaceFile {
        load_json::<FolderWorkspaceFile>(&crate::persist::config_dir().join(BINDINGS_FILE)).0
    }

    #[test]
    fn clear_stage_removes_repository_binding_and_reports_it() {
        let f = RemovalFixture::new("clear-binding");
        let neighbour = f.root.join("neighbour");
        fs::create_dir_all(&neighbour).expect("create neighbour folder");
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        state.validate_for_save().expect("seed is save-valid");
        let config = crate::persist::config_dir();
        // Both folders bound to the same workspace: the READM-05 neighbour.
        folder_workspace::record(Some(&config), &f.repo(), "shared-ws", 10);
        folder_workspace::record(Some(&config), &neighbour, "shared-ws", 20);
        let repo_key = folder_key(&f.repo());
        let neighbour_key = folder_key(&neighbour);
        let before = bindings_on_disk();
        assert!(before.folders.contains_key(&repo_key));
        let neighbour_before = before.folders[&neighbour_key].clone();
        let state_before = state.clone();
        let preview =
            preview_removal(&state_before, RemovalTarget::Repository(k)).expect("preview");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository");

        let after = bindings_on_disk();
        assert!(
            !after.folders.contains_key(&repo_key),
            "the removed repository's binding must be gone"
        );
        assert_eq!(
            after.folders.get(&neighbour_key),
            Some(&neighbour_before),
            "a neighbour sharing the workspace value keeps its entry unchanged"
        );
        assert_eq!(after.folders.len(), 1);
        assert_eq!(after.schema_version, folder_workspace::SCHEMA_VERSION);
        assert_eq!(
            folder_workspace::find_binding(&config, &f.repo(), &f.root),
            None
        );
        assert_eq!(
            folder_workspace::find_binding(&config, &neighbour, &f.root).as_deref(),
            Some("shared-ws")
        );
        assert_eq!(
            report.cleared,
            vec![ClearedItem {
                kind: ClearingKind::Binding,
                path: f.repo(),
            }]
        );
        assert!(report.clearing_failures.is_empty());
        assert_eq!(preview.cleared, report.cleared);
        assert!(preview.clearing_failures.is_empty());
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert!(!state.repositories.iter().any(|r| r.key == k));
        assert!(!load_state(&f.state_root())
            .repositories
            .iter()
            .any(|r| r.key == k));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn checkout_and_standalone_removals_clear_their_own_keys() {
        let f = RemovalFixture::new("clear-own-keys");
        let config = crate::persist::config_dir();

        // Part A: an unmanaged checkout clears its observed path and leaves
        // the repository's main-worktree key alone.
        let t = cloned_topology(&f.root);
        let outside = outside_worktree(&f, &t);
        let mut state = RepositoryState::default();
        let rows = seed_cloned(&mut state, &t);
        let x = checkout(
            &mut state,
            rows.k,
            CheckoutRole::ManagedBranch,
            false,
            &outside,
        );
        state.validate_for_save().expect("seed is save-valid");
        folder_workspace::record(Some(&config), &t.clone, "repo-ws", 10);
        folder_workspace::record(Some(&config), &outside, "outside-ws", 20);
        let main_key = folder_key(&t.clone);
        let outside_key = folder_key(&outside);
        let main_before = bindings_on_disk().folders[&main_key].clone();
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Checkout(x), &mut handles, save)
            .expect("remove unmanaged checkout");

        let after = bindings_on_disk();
        assert!(!after.folders.contains_key(&outside_key));
        assert_eq!(after.folders.get(&main_key), Some(&main_before));
        assert_eq!(
            report.cleared,
            vec![
                ClearedItem {
                    kind: ClearingKind::Binding,
                    path: outside.clone(),
                },
                ClearedItem {
                    kind: ClearingKind::SuppressionAdded,
                    path: outside.clone(),
                },
            ]
        );
        assert!(report.clearing_failures.is_empty());
        assert!(has_checkout(&state, rows.m));
        assert_eq!(calls.get(), 1);

        // Part B: a standalone row clears its canonical path.
        let notes = notes_folder(&f);
        let mut state = RepositoryState::default();
        let key = seed_notes(&mut state, &notes, StandaloneLifecycle::Inactive, None);
        state.validate_for_save().expect("seed is save-valid");
        folder_workspace::record(Some(&config), &notes, "notes-ws", 30);
        let notes_key = folder_key(&notes);
        assert!(bindings_on_disk().folders.contains_key(&notes_key));
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Standalone(key),
            &mut handles,
            save,
        )
        .expect("remove standalone");

        let after = bindings_on_disk();
        assert!(!after.folders.contains_key(&notes_key));
        assert_eq!(after.folders.get(&main_key), Some(&main_before));
        assert_eq!(
            report.cleared,
            vec![ClearedItem {
                kind: ClearingKind::Binding,
                path: notes.clone(),
            }]
        );
        assert!(report.clearing_failures.is_empty());
        assert!(notes.exists(), "the standalone folder must stay on disk");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn standalone_without_binding_leaves_bindings_file_byte_identical() {
        let f = RemovalFixture::new("clear-no-binding");
        let bindings_path = crate::persist::config_dir().join(BINDINGS_FILE);
        let notes = notes_folder(&f);

        // Part A: no bindings file at all; the removal must not create one.
        assert!(
            !bindings_path.exists(),
            "fixture starts without a bindings file"
        );
        let mut state = RepositoryState::default();
        let key = seed_notes(&mut state, &notes, StandaloneLifecycle::Inactive, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());
        let report = remove(
            &mut state,
            RemovalTarget::Standalone(key),
            &mut handles,
            save,
        )
        .expect("remove standalone without bindings file");
        assert!(
            !bindings_path.exists(),
            "an absent bindings file must stay absent"
        );
        assert!(report.cleared.is_empty());
        assert!(report.clearing_failures.is_empty());
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert_eq!(calls.get(), 1);

        // Part B: a file holding only a neighbour; the bytes must not move.
        let neighbour = f.root.join("neighbour");
        fs::create_dir_all(&neighbour).expect("create neighbour folder");
        folder_workspace::record(
            Some(&crate::persist::config_dir()),
            &neighbour,
            "neighbour-ws",
            40,
        );
        let bytes_before = fs::read(&bindings_path).expect("read bindings before");
        let mut state = RepositoryState::default();
        let key = seed_notes(&mut state, &notes, StandaloneLifecycle::Inactive, None);
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());
        let report = remove(
            &mut state,
            RemovalTarget::Standalone(key),
            &mut handles,
            save,
        )
        .expect("remove standalone beside a neighbour binding");
        assert_eq!(
            fs::read(&bindings_path).expect("read bindings after"),
            bytes_before,
            "a file without the removed key must be byte-identical"
        );
        assert!(report.cleared.is_empty());
        assert!(report.clearing_failures.is_empty());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn binding_clear_failure_is_reported_not_raised() {
        let f = RemovalFixture::new("clear-failure");
        let config = crate::persist::config_dir();
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        state.validate_for_save().expect("seed is save-valid");
        folder_workspace::record(Some(&config), &f.repo(), "repo-ws", 10);
        let repo_key = folder_key(&f.repo());
        assert!(bindings_on_disk().folders.contains_key(&repo_key));
        // A directory where the merge lock file goes makes the locked write
        // fail to open, after the state save has already committed. `record`
        // leaves its lock file behind, so clear that first.
        let lock_path = config.join(format!(".{BINDINGS_FILE}.lock"));
        let _ = fs::remove_file(&lock_path);
        fs::create_dir_all(&lock_path).expect("occupy the lock path with a directory");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("a binding failure must not fail the removal");

        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert!(report.cleared.is_empty());
        assert_eq!(report.clearing_failures.len(), 1);
        let failure = &report.clearing_failures[0];
        assert_eq!(failure.kind, ClearingKind::Binding);
        assert_eq!(failure.path, f.repo());
        assert!(
            !failure.error.is_empty(),
            "the failure must carry the io error text"
        );
        assert!(
            bindings_on_disk().folders.contains_key(&repo_key),
            "the binding stays when the clear could not run"
        );
        assert!(!state.repositories.iter().any(|r| r.key == k));
        assert!(!load_state(&f.state_root())
            .repositories
            .iter()
            .any(|r| r.key == k));
        assert_eq!(calls.get(), 1);
    }

    // ---- markers, managed directories and suppression (phase 19 plan 04) ----

    use crate::marker::{self, MarkerMetadata, MarkerRead};
    use crate::suppression::{self, SuppressionFile};
    use std::os::unix::ffi::OsStrExt;

    /// The marker file name `marker.rs` writes; private there, spelled out
    /// here so a test can read the raw bytes the way a byte-identity check
    /// must.
    const MARKER_FILE_NAME: &str = ".baude-marker.json";

    /// Where the stage expects a repository row's managed directory:
    /// `worktrees_base()/<workspace>/repository-<key>`.
    fn repository_dir_for(key: &str) -> PathBuf {
        crate::git::worktrees_base()
            .join(WORKSPACE)
            .join(format!("repository-{key}"))
    }

    /// A managed repository directory whose marker names `owner` as its
    /// common dir, the way `ensure_repository` leaves one behind.
    fn owned_repository_dir(key: &str, owner: &Path) -> PathBuf {
        let dir = repository_dir_for(key);
        fs::create_dir_all(&dir).expect("create repository dir");
        marker::write_marker(
            &dir,
            &MarkerMetadata {
                canonical_common_dir: PersistedPath::from_path(owner).as_bytes().to_vec(),
                scheme_version: 1,
                recorded_at_ms: 1,
            },
        )
        .expect("write marker");
        dir
    }

    fn marker_bytes(dir: &Path) -> Vec<u8> {
        fs::read(dir.join(MARKER_FILE_NAME)).expect("read marker bytes")
    }

    fn set_physical_key(state: &mut RepositoryState, key: RepositoryKey, physical_key: &str) {
        state
            .repositories
            .iter_mut()
            .find(|repository| repository.key == key)
            .expect("repository row")
            .physical_key = physical_key.to_string();
    }

    /// The digest `ensure_repository` recomputes for a common dir.
    fn digest_of(common_dir: &Path) -> String {
        crate::repository::compute_repository_digest(common_dir.as_os_str().as_bytes())
    }

    /// A second real repository under `root` with its own common dir and a
    /// commit, so a neighbour row has the same shape as the fixture's.
    fn init_repo(root: &Path, name: &str) -> PathBuf {
        git_ok(root, &["init", "-q", "-b", "main", name]);
        let repo = root.join(name);
        git_ok(&repo, &["config", "user.name", "Baude Test"]);
        git_ok(&repo, &["config", "user.email", "baude@example.invalid"]);
        fs::write(repo.join("tracked.txt"), b"neighbour\n").expect("write tracked file");
        git_ok(&repo, &["add", "tracked.txt"]);
        git_ok(&repo, &["commit", "-q", "-m", "neighbour"]);
        repo
    }

    /// The suppression store under the redirected config dir, read the way
    /// `is_suppressed` reads it.
    fn suppressions_on_disk() -> SuppressionFile {
        load_json::<SuppressionFile>(&suppression::file_path(&crate::persist::config_dir())).0
    }

    /// Sorted entry names of a directory, for before/after identity checks.
    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read dir")
            .map(|entry| {
                entry
                    .expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    fn cleared_of(report: &RemovalReport, kind: ClearingKind) -> Vec<PathBuf> {
        report
            .cleared
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.path.clone())
            .collect()
    }

    fn failures_of(report: &RemovalReport, kind: ClearingKind) -> Vec<&ClearingFailure> {
        report
            .clearing_failures
            .iter()
            .filter(|failure| failure.kind == kind)
            .collect()
    }

    /// D-14: every kind/path the real run cleared or failed to clear was
    /// named by the preview, so Phase 20 can show the user what will go.
    fn assert_preview_covers(preview: &RemovalReport, report: &RemovalReport) {
        for (kind, path) in report
            .cleared
            .iter()
            .map(|item| (item.kind, &item.path))
            .chain(
                report
                    .clearing_failures
                    .iter()
                    .map(|failure| (failure.kind, &failure.path)),
            )
        {
            assert!(
                preview
                    .cleared
                    .iter()
                    .any(|item| item.kind == kind && &item.path == path),
                "real run touched {kind:?} {} that the preview did not name",
                path.display()
            );
        }
    }

    // ---- end-state matrix (phase 22 plan 01) ----

    use std::collections::BTreeMap;

    /// The row kind a matrix cell removes.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum MatrixKind {
        RepositoryParent,
        MainCheckout,
        ManagedWorktree,
        Standalone,
    }

    /// Whether the cell's target has a live runtime when it is removed.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum MatrixRuntime {
        Live,
        NoRuntime,
    }

    /// The fixture shape the cell is built on.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum MatrixShape {
        Cloned,
        GitInit,
        Folder,
    }

    /// One cell of the end-state matrix.
    #[derive(Clone, Copy)]
    struct MatrixCell {
        kind: MatrixKind,
        /// The target row's role and its `managed_by_baude` flag; `None`
        /// where the target is not a checkout.
        role: Option<(CheckoutRole, bool)>,
        runtime: MatrixRuntime,
        shape: MatrixShape,
    }

    impl MatrixCell {
        /// `<kind>/<role>/<managed|external>/<live|none>/<shape>`, with `-`
        /// for the role and the flag where the target has none. Used in every
        /// assertion message and printed as the cell line.
        fn label(self) -> String {
            let kind = match self.kind {
                MatrixKind::RepositoryParent => "repository-parent",
                MatrixKind::MainCheckout => "main-checkout",
                MatrixKind::ManagedWorktree => "managed-worktree",
                MatrixKind::Standalone => "standalone",
            };
            let (role, flag) = match self.role {
                Some((role, true)) => (format!("{role:?}"), "managed"),
                Some((role, false)) => (format!("{role:?}"), "external"),
                None => ("-".to_string(), "-"),
            };
            let runtime = match self.runtime {
                MatrixRuntime::Live => "live",
                MatrixRuntime::NoRuntime => "none",
            };
            let shape = match self.shape {
                MatrixShape::Cloned => "cloned",
                MatrixShape::GitInit => "git-init",
                MatrixShape::Folder => "folder",
            };
            format!("{kind}/{role}/{flag}/{runtime}/{shape}")
        }
    }

    /// One seeded child row of a matrix repository.
    struct MatrixChild {
        key: CheckoutKey,
        role: CheckoutRole,
        managed: bool,
        path: PathBuf,
    }

    /// A repository on one fixture shape: its rows in `state`, the paths git
    /// reported for it and the name of the file its fixture commits.
    struct MatrixRepository {
        state: RepositoryState,
        key: RepositoryKey,
        main: PathBuf,
        common: PathBuf,
        tracked: &'static str,
        children: Vec<MatrixChild>,
    }

    impl MatrixRepository {
        fn child(&self, role: CheckoutRole, managed: bool) -> (CheckoutKey, PathBuf) {
            let child = self
                .children
                .iter()
                .find(|child| child.role == role && child.managed == managed)
                .expect("the shape seeds a child with this role and flag");
            (child.key, child.path.clone())
        }
    }

    /// The git init twin of `outside_worktree`: a linked worktree of the
    /// fixture repository outside the managed root, on `feature/outside`.
    fn outside_worktree_of_init(f: &RemovalFixture) -> PathBuf {
        let path = f.root.join("outside-wt");
        git_ok(
            &f.repo(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature/outside",
                path.to_str().expect("utf-8 outside path"),
            ],
        );
        fs::canonicalize(&path).expect("canonicalize outside worktree")
    }

    /// Build and seed the repository for `shape`. With `with_unmanaged` it
    /// also gets a linked worktree baude did not create, seeded as a
    /// `ManagedBranch` row whose `managed_by_baude` is false.
    fn matrix_repository(
        f: &RemovalFixture,
        shape: MatrixShape,
        with_unmanaged: bool,
    ) -> MatrixRepository {
        let mut state = RepositoryState::default();
        let unmanaged = |state: &mut RepositoryState, k: RepositoryKey, path: PathBuf| {
            let key = checkout(state, k, CheckoutRole::ManagedBranch, false, &path);
            MatrixChild {
                key,
                role: CheckoutRole::ManagedBranch,
                managed: false,
                path,
            }
        };
        match shape {
            MatrixShape::Folder => panic!("a folder cell has no repository"),
            MatrixShape::Cloned => {
                let t = cloned_topology(&f.root);
                let rows = seed_cloned(&mut state, &t);
                let mut children = vec![
                    MatrixChild {
                        key: rows.m,
                        role: CheckoutRole::Main,
                        managed: false,
                        path: t.clone.clone(),
                    },
                    MatrixChild {
                        key: rows.d,
                        role: CheckoutRole::PrimaryDefault,
                        managed: true,
                        path: t.default_worktree.clone(),
                    },
                    MatrixChild {
                        key: rows.b,
                        role: CheckoutRole::ManagedBranch,
                        managed: true,
                        path: t.managed_worktree.clone(),
                    },
                ];
                if with_unmanaged {
                    let outside = outside_worktree(f, &t);
                    children.push(unmanaged(&mut state, rows.k, outside));
                }
                MatrixRepository {
                    state,
                    key: rows.k,
                    main: t.clone,
                    common: t.common_dir,
                    tracked: "tracked",
                    children,
                }
            }
            MatrixShape::GitInit => {
                let managed = f.managed_worktree("wt-a", "feature/a");
                let k = seed_repository(&mut state, f);
                let m = checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
                let b = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed);
                let mut children = vec![
                    MatrixChild {
                        key: m,
                        role: CheckoutRole::Main,
                        managed: false,
                        path: f.repo(),
                    },
                    MatrixChild {
                        key: b,
                        role: CheckoutRole::ManagedBranch,
                        managed: true,
                        path: managed,
                    },
                ];
                if with_unmanaged {
                    let outside = outside_worktree_of_init(f);
                    children.push(unmanaged(&mut state, k, outside));
                }
                MatrixRepository {
                    state,
                    key: k,
                    main: f.repo(),
                    common: f.common_dir(),
                    tracked: "tracked.txt",
                    children,
                }
            }
        }
    }

    /// What every cell seeds beside its target, so a "cleared" read cannot
    /// pass on a wiped store: a folder bound to its own workspace and a
    /// suppression entry that belongs to another repository.
    struct Neighbours {
        folder: PathBuf,
        suppressed: PathBuf,
    }

    fn seed_neighbours(f: &RemovalFixture) -> Neighbours {
        let config = crate::persist::config_dir();
        let folder = f.root.join("neighbour");
        fs::create_dir_all(&folder).expect("create neighbour folder");
        folder_workspace::record(Some(&config), &folder, "neighbour-ws", 20);
        let suppressed = f.root.join("other-child");
        let other_common = f.root.join("other").join(".git");
        suppression::suppress(&config, &suppressed, &other_common, 2)
            .expect("seed neighbour suppression");
        Neighbours { folder, suppressed }
    }

    /// Everything a cell asks the fixture about, named by path, process id
    /// and store key. The reader takes nothing else, so no observation can
    /// come from what `remove` returned.
    struct Probe {
        state_root: PathBuf,
        /// Agent pid and process group of each runtime the cell started.
        runtimes: Vec<(u32, i32)>,
        /// The repository git is asked about; `None` for a plain folder.
        git_repo: Option<PathBuf>,
        /// Directories whose entries and git registration are read.
        dirs: Vec<PathBuf>,
        /// Folders whose binding entry is read.
        bound: Vec<PathBuf>,
        /// The repository's managed directory, where its marker lives.
        marker_dir: Option<PathBuf>,
        /// Paths whose suppression entry is read.
        suppressed: Vec<PathBuf>,
    }

    /// One read of the fixture. `None` always means "not there".
    struct EndState {
        /// `ps` state of each agent pid while it is in the process table.
        agents: Vec<Option<String>>,
        /// Each process group while a member of it is alive.
        groups: Vec<Option<i32>>,
        /// Sorted entry names of each watched directory.
        dirs: BTreeMap<PathBuf, Option<Vec<String>>>,
        /// The `git worktree list --porcelain` stanza of each watched
        /// directory: its path, HEAD and branch lines.
        registrations: BTreeMap<PathBuf, Option<Vec<String>>>,
        branches: Option<Vec<String>>,
        /// Each watched folder's binding entry, as it serializes.
        bindings: BTreeMap<PathBuf, Option<String>>,
        marker: Option<Vec<u8>>,
        marker_dir: Option<Vec<String>>,
        /// The repository each watched path is suppressed under.
        suppressions: BTreeMap<PathBuf, Option<String>>,
        /// The suppression store's bytes.
        suppression_file: Option<Vec<u8>>,
        /// Every row key in the saved state file, in file order.
        saved_rows: Vec<RowKey>,
        /// The agent pid of every runtime the saved state file records.
        saved_agents: Vec<u32>,
    }

    fn present(path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok()
    }

    /// The porcelain stanza whose `worktree` line names exactly `dir`.
    fn registration_of(list: &str, dir: &Path) -> Option<Vec<String>> {
        let head = format!("worktree {}", dir.display());
        list.split("\n\n")
            .map(|stanza| stanza.lines().map(str::to_string).collect::<Vec<_>>())
            .find(|lines| lines.first() == Some(&head))
    }

    fn saved_agents(state: &RepositoryState) -> Vec<u32> {
        let checkouts = state.checkouts.iter().map(SavedCheckout::owned_runtime);
        let standalone = state
            .standalone_sessions
            .iter()
            .map(SavedStandaloneSession::owned_runtime);
        let mut pids: Vec<u32> = checkouts
            .chain(standalone)
            .flatten()
            .map(|runtime| runtime.agent.pid)
            .collect();
        pids.sort_unstable();
        pids
    }

    fn saved_rows(state: &RepositoryState) -> Vec<RowKey> {
        state
            .repositories
            .iter()
            .map(|repository| RowKey::Repository(repository.key))
            .chain(
                state
                    .checkouts
                    .iter()
                    .map(|checkout| RowKey::Checkout(checkout.key)),
            )
            .chain(
                state
                    .standalone_sessions
                    .iter()
                    .map(|session| RowKey::Standalone(session.key)),
            )
            .collect()
    }

    /// Read process, worktree, binding, marker, suppression and saved-row
    /// state from the process table and the fixture's disk. Called once
    /// before the removal and once after, so both reads are taken the same
    /// way.
    fn read_end_state(probe: &Probe) -> EndState {
        let mut agents = Vec::new();
        let mut groups = Vec::new();
        for &(pid, pgid) in &probe.runtimes {
            assert_reaped_promptly(pid);
            let stat = proc_state(pid);
            agents.push((!stat.is_empty()).then_some(stat));
            groups.push((!group_is_extinct(pgid)).then_some(pgid));
        }
        let dirs = probe
            .dirs
            .iter()
            .map(|dir| (dir.clone(), present(dir).then(|| listing(dir))))
            .collect();
        let registrations = match &probe.git_repo {
            Some(repo) => {
                let list = worktree_list(repo);
                probe
                    .dirs
                    .iter()
                    .map(|dir| (dir.clone(), registration_of(&list, dir)))
                    .collect()
            }
            None => BTreeMap::new(),
        };
        let bindings_file = bindings_on_disk();
        let bindings = probe
            .bound
            .iter()
            .map(|folder| {
                let entry = bindings_file.folders.get(&folder_key(folder));
                let text =
                    entry.map(|entry| serde_json::to_string(entry).expect("serialize binding"));
                (folder.clone(), text)
            })
            .collect();
        let marker_dir = probe.marker_dir.as_deref().filter(|dir| present(dir));
        let saved = load_state(&probe.state_root);
        let store = suppressions_on_disk();
        let suppressions = probe
            .suppressed
            .iter()
            .map(|path| {
                let entry = store.entries.get(&folder_key(path));
                (
                    path.clone(),
                    entry.map(|entry| entry.repository_common_dir.clone()),
                )
            })
            .collect();
        EndState {
            agents,
            groups,
            dirs,
            registrations,
            branches: probe.git_repo.as_deref().map(branch_snapshot),
            bindings,
            marker: marker_dir
                .filter(|dir| present(&dir.join(MARKER_FILE_NAME)))
                .map(marker_bytes),
            marker_dir: marker_dir.map(listing),
            suppressions,
            suppression_file: fs::read(suppression::file_path(&crate::persist::config_dir())).ok(),
            saved_rows: saved_rows(&saved),
            saved_agents: saved_agents(&saved),
        }
    }

    /// Gone: present before the removal, absent after it.
    fn assert_gone<T: std::fmt::Debug>(
        label: &str,
        what: &str,
        before: &Option<T>,
        after: &Option<T>,
    ) {
        assert!(
            before.is_some(),
            "{label}: {what} must be present before the removal"
        );
        assert!(
            after.is_none(),
            "{label}: {what} must be gone after the removal, found {after:?}"
        );
    }

    /// Added: absent before the removal, present with `expected` after it.
    fn assert_added<T: PartialEq + std::fmt::Debug>(
        label: &str,
        what: &str,
        before: &Option<T>,
        after: &Option<T>,
        expected: &T,
    ) {
        assert!(
            before.is_none(),
            "{label}: {what} must be absent before the removal, found {before:?}"
        );
        assert_eq!(
            after.as_ref(),
            Some(expected),
            "{label}: {what} must be present after the removal"
        );
    }

    /// Kept: present and non-empty before the removal, equal after it.
    fn assert_kept<T: PartialEq + Default + std::fmt::Debug>(
        label: &str,
        what: &str,
        before: &Option<T>,
        after: &Option<T>,
    ) {
        let seen = before
            .as_ref()
            .unwrap_or_else(|| panic!("{label}: {what} must be present before the removal"));
        assert!(
            *seen != T::default(),
            "{label}: {what} must be non-empty before the removal"
        );
        assert_eq!(
            after, before,
            "{label}: {what} must be unchanged by the removal"
        );
    }

    /// Every runtime the cell started is in the process table and recorded
    /// in the saved state before the removal and out of both after: the
    /// agent pid reaped, the group extinct. A cell with no runtime has no pid
    /// to observe, so it is held to a fixture that started none and a saved
    /// state that records none.
    fn expect_processes(
        label: &str,
        runtime: MatrixRuntime,
        probe: &Probe,
        before: &EndState,
        after: &EndState,
    ) {
        let mut started: Vec<u32> = probe.runtimes.iter().map(|(pid, _)| *pid).collect();
        started.sort_unstable();
        assert_eq!(
            before.saved_agents, started,
            "{label}: the saved state must record exactly the runtimes the fixture started"
        );
        assert!(
            after.saved_agents.is_empty(),
            "{label}: the saved state must record no runtime after the removal, found {:?}",
            after.saved_agents
        );
        match runtime {
            MatrixRuntime::NoRuntime => assert!(
                started.is_empty() && before.agents.is_empty() && before.groups.is_empty(),
                "{label}: the fixture must have started no runtime"
            ),
            MatrixRuntime::Live => {
                assert!(
                    !started.is_empty(),
                    "{label}: a live cell must start a runtime"
                );
                for (index, (pid, pgid)) in probe.runtimes.iter().enumerate() {
                    assert_gone(
                        label,
                        &format!("agent pid {pid} in the process table"),
                        &before.agents[index],
                        &after.agents[index],
                    );
                    assert_gone(
                        label,
                        &format!("process group {pgid}"),
                        &before.groups[index],
                        &after.groups[index],
                    );
                }
            }
        }
    }

    /// The saved state file loses exactly `removed` and keeps every other
    /// row it held before, in the same order.
    fn expect_rows_dropped(label: &str, before: &EndState, after: &EndState, removed: &[RowKey]) {
        for row in removed {
            assert!(
                before.saved_rows.contains(row),
                "{label}: {row:?} must be in the saved state before the removal"
            );
        }
        let expected: Vec<RowKey> = before
            .saved_rows
            .iter()
            .copied()
            .filter(|row| !removed.contains(row))
            .collect();
        assert_eq!(
            after.saved_rows, expected,
            "{label}: the saved state must lose exactly the removed rows"
        );
    }

    /// One cell whose target is a checkout row. Seeds a binding for the
    /// target and a marker for its repository, reads the fixture, removes
    /// the row, reads the fixture again the same way and holds the two reads
    /// to the per-kind expectation.
    fn run_checkout_cell(cell: MatrixCell) {
        let label = cell.label();
        let (role, managed) = cell.role.expect("a checkout cell names a role");
        let f = RemovalFixture::new(&format!("matrix-{}", label.replace('/', "-")));
        let config = crate::persist::config_dir();
        let mut repo = matrix_repository(&f, cell.shape, false);
        let (target, target_path) = repo.child(role, managed);
        if managed {
            make_unclean(&target_path, repo.tracked);
        }
        let digest = digest_of(&repo.common);
        let marker_dir = owned_repository_dir(&digest, &repo.common);
        set_physical_key(&mut repo.state, repo.key, &digest);
        folder_workspace::record(Some(&config), &target_path, "matrix-ws", 10);
        let neighbours = seed_neighbours(&f);

        let mut handles = LiveHandles(Vec::new());
        let mut runtimes = Vec::new();
        match cell.runtime {
            MatrixRuntime::Live => {
                let pty = spawn_trapping_runtime(&target_path);
                let identity = pty.process_identity();
                runtimes.push((identity.pid, identity.process_group));
                seed_live(&mut repo.state, target, &pty, None);
                handles.0.push(handle_for(
                    RowKey::Checkout(target),
                    pty,
                    None,
                    &target_path,
                    &repo.main,
                ));
            }
            MatrixRuntime::NoRuntime => {}
        }
        repo.state.validate_for_save().expect("seed is save-valid");
        write_seed(&f, &repo.state);

        let probe = Probe {
            state_root: f.state_root(),
            runtimes,
            git_repo: Some(repo.main.clone()),
            dirs: repo
                .children
                .iter()
                .map(|child| child.path.clone())
                .collect(),
            bound: vec![target_path.clone(), neighbours.folder.clone()],
            marker_dir: Some(marker_dir),
            suppressed: vec![target_path.clone(), neighbours.suppressed.clone()],
        };
        let before = read_end_state(&probe);
        let (save, calls) = counting_save(&f.state_root());
        if let Err(error) = remove(
            &mut repo.state,
            RemovalTarget::Checkout(target),
            &mut handles.0,
            save,
        ) {
            panic!("{label}: the removal failed: {error:?}");
        }
        let after = read_end_state(&probe);

        // Process.
        expect_processes(&label, cell.runtime, &probe, &before, &after);

        // Worktree: the target's directory and registration per its kind,
        // every sibling's kept, every branch kept.
        let target_directory_goes = match cell.kind {
            MatrixKind::MainCheckout => false,
            MatrixKind::ManagedWorktree => true,
            MatrixKind::RepositoryParent | MatrixKind::Standalone => {
                panic!("{label}: not a checkout cell")
            }
        };
        for child in &repo.children {
            let dir = format!("directory {}", child.path.display());
            let registration = format!("git registration of {}", child.path.display());
            if child.key == target && target_directory_goes {
                assert_gone(
                    &label,
                    &dir,
                    &before.dirs[&child.path],
                    &after.dirs[&child.path],
                );
                assert_gone(
                    &label,
                    &registration,
                    &before.registrations[&child.path],
                    &after.registrations[&child.path],
                );
            } else {
                assert_kept(
                    &label,
                    &dir,
                    &before.dirs[&child.path],
                    &after.dirs[&child.path],
                );
                assert_kept(
                    &label,
                    &registration,
                    &before.registrations[&child.path],
                    &after.registrations[&child.path],
                );
            }
        }
        let branch = before.registrations[&target_path]
            .as_ref()
            .and_then(|stanza| stanza.iter().find_map(|line| line.strip_prefix("branch ")))
            .unwrap_or_else(|| {
                panic!("{label}: the target must sit on a branch before the removal")
            })
            .to_string();
        let on_branch = |state: &EndState| {
            state.branches.as_ref().map(|lines| {
                lines
                    .iter()
                    .filter(|line| line.starts_with(&format!("{branch} ")))
                    .cloned()
                    .collect::<Vec<_>>()
            })
        };
        assert_kept(
            &label,
            &format!("the target's branch {branch}"),
            &on_branch(&before),
            &on_branch(&after),
        );
        assert_kept(
            &label,
            "the branch snapshot",
            &before.branches,
            &after.branches,
        );

        // Binding.
        assert_gone(
            &label,
            "the target's binding",
            &before.bindings[&target_path],
            &after.bindings[&target_path],
        );
        assert_kept(
            &label,
            "the neighbour's binding",
            &before.bindings[&neighbours.folder],
            &after.bindings[&neighbours.folder],
        );

        // Marker and suppression.
        assert_kept(&label, "the parent marker", &before.marker, &after.marker);
        assert_kept(
            &label,
            "the parent's repository directory",
            &before.marker_dir,
            &after.marker_dir,
        );
        assert_added(
            &label,
            "the target's suppression entry",
            &before.suppressions[&target_path],
            &after.suppressions[&target_path],
            &folder_key(&repo.common),
        );
        assert_kept(
            &label,
            "the neighbour's suppression entry",
            &before.suppressions[&neighbours.suppressed],
            &after.suppressions[&neighbours.suppressed],
        );

        // Saved rows, and the one save that wrote them.
        expect_rows_dropped(&label, &before, &after, &[RowKey::Checkout(target)]);
        assert_eq!(calls.get(), 1, "{label}: the removal must save once");

        println!("CELL:{label}");
    }

    /// One cell whose target is a repository row with its children: the
    /// main checkout, the managed worktrees and a linked worktree baude did
    /// not create. A live cell runs one runtime on the main checkout and one
    /// on the managed branch worktree, so two groups are observed.
    fn run_repository_cell(cell: MatrixCell) {
        let label = cell.label();
        let f = RemovalFixture::new(&format!("matrix-{}", label.replace('/', "-")));
        let config = crate::persist::config_dir();
        let mut repo = matrix_repository(&f, cell.shape, true);
        for child in repo.children.iter().filter(|child| child.managed) {
            make_unclean(&child.path, repo.tracked);
        }
        let digest = digest_of(&repo.common);
        let marker_dir = owned_repository_dir(&digest, &repo.common);
        set_physical_key(&mut repo.state, repo.key, &digest);
        folder_workspace::record(Some(&config), &repo.main, "matrix-ws", 10);
        let old_child = f.root.join("old-child");
        suppression::suppress(&config, &old_child, &repo.common, 1)
            .expect("seed child suppression");
        let neighbours = seed_neighbours(&f);

        let mut handles = LiveHandles(Vec::new());
        let mut runtimes = Vec::new();
        match cell.runtime {
            MatrixRuntime::Live => {
                for (role, managed) in [
                    (CheckoutRole::Main, false),
                    (CheckoutRole::ManagedBranch, true),
                ] {
                    let (key, path) = repo.child(role, managed);
                    let pty = spawn_trapping_runtime(&path);
                    let identity = pty.process_identity();
                    runtimes.push((identity.pid, identity.process_group));
                    seed_live(&mut repo.state, key, &pty, None);
                    handles.0.push(handle_for(
                        RowKey::Checkout(key),
                        pty,
                        None,
                        &path,
                        &repo.main,
                    ));
                }
            }
            MatrixRuntime::NoRuntime => {}
        }
        repo.state.validate_for_save().expect("seed is save-valid");
        write_seed(&f, &repo.state);

        let probe = Probe {
            state_root: f.state_root(),
            runtimes,
            git_repo: Some(repo.main.clone()),
            dirs: repo
                .children
                .iter()
                .map(|child| child.path.clone())
                .collect(),
            bound: vec![repo.main.clone(), neighbours.folder.clone()],
            marker_dir: Some(marker_dir),
            suppressed: vec![old_child.clone(), neighbours.suppressed.clone()],
        };
        let before = read_end_state(&probe);
        let (save, calls) = counting_save(&f.state_root());
        if let Err(error) = remove(
            &mut repo.state,
            RemovalTarget::Repository(repo.key),
            &mut handles.0,
            save,
        ) {
            panic!("{label}: the removal failed: {error:?}");
        }
        let after = read_end_state(&probe);

        // Process: every child group.
        expect_processes(&label, cell.runtime, &probe, &before, &after);

        // Worktree: managed directories and their registrations go; the
        // main checkout and the unmanaged worktree stay, still registered.
        let mut gone = 0;
        let mut kept = 0;
        for child in &repo.children {
            let dir = format!("directory {}", child.path.display());
            let registration = format!("git registration of {}", child.path.display());
            let directory_goes = child.managed;
            if directory_goes {
                gone += 1;
                assert_gone(
                    &label,
                    &dir,
                    &before.dirs[&child.path],
                    &after.dirs[&child.path],
                );
                assert_gone(
                    &label,
                    &registration,
                    &before.registrations[&child.path],
                    &after.registrations[&child.path],
                );
            } else {
                kept += 1;
                assert_kept(
                    &label,
                    &dir,
                    &before.dirs[&child.path],
                    &after.dirs[&child.path],
                );
                assert_kept(
                    &label,
                    &registration,
                    &before.registrations[&child.path],
                    &after.registrations[&child.path],
                );
            }
        }
        assert!(
            gone >= 1 && kept == 2,
            "{label}: the cell must hold a managed child, the main checkout and an unmanaged child"
        );
        assert_kept(
            &label,
            "the branch snapshot",
            &before.branches,
            &after.branches,
        );

        // Binding.
        assert_gone(
            &label,
            "the repository's binding",
            &before.bindings[&repo.main],
            &after.bindings[&repo.main],
        );
        assert_kept(
            &label,
            "the neighbour's binding",
            &before.bindings[&neighbours.folder],
            &after.bindings[&neighbours.folder],
        );

        // Marker and suppression.
        assert_gone(&label, "the owned marker", &before.marker, &after.marker);
        assert_gone(
            &label,
            "the repository directory",
            &before.marker_dir,
            &after.marker_dir,
        );
        assert_gone(
            &label,
            "the child's suppression entry",
            &before.suppressions[&old_child],
            &after.suppressions[&old_child],
        );
        assert_kept(
            &label,
            "the neighbour's suppression entry",
            &before.suppressions[&neighbours.suppressed],
            &after.suppressions[&neighbours.suppressed],
        );

        // Saved rows, and the one save that wrote them.
        let mut removed: Vec<RowKey> = repo
            .children
            .iter()
            .map(|child| RowKey::Checkout(child.key))
            .collect();
        removed.push(RowKey::Repository(repo.key));
        expect_rows_dropped(&label, &before, &after, &removed);
        assert_eq!(calls.get(), 1, "{label}: the removal must save once");

        println!("CELL:{label}");
    }

    /// One cell whose target is a standalone folder with a binding of its
    /// own. It has no repository, so no marker is seeded and the managed
    /// root is read to show none appears.
    fn run_standalone_cell(cell: MatrixCell) {
        let label = cell.label();
        let f = RemovalFixture::new(&format!("matrix-{}", label.replace('/', "-")));
        let config = crate::persist::config_dir();
        let notes = notes_folder(&f);
        folder_workspace::record(Some(&config), &notes, "matrix-ws", 10);
        let neighbours = seed_neighbours(&f);

        let mut state = RepositoryState::default();
        let mut handles = LiveHandles(Vec::new());
        let mut runtimes = Vec::new();
        let key = match cell.runtime {
            MatrixRuntime::Live => {
                let pty = spawn_trapping_runtime(&notes);
                let identity = pty.process_identity().clone();
                runtimes.push((identity.pid, identity.process_group));
                let key = seed_notes(
                    &mut state,
                    &notes,
                    StandaloneLifecycle::Running(RuntimeGeneration::initial()),
                    Some(OwnedRuntime {
                        generation: RuntimeGeneration::initial(),
                        agent: identity,
                        shell: ShellOwnership::Closed,
                    }),
                );
                handles.0.push(handle_for(
                    RowKey::Standalone(key),
                    pty,
                    None,
                    &notes,
                    &notes,
                ));
                key
            }
            MatrixRuntime::NoRuntime => {
                seed_notes(&mut state, &notes, StandaloneLifecycle::Inactive, None)
            }
        };
        state.validate_for_save().expect("seed is save-valid");
        write_seed(&f, &state);

        let managed_root = f.managed_root();
        let probe = Probe {
            state_root: f.state_root(),
            runtimes,
            git_repo: None,
            dirs: vec![notes.clone(), managed_root.clone()],
            bound: vec![notes.clone(), neighbours.folder.clone()],
            marker_dir: None,
            suppressed: vec![notes.clone(), neighbours.suppressed.clone()],
        };
        let before = read_end_state(&probe);
        let (save, calls) = counting_save(&f.state_root());
        if let Err(error) = remove(
            &mut state,
            RemovalTarget::Standalone(key),
            &mut handles.0,
            save,
        ) {
            panic!("{label}: the removal failed: {error:?}");
        }
        let after = read_end_state(&probe);

        // Process.
        expect_processes(&label, cell.runtime, &probe, &before, &after);

        // Folder: kept with the same entries.
        assert_kept(
            &label,
            &format!("folder {}", notes.display()),
            &before.dirs[&notes],
            &after.dirs[&notes],
        );

        // Binding.
        assert_gone(
            &label,
            "the folder's binding",
            &before.bindings[&notes],
            &after.bindings[&notes],
        );
        assert_kept(
            &label,
            "the neighbour's binding",
            &before.bindings[&neighbours.folder],
            &after.bindings[&neighbours.folder],
        );

        // Marker: none exists and none appears under the managed root.
        for (when, read) in [("before", &before), ("after", &after)] {
            assert_eq!(
                read.dirs[&managed_root],
                Some(Vec::new()),
                "{label}: the managed root must hold no repository directory {when} the removal"
            );
            assert_eq!(
                read.suppressions[&notes], None,
                "{label}: the folder must have no suppression entry {when} the removal"
            );
        }

        // Suppression: the store is byte-identical.
        assert_kept(
            &label,
            "the suppression store's bytes",
            &before.suppression_file,
            &after.suppression_file,
        );

        // Saved rows, and the one save that wrote them.
        expect_rows_dropped(&label, &before, &after, &[RowKey::Standalone(key)]);
        assert_eq!(calls.get(), 1, "{label}: the removal must save once");

        println!("CELL:{label}");
    }

    const MATRIX_RUNTIMES: [MatrixRuntime; 2] = [MatrixRuntime::Live, MatrixRuntime::NoRuntime];

    /// REL-01, repository row: every child runtime goes, managed directories
    /// go, the main checkout and an unmanaged worktree stay, the repository's
    /// binding, owned marker and directory are cleared and its children's
    /// suppression entries are dropped. Four cells on both shapes.
    #[test]
    fn end_state_matrix_repository_parent() {
        for shape in [MatrixShape::Cloned, MatrixShape::GitInit] {
            for runtime in MATRIX_RUNTIMES {
                run_repository_cell(MatrixCell {
                    kind: MatrixKind::RepositoryParent,
                    role: None,
                    runtime,
                    shape,
                });
            }
        }
    }

    /// REL-01, standalone row: the runtime and the folder's own binding go;
    /// the folder stays with the same entries, no marker exists and the
    /// suppression store is byte-identical. Two cells.
    #[test]
    fn end_state_matrix_standalone() {
        for runtime in MATRIX_RUNTIMES {
            run_standalone_cell(MatrixCell {
                kind: MatrixKind::Standalone,
                role: None,
                runtime,
                shape: MatrixShape::Folder,
            });
        }
    }

    /// REL-01, managed worktree row: the runtime, the directory, the git
    /// registration and the row's own binding go; the branch and the parent
    /// marker stay; the path is suppressed. Six cells: the managed branch
    /// worktree on both shapes and the default checkout baude created on the
    /// cloned shape, which is deleted like any managed worktree and leaves
    /// `main` behind.
    #[test]
    fn end_state_matrix_managed_worktree() {
        for (role, shape) in [
            (CheckoutRole::ManagedBranch, MatrixShape::Cloned),
            (CheckoutRole::ManagedBranch, MatrixShape::GitInit),
            (CheckoutRole::PrimaryDefault, MatrixShape::Cloned),
        ] {
            for runtime in MATRIX_RUNTIMES {
                run_checkout_cell(MatrixCell {
                    kind: MatrixKind::ManagedWorktree,
                    role: Some((role, true)),
                    runtime,
                    shape,
                });
            }
        }
    }

    /// REL-01, main checkout row as the removal target: the runtime and the
    /// row's own binding go; the directory, every branch and the parent
    /// marker stay; the path is suppressed. Four cells on both shapes.
    #[test]
    fn end_state_matrix_main_checkout() {
        for shape in [MatrixShape::Cloned, MatrixShape::GitInit] {
            for runtime in MATRIX_RUNTIMES {
                run_checkout_cell(MatrixCell {
                    kind: MatrixKind::MainCheckout,
                    role: Some((CheckoutRole::Main, false)),
                    runtime,
                    shape,
                });
            }
        }
    }

    /// READM-02: the removed repository's owned marker and directory go, and
    /// the next admission of the same repository sees no owner there, reports
    /// no collision and allocates the un-suffixed digest with a fresh marker.
    #[test]
    fn repository_removal_clears_owned_marker_and_directory() {
        let f = RemovalFixture::new("marker-owned");
        let common = f.common_dir();
        let digest = digest_of(&common);
        let dir = owned_repository_dir(&digest, &common);
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        set_physical_key(&mut state, k, &digest);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        state.validate_for_save().expect("seed is save-valid");
        let preview = preview_removal(&state, RemovalTarget::Repository(k)).expect("preview");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository");

        assert_eq!(cleared_of(&report, ClearingKind::Marker), vec![dir.clone()]);
        assert_eq!(
            cleared_of(&report, ClearingKind::RepositoryDir),
            vec![dir.clone()]
        );
        assert!(report.clearing_failures.is_empty());
        assert!(!dir.exists(), "the owned repository directory is gone");
        assert_eq!(
            marker::read_marker(&dir).expect("read cleared marker"),
            MarkerRead::Missing
        );
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert_eq!(calls.get(), 1);
        assert_preview_covers(&preview, &report);

        let snapshot = crate::git::discover_repository(&f.repo()).expect("discover");
        let (new_key, collision) =
            crate::lifecycle::ensure_repository(&mut state, &snapshot).expect("re-admit");
        assert_eq!(
            collision, None,
            "the cleared directory has no owner to collide with"
        );
        assert_ne!(new_key, k);
        let row = state
            .repositories
            .iter()
            .find(|repository| repository.key == new_key)
            .expect("re-admitted row");
        assert_eq!(row.physical_key, digest);
        match marker::read_marker(&dir).expect("read fresh marker") {
            MarkerRead::Valid(meta) => assert_eq!(
                meta.canonical_common_dir,
                PersistedPath::from_path(&common).as_bytes()
            ),
            other => panic!("expected a fresh valid marker, got {other:?}"),
        }
    }

    /// D-04 and READM-03: removing one child under a surviving parent leaves
    /// the parent's marker and directory alone and records the child path for
    /// suppression, keyed to the parent's common dir.
    #[test]
    fn child_removal_keeps_parent_marker_and_writes_suppression() {
        let f = RemovalFixture::new("child-suppressed");
        let common = f.common_dir();
        let dir = owned_repository_dir("parent-key", &common);
        let managed = f.managed_worktree("wt-child", "feature/child");
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        set_physical_key(&mut state, k, "parent-key");
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        let c = checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed);
        state.validate_for_save().expect("seed is save-valid");
        let config = crate::persist::config_dir();
        let marker_before = marker_bytes(&dir);
        assert!(!suppression::file_path(&config).exists());
        let preview = preview_removal(&state, RemovalTarget::Checkout(c)).expect("preview");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Checkout(c), &mut handles, save)
            .expect("remove child");

        assert!(cleared_of(&report, ClearingKind::Marker).is_empty());
        assert!(cleared_of(&report, ClearingKind::RepositoryDir).is_empty());
        assert!(report.clearing_failures.is_empty());
        assert_eq!(marker_bytes(&dir), marker_before);
        assert!(dir.is_dir(), "the parent's directory stays");
        assert!(suppression::is_suppressed(&config, &managed));
        let file = suppressions_on_disk();
        assert_eq!(file.entries.len(), 1);
        assert_eq!(
            file.entries[&folder_key(&managed)].repository_common_dir,
            folder_key(&common)
        );
        assert_eq!(
            cleared_of(&report, ClearingKind::SuppressionAdded),
            vec![managed.clone()]
        );
        assert!(cleared_of(&report, ClearingKind::SuppressionDropped).is_empty());
        assert!(!managed.exists());
        assert!(state.repositories.iter().any(|r| r.key == k));
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert_eq!(calls.get(), 1);
        assert_preview_covers(&preview, &report);
    }

    /// D-10: a parent removal garbage-collects its children's entries and
    /// writes none of its own; a standalone removal never touches the store.
    #[test]
    fn repository_removal_drops_child_suppression_and_standalone_writes_none() {
        // Part A: a legacy row with an empty physical_key; its children's
        // entries go, a neighbour's entry stays, no marker work is planned.
        let f = RemovalFixture::new("drop-children");
        let config = crate::persist::config_dir();
        let common = f.common_dir();
        let managed = f.managed_worktree("wt-child", "feature/child");
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        checkout(&mut state, k, CheckoutRole::ManagedBranch, true, &managed);
        state.validate_for_save().expect("seed is save-valid");
        let old_child = f.root.join("old-child");
        let other_common = f.root.join("other").join(".git");
        let other_child = f.root.join("other-child");
        suppression::suppress(&config, &old_child, &common, 1).expect("seed child entry");
        suppression::suppress(&config, &other_child, &other_common, 2)
            .expect("seed neighbour entry");
        let other_before = suppressions_on_disk().entries[&folder_key(&other_child)].clone();
        let preview = preview_removal(&state, RemovalTarget::Repository(k)).expect("preview");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository");

        assert_eq!(
            cleared_of(&report, ClearingKind::SuppressionDropped),
            vec![common.clone()]
        );
        assert!(cleared_of(&report, ClearingKind::SuppressionAdded).is_empty());
        assert!(cleared_of(&report, ClearingKind::Marker).is_empty());
        assert!(cleared_of(&report, ClearingKind::RepositoryDir).is_empty());
        assert!(report.clearing_failures.is_empty());
        let after = suppressions_on_disk();
        assert!(!after.entries.contains_key(&folder_key(&old_child)));
        assert_eq!(
            after.entries.get(&folder_key(&other_child)),
            Some(&other_before)
        );
        assert_eq!(after.entries.len(), 1);
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert_eq!(calls.get(), 1);
        assert_preview_covers(&preview, &report);
        drop(f);

        // Part B: a standalone removal writes no entry, so an absent store
        // stays absent.
        let f = RemovalFixture::new("standalone-no-entry");
        let config = crate::persist::config_dir();
        let notes = notes_folder(&f);
        let mut state = RepositoryState::default();
        let s = seed_notes(&mut state, &notes, StandaloneLifecycle::Inactive, None);
        state.validate_for_save().expect("seed is save-valid");
        assert!(!suppression::file_path(&config).exists());
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Standalone(s), &mut handles, save)
            .expect("remove standalone");

        assert!(
            !suppression::file_path(&config).exists(),
            "an absent suppression store must stay absent"
        );
        assert!(report.cleared.is_empty());
        assert!(report.clearing_failures.is_empty());
        assert!(notes.exists());
        assert_eq!(calls.get(), 1);
    }

    /// D-06, D-07, D-14: a refusal and a non-empty directory are reported,
    /// the directory stays, nothing recurses, and the removal is still Ok
    /// with the state saved.
    #[test]
    fn marker_refusal_and_non_empty_dir_are_reported_not_raised() {
        // Part A: the marker names another repository.
        let f = RemovalFixture::new("marker-foreign");
        let foreign_owner = f.root.join("elsewhere").join(".git");
        let dir = owned_repository_dir("foreign-key", &foreign_owner);
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        set_physical_key(&mut state, k, "foreign-key");
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        state.validate_for_save().expect("seed is save-valid");
        let before = marker_bytes(&dir);
        let preview = preview_removal(&state, RemovalTarget::Repository(k)).expect("preview");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository with a foreign marker");

        let failures = failures_of(&report, ClearingKind::Marker);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].path, dir);
        assert!(
            failures[0].error.contains("ForeignOwner"),
            "reason names the refusal: {}",
            failures[0].error
        );
        assert_eq!(report.clearing_failures.len(), 1);
        assert!(cleared_of(&report, ClearingKind::Marker).is_empty());
        assert!(cleared_of(&report, ClearingKind::RepositoryDir).is_empty());
        assert_eq!(marker_bytes(&dir), before);
        assert!(dir.is_dir());
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert!(!state.repositories.iter().any(|r| r.key == k));
        assert!(!load_state(&f.state_root())
            .repositories
            .iter()
            .any(|r| r.key == k));
        assert_eq!(calls.get(), 1);
        assert_preview_covers(&preview, &report);
        drop(f);

        // Part B: our marker next to a stray subdirectory.
        let f = RemovalFixture::new("marker-stray");
        let common = f.common_dir();
        let dir = owned_repository_dir("stray-key", &common);
        let stray = dir.join("stray");
        fs::create_dir_all(&stray).expect("create stray dir");
        fs::write(stray.join("keep.txt"), b"keep\n").expect("write stray file");
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        set_physical_key(&mut state, k, "stray-key");
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        state.validate_for_save().expect("seed is save-valid");
        let preview = preview_removal(&state, RemovalTarget::Repository(k)).expect("preview");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository with a stray subdirectory");

        assert_eq!(cleared_of(&report, ClearingKind::Marker), vec![dir.clone()]);
        assert!(cleared_of(&report, ClearingKind::RepositoryDir).is_empty());
        let failures = failures_of(&report, ClearingKind::RepositoryDir);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].path, dir);
        assert!(
            failures[0].error.contains("not empty"),
            "reason says why the directory stayed: {}",
            failures[0].error
        );
        assert_eq!(report.clearing_failures.len(), 1);
        assert!(stray.join("keep.txt").is_file(), "nothing recursed");
        assert_eq!(
            marker::read_marker(&dir).expect("read cleared marker"),
            MarkerRead::Missing
        );
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert_eq!(calls.get(), 1);
        assert_preview_covers(&preview, &report);
    }

    /// D-05 and T-19-10: the directory comes from the stored `physical_key`,
    /// never a recomputed digest; an empty key is a legacy no-op and a key
    /// that is not one path component is refused before any path exists.
    #[test]
    fn repository_dir_comes_from_stored_physical_key() {
        // Part A: the row owns the suffixed collision directory while the
        // un-suffixed sibling carries the same owner bytes, the trap a
        // recomputed digest would fall into.
        let f = RemovalFixture::new("key-suffixed");
        let common = f.common_dir();
        let digest = digest_of(&common);
        let sibling = owned_repository_dir(&digest, &common);
        let suffixed = owned_repository_dir(&format!("{digest}-2"), &common);
        let sibling_before = marker_bytes(&sibling);
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        set_physical_key(&mut state, k, &format!("{digest}-2"));
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = no_handles();
        let (save, _) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove suffixed repository");

        assert_eq!(
            cleared_of(&report, ClearingKind::Marker),
            vec![suffixed.clone()]
        );
        assert_eq!(
            cleared_of(&report, ClearingKind::RepositoryDir),
            vec![suffixed.clone()]
        );
        assert!(report.clearing_failures.is_empty());
        assert!(!suffixed.exists());
        assert!(sibling.is_dir(), "the un-suffixed sibling survives");
        assert_eq!(marker_bytes(&sibling), sibling_before);
        drop(f);

        // Part B: a legacy bare-counter row has no directory to clear, even
        // when a bare `repository-` directory happens to exist.
        let f = RemovalFixture::new("key-legacy");
        let common = f.common_dir();
        let bare = owned_repository_dir("", &common);
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        state.validate_for_save().expect("seed is save-valid");
        let mut handles = no_handles();
        let (save, _) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove legacy repository");

        assert!(cleared_of(&report, ClearingKind::Marker).is_empty());
        assert!(cleared_of(&report, ClearingKind::RepositoryDir).is_empty());
        assert!(report.clearing_failures.is_empty());
        assert!(bare.is_dir());
        drop(f);

        // Part C: a key that would escape the workspace directory is one
        // failure and no filesystem change, with an owned marker planted at
        // the path a naive join would reach.
        let f = RemovalFixture::new("key-escape");
        let common = f.common_dir();
        let base = crate::git::worktrees_base();
        let escape = base.join("escape");
        fs::create_dir_all(&escape).expect("create escape dir");
        marker::write_marker(
            &escape,
            &MarkerMetadata {
                canonical_common_dir: PersistedPath::from_path(&common).as_bytes().to_vec(),
                scheme_version: 1,
                recorded_at_ms: 1,
            },
        )
        .expect("write escape marker");
        let ws_dir = base.join(WORKSPACE);
        fs::create_dir_all(&ws_dir).expect("create workspace dir");
        let base_before = listing(&base);
        let ws_before = listing(&ws_dir);
        let escape_before = marker_bytes(&escape);
        let mut state = RepositoryState::default();
        let k = seed_repository(&mut state, &f);
        set_physical_key(&mut state, k, "../escape");
        checkout(&mut state, k, CheckoutRole::Main, false, &f.repo());
        state.validate_for_save().expect("seed is save-valid");
        let preview = preview_removal(&state, RemovalTarget::Repository(k)).expect("preview");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository with an unsafe key");

        assert_eq!(report.clearing_failures.len(), 1);
        assert_eq!(report.clearing_failures[0].kind, ClearingKind::Marker);
        assert!(cleared_of(&report, ClearingKind::Marker).is_empty());
        assert!(cleared_of(&report, ClearingKind::RepositoryDir).is_empty());
        assert_eq!(listing(&base), base_before);
        assert_eq!(listing(&ws_dir), ws_before);
        assert_eq!(marker_bytes(&escape), escape_before);
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert_eq!(calls.get(), 1);
        assert_preview_covers(&preview, &report);
    }

    /// READM-05 and D-15: two repositories bound to one workspace plus a
    /// third with a managed worktree; removing one leaves every neighbour
    /// artefact byte-identical and the preview names everything cleared.
    #[test]
    fn removal_leaves_neighbour_bindings_markers_worktrees_and_suppressions_byte_identical() {
        let f = RemovalFixture::new("neighbours");
        let config = crate::persist::config_dir();
        let x = f.repo();
        let x_common = f.common_dir();
        let kept = f.managed_worktree("kept", "feature/kept");
        let x_dir = owned_repository_dir("x-key", &x_common);
        let a = init_repo(&f.root, "a");
        let a_common = a.join(".git");
        let b = init_repo(&f.root, "b");
        let b_common = b.join(".git");
        let a_dir = owned_repository_dir("a-key", &a_common);
        let b_dir = owned_repository_dir("b-key", &b_common);
        let mut state = RepositoryState::default();
        let kx = seed_repository(&mut state, &f);
        set_physical_key(&mut state, kx, "x-key");
        checkout(&mut state, kx, CheckoutRole::Main, false, &x);
        checkout(&mut state, kx, CheckoutRole::ManagedBranch, true, &kept);
        let ka = repository(&mut state, &a);
        set_physical_key(&mut state, ka, "a-key");
        checkout(&mut state, ka, CheckoutRole::Main, false, &a);
        let kb = repository(&mut state, &b);
        set_physical_key(&mut state, kb, "b-key");
        checkout(&mut state, kb, CheckoutRole::Main, false, &b);
        state.validate_for_save().expect("seed is save-valid");
        folder_workspace::record(Some(&config), &a, "shared", 10);
        folder_workspace::record(Some(&config), &b, "shared", 20);
        folder_workspace::record(Some(&config), &x, "x-ws", 30);
        let a_child = f.root.join("a-child");
        let b_child = f.root.join("b-child");
        suppression::suppress(&config, &a_child, &a_common, 1).expect("seed a entry");
        suppression::suppress(&config, &b_child, &b_common, 2).expect("seed b entry");

        let bindings_before = bindings_on_disk();
        let b_binding = bindings_before.folders[&folder_key(&b)].clone();
        let x_binding = bindings_before.folders[&folder_key(&x)].clone();
        let b_marker = marker_bytes(&b_dir);
        let x_marker = marker_bytes(&x_dir);
        let kept_listing = listing(&kept);
        let managed_listing = listing(&f.managed_root());
        let b_entry = suppressions_on_disk().entries[&folder_key(&b_child)].clone();
        let preview = preview_removal(&state, RemovalTarget::Repository(ka)).expect("preview");
        let mut handles = no_handles();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(
            &mut state,
            RemovalTarget::Repository(ka),
            &mut handles,
            save,
        )
        .expect("remove repository a");

        // A is gone from every store, and its main worktree is kept on disk.
        let bindings = bindings_on_disk();
        assert!(!bindings.folders.contains_key(&folder_key(&a)));
        assert!(!a_dir.exists());
        let suppressions = suppressions_on_disk();
        assert!(!suppressions.entries.contains_key(&folder_key(&a_child)));
        assert!(a.join("tracked.txt").is_file());
        assert!(!state.repositories.iter().any(|r| r.key == ka));
        // B and X are byte-identical.
        assert_eq!(bindings.folders.get(&folder_key(&b)), Some(&b_binding));
        assert_eq!(bindings.folders.get(&folder_key(&x)), Some(&x_binding));
        assert_eq!(bindings.folders.len(), 2);
        assert_eq!(marker_bytes(&b_dir), b_marker);
        assert_eq!(marker_bytes(&x_dir), x_marker);
        assert_eq!(
            listing(a_dir.parent().expect("workspace dir")),
            vec![
                "repository-b-key".to_string(),
                "repository-x-key".to_string()
            ]
        );
        assert_eq!(listing(&kept), kept_listing);
        assert_eq!(listing(&f.managed_root()), managed_listing);
        assert_eq!(
            suppressions.entries.get(&folder_key(&b_child)),
            Some(&b_entry)
        );
        assert_eq!(suppressions.entries.len(), 1);
        assert!(state.repositories.iter().any(|r| r.key == kb));
        assert!(state.repositories.iter().any(|r| r.key == kx));
        // The report names exactly A's artefacts, and the preview named them.
        assert_eq!(cleared_of(&report, ClearingKind::Binding), vec![a.clone()]);
        assert_eq!(
            cleared_of(&report, ClearingKind::Marker),
            vec![a_dir.clone()]
        );
        assert_eq!(
            cleared_of(&report, ClearingKind::RepositoryDir),
            vec![a_dir.clone()]
        );
        assert_eq!(
            cleared_of(&report, ClearingKind::SuppressionDropped),
            vec![a_common.clone()]
        );
        assert!(cleared_of(&report, ClearingKind::SuppressionAdded).is_empty());
        assert!(report.clearing_failures.is_empty());
        assert_preview_covers(&preview, &report);
        assert_eq!(report.persistence, PersistenceOutcome::Saved);
        assert_eq!(calls.get(), 1);
    }
}
