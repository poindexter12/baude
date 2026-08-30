//! Durable, workspace-scoped repository and checkout intent.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RepositoryKey(u64);

impl RepositoryKey {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CheckoutKey(u64);

impl CheckoutKey {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Lossless durable pathname bytes for the supported Unix targets.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PersistedPath(Vec<u8>);

impl PersistedPath {
    #[cfg(unix)]
    pub fn from_path(path: &Path) -> Self {
        Self(path.as_os_str().as_bytes().to_vec())
    }

    #[cfg(unix)]
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(std::ffi::OsString::from_vec(self.0.clone()))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableCause {
    Missing,
    NotRepository,
    IdentityChanged,
    Io(String),
    Other(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "cause")]
pub enum RepositoryHealth {
    Available,
    Unavailable(UnavailableCause),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "cause")]
pub enum CheckoutHealth {
    Available,
    Unavailable(UnavailableCause),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutRole {
    Main,
    PrimaryDefault,
    ManagedBranch,
}

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedRepository {
    pub key: RepositoryKey,
    pub observed_common_dir: PersistedPath,
    pub observed_main_worktree: PersistedPath,
    pub first_seen_order: u64,
    pub health: RepositoryHealth,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryState {
    pub next_repository_key: u64,
    pub next_checkout_key: u64,
    pub next_first_seen_order: u64,
    pub repositories: Vec<SavedRepository>,
    pub checkouts: Vec<SavedCheckout>,
}

impl Default for RepositoryState {
    fn default() -> Self {
        Self {
            next_repository_key: 1,
            next_checkout_key: 1,
            next_first_seen_order: 1,
            repositories: Vec::new(),
            checkouts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    DuplicateRepositoryKey(RepositoryKey),
    DuplicateRepositoryIdentity(PersistedPath),
    DuplicateCheckoutKey(CheckoutKey),
    DuplicateCheckoutOwnership {
        path: PersistedPath,
        first_repository: RepositoryKey,
        second_repository: RepositoryKey,
    },
    DanglingRepositoryKey(RepositoryKey),
    DuplicateRole {
        repository_key: RepositoryKey,
        role: CheckoutRole,
    },
    RegressingRepositoryCounter,
    RegressingCheckoutCounter,
    RegressingOrderCounter,
    DuplicateFirstSeenOrder(u64),
    ExhaustedRepositoryCounter,
    ExhaustedCheckoutCounter,
    ExhaustedOrderCounter,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid repository state: {self:?}")
    }
}

impl std::error::Error for ValidationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocationError {
    RepositoryKeysExhausted,
    CheckoutKeysExhausted,
    FirstSeenOrderExhausted,
}

impl std::fmt::Display for AllocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "repository state allocation failed: {self:?}")
    }
}

impl std::error::Error for AllocationError {}

impl RepositoryState {
    pub fn allocate_repository_key(&mut self) -> Result<RepositoryKey, AllocationError> {
        let key = RepositoryKey(self.next_repository_key);
        self.next_repository_key = self
            .next_repository_key
            .checked_add(1)
            .ok_or(AllocationError::RepositoryKeysExhausted)?;
        Ok(key)
    }

    pub fn allocate_checkout_key(&mut self) -> Result<CheckoutKey, AllocationError> {
        let key = CheckoutKey(self.next_checkout_key);
        self.next_checkout_key = self
            .next_checkout_key
            .checked_add(1)
            .ok_or(AllocationError::CheckoutKeysExhausted)?;
        Ok(key)
    }

    pub fn allocate_first_seen_order(&mut self) -> Result<u64, AllocationError> {
        let order = self.next_first_seen_order;
        self.next_first_seen_order = self
            .next_first_seen_order
            .checked_add(1)
            .ok_or(AllocationError::FirstSeenOrderExhausted)?;
        Ok(order)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.next_repository_key == u64::MAX {
            return Err(ValidationError::ExhaustedRepositoryCounter);
        }
        if self.next_checkout_key == u64::MAX {
            return Err(ValidationError::ExhaustedCheckoutCounter);
        }
        if self.next_first_seen_order == u64::MAX {
            return Err(ValidationError::ExhaustedOrderCounter);
        }
        let mut repository_keys = HashSet::new();
        let mut repository_identities = HashSet::new();
        let mut checkout_keys = HashSet::new();
        let mut checkout_owners = HashMap::new();
        let mut orders = HashSet::new();
        let mut unique_roles = HashSet::new();

        for repository in &self.repositories {
            if !repository_keys.insert(repository.key) {
                return Err(ValidationError::DuplicateRepositoryKey(repository.key));
            }
            if !repository_identities.insert(&repository.observed_common_dir) {
                return Err(ValidationError::DuplicateRepositoryIdentity(
                    repository.observed_common_dir.clone(),
                ));
            }
            if !orders.insert(repository.first_seen_order) {
                return Err(ValidationError::DuplicateFirstSeenOrder(
                    repository.first_seen_order,
                ));
            }
        }

        for checkout in &self.checkouts {
            if !checkout_keys.insert(checkout.key) {
                return Err(ValidationError::DuplicateCheckoutKey(checkout.key));
            }
            if !repository_keys.contains(&checkout.repository_key) {
                return Err(ValidationError::DanglingRepositoryKey(
                    checkout.repository_key,
                ));
            }
            if let Some(first_repository) =
                checkout_owners.insert(&checkout.observed_path, checkout.repository_key)
            {
                if first_repository != checkout.repository_key {
                    return Err(ValidationError::DuplicateCheckoutOwnership {
                        path: checkout.observed_path.clone(),
                        first_repository,
                        second_repository: checkout.repository_key,
                    });
                }
            }
            if !orders.insert(checkout.first_seen_order) {
                return Err(ValidationError::DuplicateFirstSeenOrder(
                    checkout.first_seen_order,
                ));
            }
            if matches!(
                checkout.role,
                CheckoutRole::Main | CheckoutRole::PrimaryDefault
            ) && !unique_roles.insert((checkout.repository_key, checkout.role))
            {
                return Err(ValidationError::DuplicateRole {
                    repository_key: checkout.repository_key,
                    role: checkout.role,
                });
            }
        }

        if self.next_repository_key == 0
            || self
                .repositories
                .iter()
                .any(|repository| repository.key.get() >= self.next_repository_key)
        {
            return Err(ValidationError::RegressingRepositoryCounter);
        }
        if self.next_checkout_key == 0
            || self
                .checkouts
                .iter()
                .any(|checkout| checkout.key.get() >= self.next_checkout_key)
        {
            return Err(ValidationError::RegressingCheckoutCounter);
        }
        if self.next_first_seen_order == 0
            || self
                .repositories
                .iter()
                .map(|repository| repository.first_seen_order)
                .chain(
                    self.checkouts
                        .iter()
                        .map(|checkout| checkout.first_seen_order),
                )
                .any(|order| order >= self.next_first_seen_order)
        {
            return Err(ValidationError::RegressingOrderCounter);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> PersistedPath {
        PersistedPath::from_path(Path::new(value))
    }

    fn repository(key: RepositoryKey, order: u64) -> SavedRepository {
        SavedRepository {
            key,
            observed_common_dir: path("/repo/.git"),
            observed_main_worktree: path("/repo"),
            first_seen_order: order,
            health: RepositoryHealth::Available,
        }
    }

    fn checkout(key: CheckoutKey, repository_key: RepositoryKey, order: u64) -> SavedCheckout {
        SavedCheckout {
            key,
            repository_key,
            role: CheckoutRole::PrimaryDefault,
            managed_by_baude: true,
            observed_path: path("/repo-default"),
            observed_branch: Some("main".into()),
            first_seen_order: order,
            active_intent: true,
            session: RetainedSessionState {
                name: "repo".into(),
                cwd: path("/repo-default"),
                repo_root: path("/repo"),
                branch: Some("main".into()),
                is_worktree: true,
                shell_open: true,
                archived: false,
                archived_by_user: false,
            },
            health: CheckoutHealth::Unavailable(UnavailableCause::Missing),
        }
    }

    #[test]
    fn allocation_is_monotonic_and_validation_rejects_invalid_graphs() {
        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key().unwrap();
        let checkout_key = state.allocate_checkout_key().unwrap();
        let repository_order = state.allocate_first_seen_order().unwrap();
        let checkout_order = state.allocate_first_seen_order().unwrap();
        state
            .repositories
            .push(repository(repository_key, repository_order));
        state
            .checkouts
            .push(checkout(checkout_key, repository_key, checkout_order));
        assert_eq!(repository_key.get(), 1);
        assert_eq!(checkout_key.get(), 1);
        assert!(state.validate().is_ok());

        let mut duplicate = state.clone();
        duplicate.repositories.push(repository(repository_key, 3));
        assert!(matches!(
            duplicate.validate(),
            Err(ValidationError::DuplicateRepositoryKey(_))
        ));

        let mut duplicate_identity = state.clone();
        let second_repository_key = duplicate_identity.allocate_repository_key().unwrap();
        let second_repository_order = duplicate_identity.allocate_first_seen_order().unwrap();
        duplicate_identity.repositories.push(SavedRepository {
            key: second_repository_key,
            first_seen_order: second_repository_order,
            ..duplicate_identity.repositories[0].clone()
        });
        assert!(matches!(
            duplicate_identity.validate(),
            Err(ValidationError::DuplicateRepositoryIdentity(_))
        ));

        let mut duplicate_ownership = state.clone();
        let second_repository_key = duplicate_ownership.allocate_repository_key().unwrap();
        let second_repository_order = duplicate_ownership.allocate_first_seen_order().unwrap();
        let mut second_repository = repository(second_repository_key, second_repository_order);
        second_repository.observed_common_dir = path("/other/.git");
        second_repository.observed_main_worktree = path("/other");
        duplicate_ownership.repositories.push(second_repository);
        let duplicate_checkout_key = duplicate_ownership.allocate_checkout_key().unwrap();
        let duplicate_checkout_order = duplicate_ownership.allocate_first_seen_order().unwrap();
        duplicate_ownership.checkouts.push(SavedCheckout {
            key: duplicate_checkout_key,
            repository_key: second_repository_key,
            role: CheckoutRole::ManagedBranch,
            first_seen_order: duplicate_checkout_order,
            ..duplicate_ownership.checkouts[0].clone()
        });
        assert!(matches!(
            duplicate_ownership.validate(),
            Err(ValidationError::DuplicateCheckoutOwnership { .. })
        ));

        let mut dangling = state.clone();
        dangling.checkouts[0].repository_key = RepositoryKey(99);
        assert!(matches!(
            dangling.validate(),
            Err(ValidationError::DanglingRepositoryKey(_))
        ));

        let mut duplicate_primary = state.clone();
        let duplicate_key = duplicate_primary.allocate_checkout_key().unwrap();
        let duplicate_order = duplicate_primary.allocate_first_seen_order().unwrap();
        duplicate_primary
            .checkouts
            .push(checkout(duplicate_key, repository_key, duplicate_order));
        assert!(matches!(
            duplicate_primary.validate(),
            Err(ValidationError::DuplicateRole { .. })
        ));

        let mut regressing = state;
        regressing.next_repository_key = repository_key.get();
        assert_eq!(
            regressing.validate(),
            Err(ValidationError::RegressingRepositoryCounter)
        );

        let zeroed = RepositoryState {
            next_repository_key: 0,
            next_checkout_key: 0,
            next_first_seen_order: 0,
            repositories: Vec::new(),
            checkouts: Vec::new(),
        };
        assert_eq!(
            zeroed.validate(),
            Err(ValidationError::RegressingRepositoryCounter),
            "empty state must still retain the monotonic counter origin"
        );

        for (field, expected) in [
            ("repository", ValidationError::ExhaustedRepositoryCounter),
            ("checkout", ValidationError::ExhaustedCheckoutCounter),
            ("order", ValidationError::ExhaustedOrderCounter),
        ] {
            let mut exhausted = RepositoryState::default();
            match field {
                "repository" => exhausted.next_repository_key = u64::MAX,
                "checkout" => exhausted.next_checkout_key = u64::MAX,
                "order" => exhausted.next_first_seen_order = u64::MAX,
                _ => unreachable!(),
            }
            assert_eq!(exhausted.validate(), Err(expected));
        }

        let mut exhausted = RepositoryState::default();
        exhausted.next_repository_key = u64::MAX;
        assert_eq!(
            exhausted.allocate_repository_key(),
            Err(AllocationError::RepositoryKeysExhausted)
        );
    }
}
