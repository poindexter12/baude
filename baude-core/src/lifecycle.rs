//! Shared, UI-free repository lifecycle contracts.

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
