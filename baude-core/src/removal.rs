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
//! a file; its only filesystem access is a `symlink_metadata` probe that
//! labels a managed directory as already removed when it is `NotFound`.
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

use crate::pty::Pty;
use crate::repository::{
    CheckoutKey, CheckoutRole, OwnedRuntime, RepositoryKey, RepositoryState, SavedCheckout,
    SavedRepository, ShellOwnership, StandaloneKey, ValidationError,
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
    Ok(report_from_set(&set, None))
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
pub fn remove(
    state: &mut RepositoryState,
    target: RemovalTarget,
    runtimes: &mut Vec<RowRuntime>,
    save: impl FnOnce(&crate::persist::StateFile) -> Result<(), crate::persist::SaveError>,
) -> Result<RemovalReport, RemovalError> {
    let set = select_rows(state, target)?;
    preflight(&set, runtimes)?;
    let mut candidate = state.clone();
    candidate.remove_rows(
        &set.repository_keys(),
        &set.checkout_keys(),
        &set.standalone_keys(),
    );
    candidate
        .validate_for_save()
        .map_err(RemovalError::Validation)?;
    let killed = kill_stage(&set, runtimes)?;
    let disk = disk_stage(&set)?;
    let (persistence, readers_parked) = persist_stage(state, candidate, &set, runtimes, save)?;
    Ok(report_from_set(
        &set,
        Some(StageResults {
            killed,
            disk,
            readers_parked,
            persistence,
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
}

/// Verify every handle against the persisted runtime of its row before any
/// signal is sent: exactly one handle per live row, full `ProcessIdentity`
/// equality for the agent, exact `ShellOwnership` agreement, and no handle
/// at all for a row without a persisted runtime.
fn preflight(set: &RemovalSet, runtimes: &[RowRuntime]) -> Result<(), RemovalError> {
    for row in &set.rows {
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
/// still goes through the forced helper so its administrative entry is
/// pruned; `NotRegistered` there means there was nothing left to clean.
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
            DiskAction::RemoveManagedWorktree => {
                let (main_worktree, common_dir) = owner_paths(row, path)?;
                crate::git::remove_managed_worktree_forced(main_worktree, common_dir, path)
                    .map_err(|error| disk_error(row, path, error.into()))?;
                results.deleted.push(path.clone());
            }
            DiskAction::AlreadyRemoved => {
                let (main_worktree, common_dir) = owner_paths(row, path)?;
                match crate::git::remove_managed_worktree_forced(main_worktree, common_dir, path) {
                    Ok(_) | Err(crate::git::ForcedRemovalError::NotRegistered { .. }) => {}
                    Err(error) => return Err(disk_error(row, path, error.into())),
                }
                results.already_removed.push(path.clone());
            }
        }
    }
    Ok(results)
}

/// The owning repository's main worktree and common directory, which
/// `select_rows` records on every checkout row.
fn owner_paths<'a>(row: &'a PlannedRow, path: &Path) -> Result<(&'a Path, &'a Path), RemovalError> {
    match (&row.main_worktree, &row.common_dir) {
        (Some(main_worktree), Some(common_dir)) => Ok((main_worktree, common_dir)),
        _ => Err(disk_error(
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
/// actually did.
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

    /// Per-test fixture root under the temp dir, removed on drop so a failing
    /// assertion still cleans up.
    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new(label: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("baude-removal-{}-{label}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("create fixture root");
            Self { root }
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

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
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
    /// containment check compares canonical paths. The redirect is a field
    /// declared before `root` so it is still installed while the root is
    /// torn down.
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
            let fixture = Self {
                _redirect: redirect,
                root,
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

        fn common_dir(&self) -> PathBuf {
            crate::git::discover_repository(&self.repo())
                .expect("discover repository")
                .common_dir
        }
    }

    impl Drop for RemovalFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
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

    /// Kills the out-of-group slave holder on drop so an assertion panic never
    /// leaks a 30 s perl process.
    struct HolderGuard(i32);

    impl Drop for HolderGuard {
        fn drop(&mut self) {
            // SAFETY: the pid was written by this fixture's own holder.
            unsafe {
                libc::kill(self.0, libc::SIGKILL);
            }
        }
    }

    fn holder_pgid(holder: i32) -> Option<i32> {
        let out = Command::new("ps")
            .args(["-o", "pgid=", "-p", &holder.to_string()])
            .output()
            .expect("ps");
        String::from_utf8_lossy(&out.stdout).trim().parse().ok()
    }

    /// The plan 08 holder shape: the leader backgrounds a perl process that
    /// leaves the session with setsid, records its pid outside the managed
    /// directory and sleeps holding the inherited slave terminal, then the
    /// leader exits. Returns once the holder is proven outside the recorded
    /// group and the leader has exited.
    fn spawn_parked_reader_runtime(cwd: &Path, holder_pid_path: &Path) -> (Pty, HolderGuard) {
        let _ = fs::remove_file(holder_pid_path);
        let cmd = format!(
            "set +m; trap '' HUP; perl -MPOSIX -e 'my $sid = POSIX::setsid(); \
             exit 3 unless defined $sid && $sid != -1; \
             open(my $f, \">\", \"{}\"); print $f $$; close $f; sleep 30' & exit 0",
            holder_pid_path.display()
        );
        let pty = Pty::spawn(Some(&cmd), cwd, 5, 40).expect("spawn parked-reader runtime");
        let pgid = pty.process_identity().process_group;
        let deadline = Instant::now() + Duration::from_secs(5);
        let holder: i32 = loop {
            let written = fs::read_to_string(holder_pid_path)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok());
            if let Some(holder) = written {
                if pty.is_exited() {
                    break holder;
                }
            }
            assert!(
                Instant::now() < deadline,
                "holder.pid was not written or the leader did not exit"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let guard = HolderGuard(holder);
        let observed = holder_pgid(holder).expect("holder pgid");
        assert_ne!(
            observed, pgid,
            "setsid did not move the holder out of the recorded group"
        );
        (pty, guard)
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
        let preview = preview_removal(&state, RemovalTarget::Repository(k)).expect("preview");
        let mut handles: Vec<RowRuntime> = Vec::new();
        let (save, calls) = counting_save(&f.state_root());

        let report = remove(&mut state, RemovalTarget::Repository(k), &mut handles, save)
            .expect("remove repository");

        assert_eq!(preview.rows_removed, report.rows_removed);
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

        let holder_pid = holder.0;
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
}
