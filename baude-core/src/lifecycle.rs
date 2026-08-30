//! Shared, UI-free repository lifecycle contracts.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::git::{
    self, BranchActivationError, BranchActivationOutcome, RepositoryDiscoveryError,
    RepositorySnapshot, WorktreeRecord,
};
use crate::repository::{
    AllocationError, CheckoutHealth, CheckoutKey, CheckoutRole, PersistedPath, RepositoryHealth,
    RepositoryKey, RepositoryState, RetainedSessionState, SavedCheckout, SavedRepository,
    ValidationError,
};

/// A literal branch activation rooted in one durable repository identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivationRequest {
    pub repository: RepositoryKey,
    pub branch: String,
    pub managed_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationDisposition {
    Created,
    Activated,
    Reused,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedActivation {
    pub request: ActivationRequest,
    pub checkout: CheckoutKey,
    pub first_seen_order: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedActivation {
    pub repository: RepositoryKey,
    pub checkout: CheckoutKey,
    pub disposition: ActivationDisposition,
    pub managed_by_baude: bool,
    pub path: PathBuf,
    pub main_worktree: PathBuf,
    pub branch: String,
}

impl RecordedActivation {
    pub fn outcome(&self, runtime: Option<u64>) -> LifecycleOutcome {
        match self.disposition {
            ActivationDisposition::Created => LifecycleOutcome::Created {
                checkout: self.checkout,
                runtime,
            },
            ActivationDisposition::Activated => LifecycleOutcome::Activated {
                checkout: self.checkout,
                runtime,
            },
            ActivationDisposition::Reused => LifecycleOutcome::Reused {
                checkout: self.checkout,
                runtime,
                managed_by_baude: self.managed_by_baude,
            },
        }
    }

    pub fn added_managed_worktree(&self) -> bool {
        self.managed_by_baude
            && matches!(
                self.disposition,
                ActivationDisposition::Created | ActivationDisposition::Activated
            )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreationFailureStage {
    PersistenceBeforeReplacement,
    PersistenceAfterReplacement,
    Spawn,
    Compensation,
}

impl std::fmt::Display for CreationFailureStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PersistenceBeforeReplacement => f.write_str("persistence before replacement"),
            Self::PersistenceAfterReplacement => f.write_str("persistence after replacement"),
            Self::Spawn => f.write_str("runtime spawn"),
            Self::Compensation => f.write_str("worktree compensation"),
        }
    }
}

/// Remove only a worktree added by this uncommitted activation. Plain Git
/// removal retains the local branch; fresh postconditions prove both facts.
pub fn compensate_uncommitted_activation(
    activation: &RecordedActivation,
) -> Result<(), LifecycleError> {
    if !activation.added_managed_worktree() {
        return Ok(());
    }
    git::remove_worktree(&activation.main_worktree, &activation.path).map_err(|error| {
        LifecycleError::Topology(format!(
            "plain Git worktree compensation refused {}: {error}",
            activation.path.display()
        ))
    })?;
    match std::fs::symlink_metadata(&activation.path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(LifecycleError::Topology(format!(
                "compensated worktree path {} still exists",
                activation.path.display()
            )));
        }
        Err(error) => {
            return Err(LifecycleError::Topology(format!(
                "inspect compensated worktree path {}: {error}",
                activation.path.display()
            )));
        }
    }
    let fresh = git::discover_repository(&activation.main_worktree)?;
    if fresh
        .worktrees
        .iter()
        .any(|record| record.path == activation.path)
    {
        return Err(LifecycleError::Topology(format!(
            "compensated worktree {} remains registered",
            activation.path.display()
        )));
    }
    let expected = format!("refs/heads/{}", activation.branch);
    match git::classify_branch(&fresh, &activation.branch)? {
        git::BranchActivation::ExistingLocal { full_ref, .. } if full_ref == expected => Ok(()),
        other => Err(LifecycleError::Topology(format!(
            "compensation did not retain exact local branch {expected}: {other:?}"
        ))),
    }
}

#[derive(Debug)]
pub enum LifecycleError {
    Discovery(RepositoryDiscoveryError),
    Git(BranchActivationError),
    Allocation(AllocationError),
    Validation(ValidationError),
    RepositoryMissing(RepositoryKey),
    Topology(String),
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovery(error) => write!(f, "discover activation repository: {error}"),
            Self::Git(error) => error.fmt(f),
            Self::Allocation(error) => error.fmt(f),
            Self::Validation(error) => error.fmt(f),
            Self::RepositoryMissing(key) => {
                write!(f, "activation repository {} is missing", key.get())
            }
            Self::Topology(detail) => write!(f, "activation topology mismatch: {detail}"),
        }
    }
}

impl std::error::Error for LifecycleError {}

impl From<RepositoryDiscoveryError> for LifecycleError {
    fn from(error: RepositoryDiscoveryError) -> Self {
        Self::Discovery(error)
    }
}

impl From<BranchActivationError> for LifecycleError {
    fn from(error: BranchActivationError) -> Self {
        Self::Git(error)
    }
}

impl From<AllocationError> for LifecycleError {
    fn from(error: AllocationError) -> Self {
        Self::Allocation(error)
    }
}

impl From<ValidationError> for LifecycleError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
    }
}

/// Reconcile or create the durable repository parent represented by Git facts.
pub fn ensure_repository(
    state: &mut RepositoryState,
    snapshot: &RepositorySnapshot,
) -> Result<RepositoryKey, LifecycleError> {
    let common = PersistedPath::from_path(&snapshot.common_dir);
    let repository = match state
        .repositories
        .iter()
        .find(|repository| repository.observed_common_dir == common)
        .map(|repository| repository.key)
    {
        Some(key) => key,
        None => {
            let key = state.allocate_repository_key()?;
            let first_seen_order = state.allocate_first_seen_order()?;
            state.repositories.push(SavedRepository {
                key,
                observed_common_dir: common.clone(),
                observed_main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
                first_seen_order,
                health: RepositoryHealth::Available,
            });
            key
        }
    };
    let saved = state
        .repositories
        .iter_mut()
        .find(|saved| saved.key == repository)
        .ok_or(LifecycleError::RepositoryMissing(repository))?;
    saved.observed_common_dir = common;
    saved.observed_main_worktree = PersistedPath::from_path(&snapshot.main_worktree);
    saved.health = RepositoryHealth::Available;
    Ok(repository)
}

/// Allocate stable checkout/path identity before any Git mutation.
pub fn prepare_activation(
    state: &mut RepositoryState,
    snapshot: &RepositorySnapshot,
    branch: &str,
) -> Result<PreparedActivation, LifecycleError> {
    let repository = ensure_repository(state, snapshot)?;
    let checkout = state.allocate_checkout_key()?;
    let first_seen_order = state.allocate_first_seen_order()?;
    Ok(PreparedActivation {
        request: ActivationRequest {
            repository,
            branch: branch.to_owned(),
            managed_path: git::managed_branch_worktree_path(
                repository.get(),
                checkout.get(),
                branch,
            ),
        },
        checkout,
        first_seen_order,
    })
}

fn activation_parts(
    outcome: BranchActivationOutcome,
) -> (ActivationDisposition, bool, WorktreeRecord) {
    match outcome {
        BranchActivationOutcome::CreatedManaged(record) => {
            (ActivationDisposition::Created, true, record)
        }
        BranchActivationOutcome::ActivatedManaged(record) => {
            (ActivationDisposition::Activated, true, record)
        }
        BranchActivationOutcome::Reused(record) => (ActivationDisposition::Reused, false, record),
    }
}

/// Execute fresh Git activation and record exactly one verified durable child.
pub fn execute_activation(
    state: &mut RepositoryState,
    repository_child: &std::path::Path,
    prepared: PreparedActivation,
) -> Result<RecordedActivation, LifecycleError> {
    let outcome = git::activate_branch(
        repository_child,
        &prepared.request.branch,
        &prepared.request.managed_path,
    )?;
    let (disposition, created_by_baude, record) = activation_parts(outcome);
    let fresh = git::discover_repository(&record.path)?;
    let full_ref = format!("refs/heads/{}", prepared.request.branch);
    if fresh.selected_worktree.path != record.path
        || record.branch.as_deref() != Some(full_ref.as_str())
        || fresh.selected_worktree.branch.as_deref() != Some(full_ref.as_str())
    {
        return Err(LifecycleError::Topology(format!(
            "expected {full_ref} at {}, observed {:?}",
            record.path.display(),
            fresh.selected_worktree
        )));
    }
    let repository = state
        .repositories
        .iter()
        .find(|saved| saved.key == prepared.request.repository)
        .ok_or(LifecycleError::RepositoryMissing(
            prepared.request.repository,
        ))?;
    if repository.observed_common_dir.to_path_buf() != fresh.common_dir
        || repository.observed_main_worktree.to_path_buf() != fresh.main_worktree
    {
        return Err(LifecycleError::Topology(
            "repository identity changed during activation".into(),
        ));
    }

    let existing = state.checkouts.iter().position(|checkout| {
        checkout.repository_key == prepared.request.repository
            && checkout.observed_path.to_path_buf() == record.path
    });
    let (checkout, managed_by_baude) = if let Some(index) = existing {
        let checkout = &mut state.checkouts[index];
        if checkout.observed_branch.as_deref() != Some(full_ref.as_str()) {
            return Err(LifecycleError::Topology(format!(
                "checkout {} changed branch identity",
                checkout.key.get()
            )));
        }
        checkout.active_intent = true;
        checkout.health = CheckoutHealth::Available;
        checkout.session.cwd = PersistedPath::from_path(&record.path);
        checkout.session.repo_root = PersistedPath::from_path(&fresh.main_worktree);
        checkout.session.branch = Some(prepared.request.branch.clone());
        checkout.session.is_worktree = record.path != fresh.main_worktree;
        (checkout.key, checkout.managed_by_baude)
    } else {
        let is_worktree = record.path != fresh.main_worktree;
        let role = if is_worktree {
            CheckoutRole::ManagedBranch
        } else {
            CheckoutRole::Main
        };
        let repository_name = fresh
            .main_worktree
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| fresh.main_worktree.display().to_string());
        state.checkouts.push(SavedCheckout {
            key: prepared.checkout,
            repository_key: prepared.request.repository,
            role,
            managed_by_baude: created_by_baude,
            observed_path: PersistedPath::from_path(&record.path),
            observed_branch: Some(full_ref),
            first_seen_order: prepared.first_seen_order,
            active_intent: true,
            session: RetainedSessionState {
                name: format!("{repository_name}:{}", prepared.request.branch),
                cwd: PersistedPath::from_path(&record.path),
                repo_root: PersistedPath::from_path(&fresh.main_worktree),
                branch: Some(prepared.request.branch.clone()),
                is_worktree,
                shell_open: false,
                archived: false,
                archived_by_user: false,
            },
            health: CheckoutHealth::Available,
        });
        (prepared.checkout, created_by_baude)
    };
    state.validate()?;
    Ok(RecordedActivation {
        repository: prepared.request.repository,
        checkout,
        disposition,
        managed_by_baude,
        path: record.path,
        main_worktree: fresh.main_worktree,
        branch: prepared.request.branch,
    })
}

/// Shared lifecycle meaning returned by local and daemon runtime owners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleOutcome {
    Created {
        checkout: CheckoutKey,
        runtime: Option<u64>,
    },
    Activated {
        checkout: CheckoutKey,
        runtime: Option<u64>,
    },
    Reused {
        checkout: CheckoutKey,
        runtime: Option<u64>,
        managed_by_baude: bool,
    },
    Focused {
        checkout: CheckoutKey,
        runtime: u64,
    },
    Busy {
        repository: RepositoryKey,
    },
}

/// Cloneable reservation registry. Guards release their repository on drop.
#[derive(Clone, Debug, Default)]
pub struct RepositoryReservations {
    held: Arc<Mutex<HashSet<RepositoryKey>>>,
}

impl RepositoryReservations {
    pub fn reserve(
        &self,
        repository: RepositoryKey,
    ) -> Result<RepositoryReservation, LifecycleOutcome> {
        let mut held = self.held.lock().unwrap_or_else(|error| error.into_inner());
        if !held.insert(repository) {
            return Err(LifecycleOutcome::Busy { repository });
        }
        drop(held);
        Ok(RepositoryReservation {
            held: Arc::clone(&self.held),
            repository,
        })
    }
}

#[derive(Debug)]
pub struct RepositoryReservation {
    held: Arc<Mutex<HashSet<RepositoryKey>>>,
    repository: RepositoryKey,
}

impl Drop for RepositoryReservation {
    fn drop(&mut self) {
        self.held
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.repository);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        plan_close, ActivationRequest, CloseEffect, CloseRequest, LifecycleOutcome,
        RepositoryReservations,
    };
    use crate::repository::{
        CheckoutHealth, CheckoutRole, PersistedPath, RepositoryHealth, RepositoryKey,
        RepositoryState, RetainedSessionState, SavedCheckout, SavedRepository,
    };
    use std::path::{Path, PathBuf};

    fn repository_key() -> RepositoryKey {
        let mut state = RepositoryState::default();
        state.allocate_repository_key().unwrap()
    }

    fn path(value: &str) -> PersistedPath {
        PersistedPath::from_path(Path::new(value))
    }

    fn close_state() -> RepositoryState {
        let mut state = RepositoryState::default();
        let repository = state.allocate_repository_key().unwrap();
        let checkout = state.allocate_checkout_key().unwrap();
        let repository_order = state.allocate_first_seen_order().unwrap();
        let checkout_order = state.allocate_first_seen_order().unwrap();
        state.repositories.push(SavedRepository {
            key: repository,
            observed_common_dir: path("/repo/.git"),
            observed_main_worktree: path("/repo"),
            first_seen_order: repository_order,
            health: RepositoryHealth::Available,
        });
        state.checkouts.push(SavedCheckout {
            key: checkout,
            repository_key: repository,
            role: CheckoutRole::ManagedBranch,
            managed_by_baude: true,
            observed_path: path("/repo-feature"),
            observed_branch: Some("refs/heads/feature/close".into()),
            first_seen_order: checkout_order,
            active_intent: true,
            session: RetainedSessionState {
                name: "old name".into(),
                cwd: path("/repo-feature"),
                repo_root: path("/repo"),
                branch: Some("feature/close".into()),
                is_worktree: true,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
            health: CheckoutHealth::Available,
        });
        state
    }

    #[test]
    fn activation_request_keeps_repository_identity_separate_from_branch_label() {
        let repository = repository_key();
        let request = ActivationRequest {
            repository,
            branch: "feature/literal".into(),
            managed_path: PathBuf::from("/tmp/display-feature-literal-2"),
        };

        assert_eq!(request.repository, repository);
        assert_eq!(request.branch, "feature/literal");
        assert_ne!(request.branch, request.managed_path.to_string_lossy());
    }

    #[test]
    fn repository_reservation_is_busy_until_guard_drops() {
        let repository = repository_key();
        let reservations = RepositoryReservations::default();
        let guard = reservations.reserve(repository).unwrap();

        assert_eq!(
            reservations.reserve(repository).unwrap_err(),
            LifecycleOutcome::Busy { repository }
        );
        drop(guard);
        assert!(reservations.reserve(repository).is_ok());
    }

    #[test]
    fn close_schema_defaults_missing_resume_id_and_round_trips_opaque_value() {
        let mut old = serde_json::to_value(close_state().checkouts[0].session.clone()).unwrap();
        old.as_object_mut().unwrap().remove("resume_id");
        let compatible: RetainedSessionState = serde_json::from_value(old).unwrap();
        assert_eq!(compatible.resume_id, None);

        let hostile = "../../repo; $(touch nope)\nopaque\0conversation";
        let mut present = serde_json::to_value(compatible).unwrap();
        present["resume_id"] = serde_json::Value::String(hostile.into());
        let retained: RetainedSessionState = serde_json::from_value(present).unwrap();
        assert_eq!(retained.resume_id.as_deref(), Some(hostile));
        let round_trip: RetainedSessionState =
            serde_json::from_slice(&serde_json::to_vec(&retained).unwrap()).unwrap();
        assert_eq!(round_trip.resume_id.as_deref(), Some(hostile));
    }

    #[test]
    fn close_preserves_hierarchy_and_orders_snapshot_save_before_stop() {
        let mut state = close_state();
        let before = state.clone();
        let checkout = state.checkouts[0].key;
        let runtime = RetainedSessionState {
            name: "live name".into(),
            cwd: path("/repo-feature"),
            repo_root: path("/repo"),
            branch: Some("feature/close".into()),
            is_worktree: true,
            shell_open: true,
            archived: true,
            archived_by_user: true,
            resume_id: Some("backend-owned-id".into()),
        };

        let plan = plan_close(
            &mut state,
            CloseRequest {
                checkout,
                runtime: runtime.clone(),
            },
        )
        .unwrap();

        assert_eq!(
            plan.effects,
            [
                CloseEffect::SnapshotRuntime,
                CloseEffect::SaveInactiveIntent,
                CloseEffect::StopRuntime,
            ]
        );
        assert_eq!(plan.outcome, LifecycleOutcome::Closed { checkout });
        assert_eq!(state.repositories, before.repositories);
        assert_eq!(state.checkouts.len(), before.checkouts.len());
        let closed = &state.checkouts[0];
        let original = &before.checkouts[0];
        assert_eq!(closed.key, original.key);
        assert_eq!(closed.repository_key, original.repository_key);
        assert_eq!(closed.role, original.role);
        assert_eq!(closed.managed_by_baude, original.managed_by_baude);
        assert_eq!(closed.observed_path, original.observed_path);
        assert_eq!(closed.observed_branch, original.observed_branch);
        assert_eq!(closed.first_seen_order, original.first_seen_order);
        assert_eq!(closed.health, original.health);
        assert_eq!(closed.session, runtime);
        assert!(!closed.active_intent);
    }
}
