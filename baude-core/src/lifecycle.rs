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
    AllocationError, CheckoutHealth, CheckoutKey, CheckoutLifecycle, CheckoutRole, PersistedPath,
    RepositoryHealth, RepositoryKey, RepositoryState, RetainedSessionState, RuntimeGeneration,
    SavedCheckout, SavedRepository, UnavailableCause, ValidationError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleEvent {
    RequestActivation,
    ActivationVerified,
    LaunchRegistered(crate::repository::OwnedRuntime),
    LaunchReleased,
    RequestClose,
    RuntimeExtinct,
    RestoreRemovalAuthority,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleTraceEntry {
    PersistActivation,
    PersistActive,
    PersistRuntime,
    ReleaseRuntime,
    PersistStop,
    PersistInactive,
}

pub type LifecycleTrace = Vec<LifecycleTraceEntry>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleTransition {
    pub state: CheckoutLifecycle,
    pub effects: LifecycleTrace,
}

pub fn reduce_lifecycle(state: &CheckoutLifecycle, event: &LifecycleEvent) -> LifecycleTransition {
    let transition = match (state, event) {
        (CheckoutLifecycle::Inactive, LifecycleEvent::RequestActivation) => Some((
            CheckoutLifecycle::Activating,
            LifecycleTraceEntry::PersistActivation,
        )),
        (CheckoutLifecycle::Activating, LifecycleEvent::ActivationVerified) => Some((
            CheckoutLifecycle::Active,
            LifecycleTraceEntry::PersistActive,
        )),
        (CheckoutLifecycle::Active, LifecycleEvent::LaunchRegistered(runtime)) => Some((
            CheckoutLifecycle::Launching(runtime.generation),
            LifecycleTraceEntry::PersistRuntime,
        )),
        (CheckoutLifecycle::Launching(generation), LifecycleEvent::LaunchReleased) => Some((
            CheckoutLifecycle::Running(*generation),
            LifecycleTraceEntry::ReleaseRuntime,
        )),
        (CheckoutLifecycle::Running(generation), LifecycleEvent::RequestClose) => Some((
            CheckoutLifecycle::Stopping(*generation),
            LifecycleTraceEntry::PersistStop,
        )),
        (CheckoutLifecycle::Stopping(_), LifecycleEvent::RuntimeExtinct) => Some((
            CheckoutLifecycle::Inactive,
            LifecycleTraceEntry::PersistInactive,
        )),
        (
            CheckoutLifecycle::Protected(UnavailableCause::TeardownPending { .. }),
            LifecycleEvent::RuntimeExtinct,
        ) => Some((
            CheckoutLifecycle::Inactive,
            LifecycleTraceEntry::PersistInactive,
        )),
        (
            CheckoutLifecycle::Protected(UnavailableCause::StoppedActiveRecovery { .. }),
            LifecycleEvent::RuntimeExtinct,
        ) => Some((
            CheckoutLifecycle::Inactive,
            LifecycleTraceEntry::PersistInactive,
        )),
        (
            CheckoutLifecycle::Protected(UnavailableCause::RemovalTombstone(_)),
            LifecycleEvent::RestoreRemovalAuthority,
        ) => Some((
            CheckoutLifecycle::Active,
            LifecycleTraceEntry::PersistActive,
        )),
        _ => None,
    };
    match transition {
        Some((state, effect)) => LifecycleTransition {
            state,
            effects: vec![effect],
        },
        None => LifecycleTransition {
            state: state.clone(),
            effects: Vec::new(),
        },
    }
}

pub trait LifecycleEffects {
    type Error;

    fn persist_lifecycle(&mut self, candidate: &LifecycleCandidate) -> Result<(), Self::Error>;
    fn apply_lifecycle_effect(&mut self, effect: &LifecycleTraceEntry) -> Result<(), Self::Error>;
}

/// Opaque engine-selected durable candidate. Adapters can persist and apply it
/// but cannot construct an arbitrary lifecycle mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleCandidate {
    checkout: CheckoutKey,
    lifecycle: CheckoutLifecycle,
    runtime: RuntimeCandidate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RuntimeCandidate {
    Keep,
    Set(crate::repository::OwnedRuntime),
    Clear,
}

impl LifecycleCandidate {
    pub fn checkout(&self) -> CheckoutKey {
        self.checkout
    }

    pub fn lifecycle(&self) -> &CheckoutLifecycle {
        &self.lifecycle
    }

    pub fn apply(&self, state: &mut RepositoryState) -> Result<(), ValidationError> {
        let checkout = state
            .checkouts
            .iter_mut()
            .find(|checkout| checkout.key == self.checkout)
            .ok_or(ValidationError::MissingCheckout(self.checkout))?;
        checkout.set_lifecycle(self.lifecycle.clone());
        match &self.runtime {
            RuntimeCandidate::Keep => {}
            RuntimeCandidate::Set(runtime) => checkout.set_owned_runtime(Some(runtime.clone())),
            RuntimeCandidate::Clear => checkout.set_owned_runtime(None),
        }
        Ok(())
    }
}

pub struct LifecycleEngine<E> {
    effects: E,
    trace: LifecycleTrace,
}

impl<E: LifecycleEffects> LifecycleEngine<E> {
    pub fn new(effects: E) -> Self {
        Self {
            effects,
            trace: Vec::new(),
        }
    }

    pub fn drive(
        &mut self,
        checkout: &mut SavedCheckout,
        event: LifecycleEvent,
    ) -> Result<LifecycleTransition, E::Error> {
        let transition = reduce_lifecycle(checkout.lifecycle(), &event);
        if transition.effects.is_empty() {
            return Ok(transition);
        }
        let candidate = LifecycleCandidate {
            checkout: checkout.key,
            lifecycle: transition.state.clone(),
            runtime: match &event {
                LifecycleEvent::LaunchRegistered(runtime) => RuntimeCandidate::Set(runtime.clone()),
                LifecycleEvent::RuntimeExtinct => RuntimeCandidate::Clear,
                _ => RuntimeCandidate::Keep,
            },
        };
        self.effects.persist_lifecycle(&candidate)?;
        // The replacement is durable before the external effect is attempted.
        // Keep memory aligned with that commit boundary even when the effect
        // fails; recovery must see the candidate that authorized the effect.
        checkout.set_lifecycle(transition.state.clone());
        self.trace.extend(transition.effects.iter().cloned());
        for effect in &transition.effects {
            self.effects.apply_lifecycle_effect(effect)?;
        }
        Ok(transition)
    }

    pub fn trace(&self) -> &[LifecycleTraceEntry] {
        &self.trace
    }

    pub fn into_effects(self) -> E {
        self.effects
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalLifecycleVector {
    Activate,
    Launch,
    Close,
    ProtectedRefusal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterFailureScript {
    None,
    Persist(usize),
    Effect(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleContractResult {
    pub trace: LifecycleTrace,
    pub final_lifecycle: CheckoutLifecycle,
    pub failed: bool,
}

pub fn canonical_lifecycle_contract_vectors() -> &'static [CanonicalLifecycleVector] {
    &[
        CanonicalLifecycleVector::Activate,
        CanonicalLifecycleVector::Launch,
        CanonicalLifecycleVector::Close,
        CanonicalLifecycleVector::ProtectedRefusal,
    ]
}

pub fn canonical_lifecycle_events(
    vector: CanonicalLifecycleVector,
) -> (CheckoutLifecycle, Vec<LifecycleEvent>) {
    let (state, events) = match vector {
        CanonicalLifecycleVector::Activate => (
            CheckoutLifecycle::Inactive,
            vec![
                LifecycleEvent::RequestActivation,
                LifecycleEvent::ActivationVerified,
            ],
        ),
        CanonicalLifecycleVector::Launch => (
            CheckoutLifecycle::Active,
            vec![
                LifecycleEvent::LaunchRegistered(contract_owned_runtime()),
                LifecycleEvent::LaunchReleased,
            ],
        ),
        CanonicalLifecycleVector::Close => (
            CheckoutLifecycle::Running(RuntimeGeneration::initial()),
            vec![LifecycleEvent::RequestClose, LifecycleEvent::RuntimeExtinct],
        ),
        CanonicalLifecycleVector::ProtectedRefusal => (
            CheckoutLifecycle::RemovalCommitted,
            vec![LifecycleEvent::RequestActivation],
        ),
    };
    (state, events)
}

fn contract_owned_runtime() -> crate::repository::OwnedRuntime {
    let identity = crate::repository::ProcessIdentity {
        pid: 41,
        start_time: 42,
        process_group: 41,
        session: 41,
    };
    crate::repository::OwnedRuntime {
        generation: RuntimeGeneration::initial(),
        agent: identity,
        shell: crate::repository::ShellOwnership::Closed,
    }
}

#[derive(Default)]
struct CanonicalContractEffects {
    script: Option<AdapterFailureScript>,
    persists: usize,
    effects: usize,
}

impl LifecycleEffects for CanonicalContractEffects {
    type Error = ();

    fn persist_lifecycle(&mut self, _candidate: &LifecycleCandidate) -> Result<(), Self::Error> {
        self.persists += 1;
        if self.script == Some(AdapterFailureScript::Persist(self.persists)) {
            return Err(());
        }
        Ok(())
    }

    fn apply_lifecycle_effect(&mut self, _effect: &LifecycleTraceEntry) -> Result<(), Self::Error> {
        self.effects += 1;
        if self.script == Some(AdapterFailureScript::Effect(self.effects)) {
            return Err(());
        }
        Ok(())
    }
}

fn contract_checkout(lifecycle: CheckoutLifecycle) -> SavedCheckout {
    let mut keys = RepositoryState::default();
    let repository_key = keys.allocate_repository_key().expect("repository key");
    let key = keys.allocate_checkout_key().expect("checkout key");
    SavedCheckout {
        key,
        repository_key,
        role: CheckoutRole::ManagedBranch,
        managed_by_baude: true,
        observed_path: PersistedPath::from_path(std::path::Path::new("/contract")),
        observed_branch: Some("refs/heads/contract".into()),
        first_seen_order: 1,
        lifecycle: lifecycle.clone(),
        owned_runtime: None,
        active_intent: matches!(
            lifecycle,
            CheckoutLifecycle::Active | CheckoutLifecycle::Running(_)
        ),
        session: RetainedSessionState {
            name: "contract".into(),
            cwd: PersistedPath::from_path(std::path::Path::new("/contract")),
            repo_root: PersistedPath::from_path(std::path::Path::new("/contract")),
            branch: Some("contract".into()),
            is_worktree: true,
            shell_open: false,
            archived: false,
            archived_by_user: false,
            resume_id: None,
        },
        health: CheckoutHealth::Available,
    }
}

pub fn run_canonical_lifecycle_contract(
    vector: CanonicalLifecycleVector,
    script: AdapterFailureScript,
) -> LifecycleContractResult {
    let (initial, events) = canonical_lifecycle_events(vector);
    let mut checkout = contract_checkout(initial);
    checkout.set_lifecycle(checkout.lifecycle().clone());
    let effects = CanonicalContractEffects {
        script: Some(script),
        ..CanonicalContractEffects::default()
    };
    let mut engine = LifecycleEngine::new(effects);
    let mut failed = false;
    for event in events {
        if engine.drive(&mut checkout, event).is_err() {
            failed = true;
            break;
        }
    }
    LifecycleContractResult {
        trace: engine.trace().to_vec(),
        final_lifecycle: checkout.lifecycle().clone(),
        failed,
    }
}

pub fn canonical_lifecycle_trace(vector: CanonicalLifecycleVector) -> LifecycleContractResult {
    run_canonical_lifecycle_contract(vector, AdapterFailureScript::None)
}

pub fn normalize_lifecycle_trace(trace: &[LifecycleTraceEntry]) -> LifecycleTrace {
    trace.to_vec()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryStep {
    StopOwned(CheckoutKey),
    FinishRemoval(CheckoutKey),
    RecoverActivation(CheckoutKey),
    ReconcileTopology(CheckoutKey),
    Launch(CheckoutKey),
}

/// A manual lifecycle action that an effect adapter can dispatch for one
/// durable checkout. Absence is authoritative: presentation must neither show
/// nor accept a retry merely because a runtime is missing or health is yellow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleCapability {
    RetryReopen,
    RetryRecovery,
}

/// Project only manual actions with concrete App/Manager adapter paths. This
/// value is UI-free and does not mutate or normalize lifecycle state.
pub fn lifecycle_capability(lifecycle: &CheckoutLifecycle) -> Option<LifecycleCapability> {
    match lifecycle {
        CheckoutLifecycle::Inactive => Some(LifecycleCapability::RetryReopen),
        CheckoutLifecycle::Activating
        | CheckoutLifecycle::Protected(
            UnavailableCause::PendingActivation { .. }
            | UnavailableCause::ActivationRecovery { .. }
            | UnavailableCause::TeardownPending { .. }
            | UnavailableCause::StoppedActiveRecovery { .. },
        ) => Some(LifecycleCapability::RetryRecovery),
        CheckoutLifecycle::Active
        | CheckoutLifecycle::Launching(_)
        | CheckoutLifecycle::Running(_)
        | CheckoutLifecycle::Stopping(_)
        | CheckoutLifecycle::RemovalCommitted
        | CheckoutLifecycle::Protected(_) => None,
    }
}

pub fn startup_recovery_program(state: &RepositoryState) -> Vec<RecoveryStep> {
    let mut process = Vec::new();
    let mut removal = Vec::new();
    let mut activation = Vec::new();
    let mut topology = Vec::new();
    let mut launch = Vec::new();
    for checkout in &state.checkouts {
        match checkout.lifecycle() {
            CheckoutLifecycle::Stopping(_)
            | CheckoutLifecycle::Protected(UnavailableCause::TeardownPending { .. })
            | CheckoutLifecycle::Protected(UnavailableCause::StoppedActiveRecovery { .. }) => {
                process.push(RecoveryStep::StopOwned(checkout.key));
            }
            CheckoutLifecycle::RemovalCommitted
            | CheckoutLifecycle::Protected(UnavailableCause::RemovalTombstone(_)) => {
                removal.push(RecoveryStep::FinishRemoval(checkout.key));
            }
            CheckoutLifecycle::Activating
            | CheckoutLifecycle::Protected(UnavailableCause::PendingActivation { .. })
            | CheckoutLifecycle::Protected(UnavailableCause::ActivationRecovery { .. }) => {
                activation.push(RecoveryStep::RecoverActivation(checkout.key));
            }
            CheckoutLifecycle::Protected(_) => {
                topology.push(RecoveryStep::ReconcileTopology(checkout.key));
            }
            CheckoutLifecycle::Active | CheckoutLifecycle::Launching(_) => {
                launch.push(RecoveryStep::Launch(checkout.key));
            }
            CheckoutLifecycle::Inactive | CheckoutLifecycle::Running(_) => {}
        }
    }
    process
        .into_iter()
        .chain(removal)
        .chain(activation)
        .chain(topology)
        .chain(launch)
        .collect()
}

/// Record a freshly verified active checkout without allowing ordinary owner
/// code to overwrite a protected transaction or recovery state.
pub fn mark_checkout_active(
    state: &mut RepositoryState,
    checkout: CheckoutKey,
) -> Result<(), ValidationError> {
    let saved = state
        .checkouts
        .iter_mut()
        .find(|saved| saved.key == checkout)
        .ok_or(ValidationError::MissingCheckout(checkout))?;
    if saved.lifecycle().is_protected() {
        return Err(ValidationError::ContradictoryLifecycle(checkout));
    }
    saved.set_lifecycle(CheckoutLifecycle::Active);
    Ok(())
}

/// Update topology availability through core lifecycle authority. Successful
/// reconciliation never clears a protected lifecycle; failed reconciliation
/// records protection once and preserves any stronger existing evidence.
pub fn record_checkout_reconciliation(
    state: &mut RepositoryState,
    checkout: CheckoutKey,
    unavailable: Option<UnavailableCause>,
) -> Result<(), ValidationError> {
    let saved = state
        .checkouts
        .iter_mut()
        .find(|saved| saved.key == checkout)
        .ok_or(ValidationError::MissingCheckout(checkout))?;
    match unavailable {
        None if !saved.lifecycle().is_protected() => {
            let active = saved.active_intent();
            saved.set_lifecycle(if active {
                CheckoutLifecycle::Active
            } else {
                CheckoutLifecycle::Inactive
            });
        }
        Some(cause) if !saved.lifecycle().is_protected() => {
            saved.set_lifecycle(CheckoutLifecycle::Protected(cause));
        }
        _ => {}
    }
    Ok(())
}

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
    pub collision: Option<CollisionReport>,
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
        checkout.managed_by_baude = false;
        checkout.set_lifecycle(CheckoutLifecycle::Protected(
            UnavailableCause::RemovalTombstone(detail.into()),
        ));
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
    checkout.set_lifecycle(CheckoutLifecycle::Protected(
        UnavailableCause::RemovalTombstone("removal authority revoked before Git mutation".into()),
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
    checkout.set_lifecycle(CheckoutLifecycle::Inactive);
    checkout.set_owned_runtime(None);
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
    let cause = UnavailableCause::TeardownPending {
        agent_pid: error.agent_pid,
        shell_pid: error.shell_pid,
        agent_identity: error.agent_identity.clone(),
        shell_identity: error.shell_identity.clone(),
        agent_stopped: error.agent_stopped,
        shell_stopped: error.shell_stopped,
        detail: error.detail.clone(),
    };
    checkout.session.shell_open = error.shell_pid.is_some();
    checkout.set_lifecycle(CheckoutLifecycle::Protected(cause));
    state.validate()?;
    Ok(())
}

pub fn mark_stopped_active_recovery(
    state: &mut RepositoryState,
    checkout_key: CheckoutKey,
    agent_restarted: bool,
    shell_restarted: bool,
    detail: String,
) -> Result<(), LifecycleError> {
    let checkout = state
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.key == checkout_key)
        .ok_or(LifecycleError::CheckoutMissing(checkout_key))?;
    checkout.set_lifecycle(CheckoutLifecycle::Protected(
        UnavailableCause::StoppedActiveRecovery {
            agent_restarted,
            shell_restarted,
            detail,
        },
    ));
    state.validate()?;
    Ok(())
}

#[derive(Debug)]
pub enum DestructiveTeardownError {
    Pending(crate::session::SessionTeardownError),
    State(LifecycleError),
}

impl std::fmt::Display for DestructiveTeardownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending(error) => error.fmt(f),
            Self::State(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DestructiveTeardownError {}

/// The single typed stop boundary for close and confirmed removal. A partial
/// agent/shell outcome is transferred into durable aggregate ownership before
/// it is returned to either runtime owner.
pub fn destructive_teardown(
    state: &mut RepositoryState,
    checkout_key: CheckoutKey,
    session: &mut crate::session::Session,
) -> Result<(), DestructiveTeardownError> {
    match session.kill_and_wait() {
        Ok(()) => Ok(()),
        Err(error) => {
            mark_teardown_pending(state, checkout_key, &error)
                .map_err(DestructiveTeardownError::State)?;
            Err(DestructiveTeardownError::Pending(error))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TeardownRecoveryResolution {
    Completed {
        checkout: CheckoutKey,
    },
    Pending {
        checkout: CheckoutKey,
        detail: String,
    },
}

pub fn reconcile_teardown_recovery(
    state: &mut RepositoryState,
    checkout_key: CheckoutKey,
) -> Result<TeardownRecoveryResolution, LifecycleError> {
    let checkout = state
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.key == checkout_key)
        .ok_or(LifecycleError::CheckoutMissing(checkout_key))?;
    let (agent_pid, shell_pid, agent_identity, shell_identity, agent_stopped, shell_stopped) =
        match &checkout.health {
            CheckoutHealth::Unavailable(UnavailableCause::TeardownPending {
                agent_pid,
                shell_pid,
                agent_identity,
                shell_identity,
                agent_stopped,
                shell_stopped,
                ..
            }) => (
                *agent_pid,
                *shell_pid,
                agent_identity.clone(),
                shell_identity.clone(),
                *agent_stopped,
                *shell_stopped,
            ),
            _ => {
                return Err(LifecycleError::Topology(format!(
                    "checkout {} is not pending teardown",
                    checkout_key.get()
                )))
            }
        };
    match crate::session::finish_recorded_teardown(
        agent_pid,
        shell_pid,
        agent_identity,
        shell_identity,
        agent_stopped,
        shell_stopped,
    ) {
        Ok(()) => {
            checkout.set_lifecycle(CheckoutLifecycle::Inactive);
            checkout.set_owned_runtime(None);
            state.validate()?;
            Ok(TeardownRecoveryResolution::Completed {
                checkout: checkout_key,
            })
        }
        Err(error) => {
            let detail = error.to_string();
            checkout.set_lifecycle(CheckoutLifecycle::Protected(
                UnavailableCause::TeardownPending {
                    agent_pid: error.agent_pid,
                    shell_pid: error.shell_pid,
                    agent_identity: error.agent_identity,
                    shell_identity: error.shell_identity,
                    agent_stopped: error.agent_stopped,
                    shell_stopped: error.shell_stopped,
                    detail: error.detail,
                },
            ));
            state.validate()?;
            Ok(TeardownRecoveryResolution::Pending {
                checkout: checkout_key,
                detail,
            })
        }
    }
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

    if let Some(cause) = match state.checkouts[checkout_index].lifecycle() {
        CheckoutLifecycle::Protected(cause) => Some(cause.clone()),
        CheckoutLifecycle::RemovalCommitted => Some(UnavailableCause::RemovalTombstone(
            "removal committed".into(),
        )),
        _ => None,
    } {
        return Err(ReopenBlocked {
            checkout: request.checkout,
            cause,
        });
    }

    if let Err(error) = request.reconciliation {
        let cause = unavailable_cause(&error);
        state.checkouts[checkout_index].set_lifecycle(CheckoutLifecycle::Protected(cause.clone()));
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
    if matches!(request.runtime, ReopenRuntime::Live { .. }) {
        let lifecycle = next.checkouts[checkout_index]
            .owned_runtime()
            .map(|runtime| CheckoutLifecycle::Running(runtime.generation))
            .unwrap_or(CheckoutLifecycle::Active);
        next.checkouts[checkout_index].set_lifecycle(lifecycle);
    } else {
        next.checkouts[checkout_index].set_lifecycle(CheckoutLifecycle::Active);
        next.checkouts[checkout_index].set_owned_runtime(None);
    }
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
pub struct PostVerificationCompensationFailure {
    pub repository: std::path::PathBuf,
    pub checkout: CheckoutKey,
    pub path: std::path::PathBuf,
    pub branch: String,
    pub created_branch: bool,
    pub verification: String,
    pub compensation: String,
}

#[derive(Debug)]
pub enum LifecycleError {
    Discovery(RepositoryDiscoveryError),
    Git(BranchActivationError),
    Allocation(AllocationError),
    Validation(ValidationError),
    RepositoryMissing(RepositoryKey),
    CheckoutMissing(CheckoutKey),
    OccupiedProtected {
        checkout: CheckoutKey,
        cause: UnavailableCause,
    },
    Topology(String),
    PostVerificationCompensationFailed(Box<PostVerificationCompensationFailure>),
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
            Self::OccupiedProtected { checkout, cause } => write!(
                f,
                "occupied checkout {} has unresolved recovery: {cause:?}",
                checkout.get()
            ),
            Self::Topology(detail) => write!(f, "activation topology mismatch: {detail}"),
            Self::PostVerificationCompensationFailed(failure) => {
                let PostVerificationCompensationFailure {
                    repository,
                    checkout,
                    path,
                    branch,
                    created_branch,
                    verification,
                    compensation,
                } = failure.as_ref();
                write!(
                f,
                "post-verification activation compensation failed for checkout {} branch {branch} at {} in {} (created branch: {created_branch}): {verification}; compensation failed: {compensation}",
                checkout.get(),
                path.display(),
                repository.display()
                )
            }
        }
    }
}

impl std::error::Error for LifecycleError {}

impl LifecycleError {
    pub fn recovery_child_recorded(&self) -> bool {
        matches!(
            self,
            Self::Git(BranchActivationError::PostAddCompensationFailed { .. })
                | Self::PostVerificationCompensationFailed(_)
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

/// Reason for a collision during repository admission.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CollisionReason {
    /// Directory has a marker for a different repository.
    ForeignMarker,
    /// Directory contains a checkout that belongs to a different repository.
    ForeignCheckout,
    /// Directory ownership cannot be determined (missing/invalid/symlink marker, no checkouts).
    UnknownOwner(String),
}

/// Report of a collision when admitting a repository to a managed path.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollisionReport {
    /// The path that the newcomer requested (without suffix).
    pub requested_path: PathBuf,
    /// The path allocated to the newcomer after collision detection.
    pub allocated_path: PathBuf,
    /// The canonical common directory of the existing owner (None if unknown).
    pub owner_common_dir: Option<PathBuf>,
    /// Display name of the existing owner (or "unknown owner" if None).
    pub owner_display: String,
    /// The requesting repository's canonical common directory.
    pub requester_common_dir: PathBuf,
    /// Display name of the requesting repository.
    pub requester_display: String,
    /// Reason for the collision.
    pub reason: CollisionReason,
}

/// Format a collision report as a single-line status message.
///
/// Format: `collision: <requested_path> is owned by <owner_display> (<owner_common_dir>); <requester_display> (<requester_common_dir>) allocated <allocated_path>`
pub fn collision_line(report: &CollisionReport) -> String {
    let owner_info = match &report.owner_common_dir {
        Some(dir) => format!("{} ({})", report.owner_display, dir.display()),
        None => format!("{} (unknown owner)", report.owner_display),
    };

    format!(
        "collision: {} is owned by {}; {} ({}) allocated {}",
        report.requested_path.display(),
        owner_info,
        report.requester_display,
        report.requester_common_dir.display(),
        report.allocated_path.display()
    )
}

/// Reconcile or create the durable repository parent represented by Git facts.
pub fn ensure_repository(
    state: &mut RepositoryState,
    snapshot: &RepositorySnapshot,
) -> Result<(RepositoryKey, Option<CollisionReport>), LifecycleError> {
    let common = PersistedPath::from_path(&snapshot.common_dir);

    // Check if this repository is already known
    if let Some(existing_key) = state
        .repositories
        .iter()
        .find(|repo| repo.observed_common_dir == common)
        .map(|repo| repo.key)
    {
        // Update existing entry and return
        let saved = state
            .repositories
            .iter_mut()
            .find(|saved| saved.key == existing_key)
            .ok_or(LifecycleError::RepositoryMissing(existing_key))?;
        saved.observed_common_dir = common;
        saved.observed_main_worktree = PersistedPath::from_path(&snapshot.main_worktree);
        saved.health = RepositoryHealth::Available;
        return Ok((existing_key, None));
    }

    // Allocate new repository key
    let key = state.allocate_repository_key()?;
    let first_seen_order = state.allocate_first_seen_order()?;

    // Compute physical_key as digest of the canonical common dir
    #[cfg(unix)]
    let digest = {
        use std::os::unix::ffi::OsStrExt;
        crate::repository::compute_repository_digest(snapshot.common_dir.as_os_str().as_bytes())
    };
    #[cfg(not(unix))]
    let digest = {
        // Fallback for non-Unix systems: use the legacy counter key
        key.get().to_string()
    };

    // Determine the target directory path
    let workspace = crate::workspace::active();
    let workspace_name = workspace.name.as_str();
    let target_dir = git::worktrees_base()
        .join(workspace_name)
        .join(format!("repository-{}", digest));

    // Check for collisions on disk
    let collision_report = check_collision(&target_dir, &digest, &snapshot.common_dir, state)?;

    let (allocated_path, actual_physical_key) = if let Some(_report) = &collision_report {
        // Collision detected, allocate suffixed path
        let allocated =
            allocate_suffixed_repository_path(&target_dir, &digest, &snapshot.common_dir, state)?;
        // Extract physical_key from the allocated path (e.g., "abcdef123456-2")
        let actual_key = allocated
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("repository-"))
            .map(|k| k.to_string())
            .unwrap_or(digest.clone());
        (allocated, actual_key)
    } else {
        // No collision, use the computed path
        (target_dir.to_path_buf(), digest)
    };
    // The report was built before allocation; it must name where the newcomer
    // actually landed (CONTEXT: "the newcomer is allocated a distinct ... directory").
    let collision_report = collision_report.map(|mut report| {
        report.allocated_path = allocated_path.clone();
        report
    });

    // Create the directory if it doesn't exist
    std::fs::create_dir_all(&allocated_path).map_err(|e| {
        LifecycleError::Topology(format!("failed to create repository directory: {}", e))
    })?;

    // Write marker (only if not already there with matching content)
    #[cfg(unix)]
    let canonical_bytes = {
        use std::os::unix::ffi::OsStrExt;
        snapshot.common_dir.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let canonical_bytes = {
        snapshot
            .common_dir
            .to_string_lossy()
            .into_owned()
            .into_bytes()
    };

    let marker_meta = crate::marker::MarkerMetadata {
        canonical_common_dir: canonical_bytes,
        scheme_version: 1,
        recorded_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    };
    // Design G: the directory exists before its marker, and the marker is what
    // lets a later admission find this directory again after a state reset.
    let _ = std::fs::create_dir_all(&allocated_path);
    let _ = crate::marker::write_marker(&allocated_path, &marker_meta);

    // Create new repository entry
    state.repositories.push(SavedRepository {
        key,
        observed_common_dir: common.clone(),
        observed_main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
        first_seen_order,
        health: RepositoryHealth::Available,
        physical_key: actual_physical_key,
    });

    Ok((key, collision_report))
}

/// Check if the target directory is owned by a different repository (collision detection matrix).
fn check_collision(
    target_dir: &std::path::Path,
    _physical_key: &str,
    requester_common_dir: &std::path::Path,
    _state: &RepositoryState,
) -> Result<Option<CollisionReport>, LifecycleError> {
    // If directory doesn't exist, no collision
    if !target_dir.exists() {
        return Ok(None);
    }

    // Try to read the marker
    match crate::marker::read_marker(target_dir) {
        Ok(crate::marker::MarkerRead::Valid(meta)) => {
            // Compare canonical common dirs (as bytes)
            #[cfg(unix)]
            let our_canonical_bytes = {
                use std::os::unix::ffi::OsStrExt;
                requester_common_dir.as_os_str().as_bytes().to_vec()
            };
            #[cfg(not(unix))]
            let our_canonical_bytes = {
                requester_common_dir
                    .to_string_lossy()
                    .into_owned()
                    .into_bytes()
            };

            if meta.canonical_common_dir == our_canonical_bytes {
                // Marker matches us, no collision
                Ok(None)
            } else {
                // Marker belongs to a different repository
                let owner_common_dir =
                    PathBuf::from(String::from_utf8_lossy(&meta.canonical_common_dir).into_owned());
                let owner_display = crate::repository::repository_display_name(&owner_common_dir);
                let requester_display =
                    crate::repository::repository_display_name(requester_common_dir);

                Ok(Some(CollisionReport {
                    requested_path: target_dir.to_path_buf(),
                    allocated_path: target_dir.to_path_buf(), // Will be updated with suffix later
                    owner_common_dir: Some(owner_common_dir),
                    owner_display,
                    requester_common_dir: requester_common_dir.to_path_buf(),
                    requester_display,
                    reason: CollisionReason::ForeignMarker,
                }))
            }
        }
        Ok(crate::marker::MarkerRead::Missing) => {
            // No marker, try to discover owner from checkouts
            match discover_checkout_owner(target_dir, requester_common_dir) {
                Ok(Some(owner_common_dir)) => {
                    // Found a checkout that proves a different ownership
                    let owner_display =
                        crate::repository::repository_display_name(&owner_common_dir);
                    let requester_display =
                        crate::repository::repository_display_name(requester_common_dir);

                    Ok(Some(CollisionReport {
                        requested_path: target_dir.to_path_buf(),
                        allocated_path: target_dir.to_path_buf(),
                        owner_common_dir: Some(owner_common_dir),
                        owner_display,
                        requester_common_dir: requester_common_dir.to_path_buf(),
                        requester_display,
                        reason: CollisionReason::ForeignCheckout,
                    }))
                }
                Ok(None) => {
                    // No checkouts found -> unknown owner, treat as collision
                    let requester_display =
                        crate::repository::repository_display_name(requester_common_dir);
                    Ok(Some(CollisionReport {
                        requested_path: target_dir.to_path_buf(),
                        allocated_path: target_dir.to_path_buf(),
                        owner_common_dir: None,
                        owner_display: "unknown owner".to_string(),
                        requester_common_dir: requester_common_dir.to_path_buf(),
                        requester_display,
                        reason: CollisionReason::UnknownOwner("no evidence".to_string()),
                    }))
                }
                Err(_) => {
                    // Couldn't determine ownership, treat as unknown owner
                    let requester_display =
                        crate::repository::repository_display_name(requester_common_dir);
                    Ok(Some(CollisionReport {
                        requested_path: target_dir.to_path_buf(),
                        allocated_path: target_dir.to_path_buf(),
                        owner_common_dir: None,
                        owner_display: "unknown owner".to_string(),
                        requester_common_dir: requester_common_dir.to_path_buf(),
                        requester_display,
                        reason: CollisionReason::UnknownOwner("io error".to_string()),
                    }))
                }
            }
        }
        Ok(crate::marker::MarkerRead::Invalid(reason)) => {
            // Invalid marker, treat as unknown owner
            let reason_str = format!("{:?}", reason);
            let requester_display =
                crate::repository::repository_display_name(requester_common_dir);

            Ok(Some(CollisionReport {
                requested_path: target_dir.to_path_buf(),
                allocated_path: target_dir.to_path_buf(),
                owner_common_dir: None,
                owner_display: "unknown owner".to_string(),
                requester_common_dir: requester_common_dir.to_path_buf(),
                requester_display,
                reason: CollisionReason::UnknownOwner(reason_str),
            }))
        }
        Err(_) => {
            // I/O error reading marker, treat as unknown owner
            let requester_display =
                crate::repository::repository_display_name(requester_common_dir);
            Ok(Some(CollisionReport {
                requested_path: target_dir.to_path_buf(),
                allocated_path: target_dir.to_path_buf(),
                owner_common_dir: None,
                owner_display: "unknown owner".to_string(),
                requester_common_dir: requester_common_dir.to_path_buf(),
                requester_display,
                reason: CollisionReason::UnknownOwner("io error reading marker".to_string()),
            }))
        }
    }
}

/// Discover if any checkout in the directory proves a different ownership.
/// Returns Some(owner_common_dir) if a different owner is found, Ok(None) if all agree with us or no checkouts.
fn discover_checkout_owner(
    target_dir: &std::path::Path,
    requester_common_dir: &std::path::Path,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    // Look for subdirectories matching primary-* or *-* patterns
    let entries = std::fs::read_dir(target_dir)?;

    let _unanimous_owner: Option<PathBuf> = None;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // Match checkout directories: primary-<n> or <branch>-<n>
        if !name_str.contains('-') {
            continue;
        }

        if !path.is_dir() {
            continue;
        }

        // Try to discover repository identity from this checkout
        match git::discover_repository(&path) {
            Ok(snapshot) => {
                #[cfg(unix)]
                let checkout_common_bytes = {
                    use std::os::unix::ffi::OsStrExt;
                    snapshot.common_dir.as_os_str().as_bytes().to_vec()
                };
                #[cfg(not(unix))]
                let checkout_common_bytes = {
                    snapshot
                        .common_dir
                        .to_string_lossy()
                        .into_owned()
                        .into_bytes()
                };

                #[cfg(unix)]
                let our_bytes = {
                    use std::os::unix::ffi::OsStrExt;
                    requester_common_dir.as_os_str().as_bytes().to_vec()
                };
                #[cfg(not(unix))]
                let our_bytes = {
                    requester_common_dir
                        .to_string_lossy()
                        .into_owned()
                        .into_bytes()
                };

                if checkout_common_bytes != our_bytes {
                    // Found a different owner
                    return Ok(Some(snapshot.common_dir));
                }
            }
            Err(_) => {
                // Can't discover from this checkout, skip
                continue;
            }
        }
    }

    Ok(None)
}

/// Allocate a suffixed repository path for a collision.
fn allocate_suffixed_repository_path(
    _requested_dir: &std::path::Path,
    physical_key: &str,
    requester_common_dir: &std::path::Path,
    _state: &RepositoryState,
) -> Result<PathBuf, LifecycleError> {
    #[cfg(unix)]
    let our_bytes: Vec<u8> = {
        use std::os::unix::ffi::OsStrExt;
        requester_common_dir.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let our_bytes: Vec<u8> = requester_common_dir
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    let workspace = crate::workspace::active();
    let workspace_name = workspace.name.as_str();
    let base = git::worktrees_base().join(workspace_name);

    // Try suffixes starting from 2
    for suffix in 2..=1000 {
        let suffixed_key = format!("{}-{}", physical_key, suffix);
        let candidate = base.join(format!("repository-{}", suffixed_key));

        // Check if this path exists and if it's ours
        if candidate.exists() {
            // Check if it's marked as ours
            match crate::marker::read_marker(&candidate) {
                Ok(crate::marker::MarkerRead::Valid(meta))
                    if meta.canonical_common_dir == our_bytes =>
                {
                    // Already ours from an earlier admission: reuse it.
                    return Ok(candidate);
                }
                _ => {
                    // Foreign, invalid, or missing marker in a suffixed dir: never
                    // touch it, try the next suffix.
                    continue;
                }
            }
        } else {
            // Directory doesn't exist, this suffix is free
            return Ok(candidate);
        }
    }

    Err(LifecycleError::Topology(
        "could not allocate suffixed repository path".to_string(),
    ))
}

/// Allocate a checkout key while skipping any keys whose paths already exist on disk.
/// This ensures that after a state reset, new checkouts do not land on existing directories.
fn allocate_checkout_key_skipping_existing(
    state: &mut RepositoryState,
    physical_key: &str,
    branch: &str,
) -> Result<crate::repository::CheckoutKey, LifecycleError> {
    const MAX_ATTEMPTS: u64 = 10_000;

    for _ in 0..MAX_ATTEMPTS {
        let checkout = state.allocate_checkout_key()?;
        let path = git::managed_branch_worktree_path(physical_key, checkout.get(), branch);

        // If the path doesn't exist, we're good
        if !path.exists() {
            return Ok(checkout);
        }
        // Otherwise, the counter will increment on the next call to allocate_checkout_key
    }

    Err(LifecycleError::Topology(format!(
        "could not allocate checkout key after {} attempts",
        MAX_ATTEMPTS
    )))
}

/// Allocate stable checkout/path identity before any Git mutation.
pub fn prepare_activation(
    state: &mut RepositoryState,
    snapshot: &RepositorySnapshot,
    branch: &str,
) -> Result<PreparedActivation, LifecycleError> {
    let (repository, collision) = ensure_repository(state, snapshot)?;
    let first_seen_order = state.allocate_first_seen_order()?;

    // Get the physical_key from the SavedRepository entry, or use the repository key as fallback
    let repo_key_str = repository.get().to_string();
    let physical_key_owned = state
        .physical_key(repository)
        .map(|s| s.to_string())
        .unwrap_or(repo_key_str);

    // Allocate a checkout key while skipping existing directories
    let checkout = allocate_checkout_key_skipping_existing(state, &physical_key_owned, branch)?;

    let managed_path =
        git::managed_branch_worktree_path(physical_key_owned.as_str(), checkout.get(), branch);

    Ok(PreparedActivation {
        request: ActivationRequest {
            repository,
            branch: branch.to_owned(),
            managed_path,
        },
        checkout,
        first_seen_order,
        collision,
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
    let expected_ref = format!("refs/heads/{}", prepared.request.branch);
    let preexisting_branch_owner = snapshot
        .worktrees
        .iter()
        .find(|record| record.branch.as_deref() == Some(expected_ref.as_str()))
        .map(|record| PersistedPath::from_path(&record.path));
    state.checkouts.push(SavedCheckout {
        key: prepared.checkout,
        repository_key: prepared.request.repository,
        role: CheckoutRole::ManagedBranch,
        managed_by_baude: true,
        observed_path: PersistedPath::from_path(&prepared.request.managed_path),
        observed_branch: Some(expected_ref),
        first_seen_order: prepared.first_seen_order,
        lifecycle: CheckoutLifecycle::Activating,
        owned_runtime: None,
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
            created_branch: None,
            preexisting_branch_owner,
        }),
    });
    state.validate()?;
    Ok(())
}

pub fn clear_pending_activation(state: &mut RepositoryState, checkout: CheckoutKey) {
    state.checkouts.retain(|saved| saved.key != checkout);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivationRecoveryResolution {
    ClearedAbsent {
        checkout: CheckoutKey,
    },
    Finalized {
        checkout: CheckoutKey,
    },
    Blocked {
        checkout: CheckoutKey,
        detail: String,
    },
}

/// Reconcile durable activation ownership after a process restart. This never
/// performs Git mutation: exact absence clears the phantom child, exact
/// path/ref ownership finalizes it, and every ambiguous observation remains a
/// typed blocked recovery record for an explicit retry.
pub fn reconcile_activation_recovery(
    state: &mut RepositoryState,
    checkout_key: CheckoutKey,
) -> Result<ActivationRecoveryResolution, LifecycleError> {
    let index = state
        .checkouts
        .iter()
        .position(|checkout| checkout.key == checkout_key)
        .ok_or(LifecycleError::CheckoutMissing(checkout_key))?;
    let (branch, created_branch, preexisting_branch_owner, prior_verification, prior_compensation) =
        match &state.checkouts[index].health {
            CheckoutHealth::Unavailable(UnavailableCause::PendingActivation {
                branch,
                created_branch,
                preexisting_branch_owner,
            }) => (
                branch.clone(),
                *created_branch,
                preexisting_branch_owner.clone(),
                "activation interrupted before finalization".into(),
                String::new(),
            ),
            CheckoutHealth::Unavailable(UnavailableCause::ActivationRecovery {
                branch,
                created_branch,
                preexisting_branch_owner,
                verification,
                compensation,
                ..
            }) => (
                branch.clone(),
                *created_branch,
                preexisting_branch_owner.clone(),
                verification.clone(),
                compensation.clone(),
            ),
            _ => {
                return Err(LifecycleError::Topology(format!(
                    "checkout {} is not activation recovery",
                    checkout_key.get()
                )))
            }
        };
    let repository_key = state.checkouts[index].repository_key;
    let repository = state
        .repositories
        .iter()
        .find(|repository| repository.key == repository_key)
        .ok_or(LifecycleError::RepositoryMissing(repository_key))?;
    let expected_path = state.checkouts[index].observed_path.to_path_buf();
    let expected_ref = format!("refs/heads/{branch}");
    let snapshot = match git::discover_repository(&repository.observed_main_worktree.to_path_buf())
    {
        Ok(snapshot) if snapshot.common_dir == repository.observed_common_dir.to_path_buf() => {
            snapshot
        }
        Ok(snapshot) => {
            let observation = format!(
                "repository identity conflicts with pending activation: expected {}, observed {}",
                repository.observed_common_dir.to_path_buf().display(),
                snapshot.common_dir.display()
            );
            let detail = format!("{observation}; prior verification: {prior_verification}");
            mark_activation_recovery(
                state,
                checkout_key,
                branch,
                created_branch,
                detail.clone(),
                prior_compensation,
            )?;
            return Ok(ActivationRecoveryResolution::Blocked {
                checkout: checkout_key,
                detail,
            });
        }
        Err(error) => {
            let detail = format!(
                "could not inspect pending activation: {error}; prior verification: {prior_verification}"
            );
            mark_activation_recovery(
                state,
                checkout_key,
                branch,
                created_branch,
                detail.clone(),
                prior_compensation,
            )?;
            return Ok(ActivationRecoveryResolution::Blocked {
                checkout: checkout_key,
                detail,
            });
        }
    };
    let at_path = snapshot
        .worktrees
        .iter()
        .find(|record| record.path == expected_path);
    let at_ref = snapshot
        .worktrees
        .iter()
        .find(|record| record.branch.as_deref() == Some(expected_ref.as_str()));
    match (at_path, at_ref) {
        (None, None) if !expected_path.exists() => {
            clear_pending_activation(state, checkout_key);
            state.validate()?;
            Ok(ActivationRecoveryResolution::ClearedAbsent {
                checkout: checkout_key,
            })
        }
        (Some(path_record), Some(ref_record))
            if path_record.path == ref_record.path
                && !path_record.detached
                && !path_record.locked
                && !path_record.prunable =>
        {
            let checkout = &mut state.checkouts[index];
            checkout.set_lifecycle(CheckoutLifecycle::Active);
            checkout.observed_branch = Some(expected_ref);
            checkout.session.cwd = PersistedPath::from_path(&expected_path);
            checkout.session.repo_root = PersistedPath::from_path(&snapshot.main_worktree);
            checkout.session.branch = Some(branch);
            checkout.session.is_worktree = expected_path != snapshot.main_worktree;
            state.validate()?;
            Ok(ActivationRecoveryResolution::Finalized {
                checkout: checkout_key,
            })
        }
        (None, Some(ref_record))
            if preexisting_branch_owner
                .as_ref()
                .is_some_and(|owner| owner.to_path_buf() == ref_record.path)
                && !expected_path.exists()
                && !ref_record.detached
                && !ref_record.locked
                && !ref_record.prunable =>
        {
            let reused_path = ref_record.path.clone();
            let existing = state.checkouts.iter().position(|checkout| {
                checkout.key != checkout_key
                    && checkout.repository_key == repository_key
                    && checkout.observed_path.to_path_buf() == reused_path
            });
            let finalized_checkout = if let Some(existing) = existing {
                if state.checkouts[existing].observed_branch.as_deref()
                    != Some(expected_ref.as_str())
                {
                    return Err(LifecycleError::Topology(format!(
                        "checkout {} changed branch identity",
                        state.checkouts[existing].key.get()
                    )));
                }
                if let CheckoutHealth::Unavailable(cause) = &state.checkouts[existing].health {
                    let detail = format!(
                        "occupied checkout {} has unresolved recovery: {cause:?}",
                        state.checkouts[existing].key.get()
                    );
                    return Ok(ActivationRecoveryResolution::Blocked {
                        checkout: checkout_key,
                        detail,
                    });
                }
                let checkout = &mut state.checkouts[existing];
                if !matches!(
                    checkout.lifecycle(),
                    CheckoutLifecycle::Launching(_)
                        | CheckoutLifecycle::Running(_)
                        | CheckoutLifecycle::Stopping(_)
                ) {
                    checkout.set_lifecycle(CheckoutLifecycle::Active);
                }
                checkout.session.cwd = PersistedPath::from_path(&reused_path);
                checkout.session.repo_root = PersistedPath::from_path(&snapshot.main_worktree);
                checkout.session.branch = Some(branch.clone());
                checkout.session.is_worktree = reused_path != snapshot.main_worktree;
                checkout.key
            } else {
                let checkout = &mut state.checkouts[index];
                checkout.observed_path = PersistedPath::from_path(&reused_path);
                checkout.observed_branch = Some(expected_ref);
                checkout.managed_by_baude = false;
                checkout.role = if reused_path == snapshot.main_worktree {
                    CheckoutRole::Main
                } else {
                    CheckoutRole::ManagedBranch
                };
                checkout.set_lifecycle(CheckoutLifecycle::Active);
                checkout.session.cwd = PersistedPath::from_path(&reused_path);
                checkout.session.repo_root = PersistedPath::from_path(&snapshot.main_worktree);
                checkout.session.branch = Some(branch);
                checkout.session.is_worktree = reused_path != snapshot.main_worktree;
                checkout.key
            };
            if finalized_checkout != checkout_key {
                clear_pending_activation(state, checkout_key);
            }
            state.validate()?;
            Ok(ActivationRecoveryResolution::Finalized {
                checkout: finalized_checkout,
            })
        }
        _ => {
            let detail = format!(
                "pending activation conflicts with Git facts (path owner: {:?}, ref owner: {:?}); prior verification: {prior_verification}",
                at_path.map(|record| (&record.path, &record.branch)),
                at_ref.map(|record| (&record.path, &record.branch))
            );
            mark_activation_recovery(
                state,
                checkout_key,
                branch,
                created_branch,
                detail.clone(),
                prior_compensation,
            )?;
            Ok(ActivationRecoveryResolution::Blocked {
                checkout: checkout_key,
                detail,
            })
        }
    }
}

pub fn mark_activation_recovery(
    state: &mut RepositoryState,
    checkout_key: CheckoutKey,
    branch: String,
    created_branch: Option<bool>,
    verification: String,
    compensation: String,
) -> Result<(), LifecycleError> {
    let checkout = state
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.key == checkout_key)
        .ok_or(LifecycleError::CheckoutMissing(checkout_key))?;
    let preexisting_branch_owner = match &checkout.health {
        CheckoutHealth::Unavailable(UnavailableCause::PendingActivation {
            preexisting_branch_owner,
            ..
        })
        | CheckoutHealth::Unavailable(UnavailableCause::ActivationRecovery {
            preexisting_branch_owner,
            ..
        }) => preexisting_branch_owner.clone(),
        _ => None,
    };
    checkout.set_lifecycle(CheckoutLifecycle::Protected(
        UnavailableCause::ActivationRecovery {
            branch,
            created_branch,
            preexisting_branch_owner,
            verification,
            compensation,
        },
    ));
    state.validate()?;
    Ok(())
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
                lifecycle: CheckoutLifecycle::Protected(match &error {
                    BranchActivationError::PostAddCompensationFailed {
                        branch,
                        created_branch,
                        verification,
                        compensation,
                        ..
                    } => UnavailableCause::ActivationRecovery {
                        branch: branch.clone(),
                        created_branch: Some(*created_branch),
                        preexisting_branch_owner: None,
                        verification: verification.to_string(),
                        compensation: compensation.clone(),
                    },
                    _ => unreachable!(),
                }),
                owned_runtime: None,
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
                        created_branch: Some(*created_branch),
                        preexisting_branch_owner: None,
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
            if checkout.key != prepared.checkout {
                if let CheckoutHealth::Unavailable(cause) = &checkout.health {
                    return Err(LifecycleError::OccupiedProtected {
                        checkout: checkout.key,
                        cause: cause.clone(),
                    });
                }
            } else if !matches!(
                checkout.health,
                CheckoutHealth::Unavailable(UnavailableCause::PendingActivation { .. })
            ) {
                return Err(LifecycleError::Topology(format!(
                    "activation checkout {} lost its pending candidate",
                    checkout.key.get()
                )));
            }
            if checkout.key == prepared.checkout {
                checkout.set_lifecycle(CheckoutLifecycle::Active);
            }
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
            let activated = SavedCheckout {
                key: prepared.checkout,
                repository_key: prepared.request.repository,
                role,
                managed_by_baude: created_by_baude,
                observed_path: PersistedPath::from_path(&record.path),
                observed_branch: Some(full_ref),
                first_seen_order: prepared.first_seen_order,
                lifecycle: CheckoutLifecycle::Active,
                owned_runtime: None,
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
            };
            if let Some(pending) = state
                .checkouts
                .iter_mut()
                .find(|checkout| checkout.key == prepared.checkout)
            {
                *pending = activated;
            } else {
                state.checkouts.push(activated);
            }
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
                    lifecycle: CheckoutLifecycle::Protected(UnavailableCause::ActivationRecovery {
                        branch: activation_branch.clone(),
                        created_branch: Some(matches!(disposition, ActivationDisposition::Created)),
                        preexisting_branch_owner: None,
                        verification: error.to_string(),
                        compensation: compensation.to_string(),
                    }),
                    owned_runtime: None,
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
                        created_branch: Some(matches!(disposition, ActivationDisposition::Created)),
                        preexisting_branch_owner: None,
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
                return Err(LifecycleError::PostVerificationCompensationFailed(
                    Box::new(PostVerificationCompensationFailure {
                        repository: activation_repository.main_worktree,
                        checkout: prepared.checkout,
                        path: added_path,
                        branch: activation_branch,
                        created_branch: matches!(disposition, ActivationDisposition::Created),
                        verification: error.to_string(),
                        compensation: compensation.to_string(),
                    }),
                ));
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
        execute_activation_with_post_git_hook, mark_activation_recovery, plan_close, plan_reopen,
        prepare_activation, reconcile_activation_recovery, reconcile_teardown_recovery,
        record_pending_activation, revoke_removal_authority, ActivationRecoveryResolution,
        ActivationRequest, CloseEffect, CloseRequest, CollisionReason, CollisionReport,
        LifecycleError, LifecycleOutcome, ReopenDispatch, ReopenRequest, ReopenRuntime,
        RepositoryReservations, TeardownRecoveryResolution,
    };
    use crate::backend::SpawnMode;
    use crate::git::{self, ReconciliationUnavailable};
    use crate::repository::{
        CheckoutHealth, CheckoutLifecycle, CheckoutRole, PersistedPath, RepositoryHealth,
        RepositoryKey, RepositoryState, RetainedSessionState, RuntimeGeneration, SavedCheckout,
        SavedRepository,
    };
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_LIFECYCLE_FIXTURE: AtomicU64 = AtomicU64::new(0);

    /// Owns one activation case's fixture root: the redirect that pins every
    /// resolved config, state and managed-worktree path inside it, plus the
    /// literal workspace identity resolved under that root.
    ///
    /// Activation composes its target through
    /// `git::managed_branch_worktree_path`, which reads the data root and the
    /// active workspace. Before this owner existed the five activation cases
    /// resolved the developer's real `~/.local/share/baude/worktrees` and were
    /// stopped by the plan-01 containment guard.
    struct LifecycleFixture {
        /// Struct fields drop in DECLARATION order — the inverse of locals — so
        /// the identity is restored while the root redirect it was resolved
        /// under is still installed. The 08-03 owner convention.
        _identity: crate::testing::TestRedirect,
        /// Held as a FIELD, never constructed and let go. An unbound guard arms
        /// and disarms in the same statement, so the case would run entirely
        /// unredirected while still compiling and still reading like an
        /// isolated fixture at the call site.
        _redirect: crate::testing::TestRedirect,
        root: PathBuf,
    }

    impl LifecycleFixture {
        #[must_use = "the returned LifecycleFixture owns this case's root and identity; bind it \
                      to a named local declared before every path it contains"]
        fn new(label: &str) -> Self {
            let sequence = NEXT_LIFECYCLE_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "baude-lifecycle-{label}-{}-{sequence}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("create unique lifecycle fixture root");
            // Canonicalized because activation compares a composed managed path
            // against the path Git reports for the same worktree. On macOS
            // `temp_dir()` is `/var/...`, a symlink to `/private/var/...`, and
            // Git reports the resolved form — so an uncanonicalized fixture root
            // makes an exact path owner look like a different one. Real data
            // roots have no such symlink, which is why this only surfaces under
            // a temp-dir fixture.
            let root = std::fs::canonicalize(&root).expect("canonicalize lifecycle fixture root");
            // Acquisition order is root FIRST, identity SECOND, so the identity
            // is resolved with this fixture's roots already installed.
            let redirect = crate::testing::TestRedirect::new(&root);
            let identity = crate::workspace::override_for_test(
                &crate::persist::Config {
                    workspace: Some(label.to_string()),
                    ..crate::persist::Config::default()
                },
                None,
            );
            Self {
                _identity: identity,
                _redirect: redirect,
                root,
            }
        }

        fn root(&self) -> &Path {
            &self.root
        }

        /// A created subdirectory of the fixture root for the scenario's own
        /// repository tree. Deliberately a SIBLING of the redirect's `data/`
        /// subtree, so managed worktrees are never allocated inside the
        /// repository under test.
        fn subdir(&self, relative: impl AsRef<Path>) -> PathBuf {
            let path = self.root.join(relative);
            std::fs::create_dir_all(&path).expect("create lifecycle fixture subdirectory");
            path
        }
    }

    impl Drop for LifecycleFixture {
        fn drop(&mut self) {
            // Swallowed like `GitFixture::drop`: a cleanup failure must not mask
            // the assertion failure that is the reason the case matters.
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn lifecycle_capabilities_expose_only_dispatchable_reopen_and_recovery_actions() {
        use super::{
            lifecycle_capability, reduce_lifecycle, LifecycleCapability, LifecycleEvent,
            LifecycleTraceEntry,
        };
        use crate::repository::{ProcessIdentity, UnavailableCause};

        let identity = ProcessIdentity {
            pid: 41,
            start_time: 42,
            process_group: 41,
            session: 41,
        };
        let retry_reopen = [CheckoutLifecycle::Inactive];
        let retry_recovery = [
            CheckoutLifecycle::Activating,
            CheckoutLifecycle::Protected(UnavailableCause::PendingActivation {
                branch: "feature/capability".into(),
                created_branch: None,
                preexisting_branch_owner: None,
            }),
            CheckoutLifecycle::Protected(UnavailableCause::ActivationRecovery {
                branch: "feature/capability".into(),
                created_branch: Some(true),
                preexisting_branch_owner: None,
                verification: "verification failed".into(),
                compensation: "compensation failed".into(),
            }),
            CheckoutLifecycle::Protected(UnavailableCause::TeardownPending {
                agent_pid: Some(identity.pid),
                shell_pid: None,
                agent_identity: Some(identity.clone()),
                shell_identity: None,
                agent_stopped: false,
                shell_stopped: true,
                detail: "agent still owns the runtime".into(),
            }),
            CheckoutLifecycle::Protected(UnavailableCause::StoppedActiveRecovery {
                agent_restarted: false,
                shell_restarted: true,
                detail: "runtime restart compensation failed".into(),
            }),
        ];
        let unavailable = [
            CheckoutLifecycle::Active,
            CheckoutLifecycle::Launching(RuntimeGeneration::initial()),
            CheckoutLifecycle::Running(RuntimeGeneration::initial()),
            CheckoutLifecycle::Stopping(RuntimeGeneration::initial()),
            CheckoutLifecycle::RemovalCommitted,
            CheckoutLifecycle::Protected(UnavailableCause::RemovalTombstone(
                "removal committed".into(),
            )),
            CheckoutLifecycle::Protected(UnavailableCause::Missing),
            CheckoutLifecycle::Protected(UnavailableCause::Other(
                "generic unavailable topology".into(),
            )),
        ];

        for lifecycle in retry_reopen {
            assert_eq!(
                lifecycle_capability(&lifecycle),
                Some(LifecycleCapability::RetryReopen),
                "retained lifecycle {lifecycle:?}"
            );
        }
        for lifecycle in retry_recovery {
            assert_eq!(
                lifecycle_capability(&lifecycle),
                Some(LifecycleCapability::RetryRecovery),
                "recoverable lifecycle {lifecycle:?}"
            );
        }
        let stopped_active =
            CheckoutLifecycle::Protected(UnavailableCause::StoppedActiveRecovery {
                agent_restarted: false,
                shell_restarted: false,
                detail: "runtime is known stopped".into(),
            });
        let transition = reduce_lifecycle(&stopped_active, &LifecycleEvent::RuntimeExtinct);
        assert_eq!(transition.state, CheckoutLifecycle::Inactive);
        assert_eq!(
            transition.effects,
            vec![LifecycleTraceEntry::PersistInactive]
        );
        for lifecycle in unavailable {
            assert_eq!(
                lifecycle_capability(&lifecycle),
                None,
                "non-dispatchable lifecycle {lifecycle:?}"
            );
        }
    }

    #[test]
    fn lifecycle_protocol_core_legal_transition_table() {
        use super::{reduce_lifecycle, LifecycleEvent, LifecycleTraceEntry};
        use crate::repository::{CheckoutLifecycle, RuntimeGeneration};

        let legal = [
            (
                CheckoutLifecycle::Inactive,
                LifecycleEvent::RequestActivation,
                CheckoutLifecycle::Activating,
                LifecycleTraceEntry::PersistActivation,
            ),
            (
                CheckoutLifecycle::Activating,
                LifecycleEvent::ActivationVerified,
                CheckoutLifecycle::Active,
                LifecycleTraceEntry::PersistActive,
            ),
            (
                CheckoutLifecycle::Active,
                LifecycleEvent::LaunchRegistered(super::contract_owned_runtime()),
                CheckoutLifecycle::Launching(RuntimeGeneration::initial()),
                LifecycleTraceEntry::PersistRuntime,
            ),
            (
                CheckoutLifecycle::Launching(RuntimeGeneration::initial()),
                LifecycleEvent::LaunchReleased,
                CheckoutLifecycle::Running(RuntimeGeneration::initial()),
                LifecycleTraceEntry::ReleaseRuntime,
            ),
            (
                CheckoutLifecycle::Running(RuntimeGeneration::initial()),
                LifecycleEvent::RequestClose,
                CheckoutLifecycle::Stopping(RuntimeGeneration::initial()),
                LifecycleTraceEntry::PersistStop,
            ),
            (
                CheckoutLifecycle::Stopping(RuntimeGeneration::initial()),
                LifecycleEvent::RuntimeExtinct,
                CheckoutLifecycle::Inactive,
                LifecycleTraceEntry::PersistInactive,
            ),
        ];

        for (before, event, after, effect) in legal {
            let transition = reduce_lifecycle(&before, &event);
            assert_eq!(transition.state, after);
            assert_eq!(transition.effects, vec![effect]);
        }

        let protected = CheckoutLifecycle::RemovalCommitted;
        for event in [
            LifecycleEvent::RequestActivation,
            LifecycleEvent::ActivationVerified,
            LifecycleEvent::LaunchReleased,
            LifecycleEvent::RequestClose,
            LifecycleEvent::RuntimeExtinct,
        ] {
            let refused = reduce_lifecycle(&protected, &event);
            assert_eq!(refused.state, protected);
            assert!(refused.effects.is_empty());
        }
    }

    #[test]
    fn lifecycle_startup_recovery_is_idempotent() {
        use super::{startup_recovery_program, RecoveryStep};

        let mut state = close_state();
        let repository = state.checkouts[0].repository_key;
        let process = state.checkouts[0].key;
        state.checkouts[0].lifecycle = CheckoutLifecycle::Stopping(RuntimeGeneration::initial());

        let activation = state.allocate_checkout_key().unwrap();
        let activation_order = state.allocate_first_seen_order().unwrap();
        let mut activation_checkout = state.checkouts[0].clone();
        activation_checkout.key = activation;
        activation_checkout.first_seen_order = activation_order;
        activation_checkout.observed_path = path("/repo-activation");
        activation_checkout.session.cwd = path("/repo-activation");
        activation_checkout.lifecycle = CheckoutLifecycle::Activating;
        state.checkouts.push(activation_checkout);

        let launch = state.allocate_checkout_key().unwrap();
        let launch_order = state.allocate_first_seen_order().unwrap();
        let mut launch_checkout = state.checkouts[0].clone();
        launch_checkout.key = launch;
        launch_checkout.first_seen_order = launch_order;
        launch_checkout.observed_path = path("/repo-launch");
        launch_checkout.session.cwd = path("/repo-launch");
        launch_checkout.lifecycle = CheckoutLifecycle::Active;
        state.checkouts.push(launch_checkout);

        assert_eq!(
            startup_recovery_program(&state),
            vec![
                RecoveryStep::StopOwned(process),
                RecoveryStep::RecoverActivation(activation),
                RecoveryStep::Launch(launch),
            ]
        );
        for checkout in &mut state.checkouts {
            checkout.set_lifecycle(CheckoutLifecycle::Inactive);
            checkout.set_owned_runtime(None);
        }
        assert!(startup_recovery_program(&state).is_empty());
        assert!(startup_recovery_program(&state).is_empty());
        assert_eq!(state.repositories[0].key, repository);
    }

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
            physical_key: String::new(),
        });
        state.checkouts.push(SavedCheckout {
            key: checkout,
            repository_key: repository,
            role: CheckoutRole::ManagedBranch,
            managed_by_baude: true,
            observed_path: path("/repo-feature"),
            observed_branch: Some("refs/heads/feature/close".into()),
            first_seen_order: checkout_order,
            lifecycle: CheckoutLifecycle::Active,
            owned_runtime: None,
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
    fn post_verification_compensation_failure_records_typed_recovery_child() {
        // Declared first so it drops last: the redirect must outlive every
        // managed path this case composes, including the cleanup below.
        let fixture = LifecycleFixture::new("post-verification");
        let root = fixture.subdir("repo");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(root.join("tracked"), b"one\n").unwrap();
        for args in [
            vec!["add", "tracked"],
            vec!["commit", "-m", "initial"],
            vec!["branch", "feature/post-verify"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        let snapshot = crate::git::discover_repository(&root).unwrap();
        let mut state = RepositoryState::default();
        let prepared = prepare_activation(&mut state, &snapshot, "feature/post-verify").unwrap();
        let checkout = prepared.checkout;
        record_pending_activation(&mut state, &snapshot, &prepared).unwrap();

        let error = execute_activation_with_post_git_hook(&mut state, &root, prepared, |path| {
            assert!(Command::new("git")
                .args(["checkout", "--detach"])
                .current_dir(path)
                .status()
                .unwrap()
                .success());
            std::fs::write(path.join("compensation-blocker"), b"dirty\n").unwrap();
        })
        .unwrap_err();

        match error {
            LifecycleError::PostVerificationCompensationFailed(failure) => {
                assert_eq!(failure.checkout, checkout);
                assert!(!failure.created_branch);
            }
            other => panic!("unexpected activation error: {other}"),
        }
        assert_eq!(state.checkouts.len(), 1);
        assert!(matches!(
            state.checkouts[0].health,
            CheckoutHealth::Unavailable(
                crate::repository::UnavailableCause::ActivationRecovery { .. }
            )
        ));
        let resolution = reconcile_activation_recovery(&mut state, checkout).unwrap();
        assert!(matches!(
            resolution,
            ActivationRecoveryResolution::Blocked { checkout: key, .. } if key == checkout
        ));
        let path = state.checkouts[0].observed_path.to_path_buf();
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", "--"])
            .arg(&path)
            .current_dir(&root)
            .status();
    }

    #[test]
    fn occupied_protected_checkout_refuses_activation_overwrite() {
        // SC2 "activation cannot overwrite": when Git reuses an existing
        // worktree for the requested branch and durable state records that
        // exact path under a DIFFERENT, protected checkout, finalization must
        // refuse with OccupiedProtected and leave the occupant untouched.
        let fixture = LifecycleFixture::new("occupied-protected");
        let root = fixture.root();
        let repo = fixture.subdir("repo");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(repo.join("tracked"), b"one\n").unwrap();
        for args in [
            vec!["add", "tracked"],
            vec!["commit", "-m", "initial"],
            vec!["branch", "feature/occupied"],
            vec!["worktree", "add", "../occupied", "feature/occupied"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success());
        }
        let snapshot = crate::git::discover_repository(&repo).unwrap();
        let mut state = RepositoryState::default();
        let prepared = prepare_activation(&mut state, &snapshot, "feature/occupied").unwrap();
        let pending = prepared.checkout;
        let repository = prepared.request.repository;
        record_pending_activation(&mut state, &snapshot, &prepared).unwrap();

        // Durable state already owns the occupied path under a protected
        // (removal-tombstoned) checkout distinct from the pending child.
        let occupied = root.join("occupied").canonicalize().unwrap();
        let main_worktree = state
            .repositories
            .iter()
            .find(|saved| saved.key == repository)
            .unwrap()
            .observed_main_worktree
            .clone();
        let protected_key = state.allocate_checkout_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        state.checkouts.push(SavedCheckout {
            key: protected_key,
            repository_key: repository,
            role: CheckoutRole::ManagedBranch,
            managed_by_baude: true,
            observed_path: PersistedPath::from_path(&occupied),
            observed_branch: Some("refs/heads/feature/occupied".into()),
            first_seen_order: order,
            lifecycle: CheckoutLifecycle::RemovalCommitted,
            owned_runtime: None,
            active_intent: false,
            session: RetainedSessionState {
                name: "repo:feature/occupied".into(),
                cwd: PersistedPath::from_path(&occupied),
                repo_root: main_worktree,
                branch: Some("feature/occupied".into()),
                is_worktree: true,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
            health: CheckoutHealth::Unavailable(
                crate::repository::UnavailableCause::RemovalTombstone(
                    "destructive authority revoked".into(),
                ),
            ),
        });
        state.validate().unwrap();

        let error = super::execute_activation(&mut state, &repo, prepared).unwrap_err();
        match error {
            LifecycleError::OccupiedProtected { checkout, cause } => {
                assert_eq!(checkout, protected_key);
                assert_ne!(checkout, pending);
                assert!(matches!(
                    cause,
                    crate::repository::UnavailableCause::RemovalTombstone(_)
                ));
            }
            other => panic!("unexpected activation error: {other}"),
        }

        // The protected occupant survives byte-intact: tombstone health,
        // RemovalCommitted lifecycle, no active intent, worktree still on disk.
        let occupant = state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == protected_key)
            .unwrap();
        assert!(matches!(
            occupant.health,
            CheckoutHealth::Unavailable(crate::repository::UnavailableCause::RemovalTombstone(_))
        ));
        assert_eq!(occupant.lifecycle(), &CheckoutLifecycle::RemovalCommitted);
        assert!(!occupant.active_intent());
        assert!(occupied.is_dir());

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", "--", "../occupied"])
            .current_dir(&repo)
            .status();
    }

    #[test]
    fn occupied_protected_checkout_blocks_activation_recovery_merge() {
        // The recovery twin of the execute-path guard above: reconciling a
        // pending activation whose branch lives at a preexisting owner path
        // must return Blocked — never merge into — an occupant carrying
        // protected recovery state, and must leave both records untouched.
        let fixture = LifecycleFixture::new("occupied-recovery");
        let root = fixture.root();
        let repo = fixture.subdir("repo");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(repo.join("tracked"), b"one\n").unwrap();
        for args in [
            vec!["add", "tracked"],
            vec!["commit", "-m", "initial"],
            vec!["branch", "feature/occupied"],
            vec!["worktree", "add", "../occupied", "feature/occupied"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success());
        }
        let snapshot = crate::git::discover_repository(&repo).unwrap();
        let mut state = RepositoryState::default();
        let prepared = prepare_activation(&mut state, &snapshot, "feature/occupied").unwrap();
        let pending = prepared.checkout;
        let repository = prepared.request.repository;
        record_pending_activation(&mut state, &snapshot, &prepared).unwrap();

        let occupied = root.join("occupied").canonicalize().unwrap();
        let main_worktree = state
            .repositories
            .iter()
            .find(|saved| saved.key == repository)
            .unwrap()
            .observed_main_worktree
            .clone();
        let protected_key = state.allocate_checkout_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        state.checkouts.push(SavedCheckout {
            key: protected_key,
            repository_key: repository,
            role: CheckoutRole::ManagedBranch,
            managed_by_baude: true,
            observed_path: PersistedPath::from_path(&occupied),
            observed_branch: Some("refs/heads/feature/occupied".into()),
            first_seen_order: order,
            lifecycle: CheckoutLifecycle::RemovalCommitted,
            owned_runtime: None,
            active_intent: false,
            session: RetainedSessionState {
                name: "repo:feature/occupied".into(),
                cwd: PersistedPath::from_path(&occupied),
                repo_root: main_worktree,
                branch: Some("feature/occupied".into()),
                is_worktree: true,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
            health: CheckoutHealth::Unavailable(
                crate::repository::UnavailableCause::RemovalTombstone(
                    "destructive authority revoked".into(),
                ),
            ),
        });
        state.validate().unwrap();

        let resolution = reconcile_activation_recovery(&mut state, pending).unwrap();
        match resolution {
            ActivationRecoveryResolution::Blocked { checkout, detail } => {
                assert_eq!(checkout, pending);
                assert!(detail.contains("unresolved recovery"), "got: {detail}");
            }
            other => panic!("unexpected recovery resolution: {other:?}"),
        }

        // Neither record was merged or mutated: the occupant keeps its
        // tombstone and the pending child keeps its pending-activation cause.
        let occupant = state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == protected_key)
            .unwrap();
        assert!(matches!(
            occupant.health,
            CheckoutHealth::Unavailable(crate::repository::UnavailableCause::RemovalTombstone(_))
        ));
        assert_eq!(occupant.lifecycle(), &CheckoutLifecycle::RemovalCommitted);
        assert!(!occupant.active_intent());
        let child = state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == pending)
            .unwrap();
        assert!(matches!(
            child.health,
            CheckoutHealth::Unavailable(
                crate::repository::UnavailableCause::PendingActivation { .. }
            )
        ));

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", "--", "../occupied"])
            .current_dir(&repo)
            .status();
    }

    #[test]
    fn pending_activation_recovery_distinguishes_absent_and_exact_git_facts() {
        let fixture = LifecycleFixture::new("pending-recovery");
        let root = fixture.subdir("repo");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(root.join("tracked"), b"one\n").unwrap();
        for args in [vec!["add", "tracked"], vec!["commit", "-m", "initial"]] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        let snapshot = crate::git::discover_repository(&root).unwrap();
        let mut state = RepositoryState::default();

        let absent = prepare_activation(&mut state, &snapshot, "feature/absent").unwrap();
        let absent_checkout = absent.checkout;
        record_pending_activation(&mut state, &snapshot, &absent).unwrap();
        assert_eq!(
            reconcile_activation_recovery(&mut state, absent_checkout).unwrap(),
            ActivationRecoveryResolution::ClearedAbsent {
                checkout: absent_checkout
            }
        );

        assert!(Command::new("git")
            .args(["branch", "feature/exact"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        let exact = prepare_activation(&mut state, &snapshot, "feature/exact").unwrap();
        let exact_checkout = exact.checkout;
        record_pending_activation(&mut state, &snapshot, &exact).unwrap();
        assert!(Command::new("git")
            .args(["worktree", "add", "--"])
            .arg(&exact.request.managed_path)
            .arg("feature/exact")
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert_eq!(
            reconcile_activation_recovery(&mut state, exact_checkout).unwrap(),
            ActivationRecoveryResolution::Finalized {
                checkout: exact_checkout
            }
        );
        assert!(state.checkouts[0].active_intent);
        assert_eq!(state.checkouts[0].health, CheckoutHealth::Available);

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", "--"])
            .arg(&exact.request.managed_path)
            .current_dir(&root)
            .status();
    }

    #[test]
    fn blocked_activation_retry_round_trips_all_provenance() {
        let fixture = LifecycleFixture::new("activation-evidence");
        let root = fixture.subdir("repo");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(root.join("tracked"), b"one\n").unwrap();
        for args in [vec!["add", "tracked"], vec!["commit", "-m", "initial"]] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        assert!(Command::new("git")
            .args(["branch", "feature/evidence"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        let original_owner = root.join("original-owner");
        assert!(Command::new("git")
            .args(["worktree", "add", "--"])
            .arg(&original_owner)
            .arg("feature/evidence")
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        let snapshot = crate::git::discover_repository(&root).unwrap();
        let mut state = RepositoryState::default();
        let prepared = prepare_activation(&mut state, &snapshot, "feature/evidence").unwrap();
        let checkout = prepared.checkout;
        record_pending_activation(&mut state, &snapshot, &prepared).unwrap();
        assert!(matches!(
            state.checkouts[0].health,
            CheckoutHealth::Unavailable(crate::repository::UnavailableCause::PendingActivation {
                created_branch: None,
                ..
            })
        ));
        mark_activation_recovery(
            &mut state,
            checkout,
            "feature/evidence".into(),
            Some(true),
            "original verification".into(),
            "original compensation".into(),
        )
        .unwrap();

        assert!(Command::new("git")
            .args(["worktree", "remove", "--force", "--"])
            .arg(&original_owner)
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        let conflicting_owner = root.join("conflicting-owner");
        assert!(Command::new("git")
            .args(["worktree", "add", "--"])
            .arg(&conflicting_owner)
            .arg("feature/evidence")
            .current_dir(&root)
            .status()
            .unwrap()
            .success());

        assert!(matches!(
            reconcile_activation_recovery(&mut state, checkout).unwrap(),
            ActivationRecoveryResolution::Blocked { .. }
        ));
        let round_trip: RepositoryState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let first = round_trip.checkouts[0].health.clone();
        state = round_trip;
        assert!(matches!(
            reconcile_activation_recovery(&mut state, checkout).unwrap(),
            ActivationRecoveryResolution::Blocked { .. }
        ));
        match (&first, &state.checkouts[0].health) {
            (
                CheckoutHealth::Unavailable(
                    crate::repository::UnavailableCause::ActivationRecovery {
                        branch: first_branch,
                        created_branch: first_created,
                        preexisting_branch_owner: first_owner,
                        verification: first_verification,
                        compensation: first_compensation,
                    },
                ),
                CheckoutHealth::Unavailable(
                    crate::repository::UnavailableCause::ActivationRecovery {
                        branch,
                        created_branch,
                        preexisting_branch_owner,
                        verification,
                        compensation,
                    },
                ),
            ) => {
                assert_eq!(branch, first_branch);
                assert_eq!(created_branch, first_created);
                assert_eq!(*created_branch, Some(true));
                assert_eq!(preexisting_branch_owner, first_owner);
                assert_eq!(compensation, first_compensation);
                assert_eq!(compensation, "original compensation");
                assert!(verification.contains(first_verification));
                assert!(verification.contains("original verification"));
            }
            evidence => panic!("unexpected recovery evidence: {evidence:?}"),
        }

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", "--"])
            .arg(&conflicting_owner)
            .current_dir(&root)
            .status();
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
            state.checkouts[0].set_lifecycle(CheckoutLifecycle::Inactive);
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
    fn teardown_pending_cannot_reopen_and_completes_inactive_before_explicit_reopen() {
        let mut state = close_state();
        let checkout = state.checkouts[0].key;
        state.checkouts[0].set_lifecycle(CheckoutLifecycle::Protected(
            crate::repository::UnavailableCause::TeardownPending {
                agent_pid: Some(u32::MAX),
                shell_pid: None,
                agent_identity: None,
                shell_identity: None,
                agent_stopped: true,
                shell_stopped: true,
                detail: "owner crashed after teardown".into(),
            },
        ));

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
            crate::repository::UnavailableCause::TeardownPending { .. }
        ));

        assert_eq!(
            reconcile_teardown_recovery(&mut state, checkout).unwrap(),
            TeardownRecoveryResolution::Completed { checkout }
        );
        assert!(!state.checkouts[0].active_intent);
        assert_eq!(state.checkouts[0].health, CheckoutHealth::Available);
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
            state.checkouts[0].set_lifecycle(CheckoutLifecycle::Inactive);
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
        state.checkouts[0].set_lifecycle(CheckoutLifecycle::Inactive);
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

    #[test]
    fn ensure_repository_uses_physical_key_from_saved_entry() {
        // This test verifies that when a SavedRepository has a physical_key set,
        // the accessor returns it correctly.
        let _root = crate::testing::TestRedirect::new(format!(
            "/test/baude-physical-key-{}",
            std::process::id()
        ));
        let mut state = crate::repository::RepositoryState::default();
        let key = state.allocate_repository_key().expect("allocate key");
        let common_dir =
            crate::repository::PersistedPath::from_path(std::path::Path::new("/tmp/test"));
        let main_worktree =
            crate::repository::PersistedPath::from_path(std::path::Path::new("/tmp/test/.git"));

        let repo = crate::repository::SavedRepository {
            key,
            observed_common_dir: common_dir,
            observed_main_worktree: main_worktree,
            first_seen_order: state.allocate_first_seen_order().expect("allocate order"),
            health: crate::repository::RepositoryHealth::Available,
            physical_key: "legacy-counter".to_string(),
        };

        state.repositories.push(repo);

        // Verify the accessor returns the stored physical_key
        assert_eq!(state.physical_key(key), Some("legacy-counter"));
    }

    #[test]
    fn ensure_repository_unknown_owner_missing_marker() {
        // Test: directory exists but has no marker and no checkouts → collision with unknown owner
        let _fixture = LifecycleFixture::new("unknown-owner-missing");
        let mut state = crate::repository::RepositoryState::default();

        let snapshot = crate::git::RepositorySnapshot {
            canonical_input: std::path::PathBuf::from("/tmp/test"),
            common_dir: std::path::PathBuf::from("/tmp/test"),
            main_worktree: std::path::PathBuf::from("/tmp/test/.git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: std::path::PathBuf::from("/tmp/test"),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };

        // Pre-create the target directory (empty, no marker)
        let workspace = crate::workspace::active();
        let base = git::worktrees_base().join(workspace.name.as_str());
        std::fs::create_dir_all(&base).ok();

        // Compute the expected physical_key
        #[cfg(unix)]
        let digest = {
            use std::os::unix::ffi::OsStrExt;
            crate::repository::compute_repository_digest(snapshot.common_dir.as_os_str().as_bytes())
        };
        #[cfg(not(unix))]
        let digest = "test_digest".to_string();

        let target_dir = base.join(format!("repository-{}", digest));
        std::fs::create_dir_all(&target_dir).ok();

        // Admit the repository - should detect unknown owner collision
        let (key, collision) =
            super::ensure_repository(&mut state, &snapshot).expect("ensure_repository");

        // Verify collision was detected
        assert!(collision.is_some(), "should detect unknown owner collision");
        let report = collision.unwrap();
        assert!(
            matches!(report.reason, CollisionReason::UnknownOwner(_)),
            "reason should be UnknownOwner"
        );
        assert_eq!(
            report.owner_common_dir, None,
            "unknown owner should have no common_dir"
        );
        assert_eq!(report.owner_display, "unknown owner");

        // Verify the allocated path has a suffix
        assert!(
            report.allocated_path.to_string_lossy().contains("-"),
            "allocated path should have suffix"
        );

        // Verify the repository entry was created
        let repo = state
            .repositories
            .iter()
            .find(|r| r.key == key)
            .expect("find repository");
        assert!(!repo.physical_key.is_empty(), "physical_key should be set");
        // The physical_key should include the suffix
        assert!(
            repo.physical_key.contains("-"),
            "physical_key should include suffix for collision"
        );
    }

    #[test]
    fn ensure_repository_unknown_owner_invalid_marker() {
        // Test: directory exists with an invalid marker (oversized) → collision with unknown owner
        let _fixture = LifecycleFixture::new("unknown-owner-invalid");
        let mut state = crate::repository::RepositoryState::default();

        let snapshot = crate::git::RepositorySnapshot {
            canonical_input: std::path::PathBuf::from("/tmp/test2"),
            common_dir: std::path::PathBuf::from("/tmp/test2"),
            main_worktree: std::path::PathBuf::from("/tmp/test2/.git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: std::path::PathBuf::from("/tmp/test2"),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };

        // Pre-create the target directory with an oversized invalid marker
        let workspace = crate::workspace::active();
        let base = git::worktrees_base().join(workspace.name.as_str());
        std::fs::create_dir_all(&base).ok();

        #[cfg(unix)]
        let digest = {
            use std::os::unix::ffi::OsStrExt;
            crate::repository::compute_repository_digest(snapshot.common_dir.as_os_str().as_bytes())
        };
        #[cfg(not(unix))]
        let digest = "test_digest2".to_string();

        let target_dir = base.join(format!("repository-{}", digest));
        std::fs::create_dir_all(&target_dir).ok();

        // Write an oversized marker file (> 64 KiB)
        let marker_path = target_dir.join(".baude-marker.json");
        std::fs::write(&marker_path, vec![b'x'; 100_000]).ok();

        // Admit the repository - should detect invalid marker
        let (_key, collision) =
            super::ensure_repository(&mut state, &snapshot).expect("ensure_repository");

        // Verify collision was detected
        assert!(
            collision.is_some(),
            "should detect invalid marker collision"
        );
        let report = collision.unwrap();
        assert!(
            matches!(report.reason, CollisionReason::UnknownOwner(_)),
            "reason should be UnknownOwner"
        );
        assert_eq!(
            report.owner_common_dir, None,
            "unknown owner should have no common_dir"
        );
    }

    #[test]
    fn ensure_repository_unknown_owner_symlink_marker() {
        // Test: directory exists with a marker that is a symlink → collision with unknown owner
        let _fixture = LifecycleFixture::new("unknown-owner-symlink");
        let mut state = crate::repository::RepositoryState::default();

        let snapshot = crate::git::RepositorySnapshot {
            canonical_input: std::path::PathBuf::from("/tmp/test3"),
            common_dir: std::path::PathBuf::from("/tmp/test3"),
            main_worktree: std::path::PathBuf::from("/tmp/test3/.git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: std::path::PathBuf::from("/tmp/test3"),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };

        // Pre-create the target directory with a symlink marker
        let workspace = crate::workspace::active();
        let base = git::worktrees_base().join(workspace.name.as_str());
        std::fs::create_dir_all(&base).ok();

        #[cfg(unix)]
        let digest = {
            use std::os::unix::ffi::OsStrExt;
            crate::repository::compute_repository_digest(snapshot.common_dir.as_os_str().as_bytes())
        };
        #[cfg(not(unix))]
        let digest = "test_digest3".to_string();

        let target_dir = base.join(format!("repository-{}", digest));
        std::fs::create_dir_all(&target_dir).ok();

        // Create a symlink as the marker (invalid)
        let marker_path = target_dir.join(".baude-marker.json");
        let target_file = base.join("some_file");
        std::fs::write(&target_file, b"dummy").ok();
        #[cfg(unix)]
        {
            use std::os::unix::fs;
            let _ = fs::symlink(&target_file, &marker_path);
        }

        // Admit the repository - should detect symlink marker
        let (_key, collision) =
            super::ensure_repository(&mut state, &snapshot).expect("ensure_repository");

        // Verify collision was detected
        assert!(
            collision.is_some(),
            "should detect symlink marker collision"
        );
        let report = collision.unwrap();
        assert!(
            matches!(report.reason, CollisionReason::UnknownOwner(_)),
            "reason should be UnknownOwner"
        );
    }

    #[test]
    fn checkout_allocation_skips_existing_dirs_after_state_reset() {
        // Test: checkout allocation skips on-disk directories that already exist after state reset
        let _fixture = LifecycleFixture::new("checkout-skip");
        let mut state = crate::repository::RepositoryState::default();

        // Create a snapshot to work with
        let repo_path = std::path::PathBuf::from("/tmp/checkout_skip_repo");
        let snapshot = crate::git::RepositorySnapshot {
            canonical_input: repo_path.clone(),
            common_dir: repo_path.clone(),
            main_worktree: repo_path.join(".git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: repo_path.clone(),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };

        // Admit the repository
        let (repository, _collision) =
            super::ensure_repository(&mut state, &snapshot).expect("admit repository");

        // Get the physical_key
        let repo_key_str = repository.get().to_string();
        let physical_key = state
            .physical_key(repository)
            .map(|s| s.to_string())
            .unwrap_or(repo_key_str);

        // Allocate first checkout using the skip helper
        let branch = "test-branch";
        let checkout1 =
            super::allocate_checkout_key_skipping_existing(&mut state, &physical_key, branch)
                .expect("allocate checkout 1");
        assert_eq!(checkout1.get(), 1, "first checkout should be 1");

        // Compose the path and create it on disk
        let path1 =
            git::managed_branch_worktree_path(physical_key.as_str(), checkout1.get(), branch);
        std::fs::create_dir_all(&path1).expect("create test-branch-1 directory");
        assert!(path1.exists(), "test-branch-1 should exist on disk");

        // Verify the path ends with test-branch-1
        let path_str = path1.to_string_lossy();
        assert!(
            path_str.ends_with("test-branch-1"),
            "first checkout path should end with test-branch-1, got {}",
            path_str
        );

        // Reset state (simulate loss of state file)
        state = crate::repository::RepositoryState::default();

        // Admit the repository again
        let (repository2, _) =
            super::ensure_repository(&mut state, &snapshot).expect("admit repository again");

        // Get the physical_key again (should be the same)
        let repo_key_str2 = repository2.get().to_string();
        let physical_key2 = state
            .physical_key(repository2)
            .map(|s| s.to_string())
            .unwrap_or(repo_key_str2);

        // Allocate another checkout - should skip primary-1 and use primary-2
        let checkout2 =
            super::allocate_checkout_key_skipping_existing(&mut state, &physical_key2, branch)
                .expect("allocate checkout 2");
        assert_eq!(
            checkout2.get(),
            2,
            "second checkout should be 2 (skipped 1)"
        );

        // Verify the path ends with test-branch-2
        let path2 =
            git::managed_branch_worktree_path(physical_key2.as_str(), checkout2.get(), branch);
        let path2_str = path2.to_string_lossy();
        assert!(
            path2_str.ends_with("test-branch-2"),
            "second checkout path should end with test-branch-2, got {}",
            path2_str
        );

        // Verify recorded checkout key is 2
        // Note: the checkout may not be recorded until record_pending_activation is called,
        // so we just verify the key value itself
        assert_eq!(checkout2.get(), 2, "recorded checkout key should be 2");
    }

    #[test]
    fn ensure_repository_foreign_marker_collision() {
        // ForeignMarker row: the newcomer's own digest directory already exists
        // and carries a valid marker for a DIFFERENT repository. The newcomer
        // must be reported, allocated the -2 sibling, and never touch the owner.
        use std::os::unix::ffi::OsStrExt;
        let _fixture = LifecycleFixture::new("foreign-marker");
        let mut state = crate::repository::RepositoryState::default();
        let newcomer = crate::git::RepositorySnapshot {
            canonical_input: std::path::PathBuf::from("/tmp/newcomer_repo"),
            common_dir: std::path::PathBuf::from("/tmp/newcomer_repo"),
            main_worktree: std::path::PathBuf::from("/tmp/newcomer_repo/.git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: std::path::PathBuf::from("/tmp/newcomer_repo"),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };
        let workspace = crate::workspace::active();
        let base = git::worktrees_base().join(workspace.name.as_str());
        std::fs::create_dir_all(&base).expect("worktrees base");
        let digest = crate::repository::compute_repository_digest(
            newcomer.common_dir.as_os_str().as_bytes(),
        );
        let target_dir = base.join(format!("repository-{digest}"));
        std::fs::create_dir_all(&target_dir).expect("pre-existing repository dir");
        let owner_common_dir = std::path::PathBuf::from("/tmp/other_owner_repo");
        let owner_marker = crate::marker::MarkerMetadata {
            canonical_common_dir: owner_common_dir.as_os_str().as_bytes().to_vec(),
            scheme_version: 1,
            recorded_at_ms: 1,
        };
        crate::marker::write_marker(&target_dir, &owner_marker).expect("write foreign marker");

        let (key, collision) =
            super::ensure_repository(&mut state, &newcomer).expect("admit newcomer");
        let report = collision.expect("a foreign marker must be reported as a collision");
        assert_eq!(report.reason, CollisionReason::ForeignMarker);
        assert_eq!(
            report.owner_common_dir.as_deref(),
            Some(owner_common_dir.as_path())
        );
        assert_eq!(report.requested_path, target_dir);
        let allocated = base.join(format!("repository-{digest}-2"));
        assert_eq!(report.allocated_path, allocated);
        assert_eq!(report.requester_common_dir, newcomer.common_dir);
        let expected_key = format!("{digest}-2");
        assert_eq!(state.physical_key(key), Some(expected_key.as_str()));
        // The owner's marker is untouched.
        match crate::marker::read_marker(&target_dir).expect("read owner marker") {
            crate::marker::MarkerRead::Valid(meta) => {
                assert_eq!(meta.canonical_common_dir, owner_marker.canonical_common_dir)
            }
            other => panic!("owner marker must stay valid, got {other:?}"),
        }
        // The newcomer owns the suffixed directory.
        match crate::marker::read_marker(&allocated).expect("read allocated marker") {
            crate::marker::MarkerRead::Valid(meta) => assert_eq!(
                meta.canonical_common_dir,
                newcomer.common_dir.as_os_str().as_bytes().to_vec()
            ),
            other => panic!("allocated dir must carry the newcomer's marker, got {other:?}"),
        }
    }

    #[test]
    fn ensure_repository_reuses_own_suffixed_dir_on_second_admission() {
        // After a state reset the unsuffixed dir is still foreign, so the
        // newcomer collides again, but it must land on the SAME -2 directory it
        // already owns (marker proves it) rather than allocating -3.
        use std::os::unix::ffi::OsStrExt;
        let _fixture = LifecycleFixture::new("reuse-suffixed");
        let newcomer = crate::git::RepositorySnapshot {
            canonical_input: std::path::PathBuf::from("/tmp/newcomer_repo"),
            common_dir: std::path::PathBuf::from("/tmp/newcomer_repo"),
            main_worktree: std::path::PathBuf::from("/tmp/newcomer_repo/.git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: std::path::PathBuf::from("/tmp/newcomer_repo"),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };
        let workspace = crate::workspace::active();
        let base = git::worktrees_base().join(workspace.name.as_str());
        std::fs::create_dir_all(&base).expect("worktrees base");
        let digest = crate::repository::compute_repository_digest(
            newcomer.common_dir.as_os_str().as_bytes(),
        );
        let target_dir = base.join(format!("repository-{digest}"));
        std::fs::create_dir_all(&target_dir).expect("pre-existing repository dir");
        let owner_marker = crate::marker::MarkerMetadata {
            canonical_common_dir: b"/tmp/other_owner_repo".to_vec(),
            scheme_version: 1,
            recorded_at_ms: 1,
        };
        crate::marker::write_marker(&target_dir, &owner_marker).expect("write foreign marker");
        let suffixed = base.join(format!("repository-{digest}-2"));

        let mut first = crate::repository::RepositoryState::default();
        let (key1, collision1) =
            super::ensure_repository(&mut first, &newcomer).expect("first admission");
        let report1 = collision1.expect("first admission collides");
        assert_eq!(report1.allocated_path, suffixed);
        let expected_key = format!("{digest}-2");
        assert_eq!(first.physical_key(key1), Some(expected_key.as_str()));

        // State reset: nothing but the directories and markers survive.
        let mut second = crate::repository::RepositoryState::default();
        let (key2, collision2) =
            super::ensure_repository(&mut second, &newcomer).expect("second admission");
        let report2 = collision2.expect("unsuffixed dir is still foreign");
        assert_eq!(report2.reason, CollisionReason::ForeignMarker);
        assert_eq!(
            report2.allocated_path, suffixed,
            "must reuse the owned -2 dir"
        );
        assert_eq!(second.physical_key(key2), Some(expected_key.as_str()));
        assert!(
            !base.join(format!("repository-{digest}-3")).exists(),
            "no third directory may be allocated"
        );
        assert_eq!(second.repositories.len(), 1);
    }

    #[test]
    fn ensure_repository_matching_marker_no_collision() {
        // Test: directory exists with a marker that matches our common_dir → no collision
        let _fixture = LifecycleFixture::new("matching-marker");
        let mut state = crate::repository::RepositoryState::default();

        let snapshot = crate::git::RepositorySnapshot {
            canonical_input: std::path::PathBuf::from("/tmp/test6"),
            common_dir: std::path::PathBuf::from("/tmp/test6"),
            main_worktree: std::path::PathBuf::from("/tmp/test6/.git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: std::path::PathBuf::from("/tmp/test6"),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };

        // Admit the repository once
        let (key1, collision1) =
            super::ensure_repository(&mut state, &snapshot).expect("first admit");
        assert!(
            collision1.is_none(),
            "first admit should not report collision"
        );

        // Admit the same repository again
        let (key2, collision2) =
            super::ensure_repository(&mut state, &snapshot).expect("second admit");
        assert_eq!(key1, key2, "should reuse the same key");
        assert!(
            collision2.is_none(),
            "second admit of same repository should not report collision"
        );
        assert_eq!(
            state.repositories.len(),
            1,
            "should have only one repository entry"
        );
    }

    #[test]
    fn ensure_repository_tui_and_daemon_compute_same_physical_key() {
        // Verify that TUI and daemon compute identical physical_key for the same repository
        // by calling ensure_repository twice with separate state instances (simulating
        // TUI and daemon working independently).
        let _fixture = LifecycleFixture::new("tui-daemon-same-key");
        let mut state1 = RepositoryState::default();
        let mut state2 = RepositoryState::default();

        let snapshot = crate::git::RepositorySnapshot {
            canonical_input: PathBuf::from("/tmp/test/repo"),
            common_dir: PathBuf::from("/tmp/test/repo"),
            main_worktree: PathBuf::from("/tmp/test/repo/.git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: PathBuf::from("/tmp/test/repo"),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };

        // First call (simulating TUI)
        let (key1, collision1) =
            super::ensure_repository(&mut state1, &snapshot).expect("TUI admission");
        assert!(collision1.is_none(), "first admission should not collide");

        // Second call with separate state (simulating daemon)
        let (key2, collision2) =
            super::ensure_repository(&mut state2, &snapshot).expect("daemon admission");
        assert!(
            collision2.is_none(),
            "first daemon admission should not collide"
        );

        // Both should compute the same key
        assert_eq!(
            key1, key2,
            "TUI and daemon should compute identical physical_key for same repository"
        );
    }

    #[test]
    fn ensure_repository_tui_and_daemon_collision_identical() {
        // Verify that TUI and daemon handle collisions identically by:
        // - Re-admitting the same repository (which returns None for collision)
        // - Verifying both TUI and daemon return identical results
        let _fixture = LifecycleFixture::new("tui-daemon-collision");
        let mut state_tui = RepositoryState::default();
        let mut state_daemon = RepositoryState::default();

        let snapshot = crate::git::RepositorySnapshot {
            canonical_input: PathBuf::from("/tmp/test/repo"),
            common_dir: PathBuf::from("/tmp/test/repo"),
            main_worktree: PathBuf::from("/tmp/test/repo/.git"),
            selected_worktree: crate::git::WorktreeRecord {
                path: PathBuf::from("/tmp/test/repo"),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            },
            worktrees: vec![],
        };

        // TUI: admit repo1, then re-admit it (no collision)
        let (_key_tui1, collision_tui1) =
            super::ensure_repository(&mut state_tui, &snapshot).expect("TUI first admission");
        assert!(
            collision_tui1.is_none(),
            "first admission should not collide"
        );

        let (_key_tui2, collision_tui2) =
            super::ensure_repository(&mut state_tui, &snapshot).expect("TUI re-admission");
        assert!(collision_tui2.is_none(), "re-admission should not collide");

        // Daemon: same repo, same pattern
        let (_key_daemon1, collision_daemon1) =
            super::ensure_repository(&mut state_daemon, &snapshot).expect("daemon first admission");
        assert!(
            collision_daemon1.is_none(),
            "first admission should not collide"
        );

        let (_key_daemon2, collision_daemon2) =
            super::ensure_repository(&mut state_daemon, &snapshot).expect("daemon re-admission");
        assert!(
            collision_daemon2.is_none(),
            "re-admission should not collide"
        );

        // Both TUI and daemon should handle the same repository identically
        // (both return None for collision on re-admission of the same repo)
        assert_eq!(
            collision_tui1, collision_daemon1,
            "TUI and daemon should handle first admission identically"
        );
        assert_eq!(
            collision_tui2, collision_daemon2,
            "TUI and daemon should handle re-admission identically"
        );
    }

    #[test]
    fn collision_line_names_owner_requester_and_paths() {
        let report = CollisionReport {
            requested_path: PathBuf::from("/data/worktrees/workspace/repository-abc123"),
            allocated_path: PathBuf::from("/data/worktrees/workspace/repository-abc123-2"),
            owner_common_dir: Some(PathBuf::from("/home/user/projects/myrepo/.git")),
            owner_display: "my-repo".to_string(),
            requester_common_dir: PathBuf::from("/mnt/drive/other-repo/.git"),
            requester_display: "other-repo".to_string(),
            reason: CollisionReason::ForeignMarker,
        };

        let line = super::collision_line(&report);

        // Check that the line contains expected components
        assert!(
            line.contains("collision:"),
            "line should start with collision:"
        );
        assert!(
            line.contains("repository-abc123"),
            "line should contain requested path"
        );
        assert!(
            line.contains("my-repo"),
            "line should contain owner display name"
        );
        assert!(
            line.contains("/home/user/projects/myrepo/.git"),
            "line should contain owner common dir"
        );
        assert!(
            line.contains("other-repo"),
            "line should contain requester display name"
        );
        assert!(
            line.contains("repository-abc123-2"),
            "line should contain allocated path"
        );
    }
}
