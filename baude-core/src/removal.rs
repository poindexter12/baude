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
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::repository::{
    CheckoutKey, OwnedRuntime, RepositoryKey, RepositoryState, StandaloneKey, ValidationError,
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
        error: crate::session::SessionTeardownError,
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
            Self::RuntimeSurvived { error, .. } => Some(error),
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
    let _ = state;
    Err(RemovalError::UnknownTarget(target))
}

/// Describe what removing `target` would do without doing any of it.
pub fn preview_removal(
    state: &RepositoryState,
    target: RemovalTarget,
) -> Result<RemovalReport, RemovalError> {
    let _ = state;
    Err(RemovalError::UnknownTarget(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        CheckoutLifecycle, CheckoutRole, PersistedPath, ProcessIdentity, RepositoryHealth,
        RetainedSessionState, RetainedStandaloneSessionState, RuntimeGeneration, SavedCheckout,
        SavedRepository, SavedStandaloneSession, ShellOwnership, StandaloneLifecycle,
        UnavailableCause,
    };
    use std::fs;
    use std::io::ErrorKind;
    use std::path::Path;

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

    fn row<'a>(set: &'a RemovalSet, key: RowKey) -> &'a PlannedRow {
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
}
