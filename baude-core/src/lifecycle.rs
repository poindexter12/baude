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
    use super::{ActivationRequest, LifecycleOutcome, RepositoryReservations};
    use crate::repository::{RepositoryKey, RepositoryState};
    use std::path::PathBuf;

    fn repository_key() -> RepositoryKey {
        let mut state = RepositoryState::default();
        state.allocate_repository_key().unwrap()
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
}
