//! Shared, UI-free repository lifecycle contracts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::backend::SpawnMode;
use crate::git::{
    self, BranchActivationError, BranchActivationOutcome, ReconciliationUnavailable,
    RemovalBlocker, RemovalSafety, RemoveVerifiedError, RepositoryDiscoveryError,
    RepositorySnapshot, VerifiedRemoval, VerifiedRemovalTarget, WorktreeRecord,
};
use crate::repository::{
    AllocationError, CheckoutHealth, CheckoutKey, CheckoutRole, PersistedPath, RepositoryHealth,
    RepositoryKey, RepositoryState, RetainedSessionState, SavedCheckout, SavedRepository,
    UnavailableCause, ValidationError,
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

/// Complete runtime metadata captured at the retained-close boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloseRequest {
    pub checkout: CheckoutKey,
    pub runtime: RetainedSessionState,
}

/// Effects are deliberately explicit so runtime owners cannot stop a process
/// before the inactive aggregate replacement has been authorized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseEffect {
    SnapshotRuntime,
    SaveInactiveIntent,
    StopRuntime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosePlan {
    pub checkout: CheckoutKey,
    pub effects: [CloseEffect; 3],
    pub outcome: LifecycleOutcome,
}

/// A confirmation capability identifies one freshly inspected durable child,
/// but deliberately does not retain the first preflight's mutation token.
/// Confirmation must inspect again after the runtime has stopped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalConfirmation {
    repository: RepositoryKey,
    checkout: CheckoutKey,
    path: PathBuf,
    branch_ref: String,
}

impl RemovalConfirmation {
    pub fn repository(&self) -> RepositoryKey {
        self.repository
    }

    pub fn checkout(&self) -> CheckoutKey {
        self.checkout
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn branch_ref(&self) -> &str {
        &self.branch_ref
    }
}

#[derive(Debug)]
pub enum RemovalFailure {
    CheckoutMissing(CheckoutKey),
    RepositoryMissing(RepositoryKey),
    ConfirmationStale,
    Inspection(String),
    Blocked(Vec<RemovalBlocker>),
    GitRefused(String),
    Compensation { original: String, recovery: String },
}

impl std::fmt::Display for RemovalFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CheckoutMissing(key) => write!(f, "removal checkout {} is missing", key.get()),
            Self::RepositoryMissing(key) => {
                write!(f, "removal repository {} is missing", key.get())
            }
            Self::ConfirmationStale => f.write_str("removal confirmation target changed"),
            Self::Inspection(detail) => write!(f, "removal inspection failed: {detail}"),
            Self::Blocked(blockers) => write!(f, "removal blocked: {blockers:?}"),
            Self::GitRefused(detail) => write!(f, "plain Git removal refused: {detail}"),
            Self::Compensation { original, recovery } => write!(
                f,
                "{original}; runtime compensation also failed: {recovery}"
            ),
        }
    }
}

impl std::error::Error for RemovalFailure {}

fn removal_facts(
    state: &RepositoryState,
    checkout_key: CheckoutKey,
) -> Result<(&SavedRepository, &SavedCheckout), RemovalFailure> {
    let checkout = state
        .checkouts
        .iter()
        .find(|checkout| checkout.key == checkout_key)
        .ok_or(RemovalFailure::CheckoutMissing(checkout_key))?;
    let repository = state
        .repositories
        .iter()
        .find(|repository| repository.key == checkout.repository_key)
        .ok_or(RemovalFailure::RepositoryMissing(checkout.repository_key))?;
    Ok((repository, checkout))
}

fn inspect_removal_facts(
    repository: &SavedRepository,
    checkout: &SavedCheckout,
) -> Result<VerifiedRemovalTarget, RemovalFailure> {
    match git::inspect_removal(&repository.observed_common_dir.to_path_buf(), checkout)
        .map_err(|error| RemovalFailure::Inspection(error.to_string()))?
    {
        RemovalSafety::Safe(target) => Ok(target),
        RemovalSafety::Blocked(blockers) => Err(RemovalFailure::Blocked(blockers)),
    }
}

/// First preflight: produces target-naming confirmation data while leaving the
/// aggregate and runtime untouched. The verified Git token is intentionally
/// discarded so it cannot be cached across human confirmation.
pub fn prepare_removal(
    state: &RepositoryState,
    checkout_key: CheckoutKey,
) -> Result<RemovalConfirmation, RemovalFailure> {
    let (repository, checkout) = removal_facts(state, checkout_key)?;
    let target = inspect_removal_facts(repository, checkout)?;
    Ok(RemovalConfirmation {
        repository: checkout.repository_key,
        checkout: checkout.key,
        path: target.path().to_path_buf(),
        branch_ref: target.branch_ref().to_owned(),
    })
}

/// Second preflight: validate that the confirmation still names the exact
/// durable child, then perform a wholly fresh inspection after runtime stop.
pub fn inspect_confirmed_removal(
    state: &RepositoryState,
    confirmation: &RemovalConfirmation,
) -> Result<VerifiedRemovalTarget, RemovalFailure> {
    let (repository, checkout) = removal_facts(state, confirmation.checkout)?;
    if checkout.repository_key != confirmation.repository
        || checkout.observed_path.to_path_buf() != confirmation.path
        || checkout.observed_branch.as_deref() != Some(confirmation.branch_ref.as_str())
    {
        return Err(RemovalFailure::ConfirmationStale);
    }
    inspect_removal_facts(repository, checkout)
}

pub fn execute_verified_removal(
    target: &VerifiedRemovalTarget,
) -> Result<VerifiedRemoval, RemoveVerifiedError> {
    git::remove_verified_worktree(target)
}

/// Apply child-only membership deletion after Git and all postconditions have
/// committed. Repository parent, siblings, counters, and branch facts remain.
pub fn commit_removed_checkout(
    state: &mut RepositoryState,
    confirmation: &RemovalConfirmation,
    removal: &VerifiedRemoval,
) -> Result<LifecycleOutcome, LifecycleError> {
    let index = state
        .checkouts
        .iter()
        .position(|checkout| checkout.key == confirmation.checkout)
        .ok_or(LifecycleError::CheckoutMissing(confirmation.checkout))?;
    if state.checkouts[index].repository_key != confirmation.repository
        || removal.removed_path() != confirmation.path
        || removal.branch_ref() != confirmation.branch_ref
    {
        return Err(LifecycleError::Topology(
            "verified removal no longer matches confirmed child".into(),
        ));
    }
    state.checkouts.remove(index);
    state.validate()?;
    Ok(LifecycleOutcome::Removed {
        repository: confirmation.repository,
        checkout: confirmation.checkout,
        branch_ref: removal.branch_ref().to_owned(),
    })
}

pub fn mark_removed_checkout_unavailable(
    state: &mut RepositoryState,
    checkout_key: CheckoutKey,
    detail: impl Into<String>,
) {
    if let Some(checkout) = state
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.key == checkout_key)
    {
        checkout.active_intent = true;
        checkout.managed_by_baude = false;
        checkout.health =
            CheckoutHealth::Unavailable(UnavailableCause::RemovalTombstone(detail.into()));
    }
}

/// Revoke destructive ownership before crossing the Git removal boundary.
/// Persisting this tombstone first ensures stale durable bytes cannot later
/// grant baude authority over an externally recreated checkout.
pub fn revoke_removal_authority(
    state: &mut RepositoryState,
    checkout_key: CheckoutKey,
) -> Result<(), LifecycleError> {
    let checkout = state
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.key == checkout_key)
        .ok_or(LifecycleError::CheckoutMissing(checkout_key))?;
    checkout.managed_by_baude = false;
    checkout.health = CheckoutHealth::Unavailable(UnavailableCause::RemovalTombstone(
        "removal authority revoked before Git mutation".into(),
    ));
    state.validate()?;
    Ok(())
}

/// Snapshot one runtime into its durable child and deactivate only that child.
/// The aggregate is replaced only after the complete candidate validates.
pub fn plan_close(
    state: &mut RepositoryState,
    request: CloseRequest,
) -> Result<ClosePlan, LifecycleError> {
    let mut next = state.clone();
    let checkout = next
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.key == request.checkout)
        .ok_or(LifecycleError::CheckoutMissing(request.checkout))?;
    checkout.session = request.runtime;
    checkout.active_intent = false;
    checkout.health = CheckoutHealth::Available;
    next.validate()?;
    *state = next;
    Ok(ClosePlan {
        checkout: request.checkout,
        effects: [
            CloseEffect::SnapshotRuntime,
            CloseEffect::SaveInactiveIntent,
            CloseEffect::StopRuntime,
        ],
        outcome: LifecycleOutcome::Closed {
            checkout: request.checkout,
        },
    })
}

pub fn mark_teardown_pending(
    state: &mut RepositoryState,
    checkout_key: CheckoutKey,
    error: &crate::session::SessionTeardownError,
) -> Result<(), LifecycleError> {
    let checkout = state
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.key == checkout_key)
        .ok_or(LifecycleError::CheckoutMissing(checkout_key))?;
    checkout.active_intent = true;
    checkout.health = CheckoutHealth::Unavailable(UnavailableCause::TeardownPending {
        agent_pid: error.agent_pid,
        shell_pid: error.shell_pid,
        agent_stopped: error.agent_stopped,
        shell_stopped: error.shell_stopped,
        detail: error.detail.clone(),
    });
    state.validate()?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReopenRuntime {
    Absent,
    Live { id: u64 },
    Exited { id: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReopenDispatch {
    Focus { id: u64 },
    Restart { id: u64 },
    Spawn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReopenEffect {
    SaveActiveIntent,
    DispatchRuntime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReopenRequest {
    pub checkout: CheckoutKey,
    /// A fresh exact-path/common-directory/full-ref/lock reconciliation made
    /// after the repository reservation was acquired.
    pub reconciliation: Result<(), ReconciliationUnavailable>,
    pub runtime: ReopenRuntime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReopenPlan {
    pub repository: RepositoryKey,
    pub checkout: CheckoutKey,
    pub effects: [ReopenEffect; 2],
    pub dispatch: ReopenDispatch,
    pub mode: SpawnMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReopenBlocked {
    checkout: CheckoutKey,
    pub cause: UnavailableCause,
}

impl ReopenBlocked {
    pub fn checkout(&self) -> CheckoutKey {
        self.checkout
    }
}

fn unavailable_cause(error: &ReconciliationUnavailable) -> UnavailableCause {
    match error {
        ReconciliationUnavailable::Missing { .. } => UnavailableCause::Missing,
        ReconciliationUnavailable::IdentityChanged { .. }
        | ReconciliationUnavailable::PathChanged { .. }
        | ReconciliationUnavailable::BranchChanged { .. }
        | ReconciliationUnavailable::Detached
        | ReconciliationUnavailable::LockedOrPrunable => UnavailableCause::IdentityChanged,
        ReconciliationUnavailable::Discovery { detail, .. } => {
            UnavailableCause::Other(detail.clone())
        }
    }
}

/// Apply the shared reopen transition only after the caller supplies fresh Git
/// reconciliation. Unavailable facts update health for presentation but never
/// flip active intent or authorize a runtime effect.
pub fn plan_reopen(
    state: &mut RepositoryState,
    request: ReopenRequest,
) -> Result<ReopenPlan, ReopenBlocked> {
    let Some(checkout_index) = state
        .checkouts
        .iter()
        .position(|checkout| checkout.key == request.checkout)
    else {
        return Err(ReopenBlocked {
            checkout: request.checkout,
            cause: UnavailableCause::Missing,
        });
    };
    let repository = state.checkouts[checkout_index].repository_key;
    let repository_index = state
        .repositories
        .iter()
        .position(|candidate| candidate.key == repository);

    if let CheckoutHealth::Unavailable(UnavailableCause::RemovalTombstone(detail)) =
        &state.checkouts[checkout_index].health
    {
        return Err(ReopenBlocked {
            checkout: request.checkout,
            cause: UnavailableCause::RemovalTombstone(detail.clone()),
        });
    }

    if let Err(error) = request.reconciliation {
        let cause = unavailable_cause(&error);
        state.checkouts[checkout_index].health = CheckoutHealth::Unavailable(cause.clone());
        if let Some(index) = repository_index {
            state.repositories[index].health = RepositoryHealth::Unavailable(cause.clone());
        }
        return Err(ReopenBlocked {
            checkout: request.checkout,
            cause,
        });
    }

    let mode = state.checkouts[checkout_index]
        .session
        .resume_id
        .clone()
        .map(SpawnMode::ResumeId)
        .unwrap_or(SpawnMode::ContinueLatest);
    let mut next = state.clone();
    next.checkouts[checkout_index].active_intent = true;
    next.checkouts[checkout_index].health = CheckoutHealth::Available;
    if let Some(index) = repository_index {
        next.repositories[index].health = RepositoryHealth::Available;
    }
    // The existing state was validated at load/admission. Reopen changes only
    // intent and health, so this cannot create a new aggregate invariant.
    debug_assert!(next.validate().is_ok());
    *state = next;

    let dispatch = match request.runtime {
        ReopenRuntime::Live { id } => ReopenDispatch::Focus { id },
        ReopenRuntime::Exited { id } => ReopenDispatch::Restart { id },
        ReopenRuntime::Absent => ReopenDispatch::Spawn,
    };
    Ok(ReopenPlan {
        repository,
        checkout: request.checkout,
        effects: [
            ReopenEffect::SaveActiveIntent,
            ReopenEffect::DispatchRuntime,
        ],
        dispatch,
        mode,
    })
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
    git::remove_added_worktree(&activation.main_worktree, &activation.path).map_err(|error| {
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
    CheckoutMissing(CheckoutKey),
    Topology(String),
    PostVerificationCompensationFailed {
        repository: std::path::PathBuf,
        checkout: CheckoutKey,
        path: std::path::PathBuf,
        branch: String,
        created_branch: bool,
        verification: String,
        compensation: String,
    },
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
            Self::CheckoutMissing(key) => {
                write!(f, "lifecycle checkout {} is missing", key.get())
            }
            Self::Topology(detail) => write!(f, "activation topology mismatch: {detail}"),
            Self::PostVerificationCompensationFailed {
                repository,
                checkout,
                path,
                branch,
                created_branch,
                verification,
                compensation,
            } => write!(
                f,
                "post-verification activation compensation failed for checkout {} branch {branch} at {} in {} (created branch: {created_branch}): {verification}; compensation failed: {compensation}",
                checkout.get(),
                path.display(),
                repository.display()
            ),
        }
    }
}

impl std::error::Error for LifecycleError {}

impl LifecycleError {
    pub fn recovery_child_recorded(&self) -> bool {
        matches!(
            self,
            Self::Git(BranchActivationError::PostAddCompensationFailed { .. })
                | Self::PostVerificationCompensationFailed { .. }
        )
    }
}

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

/// Record ownership of the exact checkout identity before Git may add it.
/// Owners must durably save this child before calling [`execute_activation`].
pub fn record_pending_activation(
    state: &mut RepositoryState,
    snapshot: &RepositorySnapshot,
    prepared: &PreparedActivation,
) -> Result<(), LifecycleError> {
    if state
        .checkouts
        .iter()
        .any(|checkout| checkout.key == prepared.checkout)
    {
        return Err(LifecycleError::Topology(format!(
            "pending activation checkout {} already exists",
            prepared.checkout.get()
        )));
    }
    let repository_name = snapshot
        .main_worktree
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| snapshot.main_worktree.display().to_string());
    state.checkouts.push(SavedCheckout {
        key: prepared.checkout,
        repository_key: prepared.request.repository,
        role: CheckoutRole::ManagedBranch,
        managed_by_baude: true,
        observed_path: PersistedPath::from_path(&prepared.request.managed_path),
        observed_branch: Some(format!("refs/heads/{}", prepared.request.branch)),
        first_seen_order: prepared.first_seen_order,
        active_intent: false,
        session: RetainedSessionState {
            name: format!("{repository_name}:{}", prepared.request.branch),
            cwd: PersistedPath::from_path(&prepared.request.managed_path),
            repo_root: PersistedPath::from_path(&snapshot.main_worktree),
            branch: Some(prepared.request.branch.clone()),
            is_worktree: true,
            shell_open: false,
            archived: false,
            archived_by_user: false,
            resume_id: None,
        },
        health: CheckoutHealth::Unavailable(UnavailableCause::PendingActivation {
            branch: prepared.request.branch.clone(),
        }),
    });
    state.validate()?;
    Ok(())
}

pub fn clear_pending_activation(state: &mut RepositoryState, checkout: CheckoutKey) {
    state.checkouts.retain(|saved| saved.key != checkout);
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
    execute_activation_with_post_git_hook(state, repository_child, prepared, |_| {})
}

fn execute_activation_with_post_git_hook(
    state: &mut RepositoryState,
    repository_child: &std::path::Path,
    prepared: PreparedActivation,
    after_git: impl FnOnce(&std::path::Path),
) -> Result<RecordedActivation, LifecycleError> {
    let state_before = state.clone();
    let activation_repository = git::discover_repository(repository_child)?;
    let outcome = match git::activate_branch(
        repository_child,
        &prepared.request.branch,
        &prepared.request.managed_path,
    ) {
        Ok(outcome) => outcome,
        Err(error @ BranchActivationError::PostAddCompensationFailed { .. }) => {
            let full_ref = format!("refs/heads/{}", prepared.request.branch);
            let repository_name = activation_repository
                .main_worktree
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| activation_repository.main_worktree.display().to_string());
            let recovery = SavedCheckout {
                key: prepared.checkout,
                repository_key: prepared.request.repository,
                role: CheckoutRole::ManagedBranch,
                managed_by_baude: true,
                observed_path: PersistedPath::from_path(&prepared.request.managed_path),
                observed_branch: Some(full_ref),
                first_seen_order: prepared.first_seen_order,
                active_intent: false,
                session: RetainedSessionState {
                    name: format!("{repository_name}:{}", prepared.request.branch),
                    cwd: PersistedPath::from_path(&prepared.request.managed_path),
                    repo_root: PersistedPath::from_path(&activation_repository.main_worktree),
                    branch: Some(prepared.request.branch.clone()),
                    is_worktree: true,
                    shell_open: false,
                    archived: false,
                    archived_by_user: false,
                    resume_id: None,
                },
                health: match &error {
                    BranchActivationError::PostAddCompensationFailed {
                        branch,
                        created_branch,
                        verification,
                        compensation,
                        ..
                    } => CheckoutHealth::Unavailable(UnavailableCause::ActivationRecovery {
                        branch: branch.clone(),
                        created_branch: *created_branch,
                        verification: verification.to_string(),
                        compensation: compensation.clone(),
                    }),
                    _ => unreachable!(),
                },
            };
            if let Some(existing) = state
                .checkouts
                .iter_mut()
                .find(|checkout| checkout.key == prepared.checkout)
            {
                *existing = recovery;
            } else {
                state.checkouts.push(recovery);
            }
            state.validate()?;
            return Err(LifecycleError::Git(error));
        }
        Err(error) => return Err(error.into()),
    };
    let (disposition, created_by_baude, record) = activation_parts(outcome);
    let added_path = record.path.clone();
    let activation_branch = prepared.request.branch.clone();
    after_git(&added_path);
    let result = (|| {
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
                    resume_id: None,
                },
                health: CheckoutHealth::Available,
            });
            (prepared.checkout, created_by_baude)
        };
        if checkout != prepared.checkout {
            clear_pending_activation(state, prepared.checkout);
        }
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
    })();
    if let Err(error) = result {
        *state = state_before;
        if created_by_baude {
            if let Err(compensation) =
                git::remove_added_worktree(&activation_repository.main_worktree, &added_path)
            {
                let recovery = SavedCheckout {
                    key: prepared.checkout,
                    repository_key: prepared.request.repository,
                    role: CheckoutRole::ManagedBranch,
                    managed_by_baude: true,
                    observed_path: PersistedPath::from_path(&added_path),
                    observed_branch: Some(format!("refs/heads/{activation_branch}")),
                    first_seen_order: prepared.first_seen_order,
                    active_intent: false,
                    session: RetainedSessionState {
                        name: activation_branch.clone(),
                        cwd: PersistedPath::from_path(&added_path),
                        repo_root: PersistedPath::from_path(&activation_repository.main_worktree),
                        branch: Some(activation_branch.clone()),
                        is_worktree: true,
                        shell_open: false,
                        archived: false,
                        archived_by_user: false,
                        resume_id: None,
                    },
                    health: CheckoutHealth::Unavailable(UnavailableCause::ActivationRecovery {
                        branch: activation_branch.clone(),
                        created_branch: matches!(disposition, ActivationDisposition::Created),
                        verification: error.to_string(),
                        compensation: compensation.to_string(),
                    }),
                };
                if let Some(existing) = state
                    .checkouts
                    .iter_mut()
                    .find(|checkout| checkout.key == prepared.checkout)
                {
                    *existing = recovery;
                } else {
                    state.checkouts.push(recovery);
                }
                state.validate()?;
                return Err(LifecycleError::PostVerificationCompensationFailed {
                    repository: activation_repository.main_worktree,
                    checkout: prepared.checkout,
                    path: added_path,
                    branch: activation_branch,
                    created_branch: matches!(disposition, ActivationDisposition::Created),
                    verification: error.to_string(),
                    compensation: compensation.to_string(),
                });
            }
        }
        return Err(error);
    }
    result
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
    Reopened {
        checkout: CheckoutKey,
        runtime: u64,
    },
    Closed {
        checkout: CheckoutKey,
    },
    Removed {
        repository: RepositoryKey,
        checkout: CheckoutKey,
        branch_ref: String,
    },
    TopologyCommittedStateDegraded {
        checkout: CheckoutKey,
        detail: String,
    },
    Busy {
        repository: RepositoryKey,
    },
    ReopenPending {
        checkout: CheckoutKey,
    },
}

/// Cloneable reservation registry. Guards release their repository on drop.
#[derive(Clone, Debug, Default)]
pub struct RepositoryReservations {
    held: Arc<Mutex<HashMap<RepositoryKey, ReservationKind>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReservationKind {
    Mutation,
    Reopen(CheckoutKey),
}

impl RepositoryReservations {
    pub fn reserve(
        &self,
        repository: RepositoryKey,
    ) -> Result<RepositoryReservation, LifecycleOutcome> {
        let mut held = self.held.lock().unwrap_or_else(|error| error.into_inner());
        if held.contains_key(&repository) {
            return Err(LifecycleOutcome::Busy { repository });
        }
        held.insert(repository, ReservationKind::Mutation);
        drop(held);
        Ok(RepositoryReservation {
            held: Arc::clone(&self.held),
            repository,
        })
    }

    pub fn reserve_reopen(
        &self,
        repository: RepositoryKey,
        checkout: CheckoutKey,
    ) -> Result<RepositoryReservation, LifecycleOutcome> {
        let mut held = self.held.lock().unwrap_or_else(|error| error.into_inner());
        match held.get(&repository) {
            Some(ReservationKind::Reopen(held_checkout)) if *held_checkout == checkout => {
                return Err(LifecycleOutcome::ReopenPending { checkout });
            }
            Some(_) => return Err(LifecycleOutcome::Busy { repository }),
            None => {}
        }
        held.insert(repository, ReservationKind::Reopen(checkout));
        drop(held);
        Ok(RepositoryReservation {
            held: Arc::clone(&self.held),
            repository,
        })
    }
}

#[derive(Debug)]
pub struct RepositoryReservation {
    held: Arc<Mutex<HashMap<RepositoryKey, ReservationKind>>>,
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
        plan_close, plan_reopen, revoke_removal_authority, ActivationRequest, CloseEffect,
        CloseRequest, LifecycleOutcome, ReopenDispatch, ReopenRequest, ReopenRuntime,
        RepositoryReservations,
    };
    use crate::backend::SpawnMode;
    use crate::git::ReconciliationUnavailable;
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

    #[test]
    fn reopen_blocks_every_unavailable_topology_before_active_intent() {
        let unavailable = [
            ReconciliationUnavailable::Missing {
                path: PathBuf::from("/moved"),
            },
            ReconciliationUnavailable::PathChanged {
                expected: PathBuf::from("/repo-feature"),
                observed: PathBuf::from("/elsewhere"),
            },
            ReconciliationUnavailable::BranchChanged {
                expected: Some("refs/heads/feature/close".into()),
                observed: Some("refs/heads/other".into()),
            },
            ReconciliationUnavailable::Detached,
            ReconciliationUnavailable::LockedOrPrunable,
            ReconciliationUnavailable::IdentityChanged {
                expected_common_dir: PathBuf::from("/repo/.git"),
                observed_common_dir: PathBuf::from("/replacement/.git"),
            },
        ];

        for topology in unavailable {
            let mut state = close_state();
            state.checkouts[0].active_intent = false;
            let checkout = state.checkouts[0].key;
            let error = plan_reopen(
                &mut state,
                ReopenRequest {
                    checkout,
                    reconciliation: Err(topology),
                    runtime: ReopenRuntime::Absent,
                },
            )
            .unwrap_err();

            assert_eq!(error.checkout(), checkout);
            assert!(!state.checkouts[0].active_intent);
            assert!(matches!(
                state.checkouts[0].health,
                CheckoutHealth::Unavailable(_)
            ));
        }
    }

    #[test]
    fn removal_tombstone_cannot_reopen_or_regain_management() {
        let mut state = close_state();
        let checkout = state.checkouts[0].key;
        revoke_removal_authority(&mut state, checkout).unwrap();
        let durable: RepositoryState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        state = durable;

        let blocked = plan_reopen(
            &mut state,
            ReopenRequest {
                checkout,
                reconciliation: Ok(()),
                runtime: ReopenRuntime::Absent,
            },
        )
        .unwrap_err();

        assert!(matches!(
            blocked.cause,
            crate::repository::UnavailableCause::RemovalTombstone(_)
        ));
        assert!(!state.checkouts[0].managed_by_baude);
        assert!(matches!(
            state.checkouts[0].health,
            CheckoutHealth::Unavailable(crate::repository::UnavailableCause::RemovalTombstone(_))
        ));
    }

    #[test]
    fn reopen_saves_active_intent_before_deterministic_runtime_dispatch() {
        let vectors = [
            (
                ReopenRuntime::Live { id: 7 },
                ReopenDispatch::Focus { id: 7 },
            ),
            (
                ReopenRuntime::Exited { id: 8 },
                ReopenDispatch::Restart { id: 8 },
            ),
            (ReopenRuntime::Absent, ReopenDispatch::Spawn),
        ];

        for (runtime, expected) in vectors {
            let mut state = close_state();
            state.checkouts[0].active_intent = false;
            state.checkouts[0].session.resume_id = Some("conversation-42".into());
            let checkout = state.checkouts[0].key;
            let plan = plan_reopen(
                &mut state,
                ReopenRequest {
                    checkout,
                    reconciliation: Ok(()),
                    runtime,
                },
            )
            .unwrap();

            assert!(state.checkouts[0].active_intent);
            assert_eq!(plan.dispatch, expected);
            assert_eq!(plan.mode, SpawnMode::ResumeId("conversation-42".into()));
            assert_eq!(plan.effects[0], super::ReopenEffect::SaveActiveIntent);
        }

        let mut state = close_state();
        state.checkouts[0].active_intent = false;
        let checkout = state.checkouts[0].key;
        let plan = plan_reopen(
            &mut state,
            ReopenRequest {
                checkout,
                reconciliation: Ok(()),
                runtime: ReopenRuntime::Absent,
            },
        )
        .unwrap();
        assert_eq!(plan.mode, SpawnMode::ContinueLatest);
    }

    #[test]
    fn reopen_reservation_allows_only_one_same_checkout_spawn_path() {
        let state = close_state();
        let checkout = state.checkouts[0].key;
        let repository = state.checkouts[0].repository_key;
        let reservations = RepositoryReservations::default();
        let guard = reservations.reserve_reopen(repository, checkout).unwrap();

        assert_eq!(
            reservations
                .reserve_reopen(repository, checkout)
                .unwrap_err(),
            LifecycleOutcome::ReopenPending { checkout }
        );
        assert_eq!(
            reservations.reserve(repository).unwrap_err(),
            LifecycleOutcome::Busy { repository }
        );
        drop(guard);
        assert!(reservations.reserve_reopen(repository, checkout).is_ok());
    }
}
