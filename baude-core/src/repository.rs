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

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct StandaloneKey(u64);

impl StandaloneKey {
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

/// Durable authority for one PTY-owned process group. A numeric PID alone is
/// never sufficient because it can be reused after the original child exits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// Linux `/proc/<pid>/stat` start ticks or macOS start-time microseconds.
    pub start_time: u64,
    pub process_group: i32,
    pub session: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RuntimeGeneration(u64);

impl RuntimeGeneration {
    pub const fn initial() -> Self {
        Self(1)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn successor(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "identity")]
#[serde(deny_unknown_fields)]
pub enum ShellOwnership {
    Closed,
    Owned(ProcessIdentity),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedRuntime {
    pub generation: RuntimeGeneration,
    pub agent: ProcessIdentity,
    pub shell: ShellOwnership,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum UnavailableCause {
    Missing,
    NotRepository,
    IdentityChanged,
    /// Destructive authority was durably revoked before or during a removal
    /// attempt. Ordinary reconciliation must never adopt a replacement path.
    RemovalTombstone(String),
    /// A close stopped only part of the runtime. Active intent is retained so
    /// the owning process can retry without presenting the checkout as closed.
    TeardownPending {
        agent_pid: Option<u32>,
        shell_pid: Option<u32>,
        #[serde(default)]
        agent_identity: Option<ProcessIdentity>,
        #[serde(default)]
        shell_identity: Option<ProcessIdentity>,
        agent_stopped: bool,
        shell_stopped: bool,
        detail: String,
    },
    PendingActivation {
        branch: String,
        /// Unknown until Git classifies and executes the activation.
        #[serde(default)]
        created_branch: Option<bool>,
        /// Exact checkout that owned this branch before activation began.
        /// Recovery may reuse it only while Git still reports this same owner.
        #[serde(default)]
        preexisting_branch_owner: Option<PersistedPath>,
    },
    ActivationRecovery {
        branch: String,
        /// `None` means recovery began before Git execution; known outcomes
        /// remain `Some` across every blocked retry.
        #[serde(default)]
        created_branch: Option<bool>,
        #[serde(default)]
        preexisting_branch_owner: Option<PersistedPath>,
        verification: String,
        compensation: String,
    },
    /// Close stopped the retained runtime, persistence rolled back active
    /// intent, and exact runtime restart compensation also failed.
    StoppedActiveRecovery {
        #[serde(default)]
        agent_restarted: bool,
        #[serde(default)]
        shell_restarted: bool,
        detail: String,
    },
    Io(String),
    Other(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "cause")]
#[serde(deny_unknown_fields)]
pub enum RepositoryHealth {
    Available,
    Unavailable(UnavailableCause),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "cause")]
#[serde(deny_unknown_fields)]
pub enum CheckoutHealth {
    Available,
    Unavailable(UnavailableCause),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "candidate")]
#[serde(deny_unknown_fields)]
pub enum CheckoutLifecycle {
    Inactive,
    Active,
    Activating,
    Launching(RuntimeGeneration),
    Running(RuntimeGeneration),
    Stopping(RuntimeGeneration),
    RemovalCommitted,
    Protected(UnavailableCause),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "detail")]
pub enum StandaloneLifecycle {
    Inactive,
    Active,
    Launching(RuntimeGeneration),
    Running(RuntimeGeneration),
    Stopping(RuntimeGeneration),
    ProtectedTeardown(RuntimeGeneration),
    Missing,
    Io(String),
}

impl StandaloneLifecycle {
    pub fn is_protected(&self) -> bool {
        matches!(
            self,
            Self::Launching(_)
                | Self::Running(_)
                | Self::Stopping(_)
                | Self::ProtectedTeardown(_)
                | Self::Missing
                | Self::Io(_)
        )
    }

    pub fn is_launchable(&self) -> bool {
        matches!(self, Self::Inactive | Self::Active)
    }

    pub fn runtime_generation(&self) -> Option<RuntimeGeneration> {
        match self {
            Self::Launching(generation)
            | Self::Running(generation)
            | Self::Stopping(generation)
            | Self::ProtectedTeardown(generation) => Some(*generation),
            Self::Inactive | Self::Active | Self::Missing | Self::Io(_) => None,
        }
    }
}

impl CheckoutLifecycle {
    pub fn from_legacy(active: bool, health: &CheckoutHealth) -> Self {
        match health {
            CheckoutHealth::Available if active => Self::Active,
            CheckoutHealth::Available => Self::Inactive,
            CheckoutHealth::Unavailable(UnavailableCause::PendingActivation { .. }) => {
                Self::Activating
            }
            CheckoutHealth::Unavailable(UnavailableCause::RemovalTombstone(_)) => {
                Self::RemovalCommitted
            }
            CheckoutHealth::Unavailable(cause) => Self::Protected(cause.clone()),
        }
    }

    pub fn is_protected(&self) -> bool {
        matches!(
            self,
            Self::Activating
                | Self::Launching(_)
                | Self::Running(_)
                | Self::Stopping(_)
                | Self::RemovalCommitted
                | Self::Protected(_)
        )
    }

    pub fn is_launchable(&self) -> bool {
        matches!(self, Self::Inactive | Self::Active)
    }
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
    /// Backend-owned conversation identity. It is opaque durable data, not a
    /// pathname or repository/session ownership key.
    #[serde(default)]
    pub resume_id: Option<String>,
}

/// Durable presentation state for a session that is not owned by a Git
/// checkout. Its canonical path is stored by `SavedStandaloneSession`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedStandaloneSessionState {
    pub name: String,
    pub shell_open: bool,
    pub archived: bool,
    pub archived_by_user: bool,
    #[serde(default)]
    pub resume_id: Option<String>,
    /// Distinguishes a never-started durable admission from a closed session
    /// that has no backend conversation identifier.
    #[serde(default)]
    pub ever_launched: bool,
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
    pub(crate) lifecycle: CheckoutLifecycle,
    #[serde(default)]
    pub(crate) owned_runtime: Option<OwnedRuntime>,
    /// Compatibility view for existing presentation code. Durable validation
    /// requires it to agree with `lifecycle`; lifecycle protocol code updates
    /// both through `set_lifecycle`.
    pub(crate) active_intent: bool,
    pub session: RetainedSessionState,
    pub(crate) health: CheckoutHealth,
}

impl SavedCheckout {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        key: CheckoutKey,
        repository_key: RepositoryKey,
        role: CheckoutRole,
        managed_by_baude: bool,
        observed_path: PersistedPath,
        observed_branch: Option<String>,
        first_seen_order: u64,
        lifecycle: CheckoutLifecycle,
        session: RetainedSessionState,
    ) -> Self {
        let mut checkout = Self {
            key,
            repository_key,
            role,
            managed_by_baude,
            observed_path,
            observed_branch,
            first_seen_order,
            lifecycle: CheckoutLifecycle::Inactive,
            owned_runtime: None,
            active_intent: false,
            session,
            health: CheckoutHealth::Available,
        };
        checkout.set_lifecycle(lifecycle);
        checkout
    }

    pub fn lifecycle(&self) -> &CheckoutLifecycle {
        &self.lifecycle
    }

    pub fn owned_runtime(&self) -> Option<&OwnedRuntime> {
        self.owned_runtime.as_ref()
    }

    pub fn active_intent(&self) -> bool {
        self.active_intent
    }

    pub fn health(&self) -> &CheckoutHealth {
        &self.health
    }

    pub(crate) fn set_lifecycle(&mut self, lifecycle: CheckoutLifecycle) {
        self.active_intent = matches!(
            lifecycle,
            CheckoutLifecycle::Active
                | CheckoutLifecycle::Launching(_)
                | CheckoutLifecycle::Running(_)
                | CheckoutLifecycle::Stopping(_)
                | CheckoutLifecycle::Protected(UnavailableCause::TeardownPending { .. })
                | CheckoutLifecycle::Protected(UnavailableCause::StoppedActiveRecovery { .. })
        );
        self.health = match &lifecycle {
            CheckoutLifecycle::Inactive
            | CheckoutLifecycle::Active
            | CheckoutLifecycle::Launching(_)
            | CheckoutLifecycle::Running(_)
            | CheckoutLifecycle::Stopping(_) => CheckoutHealth::Available,
            CheckoutLifecycle::Activating => match &self.health {
                CheckoutHealth::Unavailable(UnavailableCause::PendingActivation { .. }) => {
                    self.health.clone()
                }
                _ => CheckoutHealth::Unavailable(UnavailableCause::Other(
                    "activation pending".into(),
                )),
            },
            CheckoutLifecycle::RemovalCommitted => CheckoutHealth::Unavailable(
                UnavailableCause::RemovalTombstone("removal committed".into()),
            ),
            CheckoutLifecycle::Protected(cause) => CheckoutHealth::Unavailable(cause.clone()),
        };
        self.lifecycle = lifecycle;
    }

    pub(crate) fn set_owned_runtime(&mut self, runtime: Option<OwnedRuntime>) {
        self.owned_runtime = runtime;
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedStandaloneSession {
    pub key: StandaloneKey,
    pub canonical_path: PersistedPath,
    pub first_seen_order: u64,
    lifecycle: StandaloneLifecycle,
    #[serde(default)]
    owned_runtime: Option<OwnedRuntime>,
    pub session: RetainedStandaloneSessionState,
}

impl SavedStandaloneSession {
    pub fn new(
        key: StandaloneKey,
        canonical_path: PersistedPath,
        first_seen_order: u64,
        lifecycle: StandaloneLifecycle,
        owned_runtime: Option<OwnedRuntime>,
        session: RetainedStandaloneSessionState,
    ) -> Self {
        Self {
            key,
            canonical_path,
            first_seen_order,
            lifecycle,
            owned_runtime,
            session,
        }
    }

    pub fn lifecycle(&self) -> &StandaloneLifecycle {
        &self.lifecycle
    }

    pub fn owned_runtime(&self) -> Option<&OwnedRuntime> {
        self.owned_runtime.as_ref()
    }

    pub fn set_lifecycle(&mut self, lifecycle: StandaloneLifecycle) {
        self.lifecycle = lifecycle;
    }

    pub fn set_owned_runtime(&mut self, runtime: Option<OwnedRuntime>) {
        self.owned_runtime = runtime;
    }

    pub fn set_runtime_state(
        &mut self,
        lifecycle: StandaloneLifecycle,
        runtime: Option<OwnedRuntime>,
    ) {
        self.lifecycle = lifecycle;
        self.owned_runtime = runtime;
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryState {
    pub next_repository_key: u64,
    pub next_checkout_key: u64,
    pub next_standalone_key: u64,
    pub next_first_seen_order: u64,
    pub repositories: Vec<SavedRepository>,
    pub checkouts: Vec<SavedCheckout>,
    pub standalone_sessions: Vec<SavedStandaloneSession>,
}

impl Default for RepositoryState {
    fn default() -> Self {
        Self {
            next_repository_key: 1,
            next_checkout_key: 1,
            next_standalone_key: 1,
            next_first_seen_order: 1,
            repositories: Vec::new(),
            checkouts: Vec::new(),
            standalone_sessions: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    DuplicateRepositoryKey(RepositoryKey),
    DuplicateRepositoryIdentity(PersistedPath),
    DuplicateCheckoutKey(CheckoutKey),
    DuplicateStandaloneKey(StandaloneKey),
    DuplicateStandalonePath(PersistedPath),
    StandaloneCheckoutPathConflict(PersistedPath),
    MissingCheckout(CheckoutKey),
    DuplicateCheckoutOwnership {
        path: PersistedPath,
        first_repository: RepositoryKey,
        second_repository: RepositoryKey,
    },
    RetainedCheckoutPathMismatch(CheckoutKey),
    RetainedRepositoryPathMismatch(CheckoutKey),
    RetainedWorktreeFlagMismatch(CheckoutKey),
    DanglingRepositoryKey(RepositoryKey),
    DuplicateRole {
        repository_key: RepositoryKey,
        role: CheckoutRole,
    },
    RegressingRepositoryCounter,
    RegressingCheckoutCounter,
    RegressingStandaloneCounter,
    RegressingOrderCounter,
    DuplicateFirstSeenOrder(u64),
    ContradictoryLifecycle(CheckoutKey),
    ContradictoryStandaloneLifecycle(StandaloneKey),
    ExhaustedRepositoryCounter,
    ExhaustedCheckoutCounter,
    ExhaustedStandaloneCounter,
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
    StandaloneKeysExhausted,
    FirstSeenOrderExhausted,
}

impl std::fmt::Display for AllocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "repository state allocation failed: {self:?}")
    }
}

impl std::error::Error for AllocationError {}

impl RepositoryState {
    pub(crate) fn validate_lifecycle_views(&self) -> Result<(), ValidationError> {
        for checkout in &self.checkouts {
            let views_agree = match &checkout.lifecycle {
                CheckoutLifecycle::Inactive => {
                    !checkout.active_intent && checkout.health == CheckoutHealth::Available
                }
                CheckoutLifecycle::Active
                | CheckoutLifecycle::Launching(_)
                | CheckoutLifecycle::Running(_)
                | CheckoutLifecycle::Stopping(_) => {
                    checkout.active_intent && checkout.health == CheckoutHealth::Available
                }
                CheckoutLifecycle::Activating => {
                    !checkout.active_intent
                        && matches!(checkout.health, CheckoutHealth::Unavailable(_))
                }
                CheckoutLifecycle::RemovalCommitted => {
                    !checkout.active_intent
                        && matches!(
                            checkout.health,
                            CheckoutHealth::Unavailable(UnavailableCause::RemovalTombstone(_))
                        )
                }
                CheckoutLifecycle::Protected(cause) => {
                    checkout.active_intent
                        == matches!(
                            cause,
                            UnavailableCause::TeardownPending { .. }
                                | UnavailableCause::StoppedActiveRecovery { .. }
                        )
                        && checkout.health == CheckoutHealth::Unavailable(cause.clone())
                }
            };
            if !views_agree {
                return Err(ValidationError::ContradictoryLifecycle(checkout.key));
            }
            match (&checkout.lifecycle, &checkout.owned_runtime) {
                (
                    CheckoutLifecycle::Launching(expected)
                    | CheckoutLifecycle::Running(expected)
                    | CheckoutLifecycle::Stopping(expected),
                    Some(runtime),
                ) if *expected == runtime.generation => {}
                (
                    CheckoutLifecycle::Launching(_)
                    | CheckoutLifecycle::Running(_)
                    | CheckoutLifecycle::Stopping(_),
                    _,
                ) => return Err(ValidationError::ContradictoryLifecycle(checkout.key)),
                (
                    CheckoutLifecycle::Protected(UnavailableCause::TeardownPending { .. }),
                    Some(_),
                ) => {}
                (_, Some(_)) => {
                    return Err(ValidationError::ContradictoryLifecycle(checkout.key));
                }
                (_, None) => {}
            }
        }
        Ok(())
    }

    pub(crate) fn validate_standalone_lifecycles(&self) -> Result<(), ValidationError> {
        for standalone in &self.standalone_sessions {
            match (
                standalone.lifecycle.runtime_generation(),
                &standalone.owned_runtime,
            ) {
                (Some(expected), Some(runtime)) if expected == runtime.generation => {}
                (None, None) => {}
                _ => {
                    return Err(ValidationError::ContradictoryStandaloneLifecycle(
                        standalone.key,
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn has_pending_activation(&self) -> bool {
        self.checkouts.iter().any(|checkout| {
            matches!(
                checkout.health,
                CheckoutHealth::Unavailable(UnavailableCause::PendingActivation { .. })
            )
        })
    }

    pub fn allocate_repository_key(&mut self) -> Result<RepositoryKey, AllocationError> {
        let key = RepositoryKey(self.next_repository_key);
        let next = self
            .next_repository_key
            .checked_add(1)
            .ok_or(AllocationError::RepositoryKeysExhausted)?;
        if next == u64::MAX {
            return Err(AllocationError::RepositoryKeysExhausted);
        }
        self.next_repository_key = next;
        Ok(key)
    }

    pub fn allocate_checkout_key(&mut self) -> Result<CheckoutKey, AllocationError> {
        let key = CheckoutKey(self.next_checkout_key);
        let next = self
            .next_checkout_key
            .checked_add(1)
            .ok_or(AllocationError::CheckoutKeysExhausted)?;
        if next == u64::MAX {
            return Err(AllocationError::CheckoutKeysExhausted);
        }
        self.next_checkout_key = next;
        Ok(key)
    }

    pub fn allocate_standalone_key(&mut self) -> Result<StandaloneKey, AllocationError> {
        let key = StandaloneKey(self.next_standalone_key);
        let next = self
            .next_standalone_key
            .checked_add(1)
            .ok_or(AllocationError::StandaloneKeysExhausted)?;
        if next == u64::MAX {
            return Err(AllocationError::StandaloneKeysExhausted);
        }
        self.next_standalone_key = next;
        Ok(key)
    }

    pub fn standalone_session(&self, key: StandaloneKey) -> Option<&SavedStandaloneSession> {
        self.standalone_sessions
            .iter()
            .find(|session| session.key == key)
    }

    pub fn standalone_session_mut(
        &mut self,
        key: StandaloneKey,
    ) -> Option<&mut SavedStandaloneSession> {
        self.standalone_sessions
            .iter_mut()
            .find(|session| session.key == key)
    }

    pub fn standalone_session_by_path(
        &self,
        canonical_path: &PersistedPath,
    ) -> Option<&SavedStandaloneSession> {
        self.standalone_sessions
            .iter()
            .find(|session| &session.canonical_path == canonical_path)
    }

    pub fn standalone_session_by_path_mut(
        &mut self,
        canonical_path: &PersistedPath,
    ) -> Option<&mut SavedStandaloneSession> {
        self.standalone_sessions
            .iter_mut()
            .find(|session| &session.canonical_path == canonical_path)
    }

    pub fn remove_standalone_session(
        &mut self,
        key: StandaloneKey,
    ) -> Option<SavedStandaloneSession> {
        let index = self
            .standalone_sessions
            .iter()
            .position(|session| session.key == key)?;
        Some(self.standalone_sessions.remove(index))
    }

    pub fn allocate_first_seen_order(&mut self) -> Result<u64, AllocationError> {
        let order = self.next_first_seen_order;
        let next = self
            .next_first_seen_order
            .checked_add(1)
            .ok_or(AllocationError::FirstSeenOrderExhausted)?;
        if next == u64::MAX {
            return Err(AllocationError::FirstSeenOrderExhausted);
        }
        self.next_first_seen_order = next;
        Ok(order)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.next_repository_key == u64::MAX {
            return Err(ValidationError::ExhaustedRepositoryCounter);
        }
        if self.next_checkout_key == u64::MAX {
            return Err(ValidationError::ExhaustedCheckoutCounter);
        }
        if self.next_standalone_key == u64::MAX {
            return Err(ValidationError::ExhaustedStandaloneCounter);
        }
        if self.next_first_seen_order == u64::MAX {
            return Err(ValidationError::ExhaustedOrderCounter);
        }
        let mut repository_keys = HashSet::new();
        let mut repository_identities = HashSet::new();
        let mut checkout_keys = HashSet::new();
        let mut checkout_owners = HashMap::new();
        let mut standalone_keys = HashSet::new();
        let mut standalone_paths = HashSet::new();
        let repositories_by_key: HashMap<_, _> = self
            .repositories
            .iter()
            .map(|repository| (repository.key, repository))
            .collect();
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
            let repository = repositories_by_key[&checkout.repository_key];
            if checkout.session.cwd != checkout.observed_path {
                return Err(ValidationError::RetainedCheckoutPathMismatch(checkout.key));
            }
            if checkout.session.repo_root != repository.observed_main_worktree {
                return Err(ValidationError::RetainedRepositoryPathMismatch(
                    checkout.key,
                ));
            }
            if checkout.session.is_worktree
                != (checkout.observed_path != repository.observed_main_worktree)
            {
                return Err(ValidationError::RetainedWorktreeFlagMismatch(checkout.key));
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

        for standalone in &self.standalone_sessions {
            if !standalone_keys.insert(standalone.key) {
                return Err(ValidationError::DuplicateStandaloneKey(standalone.key));
            }
            if !standalone_paths.insert(&standalone.canonical_path) {
                return Err(ValidationError::DuplicateStandalonePath(
                    standalone.canonical_path.clone(),
                ));
            }
            if checkout_owners.contains_key(&standalone.canonical_path) {
                return Err(ValidationError::StandaloneCheckoutPathConflict(
                    standalone.canonical_path.clone(),
                ));
            }
            if self
                .repositories
                .iter()
                .any(|repository| repository.observed_main_worktree == standalone.canonical_path)
            {
                return Err(ValidationError::StandaloneCheckoutPathConflict(
                    standalone.canonical_path.clone(),
                ));
            }
            if !orders.insert(standalone.first_seen_order) {
                return Err(ValidationError::DuplicateFirstSeenOrder(
                    standalone.first_seen_order,
                ));
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
        if self.next_standalone_key == 0
            || self
                .standalone_sessions
                .iter()
                .any(|standalone| standalone.key.get() >= self.next_standalone_key)
        {
            return Err(ValidationError::RegressingStandaloneCounter);
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
                .chain(
                    self.standalone_sessions
                        .iter()
                        .map(|standalone| standalone.first_seen_order),
                )
                .any(|order| order >= self.next_first_seen_order)
        {
            return Err(ValidationError::RegressingOrderCounter);
        }
        self.validate_standalone_lifecycles()
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
            lifecycle: CheckoutLifecycle::Protected(UnavailableCause::Missing),
            owned_runtime: None,
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
                resume_id: None,
            },
            health: CheckoutHealth::Unavailable(UnavailableCause::Missing),
        }
    }

    fn owned_runtime(generation: RuntimeGeneration) -> OwnedRuntime {
        OwnedRuntime {
            generation,
            agent: ProcessIdentity {
                pid: 100,
                start_time: 200,
                process_group: 100,
                session: 100,
            },
            shell: ShellOwnership::Closed,
        }
    }

    fn standalone(key: StandaloneKey, canonical_path: &str, order: u64) -> SavedStandaloneSession {
        SavedStandaloneSession::new(
            key,
            path(canonical_path),
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
        )
    }

    #[test]
    fn standalone_allocation_paths_ordering_and_lifecycle_are_validated() {
        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key().unwrap();
        let checkout_key = state.allocate_checkout_key().unwrap();
        let standalone_key = state.allocate_standalone_key().unwrap();
        let repository_order = state.allocate_first_seen_order().unwrap();
        let checkout_order = state.allocate_first_seen_order().unwrap();
        let standalone_order = state.allocate_first_seen_order().unwrap();
        state
            .repositories
            .push(repository(repository_key, repository_order));
        state
            .checkouts
            .push(checkout(checkout_key, repository_key, checkout_order));
        state
            .standalone_sessions
            .push(standalone(standalone_key, "/standalone", standalone_order));

        assert_eq!(standalone_key.get(), 1);
        assert_eq!(
            state
                .standalone_session_by_path(&path("/standalone"))
                .map(|session| session.key),
            Some(standalone_key)
        );
        assert!(state.validate().is_ok());

        let generation = RuntimeGeneration::initial();
        state
            .standalone_session_mut(standalone_key)
            .unwrap()
            .set_runtime_state(
                StandaloneLifecycle::ProtectedTeardown(generation),
                Some(owned_runtime(generation)),
            );
        assert!(state.validate().is_ok());

        for lifecycle in [
            StandaloneLifecycle::Launching(generation),
            StandaloneLifecycle::Running(generation),
            StandaloneLifecycle::Stopping(generation),
            StandaloneLifecycle::ProtectedTeardown(generation),
        ] {
            let mut candidate = state.clone();
            candidate.standalone_sessions[0]
                .set_runtime_state(lifecycle, Some(owned_runtime(generation)));
            assert!(candidate.validate().is_ok());
        }
        for lifecycle in [
            StandaloneLifecycle::Inactive,
            StandaloneLifecycle::Active,
            StandaloneLifecycle::Missing,
            StandaloneLifecycle::Io("read failed".into()),
        ] {
            let mut candidate = state.clone();
            candidate.standalone_sessions[0].set_runtime_state(lifecycle, None);
            assert!(candidate.validate().is_ok());
        }

        let mut wrong_generation = state.clone();
        wrong_generation.standalone_sessions[0]
            .set_owned_runtime(Some(owned_runtime(generation.successor().unwrap())));
        assert_eq!(
            wrong_generation.validate(),
            Err(ValidationError::ContradictoryStandaloneLifecycle(
                standalone_key
            ))
        );

        let mut runtime_without_owner = state.clone();
        runtime_without_owner.standalone_sessions[0]
            .set_runtime_state(StandaloneLifecycle::Active, Some(owned_runtime(generation)));
        assert_eq!(
            runtime_without_owner.validate(),
            Err(ValidationError::ContradictoryStandaloneLifecycle(
                standalone_key
            ))
        );

        let mut duplicate_path = state.clone();
        let duplicate_key = duplicate_path.allocate_standalone_key().unwrap();
        let duplicate_order = duplicate_path.allocate_first_seen_order().unwrap();
        duplicate_path.standalone_sessions.push(standalone(
            duplicate_key,
            "/standalone",
            duplicate_order,
        ));
        assert!(matches!(
            duplicate_path.validate(),
            Err(ValidationError::DuplicateStandalonePath(_))
        ));

        let mut duplicate_key = state.clone();
        let duplicate_order = duplicate_key.allocate_first_seen_order().unwrap();
        duplicate_key.standalone_sessions.push(standalone(
            standalone_key,
            "/other-standalone",
            duplicate_order,
        ));
        assert_eq!(
            duplicate_key.validate(),
            Err(ValidationError::DuplicateStandaloneKey(standalone_key))
        );

        let mut checkout_conflict = state.clone();
        checkout_conflict.standalone_sessions[0].canonical_path = path("/repo-default");
        assert!(matches!(
            checkout_conflict.validate(),
            Err(ValidationError::StandaloneCheckoutPathConflict(_))
        ));

        let mut main_worktree_conflict = state.clone();
        main_worktree_conflict.standalone_sessions[0].canonical_path = path("/repo");
        assert!(matches!(
            main_worktree_conflict.validate(),
            Err(ValidationError::StandaloneCheckoutPathConflict(_))
        ));

        let mut duplicate_order = state.clone();
        duplicate_order.standalone_sessions[0].first_seen_order = checkout_order;
        assert_eq!(
            duplicate_order.validate(),
            Err(ValidationError::DuplicateFirstSeenOrder(checkout_order))
        );

        assert_eq!(
            state
                .remove_standalone_session(standalone_key)
                .map(|saved| saved.key),
            Some(standalone_key)
        );
        assert!(state.validate().is_ok());
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
        let mut duplicate_checkout = SavedCheckout {
            key: duplicate_checkout_key,
            repository_key: second_repository_key,
            role: CheckoutRole::ManagedBranch,
            first_seen_order: duplicate_checkout_order,
            ..duplicate_ownership.checkouts[0].clone()
        };
        duplicate_checkout.session.repo_root = path("/other");
        duplicate_ownership.checkouts.push(duplicate_checkout);
        assert!(matches!(
            duplicate_ownership.validate(),
            Err(ValidationError::DuplicateCheckoutOwnership { .. })
        ));

        let mut mismatched_checkout = state.clone();
        mismatched_checkout.checkouts[0].session.cwd = path("/unverified");
        assert_eq!(
            mismatched_checkout.validate(),
            Err(ValidationError::RetainedCheckoutPathMismatch(checkout_key))
        );

        let mut mismatched_repository = state.clone();
        mismatched_repository.checkouts[0].session.repo_root = path("/other");
        assert_eq!(
            mismatched_repository.validate(),
            Err(ValidationError::RetainedRepositoryPathMismatch(
                checkout_key
            ))
        );

        let mut mismatched_worktree = state.clone();
        mismatched_worktree.checkouts[0].session.is_worktree = false;
        assert_eq!(
            mismatched_worktree.validate(),
            Err(ValidationError::RetainedWorktreeFlagMismatch(checkout_key))
        );

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
            next_standalone_key: 0,
            next_first_seen_order: 0,
            repositories: Vec::new(),
            checkouts: Vec::new(),
            standalone_sessions: Vec::new(),
        };
        assert_eq!(
            zeroed.validate(),
            Err(ValidationError::RegressingRepositoryCounter),
            "empty state must still retain the monotonic counter origin"
        );

        for (field, expected) in [
            ("repository", ValidationError::ExhaustedRepositoryCounter),
            ("checkout", ValidationError::ExhaustedCheckoutCounter),
            ("standalone", ValidationError::ExhaustedStandaloneCounter),
            ("order", ValidationError::ExhaustedOrderCounter),
        ] {
            let mut exhausted = RepositoryState::default();
            match field {
                "repository" => exhausted.next_repository_key = u64::MAX,
                "checkout" => exhausted.next_checkout_key = u64::MAX,
                "standalone" => exhausted.next_standalone_key = u64::MAX,
                "order" => exhausted.next_first_seen_order = u64::MAX,
                _ => unreachable!(),
            }
            assert_eq!(exhausted.validate(), Err(expected));
        }

        for (counter, expected) in [
            ("repository", AllocationError::RepositoryKeysExhausted),
            ("checkout", AllocationError::CheckoutKeysExhausted),
            ("standalone", AllocationError::StandaloneKeysExhausted),
            ("order", AllocationError::FirstSeenOrderExhausted),
        ] {
            let mut exhausted = RepositoryState::default();
            match counter {
                "repository" => exhausted.next_repository_key = u64::MAX - 1,
                "checkout" => exhausted.next_checkout_key = u64::MAX - 1,
                "standalone" => exhausted.next_standalone_key = u64::MAX - 1,
                "order" => exhausted.next_first_seen_order = u64::MAX - 1,
                _ => unreachable!(),
            }
            let before = exhausted.clone();
            let result = match counter {
                "repository" => exhausted.allocate_repository_key().map(|_| ()),
                "checkout" => exhausted.allocate_checkout_key().map(|_| ()),
                "standalone" => exhausted.allocate_standalone_key().map(|_| ()),
                "order" => exhausted.allocate_first_seen_order().map(|_| ()),
                _ => unreachable!(),
            };
            assert_eq!(result, Err(expected));
            assert_eq!(exhausted, before, "failed allocation must not mutate state");
            assert!(exhausted.validate().is_ok());
        }
    }
}
