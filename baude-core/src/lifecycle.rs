//! Shared, UI-free repository lifecycle contracts.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::repository::{CheckoutKey, RepositoryKey};

/// A literal branch activation rooted in one durable repository identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivationRequest {
    pub repository: RepositoryKey,
    pub branch: String,
    pub managed_path: PathBuf,
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
