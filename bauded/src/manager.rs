//! Session ownership for the daemon. Unlike the TUI, the daemon never kills
//! sessions when a client goes away — only on explicit DELETE or daemon
//! shutdown. State persists to its own file (`daemon-state.json`) so a daemon
//! restart restores every session via `claude --continue`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use tokio::sync::{broadcast, Notify};

use baude_core::backend;
use baude_core::git;
use baude_core::lifecycle::{self, CollisionReport, LifecycleOutcome, RepositoryReservations};
use baude_core::meta::{now_unix_ms, ClaudeMeta, HookEvent};
use baude_core::persist::{self, LegacyReconciliation, LoadOutcome, StateFile};
use baude_core::pty::Pty;
use baude_core::removal::{
    self, PersistenceOutcome, PreparedRemoval, RemovalError, RemovalReport, RemovalTarget, RowKey,
    RowRuntime, StageEffects,
};
use baude_core::repository::{
    CheckoutHealth, CheckoutKey, CheckoutLifecycle, CheckoutRole, OwnedRuntime, PersistedPath,
    RepositoryHealth, RepositoryKey, RepositoryState, RetainedSessionState,
    RetainedStandaloneSessionState, RuntimeGeneration, SavedCheckout, SavedRepository,
    SavedStandaloneSession, ShellOwnership, StandaloneKey, StandaloneLifecycle, UnavailableCause,
};
use baude_core::session::{Session, StateSource, Status};
use baude_core::suppression;

/// Headless PTY geometry. Nothing renders it; it only needs to be big enough
/// that Claude Code's TUI lays out sanely in the transcript-driving sense.
const ROWS: u16 = 40;
const COLS: u16 = 120;

/// Base name of the daemon's state file; workspace-suffixed by persist
/// (`daemon-state-<ws>.json`, legacy `daemon-state.json` read-fallback for
/// the claude workspace) so two daemons serving different workspaces never
/// share a session list.
const STATE_BASE: &str = "daemon-state";

pub type Shared = Arc<Mutex<Manager>>;

#[derive(Debug)]
pub enum MutationError {
    Domain(anyhow::Error),
    Persistence(persist::SaveError),
    /// The session's runtime could not be confirmed stopped. A server-side
    /// failure, never "no such session": the session still exists and its
    /// checkout is left TeardownPending for a retry.
    Teardown(anyhow::Error),
    /// The repository or folder the request would change is reserved by
    /// another mutation, such as a removal. The message is the whole body a
    /// client is shown, `repository busy: <main checkout>` or
    /// `folder busy: <folder>`, built under the manager lock.
    Busy(String),
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(f),
            Self::Persistence(error) => error.fmt(f),
            Self::Teardown(error) => error.fmt(f),
            Self::Busy(message) => f.write_str(message),
        }
    }
}

/// Marks an error from [`Manager::teardown_retained_runtime`] so it survives
/// the lifecycle engine as a typed error and can be classified as
/// [`MutationError::Teardown`] instead of falling through to `Domain`.
#[derive(Debug)]
struct RuntimeTeardownError(anyhow::Error);

impl std::fmt::Display for RuntimeTeardownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for RuntimeTeardownError {}

impl MutationError {
    /// Classify an error that crossed the lifecycle engine: a persistence
    /// failure, a runtime teardown failure, or a domain refusal.
    fn from_lifecycle(error: anyhow::Error) -> Self {
        match error.downcast::<persist::SaveError>() {
            Ok(error) => Self::Persistence(error),
            Err(error) => match error.downcast::<RuntimeTeardownError>() {
                Ok(RuntimeTeardownError(error)) => Self::Teardown(error),
                Err(error) => Self::Domain(error),
            },
        }
    }
}

impl std::error::Error for MutationError {}

impl From<anyhow::Error> for MutationError {
    fn from(error: anyhow::Error) -> Self {
        Self::Domain(error)
    }
}

impl From<persist::SaveError> for MutationError {
    fn from(error: persist::SaveError) -> Self {
        Self::Persistence(error)
    }
}

type MutationResult<T> = std::result::Result<T, MutationError>;

/// The row that owns a runtime: a checkout or a standalone folder session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeOwner {
    Checkout(CheckoutKey),
    Standalone(StandaloneKey),
}

/// Where a plain `POST /sessions` for a canonical path is admitted.
#[derive(Clone, Debug, Eq, PartialEq)]
enum PlainAdmission {
    ReuseStandalone(StandaloneKey),
    Discovered,
    JoinStored(RepositoryKey),
    Standalone,
    Refuse(String),
}

/// Route a plain admission. It reads its arguments and nothing else, and
/// returns the first route that matches, in this order.
///
/// First, a standalone row that already stores `canonical` is reused, since
/// core allows one standalone row per canonical path. Second, a path git
/// discovers as a repository takes the checkout arm. Third, a path git says
/// is not a repository, but which a stored checkout's `observed_path` or a
/// stored repository's `observed_main_worktree` already holds, joins that
/// stored repository: core's `StandaloneCheckoutPathConflict` rule rejects a
/// state in which a standalone path equals either of those, and a rejected
/// state fails every later save, so a folder an earlier daemon stored as a
/// repository stays on that shape until its repository row is removed.
/// Fourth, a path git says is not a repository and no stored row holds is a
/// standalone admission. Fifth, any other discovery error is refused: the
/// daemon does not know what the folder is, and a guess in either direction
/// stores a row of the wrong kind.
fn route_plain_admission(
    state: &RepositoryState,
    canonical: &Path,
    discovery: &std::result::Result<git::RepositorySnapshot, git::RepositoryDiscoveryError>,
) -> PlainAdmission {
    let path = PersistedPath::from_path(canonical);
    if let Some(row) = state.standalone_session_by_path(&path) {
        return PlainAdmission::ReuseStandalone(row.key);
    }
    match discovery {
        Ok(_) => PlainAdmission::Discovered,
        Err(git::RepositoryDiscoveryError::NotRepository(_)) => state
            .checkouts
            .iter()
            .find(|checkout| checkout.observed_path == path)
            .map(|checkout| checkout.repository_key)
            .or_else(|| {
                state
                    .repositories
                    .iter()
                    .find(|repository| repository.observed_main_worktree == path)
                    .map(|repository| repository.key)
            })
            .map_or(PlainAdmission::Standalone, PlainAdmission::JoinStored),
        Err(error) => PlainAdmission::Refuse(format!("repository discovery failed: {error}")),
    }
}

/// How many events a `GET /events` subscriber may fall behind before it
/// starts losing the oldest ones. The stream itself survives the loss.
pub const EVENTS_CAPACITY: usize = 64;

/// A daemon-wide event, published to every `GET /events` subscriber.
#[derive(Clone, Debug)]
pub enum DaemonEvent {
    /// One committed `POST /removals`, carrying the report that call returned.
    RowsRemoved(RemovalReport),
}

impl DaemonEvent {
    /// The SSE `event:` name.
    pub fn name(&self) -> &'static str {
        match self {
            DaemonEvent::RowsRemoved(_) => "rows_removed",
        }
    }

    /// The SSE `data:` payload: the same serde derive as the REST body.
    pub fn data(&self) -> serde_json::Result<String> {
        match self {
            DaemonEvent::RowsRemoved(report) => serde_json::to_string(report),
        }
    }
}

/// Why `Manager::begin_removal` or `Manager::finish_removal` removed nothing.
#[derive(Debug)]
pub enum RemoveRowsError {
    /// The row set, runtimes or deleted paths changed since the preview; the
    /// payload is the fresh report the caller re-shows. Boxed so the error
    /// stays small next to the report it replaces.
    Drift(Box<RemovalReport>),
    Removal(RemovalError),
    Blocked(persist::SaveError),
    Busy(RepositoryKey),
    /// The standalone row is reserved by another removal.
    FolderBusy(StandaloneKey),
    /// The slow stage, the kill and the delete, panicked; the payload is the
    /// panic message. Every runtime handle was put back and nothing saved.
    SlowStagePanicked(String),
}

impl std::fmt::Display for RemoveRowsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Drift(_) => f.write_str("the removal changed since it was previewed"),
            Self::Removal(error) => write!(f, "{error}"),
            Self::Blocked(error) => write!(f, "{error}"),
            Self::Busy(repository) => write!(f, "repository busy: {repository:?}"),
            Self::FolderBusy(key) => write!(f, "folder busy: {key:?}"),
            Self::SlowStagePanicked(message) => write!(
                f,
                "removal panicked while stopping runtimes and deleting worktrees, \
                 nothing was saved: {message}"
            ),
        }
    }
}

/// What the unlocked slow stage of a removal returned: core's
/// `stop_and_delete` result, or the panic it raised.
pub type SlowStageOutcome = std::thread::Result<std::result::Result<StageEffects, RemovalError>>;

/// A removal between its two locked stages, from [`Manager::begin_removal`]
/// to [`Manager::finish_removal`]. It owns the runtime handles moved out of
/// the registry and the reservation that keeps every other mutation of the
/// rows out until the commit.
pub struct RemovalInFlight {
    prepared: PreparedRemoval,
    runtimes: Vec<RowRuntime>,
    /// The session ids moved out of the registry.
    consumed: Vec<u64>,
    overrides: Vec<(CheckoutKey, RetainedSessionState)>,
    standalone_overrides: Vec<(StandaloneKey, RetainedStandaloneSessionState)>,
    #[cfg(test)]
    pause: Option<RemovalPauseForTest>,
    /// Held, never read: dropping the value releases the repository.
    _repository_reservation: Option<lifecycle::RepositoryReservation>,
    /// Held, never read: dropping the value releases the standalone row.
    _standalone_reservation: Option<StandaloneReservation>,
}

impl RemovalInFlight {
    /// Stage two, with no manager lock held: core's kill and disk stages,
    /// which read no `RepositoryState`. The test gate and `stop_and_delete`
    /// run inside one `catch_unwind`, so a panic from either is caught the
    /// same way; the handles stay owned by `self` whatever happens, for
    /// `finish_removal` to commit or put back.
    pub fn stop_and_delete(&mut self) -> SlowStageOutcome {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(test)]
            if let Some(pause) = &self.pause {
                pause.enter_and_wait();
            }
            removal::stop_and_delete(&self.prepared, &mut self.runtimes)
        }))
    }
}

/// The text of a caught panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "a panic with no message".to_string()
    }
}

/// Cloneable reservation registry for standalone rows, the same shape as
/// core's `RepositoryReservations`: a guard releases its key on drop and
/// holds its own share of the table, independent of the manager mutex.
#[derive(Clone, Debug, Default)]
struct StandaloneReservations {
    held: Arc<Mutex<HashSet<StandaloneKey>>>,
}

impl StandaloneReservations {
    /// Reserve `key`, or refuse when another holder has it.
    fn reserve(&self, key: StandaloneKey) -> std::result::Result<StandaloneReservation, ()> {
        let mut held = self.held.lock().unwrap_or_else(|error| error.into_inner());
        if !held.insert(key) {
            return Err(());
        }
        drop(held);
        Ok(StandaloneReservation {
            held: Arc::clone(&self.held),
            key,
        })
    }

    /// Whether some guard holds `key` now.
    fn is_reserved(&self, key: StandaloneKey) -> bool {
        self.held
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(&key)
    }
}

/// A held standalone reservation; dropping it releases the row.
#[derive(Debug)]
pub(crate) struct StandaloneReservation {
    held: Arc<Mutex<HashSet<StandaloneKey>>>,
    key: StandaloneKey,
}

impl Drop for StandaloneReservation {
    fn drop(&mut self) {
        self.held
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.key);
    }
}

/// Test-only: a gate every removal passes on its way into the slow stage,
/// after prepare and before the kill, so a test can act while a removal is
/// parked there. Cloneable; every clone is the same gate.
///
/// Each removal entering the slow stage counts itself as entered and waits
/// for [`Self::release`], bounded at ten seconds; a removal that gave up
/// waiting is recorded, so a test can tell a real release from a timeout.
/// Once released the gate stays open. An armed panic is raised by the first
/// removal that leaves the gate after the release, and by no later one.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct RemovalPauseForTest {
    gate: Arc<(Mutex<RemovalPauseState>, std::sync::Condvar)>,
}

#[cfg(test)]
#[derive(Default)]
struct RemovalPauseState {
    entered: usize,
    released: bool,
    timed_out: bool,
    panic_after_release: Option<String>,
}

#[cfg(test)]
impl RemovalPauseForTest {
    const WAIT: std::time::Duration = std::time::Duration::from_secs(10);

    fn state(&self) -> MutexGuard<'_, RemovalPauseState> {
        self.gate
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Called by a removal at the start of its slow stage.
    fn enter_and_wait(&self) {
        let (_, condvar) = &*self.gate;
        let mut state = self.state();
        state.entered += 1;
        condvar.notify_all();
        let (mut state, wait) = condvar
            .wait_timeout_while(state, Self::WAIT, |state| !state.released)
            .unwrap_or_else(|error| error.into_inner());
        if wait.timed_out() {
            state.timed_out = true;
        }
        let panic = state.panic_after_release.take();
        // The guard drops before the panic, so the gate is never poisoned.
        drop(state);
        if let Some(message) = panic {
            panic!("{message}");
        }
    }

    /// Wait until `count` removals have entered, bounded at `within`.
    pub(crate) fn wait_entered(&self, count: usize, within: std::time::Duration) -> bool {
        let (_, condvar) = &*self.gate;
        let state = self.state();
        let (state, _) = condvar
            .wait_timeout_while(state, within, |state| state.entered < count)
            .unwrap_or_else(|error| error.into_inner());
        state.entered >= count
    }

    /// How many removals have entered so far.
    pub(crate) fn entered(&self) -> usize {
        self.state().entered
    }

    /// Let every parked removal, and every later one, through.
    pub(crate) fn release(&self) {
        let (_, condvar) = &*self.gate;
        self.state().released = true;
        condvar.notify_all();
    }

    /// Whether any removal gave up waiting for the release.
    pub(crate) fn timed_out(&self) -> bool {
        self.state().timed_out
    }

    /// Make the first removal that leaves the gate after the release panic
    /// with `message`, from inside its slow stage.
    pub(crate) fn panic_after_release(&self, message: &str) {
        self.state().panic_after_release = Some(message.to_string());
    }
}

/// Overwrite the retained session metadata of every checkout still present in
/// `state` with its live value. The single merge implementation: both the
/// ordinary save and the removal save go through it, so the two cannot drift.
fn merge_live_runtime_metadata(
    state: &mut RepositoryState,
    overrides: &[(CheckoutKey, RetainedSessionState)],
    standalone_overrides: &[(StandaloneKey, RetainedStandaloneSessionState)],
) {
    for checkout in &mut state.checkouts {
        if let Some((_, live)) = overrides.iter().find(|(key, _)| *key == checkout.key) {
            checkout.session = live.clone();
        }
    }
    for row in &mut state.standalone_sessions {
        if let Some((_, live)) = standalone_overrides.iter().find(|(key, _)| *key == row.key) {
            // The five fields a live session owns. `ever_launched`, the
            // lifecycle and the owned runtime change only on the custody
            // transitions, never in a metadata merge.
            row.session.name = live.name.clone();
            row.session.shell_open = live.shell_open;
            row.session.archived = live.archived;
            row.session.archived_by_user = live.archived_by_user;
            row.session.resume_id = live.resume_id.clone();
        }
    }
}

/// Lock the manager, recovering from poisoning (a panicked handler must not
/// take the whole daemon's session list with it).
pub fn lock(shared: &Shared) -> MutexGuard<'_, Manager> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct Manager {
    sessions: Vec<Session>,
    next_id: u64,
    claude_cmd: String,
    /// false in tests — never touch the real daemon-state.json.
    persist: bool,
    /// Waiting this long auto-archives a session; 0 disables.
    pub auto_archive_ms: u64,
    /// PERF-07: what happens to a session's child when its row is archived
    /// (by the timer or by hand). Read once from config at daemon start.
    pub idle_child_policy: persist::IdleChildPolicy,
    /// PERM-02: per-session wake handle for the permission long-poll. Set/clear
    /// pending state happens UNDER the manager lock; the bridge/handler then
    /// `notified().await`s on this Arc OUTSIDE the lock so one pending
    /// permission never stalls other sessions (Pitfall 4 — "decide under the
    /// lock, act outside it"). `resolve_pending` fires `notify_waiters()`.
    permission_notify: HashMap<u64, Arc<Notify>>,
    /// The sender behind `GET /events`. Nothing is retained for a subscriber
    /// that is not connected: `GET /rows` stays the record of what exists.
    events: broadcast::Sender<DaemonEvent>,
    repository_state: RepositoryState,
    runtime_checkouts: HashMap<CheckoutKey, u64>,
    /// The live session of each standalone row. A session id is in at most one
    /// of this registry and `runtime_checkouts`.
    runtime_standalones: HashMap<StandaloneKey, u64>,
    repository_reservations: RepositoryReservations,
    /// The standalone rows a removal holds. The daemon's own registry, the
    /// standalone counterpart of `repository_reservations`: core reserves
    /// repositories only.
    standalone_reservations: StandaloneReservations,
    persistence_blocked: bool,
    /// True after a failed save so API owners can surface degraded durability.
    pub persistence_dirty: bool,
    persistence_error: Option<String>,
    /// Collisions detected during repository admissions since daemon start.
    #[allow(dead_code)]
    pub collisions: Vec<CollisionReport>,
    #[cfg(test)]
    persistence_target_for_test: Option<(PathBuf, String)>,
    #[cfg(test)]
    atomic_failure_for_test: Option<persist::AtomicFailure>,
    /// Test-only: how many saves still go through before
    /// `atomic_failure_for_test` applies, and how many saves it then applies
    /// to. `(0, usize::MAX)` is a failure that is there from the first save
    /// and stays.
    #[cfg(test)]
    atomic_failure_window_for_test: (usize, usize),
    #[cfg(test)]
    spawn_error_for_test: Option<String>,
    /// Test-only: the gate a removal passes between prepare and the kill.
    #[cfg(test)]
    removal_pause_for_test: Option<RemovalPauseForTest>,
    /// Startup timing stages (config load, state load, listener bound, etc.)
    pub startup_timing: crate::timing::StartupTiming,
}

#[derive(Serialize)]
pub struct PersistenceStatus {
    pub enabled: bool,
    pub blocked: bool,
    pub dirty: bool,
    pub error: Option<String>,
}

fn reconcile_legacy_session(saved: &persist::SavedSession) -> LegacyReconciliation {
    match git::discover_repository(&saved.cwd) {
        Ok(snapshot) => LegacyReconciliation::Available {
            common_dir: PersistedPath::from_path(&snapshot.common_dir),
            main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
            checkout_path: PersistedPath::from_path(&snapshot.selected_worktree.path),
            observed_branch: snapshot.selected_worktree.branch.clone(),
            checkout_role: if snapshot.selected_worktree.path == snapshot.main_worktree {
                CheckoutRole::Main
            } else {
                CheckoutRole::ManagedBranch
            },
            managed_by_baude: false,
        },
        Err(error) => LegacyReconciliation::Unavailable {
            repository_cause: UnavailableCause::Other(error.to_string()),
            checkout_cause: if saved.cwd.exists() {
                UnavailableCause::NotRepository
            } else {
                UnavailableCause::Missing
            },
        },
    }
}

/// The body of `GET /rows`: the daemon's hierarchy, and the authoritative
/// index from a durable row key to a display name, a path and a live session
/// id. An object with two arrays and never a bare array, so a third root kind
/// is additive. The field names are a wire contract read by the PWA and the
/// TUI remote client; renaming one means touching every caller.
///
/// Deliberately absent: any conversation identifier, the owned runtime, the
/// lifecycle, and any `removable` flag. Removal eligibility is core's, through
/// the preview report, and a second daemon-computed answer could disagree.
#[derive(Serialize, Clone)]
pub struct RowsView {
    pub repositories: Vec<RepositoryRowView>,
    pub standalone: Vec<StandaloneRowView>,
}

/// A repository parent entry of `GET /rows`.
#[derive(Serialize, Clone)]
pub struct RepositoryRowView {
    pub repository_key: u64,
    pub display_name: String,
    pub path: String,
    pub health: RepositoryHealth,
    pub checkouts: Vec<CheckoutRowView>,
}

/// A checkout child entry of `GET /rows`.
#[derive(Serialize, Clone)]
pub struct CheckoutRowView {
    pub checkout_key: u64,
    /// The parent key repeated, so a flattened client list still finds it.
    pub repository_key: u64,
    pub display_name: String,
    /// The short branch name, `None` for a detached HEAD. The stored
    /// `observed_branch` is git's full ref and is never rewritten: core
    /// compares and re-observes full refs, so the short form exists in this
    /// projection only.
    pub branch: Option<String>,
    pub path: String,
    pub role: CheckoutRole,
    pub managed_by_baude: bool,
    pub archived: bool,
    /// The live daemon session id, `None` when the row has no runtime. Equal
    /// to `SessionInfo::id`, which is the join column to `GET /sessions`.
    pub session_id: Option<u64>,
}

/// The branch name `GET /rows` serves for a stored `observed_branch`.
///
/// A value beginning `refs/heads/` loses that one leading prefix and keeps
/// the rest verbatim, so a slash-bearing branch is never split. Any other
/// value is served as stored: the daemon does not guess at a ref it does not
/// recognize. No stored value is `None`, with no fallback to a default
/// branch.
fn short_branch_name(observed: Option<&str>) -> Option<String> {
    observed.map(|full| full.strip_prefix("refs/heads/").unwrap_or(full).to_string())
}

/// A standalone root entry of `GET /rows`.
#[derive(Serialize, Clone)]
pub struct StandaloneRowView {
    pub standalone_key: u64,
    pub display_name: String,
    pub path: String,
    pub archived: bool,
    pub session_id: Option<u64>,
}

/// One row of `GET /sessions`.
///
/// `checkout_key` is deliberately NOT a field here. D-09 was resolved to
/// Option B by the user on 2026-11-11: durable keys are served only from
/// `GET /rows`, and a client joins a `/rows` checkout entry to a `/sessions`
/// row on `session_id`, which equals `id` below. `/sessions` stays the flat
/// compatibility projection Phase 7 locked. The enforcement point is the test
/// `flat_session_api_remains_a_non_hierarchical_compatibility_projection` in
/// `api.rs`, which forbids `checkout_key` and seven sibling names on every
/// entry. Do not add the field on the strength of D-09's original wording.
#[derive(Serialize, Clone)]
pub struct SessionInfo {
    pub id: u64,
    pub name: String,
    pub title: Option<String>,
    pub status: &'static str,
    /// Which source decided `status`: "hook" / "session-file" / "silence".
    /// Surfaces a regression to the silence fallback (capture-but-render-lightly).
    pub state_source: &'static str,
    /// The last tool name Claude ran (from the hook event stream), if any.
    pub last_tool: Option<String>,
    /// Present while `Waiting` (blocked on us) or `Completed` (idle since a
    /// clean turn end) — how long Claude has been in that idle state.
    pub waiting_for_ms: Option<u64>,
    /// PERM-04 + three-state status: why the session is idle —
    /// `"permission"` (a pending tool-permission request, drives the
    /// distinct push + PWA card), `"input"` (a generic waiting prompt),
    /// `"completed"` (a clean `Stop`, calm — no push urgency), or `None` when
    /// active/busy. Derived from `meta.last_notification` + the resolved
    /// `Status` via `baude_core::permission::waiting_reason`.
    pub waiting_reason: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub context_used_pct: Option<u8>,
    /// The session's active 5-hour rate-limit window (used % + reset time),
    /// captured per-session from its statusline bridge file (`meta.rate_5h`).
    /// Flat optionals (not a nested object) so an older PWA/TUI client just
    /// ignores them and `#[serde(default)]` back-compat on `RemoteInfo` is free.
    pub rate_5h_used_pct: Option<u8>,
    pub rate_5h_resets_at_unix_s: Option<u64>,
    pub branch: Option<String>,
    pub cwd: String,
    pub repo_root: String,
    pub is_worktree: bool,
    pub gsd_milestone: Option<String>,
    pub gsd_phase: Option<String>,
    /// BL-03: the compact active phase (e.g. `4` → rendered `ph4`), mirroring
    /// what the TUI local line shows, so remote sessions + the PWA surface GSD
    /// state consistently with local sessions.
    pub gsd_active_phase: Option<String>,
    pub session_cost_usd: Option<f64>,
    pub claude_session_id: Option<String>,
    pub archived: bool,
    /// PERF-07: the child is SIGSTOPped under `idle_child_policy = "suspend"`.
    pub suspended: bool,
    /// A bounded (~30) tail of the session's recent hook events so the remote
    /// TUI overlay rides the existing `/sessions` poll without an extra round
    /// trip. The full ring is served by `GET /sessions/{id}/activity`.
    pub activity: Vec<HookEvent>,
}

/// PERM-02: an in-flight tool-permission request the `permission-mcp` bridge
/// POSTed, awaiting a human decision. `request_id` is bridge-generated; `ts` is
/// unix-ms (the bridge owns its own deadline). Serializable so `GET
/// /sessions/{id}/permission` returns it directly.
#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct PendingPermission {
    pub request_id: String,
    pub tool: String,
    pub input: serde_json::Value,
    pub ts: u64,
}

/// PERM-02: the human decision recorded for the most recent request. The
/// bridge's GET poll reads `decision` (`allow`|`deny`) to unblock.
///
/// WR-03: there is deliberately NO `scope` field — scope is not enforced in
/// v0.7. Carrying it here would imply a session-scoped-allow contract that does
/// not exist (every `tools/call` mints a fresh request). Enforcement is
/// deferred; the POST handler still accepts `scope` for forward-compat but
/// discards it.
#[derive(Serialize, Clone, Debug)]
pub struct PermissionDecision {
    pub request_id: String,
    pub decision: String,
    pub ts: u64,
}

/// `GET /sessions/{id}/permission` payload — the pending request (if any) plus
/// the resolved decision (if any). While pending, `decision` is `None`; after a
/// POST resolves it, `request_id`/`tool`/`input` describe the just-decided call
/// and `decision` carries the verdict for the bridge poll. `None`-everywhere
/// (no request ever) serializes to JSON `null` at the handler.
#[derive(Serialize)]
pub struct PermissionView {
    pub request_id: Option<String>,
    pub tool: Option<String>,
    pub input: Option<serde_json::Value>,
    pub ts: Option<u64>,
    /// `allow` | `deny` once resolved; absent while pending or idle.
    pub decision: Option<String>,
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Waiting => "waiting",
        Status::Completed => "completed",
        Status::Busy => "busy",
        Status::Exited => "exited",
    }
}

fn source_str(s: StateSource) -> &'static str {
    match s {
        StateSource::Hook => "hook",
        StateSource::SessionFile => "session-file",
        StateSource::Silence => "silence",
    }
}

/// The daemon's own loopback event endpoint for a session. `Manager` does not
/// store the daemon's bind addr (manager.rs has no bind field), and the hook
/// only needs same-host reachability, so we use the loopback default bind
/// (`DEFAULT_BIND = "127.0.0.1:8642"` in bauded/src/main.rs). Known limitation
/// (out of scope for Phase 2): a custom `--bind` port is NOT honored here —
/// honoring it would require threading the bind addr into `Manager`.
fn event_url(id: u64) -> String {
    format!("http://127.0.0.1:8642/sessions/{id}/event")
}

struct ManagerLifecycleEffects<'a, F> {
    manager: &'a mut Manager,
    effect: Option<F>,
}

impl<F> lifecycle::LifecycleEffects for ManagerLifecycleEffects<'_, F>
where
    F: FnOnce(&mut Manager, &lifecycle::LifecycleTraceEntry) -> Result<()>,
{
    type Error = anyhow::Error;

    fn persist_lifecycle(&mut self, candidate: &lifecycle::LifecycleCandidate) -> Result<()> {
        let before = self.manager.repository_state.clone();
        candidate.apply(&mut self.manager.repository_state)?;
        if let Err(error) = self.manager.save_checked() {
            if error.replacement_committed() {
                return Ok(());
            } else {
                self.manager.repository_state = before;
            }
            return Err(anyhow::Error::new(error));
        }
        Ok(())
    }

    fn apply_lifecycle_effect(&mut self, effect: &lifecycle::LifecycleTraceEntry) -> Result<()> {
        self.effect
            .take()
            .ok_or_else(|| anyhow!("lifecycle effect invoked more than once"))?(
            self.manager,
            effect,
        )
    }
}

/// BAUDED_AUTO_ARCHIVE_MIN env (minutes, 0 disables), then config.json
/// `auto_archive_minutes`, then 30.
pub fn default_auto_archive_ms() -> u64 {
    persist::load_config().auto_archive_ms()
}

/// BAUDE_IDLE_CHILD_POLICY env, then config.json `idle_child_policy`
/// (`keep` | `suspend` | `stop`), then `keep`.
pub fn default_idle_child_policy() -> persist::IdleChildPolicy {
    persist::load_config().idle_child_policy()
}

/// The command run per session: BAUDE_CLAUDE_CMD env, then config.json
/// `claude_cmd`, then plain `claude`.
/// The per-session base command, resolved PER BACKEND (claude:
/// BAUDE_CLAUDE_CMD/`claude_cmd`; opencode: BAUDE_OPENCODE_CMD/
/// `opencode_cmd`; then the backend default) — a configured claude_cmd must
/// never become an opencode spawn command.
pub fn default_claude_cmd() -> String {
    backend::command_for(backend::active(), &persist::load_config())
}

impl Manager {
    fn drive_lifecycle_effect<F>(
        &mut self,
        checkout: CheckoutKey,
        event: lifecycle::LifecycleEvent,
        effect: F,
    ) -> Result<lifecycle::LifecycleTransition>
    where
        F: FnOnce(&mut Manager, &lifecycle::LifecycleTraceEntry) -> Result<()>,
    {
        let mut saved = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .cloned()
            .ok_or_else(|| anyhow!("checkout {} is missing", checkout.get()))?;
        let adapter = ManagerLifecycleEffects {
            manager: self,
            effect: Some(effect),
        };
        let mut engine = lifecycle::LifecycleEngine::new(adapter);
        let transition = engine.drive(&mut saved, event)?;
        if transition.effects.is_empty() {
            bail!(
                "illegal lifecycle transition for checkout {}",
                checkout.get()
            );
        }
        Ok(transition)
    }

    #[cfg(test)]
    fn lifecycle_contract_trace(
        vector: lifecycle::CanonicalLifecycleVector,
        script: lifecycle::AdapterFailureScript,
    ) -> lifecycle::LifecycleContractResult {
        struct Effects {
            script: lifecycle::AdapterFailureScript,
            persists: usize,
            effects: usize,
        }
        impl lifecycle::LifecycleEffects for Effects {
            type Error = ();

            fn persist_lifecycle(
                &mut self,
                _candidate: &lifecycle::LifecycleCandidate,
            ) -> std::result::Result<(), Self::Error> {
                self.persists += 1;
                (self.script != lifecycle::AdapterFailureScript::Persist(self.persists))
                    .then_some(())
                    .ok_or(())
            }

            fn apply_lifecycle_effect(
                &mut self,
                _effect: &lifecycle::LifecycleTraceEntry,
            ) -> std::result::Result<(), Self::Error> {
                self.effects += 1;
                (self.script != lifecycle::AdapterFailureScript::Effect(self.effects))
                    .then_some(())
                    .ok_or(())
            }
        }

        let (initial, events) = lifecycle::canonical_lifecycle_events(vector);
        let mut keys = RepositoryState::default();
        let repository_key = keys.allocate_repository_key().expect("repository key");
        let key = keys.allocate_checkout_key().expect("checkout key");
        let mut checkout = SavedCheckout::new(
            key,
            repository_key,
            CheckoutRole::ManagedBranch,
            true,
            PersistedPath::from_path(Path::new("/manager-contract")),
            Some("refs/heads/contract".into()),
            1,
            initial,
            RetainedSessionState {
                name: "manager-contract".into(),
                cwd: PersistedPath::from_path(Path::new("/manager-contract")),
                repo_root: PersistedPath::from_path(Path::new("/manager-contract")),
                branch: Some("contract".into()),
                is_worktree: true,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
        );
        let mut engine = lifecycle::LifecycleEngine::new(Effects {
            script,
            persists: 0,
            effects: 0,
        });
        let mut failed = false;
        for event in events {
            if engine.drive(&mut checkout, event).is_err() {
                failed = true;
                break;
            }
        }
        lifecycle::LifecycleContractResult {
            trace: engine.trace().to_vec(),
            final_lifecycle: checkout.lifecycle().clone(),
            failed,
        }
    }

    pub fn new(claude_cmd: String, persist: bool) -> Manager {
        Manager {
            sessions: Vec::new(),
            next_id: 1,
            claude_cmd,
            persist,
            auto_archive_ms: default_auto_archive_ms(),
            idle_child_policy: default_idle_child_policy(),
            permission_notify: HashMap::new(),
            events: broadcast::channel(EVENTS_CAPACITY).0,
            repository_state: RepositoryState::default(),
            runtime_checkouts: HashMap::new(),
            runtime_standalones: HashMap::new(),
            repository_reservations: RepositoryReservations::default(),
            standalone_reservations: StandaloneReservations::default(),
            persistence_blocked: false,
            persistence_dirty: false,
            persistence_error: None,
            collisions: Vec::new(),
            #[cfg(test)]
            persistence_target_for_test: None,
            #[cfg(test)]
            atomic_failure_for_test: None,
            #[cfg(test)]
            atomic_failure_window_for_test: (0, usize::MAX),
            #[cfg(test)]
            spawn_error_for_test: None,
            #[cfg(test)]
            removal_pause_for_test: None,
            startup_timing: crate::timing::StartupTiming::new(0),
        }
    }

    /// Respawn every saved session with `claude --continue`. Returns how many
    /// came back.
    pub fn restore(&mut self) -> usize {
        if !self.persist {
            return 0;
        }
        let loaded = persist::load_for_workspace(
            STATE_BASE,
            baude_core::workspace::active(),
            reconcile_legacy_session,
        );
        let restored = self.restore_loaded(loaded);
        self.save();
        restored
    }

    #[cfg(test)]
    pub(crate) fn restore_at(
        &mut self,
        root: &Path,
        workspace: &baude_core::workspace::Workspace,
    ) -> usize {
        if !self.persist {
            return 0;
        }
        let loaded = persist::load_for_workspace_strict_at(
            root,
            STATE_BASE,
            workspace,
            reconcile_legacy_session,
        );
        let restored = self.restore_loaded(loaded);
        self.save_at(root, workspace);
        restored
    }

    fn restore_loaded(
        &mut self,
        loaded: std::result::Result<LoadOutcome, persist::LoadError>,
    ) -> usize {
        self.repository_state = match loaded {
            Ok(LoadOutcome::Missing) => RepositoryState::default(),
            Ok(LoadOutcome::Legacy(state) | LoadOutcome::Current(state)) => state.state,
            Err(error) => {
                self.persistence_blocked = true;
                self.persistence_error = Some(error.to_string());
                eprintln!(
                    "daemon persistence blocked: {error}; repair or move the named state file, then restart"
                );
                return 0;
            }
        };
        if let Err(error) = self.reconcile_teardown_recoveries() {
            eprintln!("teardown recovery: {error}");
        }
        if let Err(error) = self.reconcile_activation_recoveries() {
            eprintln!("activation recovery: {error}");
        }
        let checkouts: Vec<_> = self
            .repository_state
            .checkouts
            .iter()
            .filter(|checkout| {
                matches!(
                    checkout.lifecycle(),
                    CheckoutLifecycle::Active | CheckoutLifecycle::Launching(_)
                ) || (checkout.active_intent() && checkout.health() == &CheckoutHealth::Available)
            })
            .cloned()
            .collect();
        let mut restored = 0;
        for checkout in checkouts {
            match self.reopen_checkout(checkout.key) {
                Ok(LifecycleOutcome::Reopened { .. } | LifecycleOutcome::Focused { .. }) => {
                    restored += 1;
                }
                Ok(_) => {}
                Err(e) => eprintln!("restore {}: {e}", checkout.session.name),
            }
        }
        restored + self.restore_standalones()
    }

    fn reconcile_activation_recoveries(&mut self) -> MutationResult<()> {
        let recoveries: Vec<_> = self
            .repository_state
            .checkouts
            .iter()
            .filter_map(|checkout| {
                matches!(
                    checkout.health(),
                    CheckoutHealth::Unavailable(
                        UnavailableCause::PendingActivation { .. }
                            | UnavailableCause::ActivationRecovery { .. }
                    )
                )
                .then_some((checkout.repository_key, checkout.key))
            })
            .collect();
        if recoveries.is_empty() {
            return Ok(());
        }
        let before = self.repository_state.clone();
        for (repository, checkout) in recoveries {
            let _reservation = self
                .repository_reservations
                .reserve(repository)
                .map_err(|busy| anyhow!("{busy:?}"))?;
            lifecycle::reconcile_activation_recovery(&mut self.repository_state, checkout)
                .map_err(anyhow::Error::new)?;
        }
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(MutationError::Persistence(error));
        }
        Ok(())
    }

    fn reconcile_teardown_recoveries(&mut self) -> MutationResult<()> {
        let recoveries: Vec<_> = self
            .repository_state
            .checkouts
            .iter()
            .filter_map(|checkout| {
                matches!(
                    checkout.health(),
                    CheckoutHealth::Unavailable(UnavailableCause::TeardownPending { .. })
                )
                .then_some((checkout.repository_key, checkout.key))
            })
            .collect();
        if recoveries.is_empty() {
            return Ok(());
        }
        let before = self.repository_state.clone();
        for (repository, checkout) in recoveries {
            let _reservation = self
                .repository_reservations
                .reserve(repository)
                .map_err(|busy| anyhow!("{busy:?}"))?;
            lifecycle::reconcile_teardown_recovery(&mut self.repository_state, checkout)
                .map_err(anyhow::Error::new)?;
        }
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(MutationError::Persistence(error));
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn retry_teardown_recovery(
        &mut self,
        checkout: CheckoutKey,
    ) -> MutationResult<lifecycle::TeardownRecoveryResolution> {
        let repository = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .ok_or_else(|| anyhow!("checkout {} is missing", checkout.get()))?
            .repository_key;
        let before = self.repository_state.clone();
        let resolution = {
            let _reservation = self
                .repository_reservations
                .reserve(repository)
                .map_err(|busy| anyhow!("{busy:?}"))?;
            lifecycle::reconcile_teardown_recovery(&mut self.repository_state, checkout)
                .map_err(anyhow::Error::new)?
        };
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(MutationError::Persistence(error));
        }
        Ok(resolution)
    }

    #[allow(dead_code)]
    pub fn retry_activation_recovery(
        &mut self,
        checkout: CheckoutKey,
    ) -> MutationResult<lifecycle::ActivationRecoveryResolution> {
        let repository = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .ok_or_else(|| anyhow!("checkout {} is missing", checkout.get()))?
            .repository_key;
        let before = self.repository_state.clone();
        let resolution = {
            let _reservation = self
                .repository_reservations
                .reserve(repository)
                .map_err(|busy| anyhow!("{busy:?}"))?;
            lifecycle::reconcile_activation_recovery(&mut self.repository_state, checkout)
                .map_err(anyhow::Error::new)?
        };
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(MutationError::Persistence(error));
        }
        Ok(resolution)
    }

    pub fn save(&mut self) {
        if let Err(error) = self.save_checked() {
            eprintln!("save state: {error}");
        }
    }

    fn save_checked(&mut self) -> std::result::Result<(), persist::SaveError> {
        if !self.persist {
            return Ok(());
        }
        if self.persistence_blocked {
            return Err(persist::SaveError::before_replacement(anyhow!(
                "persistence is blocked after a state load failure"
            )));
        }
        let state = StateFile::new(self.state_for_save());
        #[cfg(test)]
        let failure = self.scheduled_atomic_failure_for_test();
        #[cfg(test)]
        let saved = if let Some((root, file)) = &self.persistence_target_for_test {
            persist::save_current_at_test(root, file, &state, failure, None)
        } else {
            persist::save_for_workspace_status(STATE_BASE, baude_core::workspace::active(), &state)
        };
        #[cfg(not(test))]
        let saved =
            persist::save_for_workspace_status(STATE_BASE, baude_core::workspace::active(), &state);
        match saved {
            Ok(()) => {
                self.persistence_dirty = false;
                self.persistence_error = None;
                Ok(())
            }
            Err(error) => {
                self.persistence_dirty = true;
                self.persistence_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn save_removal_revocation(&mut self) -> std::result::Result<(), persist::SaveError> {
        let result = self.save_checked();
        #[cfg(test)]
        if result.is_err() {
            self.atomic_failure_for_test = None;
        }
        result
    }

    pub fn persistence_status(&self) -> PersistenceStatus {
        PersistenceStatus {
            enabled: self.persist,
            blocked: self.persistence_blocked,
            dirty: self.persistence_dirty,
            error: self.persistence_error.clone(),
        }
    }

    #[cfg(test)]
    fn save_at(&mut self, root: &Path, workspace: &baude_core::workspace::Workspace) {
        if !self.persist || self.persistence_blocked {
            return;
        }
        let file = workspace.state_file(STATE_BASE);
        if let Err(error) =
            persist::save_current_at(root, &file, &StateFile::new(self.state_for_save()))
        {
            self.persistence_dirty = true;
            self.persistence_error = Some(error.to_string());
            eprintln!("save state: {error}");
        } else {
            self.persistence_dirty = false;
            self.persistence_error = None;
        }
    }

    fn state_for_save(&self) -> RepositoryState {
        let mut state = self.repository_state.clone();
        merge_live_runtime_metadata(
            &mut state,
            &self.survivor_runtime_overrides(),
            &self.standalone_runtime_overrides(),
        );
        state
    }

    /// The live metadata of every standalone row that owns a runtime, the
    /// standalone counterpart of [`Self::survivor_runtime_overrides`].
    fn standalone_runtime_overrides(&self) -> Vec<(StandaloneKey, RetainedStandaloneSessionState)> {
        let mut overrides = Vec::new();
        for row in &self.repository_state.standalone_sessions {
            let Some(runtime_id) = self.runtime_standalones.get(&row.key) else {
                continue;
            };
            let Some(session) = self
                .sessions
                .iter()
                .find(|session| session.id == *runtime_id)
            else {
                continue;
            };
            overrides.push((row.key, Self::standalone_snapshot(session, row)));
        }
        overrides
    }

    /// What a standalone row retains of its live session.
    fn standalone_snapshot(
        session: &Session,
        row: &SavedStandaloneSession,
    ) -> RetainedStandaloneSessionState {
        RetainedStandaloneSessionState {
            name: session.name.clone(),
            shell_open: session.shell_open,
            archived: session.archived,
            archived_by_user: session.archived_by_user,
            resume_id: session
                .meta
                .session_id
                .clone()
                .or_else(|| row.session.resume_id.clone()),
            ever_launched: row.session.ever_launched,
        }
    }

    /// The live metadata of every checkout that owns a runtime, in the shape
    /// the state file retains it. Taken as an owned snapshot so the removal
    /// save closure can apply it while core holds `&mut self.repository_state`.
    fn survivor_runtime_overrides(&self) -> Vec<(CheckoutKey, RetainedSessionState)> {
        let mut overrides = Vec::new();
        for checkout in &self.repository_state.checkouts {
            let Some(runtime_id) = self.runtime_checkouts.get(&checkout.key) else {
                continue;
            };
            let Some(session) = self
                .sessions
                .iter()
                .find(|session| session.id == *runtime_id)
            else {
                continue;
            };
            overrides.push((
                checkout.key,
                RetainedSessionState {
                    name: session.name.clone(),
                    cwd: PersistedPath::from_path(&session.cwd),
                    repo_root: PersistedPath::from_path(&session.repo_root),
                    branch: session.branch.clone(),
                    is_worktree: session.is_worktree,
                    shell_open: session.shell_open,
                    archived: session.archived,
                    archived_by_user: session.archived_by_user,
                    resume_id: session
                        .meta
                        .session_id
                        .clone()
                        .or_else(|| checkout.session.resume_id.clone()),
                },
            ));
        }
        overrides
    }

    /// `POST /removals/preview`: what removing `target` would do.
    pub fn preview_removal(
        &self,
        target: RemovalTarget,
    ) -> std::result::Result<RemovalReport, RemovalError> {
        // No reservation: a preview is a read of the stored rows under the
        // caller's lock. It may run while another removal is in its slow
        // stage, when the doomed rows are still stored, and then reads them
        // as they were; executing such a preview meets that removal's
        // reservation and answers busy. `begin_removal` recomputes the
        // report under its own lock and core's `commit` re-selects its rows,
        // so neither trusts a preview.
        removal::preview_removal(&self.repository_state, target)
    }

    /// `POST /removals`, stage one, under the manager lock: refuse a blocked
    /// persistence, reserve the target's repository or standalone row,
    /// recompute the report and compare it with the previewed one, move the
    /// rows' live sessions out of the registry and run core's `prepare`.
    ///
    /// The returned value owns the reservation, the moved runtime handles
    /// and the prepared set. The caller drops the lock, runs
    /// [`RemovalInFlight::stop_and_delete`] without it, and hands both back
    /// to [`Self::finish_removal`] under the lock again. The reservation is
    /// held from before the recheck until `finish_removal` has published,
    /// which is what keeps every other mutation of these rows out of the
    /// unlocked window.
    ///
    /// The Phase 18 force path, not the close protocol: nothing here routes
    /// through `Manager::remove` or `teardown_retained_runtime`.
    pub fn begin_removal(
        &mut self,
        previewed: &RemovalReport,
    ) -> std::result::Result<RemovalInFlight, RemoveRowsError> {
        if self.persistence_blocked {
            return Err(RemoveRowsError::Blocked(
                persist::SaveError::before_replacement(anyhow!(
                    "persistence is blocked after a state load failure"
                )),
            ));
        }
        let target = previewed.target;
        let repository_reservation = match self.removal_target_repository(target) {
            Some(repository) => Some(
                self.repository_reservations
                    .reserve(repository)
                    .map_err(|_| RemoveRowsError::Busy(repository))?,
            ),
            None => None,
        };
        // A standalone row has no repository to reserve, so the daemon's own
        // registry refuses a second removal of the same folder. Like the
        // repository guard, it is carried in the in-flight value.
        let standalone_reservation = match target {
            RemovalTarget::Standalone(key) => Some(
                self.standalone_reservations
                    .reserve(key)
                    .map_err(|()| RemoveRowsError::FolderBusy(key))?,
            ),
            RemovalTarget::Repository(_) | RemovalTarget::Checkout(_) => None,
        };
        // The report is recomputed under the lock. Only what the run would
        // do is compared; `cleared` and `already_removed` are filesystem
        // probes that would make the recheck flap on advisory state.
        let fresh = self
            .preview_removal(target)
            .map_err(RemoveRowsError::Removal)?;
        if fresh.rows_removed != previewed.rows_removed
            || fresh.runtimes_killed != previewed.runtimes_killed
            || fresh.paths_deleted != previewed.paths_deleted
        {
            return Err(RemoveRowsError::Drift(Box::new(fresh)));
        }
        // Snapshot while the removed rows' sessions are still registered;
        // the merge at save time skips them because their rows are gone from
        // the candidate.
        let overrides = self.survivor_runtime_overrides();
        let standalone_overrides = self.standalone_runtime_overrides();
        // `rows()` merges live metadata from the registry, and the doomed
        // rows' sessions are about to leave it until the commit. Writing
        // their live metadata into the stored rows first keeps `GET /rows`
        // serving them unchanged through the window. It is what the next
        // save would write for them anyway, so a removal that does not
        // commit leaves nothing a save would not have.
        self.merge_doomed_rows_live_metadata(&fresh, &overrides, &standalone_overrides);
        let runtimes = self.collect_row_runtimes(&fresh);
        let consumed: Vec<u64> = runtimes.iter().map(|runtime| runtime.session.id).collect();
        let prepared = match removal::prepare(&self.repository_state, target, &runtimes) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.abandon_removal(runtimes, &error);
                return Err(RemoveRowsError::Removal(error));
            }
        };
        Ok(RemovalInFlight {
            prepared,
            runtimes,
            consumed,
            overrides,
            standalone_overrides,
            #[cfg(test)]
            pause: self.removal_pause_for_test.clone(),
            _repository_reservation: repository_reservation,
            _standalone_reservation: standalone_reservation,
        })
    }

    /// `POST /removals`, stage three, under the manager lock again: commit
    /// what the unlocked stage did, or put everything back.
    ///
    /// A panic or an error from the slow stage, and every error from core's
    /// `commit`, return every runtime handle to the registry and publish
    /// nothing; `repository_state` is untouched on all of them. Only a
    /// committed save reaches the one `publish`. The reservation the
    /// in-flight value carries is released when this returns, after that
    /// publish.
    pub fn finish_removal(
        &mut self,
        inflight: RemovalInFlight,
        stage2: SlowStageOutcome,
    ) -> std::result::Result<RemovalReport, RemoveRowsError> {
        let RemovalInFlight {
            prepared,
            mut runtimes,
            consumed,
            overrides,
            standalone_overrides,
            #[cfg(test)]
                pause: _,
            _repository_reservation,
            _standalone_reservation,
        } = inflight;
        let effects = match stage2 {
            Ok(Ok(effects)) => effects,
            Ok(Err(error)) => {
                self.abandon_removal(runtimes, &error);
                return Err(RemoveRowsError::Removal(error));
            }
            Err(panic) => {
                // The handles were never moved into the panicking closure, so
                // every one of them is still here to put back.
                let message = panic_message(panic.as_ref());
                self.restore_row_runtimes(runtimes);
                #[cfg(test)]
                {
                    self.atomic_failure_for_test = None;
                }
                return Err(RemoveRowsError::SlowStagePanicked(message));
            }
        };
        let persist_enabled = self.persist;
        #[cfg(test)]
        let test_target = self.persistence_target_for_test.clone();
        #[cfg(test)]
        let atomic_failure = self.atomic_failure_for_test;
        let save = move |file: &StateFile| -> std::result::Result<(), persist::SaveError> {
            if !persist_enabled {
                return Ok(());
            }
            // `file` is core's candidate and must stay untouched: core assigns
            // it to the live state after this returns. The merge is applied to
            // a copy, so only the bytes on disk carry the live metadata.
            let mut merged = file.state.clone();
            merge_live_runtime_metadata(&mut merged, &overrides, &standalone_overrides);
            let merged = StateFile::new(merged);
            #[cfg(test)]
            if let Some((root, file_name)) = &test_target {
                return persist::save_current_at_test(
                    root,
                    file_name,
                    &merged,
                    atomic_failure,
                    None,
                );
            }
            persist::save_for_workspace_status(STATE_BASE, baude_core::workspace::active(), &merged)
        };
        let result = removal::commit(
            &mut self.repository_state,
            prepared,
            effects,
            &mut runtimes,
            save,
        );
        let report = match result {
            Ok(report) => report,
            Err(error) => {
                // A persist failure, `SelectionChanged` or a validation
                // refusal: core left `repository_state` untouched and every
                // handle in the Vec, so the rows and their registry entries
                // all stand.
                self.abandon_removal(runtimes, &error);
                return Err(RemoveRowsError::Removal(error));
            }
        };
        for row in &report.rows_removed {
            match row {
                RowKey::Checkout(key) => {
                    self.runtime_checkouts.remove(key);
                }
                RowKey::Standalone(key) => {
                    self.runtime_standalones.remove(key);
                }
                RowKey::Repository(_) => {}
            }
        }
        // Handles the core did not consume stay registered.
        let restored: Vec<u64> = runtimes.iter().map(|runtime| runtime.session.id).collect();
        self.restore_row_runtimes(runtimes);
        for id in consumed.iter().filter(|id| !restored.contains(id)) {
            // Wake before forgetting, so no long-poll hangs on a row that no
            // longer exists.
            if let Some(notify) = self.permission_notify.remove(id) {
                notify.notify_waiters();
            }
        }
        match &report.persistence {
            PersistenceOutcome::Saved => {
                self.persistence_dirty = false;
                self.persistence_error = None;
            }
            PersistenceOutcome::SavedDegraded { detail } => {
                self.persistence_dirty = true;
                self.persistence_error = Some(detail.clone());
            }
            // A real run never reports a planned outcome.
            PersistenceOutcome::Planned => {}
        }
        // The caller holds the manager mutex across this whole method, the
        // commit lock scope, so no reader can see the row after the event is
        // queued: core's commit replaced the state above under the same
        // guard. That is a property of this code position, not something a
        // test observes. Core's durable save has returned, which covers
        // `SavedDegraded`: the replacement committed. No other exit of this
        // method or of `begin_removal` publishes.
        self.publish(DaemonEvent::RowsRemoved(report.clone()));
        Ok(report)
    }

    /// Merge the live metadata of the rows `report` removes, and of no other
    /// row, into `repository_state`, through the one merge implementation.
    fn merge_doomed_rows_live_metadata(
        &mut self,
        report: &RemovalReport,
        overrides: &[(CheckoutKey, RetainedSessionState)],
        standalone_overrides: &[(StandaloneKey, RetainedStandaloneSessionState)],
    ) {
        let doomed: Vec<(CheckoutKey, RetainedSessionState)> = overrides
            .iter()
            .filter(|(key, _)| report.rows_removed.contains(&RowKey::Checkout(*key)))
            .cloned()
            .collect();
        let doomed_standalone: Vec<(StandaloneKey, RetainedStandaloneSessionState)> =
            standalone_overrides
                .iter()
                .filter(|(key, _)| report.rows_removed.contains(&RowKey::Standalone(*key)))
                .cloned()
                .collect();
        merge_live_runtime_metadata(&mut self.repository_state, &doomed, &doomed_standalone);
    }

    /// Put a removal that will not commit back: every handle returns to the
    /// registry, the test save failure is cleared, and a persist failure
    /// marks persistence degraded, exactly as one failed save does.
    fn abandon_removal(&mut self, runtimes: Vec<RowRuntime>, error: &RemovalError) {
        self.restore_row_runtimes(runtimes);
        #[cfg(test)]
        {
            self.atomic_failure_for_test = None;
        }
        if matches!(error, RemovalError::Persist { .. }) {
            self.persistence_dirty = true;
            self.persistence_error = Some(error.to_string());
        }
    }

    /// A receiver for every event published from now on. Nothing published
    /// earlier is replayed.
    pub fn subscribe_events(&self) -> broadcast::Receiver<DaemonEvent> {
        self.events.subscribe()
    }

    fn publish(&self, event: DaemonEvent) {
        // The only send error is "no receiver", which is the normal state of
        // a daemon nobody is watching.
        let _ = self.events.send(event);
    }

    /// Send `event` on the real sender, so a test can overflow a receiver the
    /// `GET /events` handler owns without sixty-five live removals.
    #[cfg(test)]
    pub(crate) fn publish_for_test(&self, event: DaemonEvent) {
        self.publish(event);
    }

    /// The repository whose reservation a removal of `target` holds.
    fn removal_target_repository(&self, target: RemovalTarget) -> Option<RepositoryKey> {
        match target {
            RemovalTarget::Repository(key) => Some(key),
            RemovalTarget::Checkout(key) => self
                .repository_state
                .checkouts
                .iter()
                .find(|checkout| checkout.key == key)
                .map(|checkout| checkout.repository_key),
            RemovalTarget::Standalone(_) => None,
        }
    }

    /// Move the live `Session` of every checkout and standalone row in
    /// `report` out of the registry into the shape core demands. The
    /// `runtime_checkouts` and `runtime_standalones` entries stay until core
    /// returns `Ok`, so a handle can be put straight back. A repository row
    /// carries no runtime of its own.
    fn collect_row_runtimes(&mut self, report: &RemovalReport) -> Vec<RowRuntime> {
        let mut runtimes = Vec::new();
        for row in &report.rows_removed {
            let registered = match row {
                RowKey::Checkout(key) => self.runtime_checkouts.get(key).copied(),
                RowKey::Standalone(key) => self.runtime_standalones.get(key).copied(),
                RowKey::Repository(_) => None,
            };
            let Some(runtime_id) = registered else {
                continue;
            };
            let Some(index) = self
                .sessions
                .iter()
                .position(|session| session.id == runtime_id)
            else {
                continue;
            };
            runtimes.push(RowRuntime {
                row: *row,
                session: self.sessions.remove(index),
            });
        }
        runtimes
    }

    /// Return every handle core did not consume to the registry, in id order.
    fn restore_row_runtimes(&mut self, runtimes: Vec<RowRuntime>) {
        for runtime in runtimes {
            let session = runtime.session;
            let index = self
                .sessions
                .iter()
                .position(|existing| existing.id > session.id)
                .unwrap_or(self.sessions.len());
            self.sessions.insert(index, session);
        }
    }

    /// The observed main worktree of a repository, for a busy refusal's body.
    pub fn repository_path(&self, repository: RepositoryKey) -> Option<PathBuf> {
        self.repository_state
            .repositories
            .iter()
            .find(|saved| saved.key == repository)
            .map(|saved| saved.observed_main_worktree.to_path_buf())
    }

    /// The canonical path a standalone row stores, for a busy refusal's body.
    pub fn standalone_path(&self, key: StandaloneKey) -> Option<PathBuf> {
        self.repository_state
            .standalone_session(key)
            .map(|row| row.canonical_path.to_path_buf())
    }

    /// The refusal for a reserved repository, its body naming the stored main
    /// checkout, the same body a busy `POST /removals` answers with.
    fn repository_busy(&self, repository: RepositoryKey) -> MutationError {
        MutationError::Busy(match self.repository_path(repository) {
            Some(path) => format!("repository busy: {}", path.display()),
            None => format!("repository busy: {repository:?}"),
        })
    }

    /// The refusal for session `id` when the row it runs in is reserved: its
    /// checkout's repository or its standalone folder. The id is resolved
    /// through `runtime_checkouts` and `runtime_standalones`, which keep their
    /// entries while a removal holds the session, so a session a removal has
    /// moved out of `sessions` still answers busy rather than "no session".
    fn session_busy(&self, id: u64) -> Option<MutationError> {
        let checkout = self
            .runtime_checkouts
            .iter()
            .find_map(|(key, runtime)| (*runtime == id).then_some(*key));
        if let Some(checkout) = checkout {
            let repository = self
                .repository_state
                .checkouts
                .iter()
                .find(|saved| saved.key == checkout)?
                .repository_key;
            return self
                .repository_reservations
                .is_reserved(repository)
                .then(|| self.repository_busy(repository));
        }
        let key = self.standalone_owner(id)?;
        self.standalone_reservations
            .is_reserved(key)
            .then(|| self.folder_busy(key))
    }

    /// The refusal for a reserved standalone row, its body naming the folder.
    fn folder_busy(&self, key: StandaloneKey) -> MutationError {
        MutationError::Busy(match self.standalone_path(key) {
            Some(path) => format!("folder busy: {}", path.display()),
            None => format!("folder busy: {key:?}"),
        })
    }

    /// `GET /rows` — the hierarchy projection. A pure read: it admits nothing,
    /// allocates no key and saves nothing.
    pub fn rows(&self) -> RowsView {
        // The durable rows with live session metadata merged in, the same view
        // every save writes. `repository_state` alone is stale for a row that
        // owns a runtime: archiving or renaming a live session reaches it only
        // at the next save, and an archived row must never read as active.
        let state = &self.state_for_save();
        let mut repositories: Vec<&SavedRepository> = state.repositories.iter().collect();
        repositories.sort_by_key(|repository| repository.first_seen_order);
        let repositories = repositories
            .into_iter()
            .map(|repository| {
                let mut checkouts: Vec<&SavedCheckout> = state
                    .checkouts
                    .iter()
                    .filter(|checkout| checkout.repository_key == repository.key)
                    .collect();
                checkouts.sort_by_key(|checkout| checkout.first_seen_order);
                let path = repository.observed_main_worktree.to_path_buf();
                RepositoryRowView {
                    repository_key: repository.key.get(),
                    // A root path has no file name; its full form is the name.
                    display_name: path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    ),
                    path: path.display().to_string(),
                    health: repository.health.clone(),
                    checkouts: checkouts
                        .into_iter()
                        .map(|checkout| CheckoutRowView {
                            checkout_key: checkout.key.get(),
                            repository_key: checkout.repository_key.get(),
                            display_name: checkout.session.name.clone(),
                            branch: short_branch_name(checkout.observed_branch.as_deref()),
                            path: checkout.observed_path.to_path_buf().display().to_string(),
                            role: checkout.role,
                            managed_by_baude: checkout.managed_by_baude,
                            archived: checkout.session.archived,
                            session_id: self.session_id_for_checkout(checkout.key),
                        })
                        .collect(),
                }
            })
            .collect();
        let mut standalone: Vec<_> = state.standalone_sessions.iter().collect();
        standalone.sort_by_key(|session| session.first_seen_order);
        let standalone = standalone
            .into_iter()
            .map(|session| StandaloneRowView {
                standalone_key: session.key.get(),
                display_name: session.session.name.clone(),
                path: session.canonical_path.to_path_buf().display().to_string(),
                archived: session.session.archived,
                session_id: self.session_id_for_standalone(session.key),
            })
            .collect();
        RowsView {
            repositories,
            standalone,
        }
    }

    /// The live daemon session id of a checkout row, from the same registry
    /// `POST /removals` resolves runtime handles through.
    fn session_id_for_checkout(&self, key: CheckoutKey) -> Option<u64> {
        self.runtime_checkouts.get(&key).copied()
    }

    /// The live daemon session id of a standalone row, by the same rule.
    fn session_id_for_standalone(&self, key: StandaloneKey) -> Option<u64> {
        self.runtime_standalones.get(&key).copied()
    }

    /// The stored `observed_branch` of the checkout with `key`, the full ref
    /// the state file is written from, so a test can assert it without
    /// parsing the state file's layout.
    #[cfg(test)]
    pub(crate) fn stored_observed_branch_for_test(&self, key: CheckoutKey) -> Option<String> {
        self.repository_state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == key)
            .and_then(|checkout| checkout.observed_branch.clone())
    }

    /// Overwrite the stored role of the checkout with `key`, to reproduce
    /// the stored shape an earlier daemon wrote: before 21-09 every admitted
    /// row, the main checkout included, was stored as `ManagedBranch`. Not a
    /// general mutator. It does not save, and it touches neither the managed
    /// flag nor any runtime.
    #[cfg(test)]
    pub(crate) fn set_checkout_role_for_test(&mut self, key: CheckoutKey, role: CheckoutRole) {
        if let Some(checkout) = self
            .repository_state
            .checkouts
            .iter_mut()
            .find(|checkout| checkout.key == key)
        {
            checkout.role = role;
        }
    }

    #[cfg(test)]
    pub(crate) fn checkout_key_for_runtime_for_test(&self, id: u64) -> Option<CheckoutKey> {
        self.runtime_checkouts
            .iter()
            .find_map(|(key, runtime)| (*runtime == id).then_some(*key))
    }

    /// Drop the registry entry that maps `key` to its live session, and
    /// nothing else. Reproduces the stale runtime row a daemon is left with
    /// when restore could not reopen a checkout that still records an owned
    /// runtime: the row keeps its runtime, the daemon holds no handle for it.
    /// The process keeps running, the `Session` stays in `self.sessions`,
    /// `repository_state` is untouched and nothing is saved.
    #[cfg(test)]
    pub(crate) fn forget_runtime_handle_for_test(&mut self, key: CheckoutKey) {
        self.runtime_checkouts.remove(&key);
    }

    /// Take the real reservation of repository `key`, so a test can hold it
    /// across a request and drive `RemoveRowsError::Busy` by its cause. The
    /// guard owns its share of the reservation table and outlives the manager
    /// lock; this adds no second locking mechanism.
    #[cfg(test)]
    pub(crate) fn reserve_repository_for_test(
        &self,
        key: RepositoryKey,
    ) -> lifecycle::RepositoryReservation {
        self.repository_reservations
            .reserve(key)
            .unwrap_or_else(|_| panic!("repository {key:?} is already reserved"))
    }

    /// Test-only: park every removal from now on at the start of its slow
    /// stage, after prepare and before the kill, until the returned gate is
    /// released. See [`RemovalPauseForTest`].
    #[cfg(test)]
    pub(crate) fn pause_removals_for_test(&mut self) -> RemovalPauseForTest {
        let pause = RemovalPauseForTest::default();
        self.removal_pause_for_test = Some(pause.clone());
        pause
    }

    /// Take the real reservation of standalone row `key`, the folder
    /// counterpart of `reserve_repository_for_test`. The guard owns its share
    /// of the table and outlives the manager lock.
    #[cfg(test)]
    pub(crate) fn reserve_standalone_for_test(&self, key: StandaloneKey) -> StandaloneReservation {
        self.standalone_reservations
            .reserve(key)
            .unwrap_or_else(|()| panic!("standalone {key:?} is already reserved"))
    }

    /// Store and launch what a daemon before 21-10 stored and launched for a
    /// folder git does not report as a repository, so a test can stand on
    /// that shape. The allocation is the one `record_checkout_intent` made
    /// before 21-10 removed it: a repository row whose common directory and
    /// main worktree are the folder and whose health is unavailable for the
    /// not-a-repository cause, holding one unmanaged `ManagedBranch` checkout
    /// at the folder. No route reaches this.
    #[cfg(test)]
    pub(crate) fn create_pre_plan_folder_session_for_test(
        &mut self,
        folder: &Path,
        name: Option<&str>,
    ) -> MutationResult<u64> {
        let state_before = self.repository_state.clone();
        let base = name.map(str::to_owned).unwrap_or_else(|| {
            folder
                .file_name()
                .map(|part| part.to_string_lossy().into_owned())
                .unwrap_or_else(|| folder.display().to_string())
        });
        let session_name = self.unique_name(&base);
        let allocated = (|| -> Result<CheckoutKey> {
            let key = self.repository_state.allocate_repository_key()?;
            let first_seen_order = self.repository_state.allocate_first_seen_order()?;
            self.repository_state.repositories.push(SavedRepository {
                key,
                observed_common_dir: PersistedPath::from_path(folder),
                observed_main_worktree: PersistedPath::from_path(folder),
                first_seen_order,
                health: RepositoryHealth::Unavailable(UnavailableCause::NotRepository),
                physical_key: key.get().to_string(),
            });
            let checkout_key = self.repository_state.allocate_checkout_key()?;
            let first_seen_order = self.repository_state.allocate_first_seen_order()?;
            self.repository_state.checkouts.push(SavedCheckout::new(
                checkout_key,
                key,
                CheckoutRole::ManagedBranch,
                false,
                PersistedPath::from_path(folder),
                None,
                first_seen_order,
                CheckoutLifecycle::Active,
                RetainedSessionState {
                    name: session_name.clone(),
                    cwd: PersistedPath::from_path(folder),
                    repo_root: PersistedPath::from_path(folder),
                    branch: None,
                    is_worktree: false,
                    shell_open: false,
                    archived: false,
                    archived_by_user: false,
                    resume_id: None,
                },
            ));
            Ok(checkout_key)
        })();
        let checkout_key = match allocated {
            Ok(key) => key,
            Err(error) => {
                self.repository_state = state_before;
                return Err(error.into());
            }
        };
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = state_before;
            }
            return Err(MutationError::Persistence(error));
        }
        let id = self.spawn_retained_with_mode(
            checkout_key,
            folder.to_path_buf(),
            folder.to_path_buf(),
            None,
            false,
            Some(&session_name),
            backend::SpawnMode::Fresh,
            false,
        )?;
        self.runtime_checkouts.insert(checkout_key, id);
        Ok(id)
    }

    /// `POST /sessions` — spawn a fresh session in `repo`, optionally in a
    /// managed worktree for `worktree` (branch name).
    ///
    /// An explicit admission is the one thing that lifts a suppression
    /// entry (D-11). Every arm of `admit` that returns a session has stored
    /// its row durably and launched it by then, so the lift runs here, once.
    /// The key is the checkout root of the path that row stores,
    /// `suppression::lift_key`, the same key the TUI lifts: core keyed the
    /// entry on the checkout path of the row it removed, never on a request
    /// string, and a row stored on a subfolder names its checkout's entry
    /// through that root. An arm that returns an error lifts nothing, and a
    /// non-persisting manager stores no row and lifts nothing. Restore,
    /// reopen, the restart route and the legacy migration never come
    /// through here.
    pub fn create(
        &mut self,
        repo: &str,
        worktree: Option<&str>,
        name: Option<&str>,
    ) -> MutationResult<SessionInfo> {
        let info = self.admit(repo, worktree, name)?;
        if self.persist {
            if let Some(admitted) = self.stored_row_path(info.id) {
                self.lift_admission_suppression(&admitted);
            }
        }
        Ok(info)
    }

    /// The path stored on the row that owns live session `id`: a standalone
    /// row's `canonical_path` or a checkout row's `observed_path`.
    fn stored_row_path(&self, id: u64) -> Option<PathBuf> {
        if let Some(key) = self.standalone_owner(id) {
            return self
                .repository_state
                .standalone_sessions
                .iter()
                .find(|row| row.key == key)
                .map(|row| row.canonical_path.to_path_buf());
        }
        let key = self
            .runtime_checkouts
            .iter()
            .find_map(|(key, runtime)| (*runtime == id).then_some(*key))?;
        self.repository_state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == key)
            .map(|checkout| checkout.observed_path.to_path_buf())
    }

    /// Forget the suppression entry for the checkout root of `admitted`, the
    /// stored row path, as `suppression::lift_key` names it for both
    /// clients. Advisory: a store error is logged with the path and never
    /// fails the admission that asked for it. It reads no row and writes no
    /// state of its own.
    fn lift_admission_suppression(&self, admitted: &Path) {
        let key = suppression::lift_key(admitted);
        if let Err(error) = suppression::lift(&persist::config_dir(), &key) {
            eprintln!("lift suppression {}: {error}", key.display());
        }
    }

    fn admit(
        &mut self,
        repo: &str,
        worktree: Option<&str>,
        name: Option<&str>,
    ) -> MutationResult<SessionInfo> {
        if self.persistence_blocked {
            return Err(persist::SaveError::before_replacement(anyhow!(
                "daemon persistence is blocked after a state load failure"
            ))
            .into());
        }
        // Guarded: a `~`-prefixed `repo` in a request body would otherwise
        // stat the developer's real home from inside this crate's test binary.
        let repo = persist::expand_tilde(repo);
        let repo = repo.canonicalize().unwrap_or(repo);
        if !repo.is_dir() {
            return Err(anyhow!("not a directory: {}", repo.display()).into());
        }
        if let Some(branch) = worktree {
            let outcome = self.activate_branch_worktree(&repo, branch, name)?;
            let runtime = match outcome {
                LifecycleOutcome::Created {
                    runtime: Some(runtime),
                    ..
                }
                | LifecycleOutcome::Activated {
                    runtime: Some(runtime),
                    ..
                }
                | LifecycleOutcome::Reused {
                    runtime: Some(runtime),
                    ..
                }
                | LifecycleOutcome::Focused { runtime, .. } => runtime,
                LifecycleOutcome::Busy { repository } => {
                    return Err(self.repository_busy(repository))
                }
                _ => return Err(anyhow!("activation produced no runtime").into()),
            };
            return Ok(self
                .info(runtime)
                .expect("activation runtime just resolved"));
        }
        let (cwd, repo_root, branch, is_worktree) = match worktree {
            Some(_) => unreachable!("worktree activation returned above"),
            None => {
                let root = git::repo_root(&repo).unwrap_or_else(|| repo.clone());
                (repo, root, None, false)
            }
        };
        if self.persist {
            let discovery = git::discover_repository(&cwd);
            let route = route_plain_admission(&self.repository_state, &cwd, &discovery);
            // What core validation holds a checkout's retained session to:
            // its repository path is the repository's main worktree, and it
            // is a worktree exactly when it is not that main worktree. Both
            // come from discovery. `git rev-parse --show-toplevel` answers
            // with the linked worktree itself, which core rejects, so a
            // worktree the user made by hand could not be opened by path.
            let (repo_root, is_worktree) = match &discovery {
                Ok(snapshot) => (
                    snapshot.main_worktree.clone(),
                    snapshot.selected_worktree.path != snapshot.main_worktree,
                ),
                Err(_) => (repo_root, is_worktree),
            };
            let join = match &route {
                PlainAdmission::ReuseStandalone(key) => {
                    // A row a removal holds is neither handed out nor
                    // reopened: a reopen would spawn a runtime the removal's
                    // commit then drops from the registry.
                    if self.standalone_reservations.is_reserved(*key) {
                        return Err(self.folder_busy(*key));
                    }
                    // One row per folder. A live row is the same session; a
                    // closed one is reopened under its key. Either way the
                    // request's name is not applied.
                    let id = self.reopen_standalone(*key)?;
                    return Ok(self.info(id).expect("reopened standalone session"));
                }
                PlainAdmission::Refuse(text) => {
                    return Err(MutationError::Domain(anyhow!("{text}")));
                }
                PlainAdmission::Standalone => {
                    self.ensure_runtime_capacity(&cwd, &repo_root, &route)?;
                    let id = self.admit_standalone(&cwd, name)?;
                    return Ok(self.info(id).expect("session just spawned"));
                }
                PlainAdmission::Discovered => None,
                PlainAdmission::JoinStored(key) => Some(*key),
            };
            // The stored repository this admission adds a checkout to: the
            // joined row, or the row whose common directory discovery found.
            // It is reserved until the admission returns, so no admission
            // adds a child to a repository a removal holds. Only this plain
            // path reserves here; the worktree path above took its own
            // reservation inside `activate_branch_worktree` and has returned.
            let joined = match (&route, &discovery) {
                (PlainAdmission::JoinStored(key), _) => Some(*key),
                (PlainAdmission::Discovered, Ok(snapshot)) => {
                    let common = PersistedPath::from_path(&snapshot.common_dir);
                    self.repository_state
                        .repositories
                        .iter()
                        .find(|repository| repository.observed_common_dir == common)
                        .map(|repository| repository.key)
                }
                _ => None,
            };
            let _reservation = match joined {
                Some(repository) => match self.repository_reservations.reserve(repository) {
                    Ok(reservation) => Some(reservation),
                    Err(_) => return Err(self.repository_busy(repository)),
                },
                None => None,
            };
            self.ensure_runtime_capacity(&cwd, &repo_root, &route)?;
            let state_before = self.repository_state.clone();
            let dir_name = |path: &Path| {
                path.file_name()
                    .map(|part| part.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            };
            let base = name.map(str::to_owned).unwrap_or_else(|| match &branch {
                Some(branch) => format!("{}:{branch}", dir_name(&repo_root)),
                None => dir_name(&cwd),
            });
            let session_name = self.unique_name(&base);
            let checkout_key = self.record_checkout_intent(
                &cwd,
                &repo_root,
                branch.clone(),
                is_worktree,
                session_name.clone(),
                join,
            )?;
            if let Err(error) = self.save_checked() {
                if !error.replacement_committed() {
                    self.repository_state = state_before;
                }
                return Err(MutationError::Persistence(error));
            }
            let id = self.spawn_retained_with_mode(
                checkout_key,
                cwd,
                repo_root,
                branch,
                is_worktree,
                Some(&session_name),
                backend::SpawnMode::Fresh,
                false,
            )?;
            self.runtime_checkouts.insert(checkout_key, id);
            return Ok(self.info(id).expect("session just spawned"));
        }
        let id = self.spawn(cwd, repo_root, branch, is_worktree, name, false)?;
        Ok(self.info(id).expect("session just spawned"))
    }

    fn activate_branch_worktree(
        &mut self,
        repository_child: &Path,
        branch: &str,
        name: Option<&str>,
    ) -> MutationResult<LifecycleOutcome> {
        if self.persistence_dirty || self.repository_state.has_pending_activation() {
            return Err(anyhow!(
                "repository lifecycle is blocked while pending ownership is not durable; repair persistence and save before retrying"
            )
            .into());
        }
        let snapshot = git::discover_repository(repository_child).map_err(anyhow::Error::new)?;
        let state_before = self.repository_state.clone();
        let mut next = self.repository_state.clone();
        let prepared = lifecycle::prepare_activation(&mut next, &snapshot, branch)
            .map_err(anyhow::Error::new)?;
        let pending_checkout = prepared.checkout;

        // Track collision report if detected
        if let Some(ref collision_report) = prepared.collision {
            self.collisions.push(collision_report.clone());
        }

        lifecycle::record_pending_activation(&mut next, &snapshot, &prepared)
            .map_err(anyhow::Error::new)?;
        let repository = prepared.request.repository;
        let _reservation = match self.repository_reservations.reserve(repository) {
            Ok(reservation) => reservation,
            Err(busy) => return Ok(busy),
        };
        self.repository_state = next.clone();
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = state_before;
            }
            return Err(MutationError::Persistence(error));
        }
        let activation = match lifecycle::execute_activation(&mut next, repository_child, prepared)
        {
            Ok(activation) => activation,
            Err(error) if error.recovery_child_recorded() => {
                self.repository_state = next;
                if let Err(save_error) = self.save_checked() {
                    return Err(anyhow!(
                        "{error}; recovery ownership persistence failed: {save_error}"
                    )
                    .into());
                }
                return Err(anyhow::Error::new(error).into());
            }
            Err(error) => {
                lifecycle::clear_pending_activation(&mut next, pending_checkout);
                self.repository_state = next;
                if let Err(save_error) = self.save_checked() {
                    return Err(anyhow!(
                        "{error}; clearing pending activation ownership failed: {save_error}"
                    )
                    .into());
                }
                return Err(anyhow::Error::new(error).into());
            }
        };
        if let Some(name) = name {
            if let Some(checkout) = next
                .checkouts
                .iter_mut()
                .find(|checkout| checkout.key == activation.checkout)
            {
                checkout.session.name = name.to_owned();
            }
        }
        self.repository_state = next;
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                if let Err(compensation) = lifecycle::compensate_uncommitted_activation(&activation)
                {
                    lifecycle::mark_activation_recovery(
                        &mut self.repository_state,
                        activation.checkout,
                        activation.branch.clone(),
                        Some(matches!(
                            activation.disposition,
                            lifecycle::ActivationDisposition::Created
                        )),
                        error.to_string(),
                        compensation.to_string(),
                    )
                    .map_err(anyhow::Error::new)?;
                    let recovery_save = self.save_checked();
                    return Err(anyhow!(
                        "{} failed: {error}; {} failed: {compensation}; recovery persistence: {}",
                        lifecycle::CreationFailureStage::PersistenceBeforeReplacement,
                        lifecycle::CreationFailureStage::Compensation,
                        recovery_save
                            .map(|()| "saved".to_owned())
                            .unwrap_or_else(|save| save.to_string())
                    )
                    .into());
                }
                self.repository_state = state_before;
                if let Err(clear_error) = self.save_checked() {
                    return Err(anyhow!(
                        "activation persistence failed: {error}; activation compensated but pending ownership cleanup failed: {clear_error}"
                    )
                    .into());
                }
            }
            return Err(MutationError::Persistence(error));
        }

        if let Some(runtime) = self.runtime_checkouts.get(&activation.checkout).copied() {
            if let Some(exited) = self
                .sessions
                .iter()
                .find(|session| session.id == runtime)
                .map(|session| session.claude.is_exited())
            {
                if exited {
                    drop(_reservation);
                    return self.reopen_checkout(activation.checkout);
                }
                return Ok(LifecycleOutcome::Focused {
                    checkout: activation.checkout,
                    runtime,
                });
            }
        }
        let checkout = self
            .repository_state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == activation.checkout)
            .cloned()
            .ok_or_else(|| anyhow!("activated checkout is missing"))?;
        let id = self
            .spawn_retained_with_mode(
                activation.checkout,
                activation.path.clone(),
                activation.main_worktree.clone(),
                Some(activation.branch.clone()),
                activation.path != activation.main_worktree,
                Some(&checkout.session.name),
                backend::SpawnMode::Fresh,
                false,
            )
            .map_err(|error| {
                anyhow!(
                    "{} failed after durable activation: {error}",
                    lifecycle::CreationFailureStage::Spawn
                )
            })?;
        self.runtime_checkouts.insert(activation.checkout, id);
        Ok(activation.outcome(Some(id)))
    }

    /// The role a newly admitted checkout row is stored with.
    ///
    /// `Main` when all three hold: git discovery succeeded for the session's
    /// working directory, the discovered worktree is the repository's main
    /// worktree, and no checkout row of that repository already holds
    /// `Main`. `ManagedBranch` in every other case, including a directory
    /// git discovery returned nothing for. The third condition is required,
    /// not cosmetic: `RepositoryState::validate` rejects a second `Main` in
    /// one repository, so a second plain session on the same main checkout
    /// would otherwise fail every later save. `PrimaryDefault` is never
    /// assigned here.
    fn admitted_checkout_role(
        &self,
        repository_key: RepositoryKey,
        snapshot: Option<&git::RepositorySnapshot>,
    ) -> CheckoutRole {
        let is_main_worktree = snapshot
            .is_some_and(|snapshot| snapshot.selected_worktree.path == snapshot.main_worktree);
        let main_role_taken = self.repository_state.checkouts.iter().any(|checkout| {
            checkout.repository_key == repository_key && checkout.role == CheckoutRole::Main
        });
        if is_main_worktree && !main_role_taken {
            CheckoutRole::Main
        } else {
            CheckoutRole::ManagedBranch
        }
    }

    fn record_checkout_intent(
        &mut self,
        cwd: &Path,
        repo_root: &Path,
        branch: Option<String>,
        is_worktree: bool,
        name: String,
        join: Option<RepositoryKey>,
    ) -> Result<CheckoutKey> {
        let snapshot = git::discover_repository(cwd).ok();
        let repository_key = if let Some(key) = join {
            // The router matched a stored row that already holds this path;
            // the checkout joins that repository with no lookup.
            key
        } else {
            // A folder that stopped being a repository between routing and
            // recording is refused: this function never stores a repository
            // row for a path git does not report as one.
            let Some(discovered) = snapshot.as_ref() else {
                bail!("{} is no longer a git repository", cwd.display());
            };
            let common = PersistedPath::from_path(&discovered.common_dir);
            if let Some(key) = self
                .repository_state
                .repositories
                .iter()
                .find(|repository| repository.observed_common_dir == common)
                .map(|repository| repository.key)
            {
                key
            } else {
                let key = self.repository_state.allocate_repository_key()?;
                let first_seen_order = self.repository_state.allocate_first_seen_order()?;
                self.repository_state.repositories.push(SavedRepository {
                    key,
                    observed_common_dir: common,
                    observed_main_worktree: PersistedPath::from_path(&discovered.main_worktree),
                    first_seen_order,
                    health: RepositoryHealth::Available,
                    physical_key: key.get().to_string(),
                });
                key
            }
        };
        let role = self.admitted_checkout_role(repository_key, snapshot.as_ref());
        let checkout_key = self.repository_state.allocate_checkout_key()?;
        let first_seen_order = self.repository_state.allocate_first_seen_order()?;
        self.repository_state.checkouts.push(SavedCheckout::new(
            checkout_key,
            repository_key,
            role,
            // Never managed: a plain admission opens a directory the user
            // already has, so removing its row must keep that directory.
            // Whether the directory is a linked worktree is a separate
            // fact, recorded on the retained session below.
            false,
            PersistedPath::from_path(cwd),
            snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.selected_worktree.branch.clone()),
            first_seen_order,
            CheckoutLifecycle::Active,
            RetainedSessionState {
                name,
                cwd: PersistedPath::from_path(cwd),
                repo_root: PersistedPath::from_path(repo_root),
                branch,
                is_worktree,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
        ));
        Ok(checkout_key)
    }

    /// A standalone key and a first-seen order, allocated together.
    fn allocate_standalone_slot(state: &mut RepositoryState) -> Result<(StandaloneKey, u64)> {
        let key = state.allocate_standalone_key()?;
        let first_seen_order = state.allocate_first_seen_order()?;
        Ok((key, first_seen_order))
    }

    /// Admit `canonical`, a folder git does not report as a repository, as
    /// one standalone row and launch its session. The row is saved before
    /// anything is spawned and rolled back when that save commits nothing.
    fn admit_standalone(&mut self, canonical: &Path, name: Option<&str>) -> MutationResult<u64> {
        let state_before = self.repository_state.clone();
        let base = name.map(str::to_owned).unwrap_or_else(|| {
            canonical
                .file_name()
                .map(|part| part.to_string_lossy().into_owned())
                .unwrap_or_else(|| canonical.display().to_string())
        });
        let session_name = self.unique_name(&base);
        let (key, first_seen_order) =
            match Self::allocate_standalone_slot(&mut self.repository_state) {
                Ok(slot) => slot,
                Err(error) => {
                    self.repository_state = state_before;
                    return Err(error.into());
                }
            };
        self.repository_state
            .standalone_sessions
            .push(SavedStandaloneSession::new(
                key,
                PersistedPath::from_path(canonical),
                first_seen_order,
                StandaloneLifecycle::Active,
                None,
                RetainedStandaloneSessionState {
                    name: session_name.clone(),
                    shell_open: false,
                    archived: false,
                    archived_by_user: false,
                    resume_id: None,
                    ever_launched: false,
                },
            ));
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = state_before;
            }
            return Err(MutationError::Persistence(error));
        }
        let id = self.spawn_with_mode_internal(
            Some(RuntimeOwner::Standalone(key)),
            canonical.to_path_buf(),
            canonical.to_path_buf(),
            None,
            false,
            Some(&session_name),
            backend::SpawnMode::Fresh,
            false,
        )?;
        self.runtime_standalones.insert(key, id);
        Ok(id)
    }

    /// The standalone row that owns live session `id`, if one does.
    fn standalone_owner(&self, id: u64) -> Option<StandaloneKey> {
        self.runtime_standalones
            .iter()
            .find_map(|(key, runtime_id)| (*runtime_id == id).then_some(*key))
    }

    /// Save a standalone transition. A save that never replaced the file
    /// rolls the in-memory rows back to `before`, so memory and disk agree.
    fn save_standalone_change(&mut self, before: RepositoryState) -> MutationResult<()> {
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(MutationError::Persistence(error));
        }
        Ok(())
    }

    /// Record that a standalone row's runtime could not be confirmed
    /// stopped. Best effort: the teardown error is the one the caller
    /// reports, and a row left `Stopping` is recovered the same way.
    fn protect_standalone_teardown(&mut self, key: StandaloneKey, generation: RuntimeGeneration) {
        let before = self.repository_state.clone();
        if let Some(row) = self.repository_state.standalone_session_mut(key) {
            row.set_lifecycle(StandaloneLifecycle::ProtectedTeardown(generation));
        }
        if let Err(error) = self.save_standalone_change(before) {
            eprintln!(
                "standalone {}: could not persist protected teardown: {error}",
                key.get()
            );
        }
    }

    /// Close the live session `id` of standalone row `key`. The runtime is
    /// ended and the row stays, `Inactive` with no owned runtime. A session
    /// that has already exited takes the same path: nothing is left to end,
    /// and its ownership is cleared.
    fn close_standalone(&mut self, key: StandaloneKey, id: u64) -> MutationResult<()> {
        let index = self
            .sessions
            .iter()
            .position(|session| session.id == id)
            .ok_or_else(|| anyhow!("no session {id}"))?;
        let row = self
            .repository_state
            .standalone_session(key)
            .ok_or_else(|| anyhow!("standalone {} is missing", key.get()))?;
        let owned = row
            .owned_runtime()
            .cloned()
            .ok_or_else(|| anyhow!("standalone runtime has no durable ownership"))?;
        let session = &self.sessions[index];
        let snapshot = Self::standalone_snapshot(session, row);
        let shell_matches = match (&owned.shell, &session.shell) {
            (ShellOwnership::Closed, None) => true,
            (ShellOwnership::Owned(expected), Some(shell)) => shell.process_identity() == expected,
            _ => false,
        };
        if session.claude.process_identity() != &owned.agent || !shell_matches {
            self.protect_standalone_teardown(key, owned.generation);
            return Err(MutationError::Teardown(anyhow!(
                "standalone runtime identity no longer matches durable ownership"
            )));
        }

        // Intent first: a daemon that dies between here and the final save
        // restores a `Stopping` row, ends the recorded runtime and does not
        // relaunch it.
        let before = self.repository_state.clone();
        let row = self
            .repository_state
            .standalone_session_mut(key)
            .expect("standalone resolved above");
        row.session = snapshot;
        row.set_lifecycle(StandaloneLifecycle::Stopping(owned.generation));
        if let Err(error) = self.save_checked() {
            // A save that replaced the file but could not confirm durability
            // is committed: the close goes on and reports it at the end, as
            // a checkout's close does.
            if !error.replacement_committed() {
                self.repository_state = before;
                return Err(MutationError::Persistence(error));
            }
        }

        if let Err(error) = self.sessions[index].kill_and_wait() {
            self.protect_standalone_teardown(key, owned.generation);
            return Err(MutationError::Teardown(anyhow::Error::new(error)));
        }

        let before = self.repository_state.clone();
        self.repository_state
            .standalone_session_mut(key)
            .expect("standalone resolved above")
            .set_runtime_state(StandaloneLifecycle::Inactive, None);
        let saved = self.save_checked();
        if let Err(error) = &saved {
            if !error.replacement_committed() {
                // Disk still says `Stopping`, so memory does too. The stopped
                // session stays registered and the close is retryable.
                self.repository_state = before;
                return Err(MutationError::Persistence(saved.unwrap_err()));
            }
        }
        self.sessions.retain(|session| session.id != id);
        self.runtime_standalones.remove(&key);
        if let Some(notify) = self.permission_notify.remove(&id) {
            notify.notify_waiters();
        }
        saved.map_err(MutationError::Persistence)
    }

    /// End the runtime a standalone row still records and the daemon holds
    /// no handle for, which is the state a restart finds, then leave the row
    /// in `successor` with no owned runtime. The runtime is ended by its
    /// recorded process identity, never by a bare pid. A row that records no
    /// runtime only takes the transition.
    fn end_recorded_standalone_runtime(
        &mut self,
        key: StandaloneKey,
        successor: StandaloneLifecycle,
    ) -> MutationResult<()> {
        let recorded = self
            .repository_state
            .standalone_session(key)
            .ok_or_else(|| anyhow!("standalone {} is missing", key.get()))?
            .owned_runtime()
            .cloned();
        if let Some(runtime) = recorded {
            let generation = runtime.generation;
            let shell = match runtime.shell {
                ShellOwnership::Closed => None,
                ShellOwnership::Owned(identity) => Some(identity),
            };
            if let Err(error) = baude_core::session::finish_recorded_teardown(
                Some(runtime.agent.pid),
                shell.as_ref().map(|identity| identity.pid),
                Some(runtime.agent),
                shell,
                false,
                false,
            ) {
                self.protect_standalone_teardown(key, generation);
                return Err(MutationError::Teardown(anyhow::Error::new(error)));
            }
        }
        let before = self.repository_state.clone();
        self.repository_state
            .standalone_session_mut(key)
            .expect("standalone resolved above")
            .set_runtime_state(successor, None);
        self.save_standalone_change(before)
    }

    /// Bring standalone row `key` to a live session and return its id. A row
    /// whose session is live returns that session and changes nothing. Every
    /// other row has whatever runtime it still records ended first, so no
    /// path leaves a row recording a runtime the daemon does not hold.
    fn reopen_standalone(&mut self, key: StandaloneKey) -> MutationResult<u64> {
        if let Some(id) = self.runtime_standalones.get(&key).copied() {
            match self.sessions.iter().find(|session| session.id == id) {
                Some(session) if !session.claude.is_exited() => return Ok(id),
                Some(_) => self.close_standalone(key, id)?,
                None => {
                    self.runtime_standalones.remove(&key);
                }
            }
        }
        let saved = self
            .repository_state
            .standalone_session(key)
            .cloned()
            .ok_or_else(|| anyhow!("standalone {} is missing", key.get()))?;
        if saved.owned_runtime().is_some() {
            self.end_recorded_standalone_runtime(key, StandaloneLifecycle::Active)?;
        }
        let path = saved.canonical_path.to_path_buf();
        let canonical = match path.canonicalize() {
            Ok(canonical) if canonical.is_dir() && canonical == path => canonical,
            _ => {
                let before = self.repository_state.clone();
                self.repository_state
                    .standalone_session_mut(key)
                    .expect("standalone cloned above")
                    .set_runtime_state(StandaloneLifecycle::Missing, None);
                self.save_standalone_change(before)?;
                return Err(anyhow!("folder is missing: {}", path.display()).into());
            }
        };
        let before = self.repository_state.clone();
        let row = self
            .repository_state
            .standalone_session_mut(key)
            .expect("standalone cloned above");
        row.set_runtime_state(StandaloneLifecycle::Active, None);
        // The resume rule. A row that was never launched starts fresh. A
        // launched one resumes its retained conversation when the backend
        // still has it in the canonical folder and continues otherwise; a
        // stored id with nothing behind it is cleared in the same save that
        // marks the row active.
        let mode = if row.session.ever_launched {
            lifecycle::resume_mode(&mut row.session.resume_id, |id| {
                backend::active().resume_target_exists(&canonical, id)
            })
        } else {
            backend::SpawnMode::Fresh
        };
        self.save_standalone_change(before)?;
        let id = self
            .spawn_with_mode_internal(
                Some(RuntimeOwner::Standalone(key)),
                canonical.clone(),
                canonical,
                None,
                false,
                Some(&saved.session.name),
                mode,
                saved.session.shell_open,
            )
            .map_err(MutationError::from_lifecycle)?;
        if let Ok(session) = self.session_mut(id) {
            session.name = saved.session.name;
            session.archived = saved.session.archived;
            session.archived_by_user = saved.session.archived_by_user;
        }
        self.runtime_standalones.insert(key, id);
        Ok(id)
    }

    /// Bring the stored standalone rows back after a daemon start and return
    /// how many were relaunched. Rows that were live are reopened, which ends
    /// any recorded runtime first. Rows caught mid-close have their recorded
    /// runtime ended and stay closed. Closed, missing and unreadable rows are
    /// not touched. One row's failure does not stop the others. This never
    /// creates a row and never lifts a suppression.
    fn restore_standalones(&mut self) -> usize {
        let mut rows: Vec<_> = self
            .repository_state
            .standalone_sessions
            .iter()
            .map(|row| {
                (
                    row.first_seen_order,
                    row.key,
                    row.lifecycle().clone(),
                    row.session.name.clone(),
                )
            })
            .collect();
        rows.sort_by_key(|(order, ..)| *order);
        let mut restored = 0;
        for (_, key, lifecycle, name) in rows {
            match lifecycle {
                StandaloneLifecycle::Active
                | StandaloneLifecycle::Launching(_)
                | StandaloneLifecycle::Running(_) => match self.reopen_standalone(key) {
                    Ok(_) => restored += 1,
                    Err(error) => eprintln!("restore {name}: {error}"),
                },
                StandaloneLifecycle::Stopping(_) | StandaloneLifecycle::ProtectedTeardown(_) => {
                    if let Err(error) =
                        self.end_recorded_standalone_runtime(key, StandaloneLifecycle::Inactive)
                    {
                        eprintln!("restore {name}: {error}");
                    }
                }
                StandaloneLifecycle::Inactive
                | StandaloneLifecycle::Missing
                | StandaloneLifecycle::Io(_) => {}
            }
        }
        restored
    }

    /// Record a registered launch on the row that owns it, durably, before
    /// the launch is released.
    fn register_runtime(&mut self, owner: RuntimeOwner, runtime: OwnedRuntime) -> Result<()> {
        match owner {
            RuntimeOwner::Checkout(checkout) => {
                self.drive_lifecycle_effect(
                    checkout,
                    lifecycle::LifecycleEvent::LaunchRegistered(runtime),
                    |_, _| Ok(()),
                )?;
            }
            RuntimeOwner::Standalone(key) => {
                let before = self.repository_state.clone();
                self.repository_state
                    .standalone_session_mut(key)
                    .ok_or_else(|| anyhow!("standalone {} is missing", key.get()))?
                    .set_runtime_state(
                        StandaloneLifecycle::Launching(runtime.generation),
                        Some(runtime),
                    );
                if let Err(error) = self.save_checked() {
                    if !error.replacement_committed() {
                        self.repository_state = before;
                    }
                    return Err(anyhow::Error::new(error));
                }
            }
        }
        Ok(())
    }

    /// Record a released launch on the row that owns it, durably.
    fn release_runtime(&mut self, owner: RuntimeOwner) -> Result<()> {
        match owner {
            RuntimeOwner::Checkout(checkout) => {
                self.drive_lifecycle_effect(
                    checkout,
                    lifecycle::LifecycleEvent::LaunchReleased,
                    |_, _| Ok(()),
                )?;
            }
            RuntimeOwner::Standalone(key) => {
                let before = self.repository_state.clone();
                let generation = self
                    .repository_state
                    .standalone_session(key)
                    .and_then(SavedStandaloneSession::owned_runtime)
                    .map(|runtime| runtime.generation)
                    .ok_or_else(|| anyhow!("standalone launch has no owned runtime"))?;
                let row = self
                    .repository_state
                    .standalone_session_mut(key)
                    .expect("standalone checked above");
                row.set_lifecycle(StandaloneLifecycle::Running(generation));
                row.session.ever_launched = true;
                if let Err(error) = self.save_checked() {
                    if !error.replacement_committed() {
                        self.repository_state = before;
                    }
                    return Err(anyhow::Error::new(error));
                }
            }
        }
        Ok(())
    }

    /// Return a standalone row whose launch failed to `lifecycle` with no
    /// owned runtime, durably. A row that is already gone is left alone.
    fn recover_standalone_ownership(
        &mut self,
        key: StandaloneKey,
        lifecycle: StandaloneLifecycle,
    ) -> Result<()> {
        let before = self.repository_state.clone();
        let Some(row) = self.repository_state.standalone_session_mut(key) else {
            return Ok(());
        };
        row.set_runtime_state(lifecycle, None);
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(anyhow::Error::new(error));
        }
        Ok(())
    }

    fn spawn(
        &mut self,
        cwd: PathBuf,
        repo_root: PathBuf,
        branch: Option<String>,
        is_worktree: bool,
        name: Option<&str>,
        resume: bool,
    ) -> Result<u64> {
        let mode = if resume {
            backend::SpawnMode::ContinueLatest
        } else {
            backend::SpawnMode::Fresh
        };
        self.spawn_with_mode_internal(None, cwd, repo_root, branch, is_worktree, name, mode, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_retained_with_mode(
        &mut self,
        checkout: CheckoutKey,
        cwd: PathBuf,
        repo_root: PathBuf,
        branch: Option<String>,
        is_worktree: bool,
        name: Option<&str>,
        mode: backend::SpawnMode,
        shell_open: bool,
    ) -> Result<u64> {
        self.spawn_with_mode_internal(
            Some(RuntimeOwner::Checkout(checkout)),
            cwd,
            repo_root,
            branch,
            is_worktree,
            name,
            mode,
            shell_open,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_with_mode_internal(
        &mut self,
        owner: Option<RuntimeOwner>,
        cwd: PathBuf,
        repo_root: PathBuf,
        branch: Option<String>,
        is_worktree: bool,
        name: Option<&str>,
        mode: backend::SpawnMode,
        shell_open: bool,
    ) -> Result<u64> {
        #[cfg(test)]
        if let Some(error) = &self.spawn_error_for_test {
            bail!("test spawn failure: {error}");
        }
        let dir_name = |p: &Path| {
            p.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.to_string_lossy().to_string())
        };
        let base = match name {
            Some(n) => n.to_string(),
            None => match &branch {
                Some(b) => format!("{}:{}", dir_name(&repo_root), b),
                None => dir_name(&cwd),
            },
        };
        let name = self.unique_name(&base);

        let id = self.next_id;

        let be = backend::active();

        // Wire the session's actual cwd (worktree dir for worktree sessions)
        // so the daemon-spawned CLI reports back to baude: for Claude that's
        // the `.claude/settings.local.json` hook seed plus the prompt-mode
        // `.mcp.json` — best-effort, idempotent, non-clobbering. Idempotency
        // matters because `restore()` re-spawns every persisted session on
        // each daemon startup. HREG-03/D-02: warnings surface on stderr (the
        // `save state:` precedent — bauded has no logging crate) and never
        // block the spawn (D-04); a repeat per re-spawn is the signal, so no
        // dedup across the restore loop.
        for warning in be.prepare_cwd(&cwd) {
            eprintln!("seed warning: {warning}");
        }

        // PERM-01: resolve the permission flag (default skip preserves today's
        // unattended `--dangerously-skip-permissions`; `prompt` is opt-in via
        // BAUDE_PERMISSION_MODE). Applied to the base cmd BEFORE the `export …;
        // {inner}` wrap so the flag survives the `--continue || exec` resume
        // fallback (WR-01). BL-04: prompt mode strips a conflicting skip flag
        // from `claude_cmd` and warns, so an operator's skip default no longer
        // silently suppresses prompt mode.
        let resolved = be.resolve_cmd(&self.claude_cmd);
        if resolved.stripped_skip {
            eprintln!(
                "baude: prompt mode active — stripped --dangerously-skip-permissions \
                 from claude_cmd so the permission prompt can fire (BL-04)"
            );
        }

        let targeted_resume = matches!(&mode, backend::SpawnMode::ResumeId(_));
        let plan = be.spawn_plan(&resolved.cmd, Some(&event_url(id)), mode);
        let generation = owner
            .and_then(|owner| {
                let owned = match owner {
                    RuntimeOwner::Checkout(key) => self
                        .repository_state
                        .checkouts
                        .iter()
                        .find(|saved| saved.key == key)
                        .and_then(SavedCheckout::owned_runtime),
                    RuntimeOwner::Standalone(key) => self
                        .repository_state
                        .standalone_session(key)
                        .and_then(SavedStandaloneSession::owned_runtime),
                };
                owned.and_then(|runtime| runtime.generation.successor())
            })
            .unwrap_or(RuntimeGeneration::initial());
        let mut registered_shell = None;
        let launched = if let Some(owner) = owner {
            Pty::spawn_registered_with(Some(&plan.cmd), &plan.env, &cwd, ROWS, COLS, |agent| {
                if shell_open {
                    let shell =
                        Pty::spawn_registered_with(None, &[], &cwd, ROWS, COLS, |identity| {
                            self.register_runtime(
                                owner,
                                OwnedRuntime {
                                    generation,
                                    agent: agent.clone(),
                                    shell: ShellOwnership::Owned(identity.clone()),
                                },
                            )?;
                            Ok(())
                        })?;
                    registered_shell = Some(shell);
                } else {
                    self.register_runtime(
                        owner,
                        OwnedRuntime {
                            generation,
                            agent: agent.clone(),
                            shell: ShellOwnership::Closed,
                        },
                    )?;
                }
                Ok(())
            })
        } else {
            Pty::spawn_with_env(Some(&plan.cmd), &plan.env, &cwd, ROWS, COLS)
        };
        let mut claude = match launched {
            Ok(claude) => claude,
            Err(error) => {
                // A failed launch returns a standalone row to `Active` with
                // no owned runtime. A checkout's failed launch is unchanged.
                if let Some(RuntimeOwner::Standalone(key)) = owner {
                    if let Some(shell) = &mut registered_shell {
                        let _ = shell.kill_and_wait();
                    }
                    if let Err(recovery) =
                        self.recover_standalone_ownership(key, StandaloneLifecycle::Active)
                    {
                        return Err(anyhow!(
                            "{error}; standalone launch cleanup failed: {recovery}"
                        ));
                    }
                }
                return Err(error);
            }
        };
        if let Some(owner) = owner {
            if let Err(error) = self.release_runtime(owner) {
                let mut stopped = claude.kill_and_wait().is_ok();
                if let Some(shell) = &mut registered_shell {
                    stopped &= shell.kill_and_wait().is_ok();
                }
                // The launch that was just stopped is still on the standalone
                // row: `Launching` when the save replaced nothing, `Running`
                // when it replaced the file and could not confirm it. Return
                // the row to `Active` with no owned runtime, as the failed
                // launch above does. A stop that could not be confirmed keeps
                // the record, which is what lets a later reopen end that
                // runtime by its identity. The save failure stays the error
                // the caller gets; a cleanup that cannot be saved either
                // leaves the record for that same reopen.
                if let (true, RuntimeOwner::Standalone(key)) = (stopped, owner) {
                    if let Err(recovery) =
                        self.recover_standalone_ownership(key, StandaloneLifecycle::Active)
                    {
                        eprintln!(
                            "standalone {}: could not persist launch cleanup: {recovery}",
                            key.get()
                        );
                    }
                }
                return Err(error);
            }
        }
        let mut meta = ClaudeMeta::default();
        meta.backend_port = plan.server_port;

        self.next_id += 1;
        self.sessions.push(Session {
            id,
            name,
            cwd,
            repo_root,
            branch,
            is_worktree,
            claude,
            shell: registered_shell,
            shell_open,
            spawn_unix_ms: now_unix_ms(),
            targeted_resume,
            meta,
            archived: false,
            archived_by_user: false,
            was_busy: false,
            unarchived_at_ms: None,
            pending_permission: None,
            permission_decision: None,
            child_suspended: false,
            pane_focus_shell: false,
            poll_meta_calls_for_test: std::cell::Cell::new(0),
        });
        Ok(id)
    }

    fn unique_name(&self, base: &str) -> String {
        if !self.sessions.iter().any(|s| s.name == base) {
            return base.to_string();
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base} ({n})");
            if !self.sessions.iter().any(|s| s.name == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    #[cfg(test)]
    fn record_runtime(&mut self, id: u64) -> Result<()> {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return Ok(());
        };
        let snapshot = git::discover_repository(&session.cwd).ok();
        let common = snapshot
            .as_ref()
            .map(|snapshot| PersistedPath::from_path(&snapshot.common_dir))
            .unwrap_or_else(|| PersistedPath::from_path(&session.repo_root));
        let existing_repository_key = self
            .repository_state
            .repositories
            .iter()
            .find(|repository| repository.observed_common_dir == common)
            .map(|repository| repository.key);
        let repository_key = match existing_repository_key {
            Some(key) => key,
            None => {
                let key = self.repository_state.allocate_repository_key()?;
                let first_seen_order = self.repository_state.allocate_first_seen_order()?;
                self.repository_state.repositories.push(SavedRepository {
                    key,
                    observed_common_dir: common,
                    observed_main_worktree: PersistedPath::from_path(
                        snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.main_worktree.as_path())
                            .unwrap_or(&session.repo_root),
                    ),
                    first_seen_order,
                    health: if snapshot.is_some() {
                        RepositoryHealth::Available
                    } else {
                        RepositoryHealth::Unavailable(UnavailableCause::NotRepository)
                    },
                    physical_key: key.get().to_string(),
                });
                key
            }
        };
        let role = self.admitted_checkout_role(repository_key, snapshot.as_ref());
        let checkout_key = self.repository_state.allocate_checkout_key()?;
        let first_seen_order = self.repository_state.allocate_first_seen_order()?;
        self.repository_state.checkouts.push(SavedCheckout::new(
            checkout_key,
            repository_key,
            role,
            session.is_worktree,
            PersistedPath::from_path(&session.cwd),
            snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.selected_worktree.branch.clone()),
            first_seen_order,
            if snapshot.is_some() {
                CheckoutLifecycle::Active
            } else {
                CheckoutLifecycle::Protected(UnavailableCause::NotRepository)
            },
            RetainedSessionState {
                name: session.name.clone(),
                cwd: PersistedPath::from_path(&session.cwd),
                repo_root: PersistedPath::from_path(&session.repo_root),
                branch: session.branch.clone(),
                is_worktree: session.is_worktree,
                shell_open: session.shell_open,
                archived: session.archived,
                archived_by_user: session.archived_by_user,
                resume_id: None,
            },
        ));
        self.runtime_checkouts.insert(checkout_key, id);
        Ok(())
    }

    /// Refuse an admission whose durable counters are exhausted, before
    /// anything is stored or spawned. Allocation happens on a clone, so the
    /// live state is never advanced here.
    fn ensure_runtime_capacity(
        &self,
        cwd: &Path,
        repo_root: &Path,
        route: &PlainAdmission,
    ) -> Result<()> {
        let mut state = self.repository_state.clone();
        if matches!(route, PlainAdmission::Standalone) {
            Self::allocate_standalone_slot(&mut state)?;
            return Ok(());
        }
        let snapshot = git::discover_repository(cwd).ok();
        let common = snapshot
            .as_ref()
            .map(|snapshot| PersistedPath::from_path(&snapshot.common_dir))
            .unwrap_or_else(|| PersistedPath::from_path(repo_root));
        // A joined admission adds no repository row.
        if !matches!(route, PlainAdmission::JoinStored(_))
            && !state
                .repositories
                .iter()
                .any(|repository| repository.observed_common_dir == common)
        {
            state.allocate_repository_key()?;
            state.allocate_first_seen_order()?;
        }
        state.allocate_checkout_key()?;
        state.allocate_first_seen_order()?;
        Ok(())
    }

    // Phase 6 establishes daemon-owner parity internally; the network entrypoint
    // is deliberately deferred to Phase 8.
    #[cfg_attr(not(test), allow(dead_code))]
    fn retained_runtime_snapshot(&self, id: u64) -> Result<RetainedSessionState> {
        let session = self.session(id)?;
        let durable_resume_id = self
            .runtime_checkouts
            .iter()
            .find_map(|(key, runtime)| (*runtime == id).then_some(*key))
            .and_then(|key| {
                self.repository_state
                    .checkouts
                    .iter()
                    .find(|checkout| checkout.key == key)
                    .and_then(|checkout| checkout.session.resume_id.clone())
            });
        Ok(RetainedSessionState {
            name: session.name.clone(),
            cwd: PersistedPath::from_path(&session.cwd),
            repo_root: PersistedPath::from_path(&session.repo_root),
            branch: session.branch.clone(),
            is_worktree: session.is_worktree,
            shell_open: session.shell_open,
            archived: session.archived,
            archived_by_user: session.archived_by_user,
            resume_id: session.meta.session_id.clone().or(durable_resume_id),
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn teardown_retained_runtime(&mut self, checkout: CheckoutKey, id: u64) -> Result<()> {
        let session_index = self
            .sessions
            .iter()
            .position(|session| session.id == id)
            .ok_or_else(|| anyhow!("session {id} not found"))?;
        if let Err(error) = lifecycle::destructive_teardown(
            &mut self.repository_state,
            checkout,
            &mut self.sessions[session_index],
        ) {
            if let Err(save_error) = self.save_checked() {
                return Err(anyhow::Error::new(RuntimeTeardownError(anyhow!(
                    "{error}; could not persist pending teardown recovery: {save_error}"
                ))));
            }
            return Err(anyhow::Error::new(RuntimeTeardownError(
                anyhow::Error::new(error),
            )));
        }
        Ok(())
    }

    fn forget_stopped_runtime(&mut self, checkout: CheckoutKey, id: u64) {
        self.sessions.retain(|session| session.id != id);
        self.runtime_checkouts.remove(&checkout);
        if let Some(notify) = self.permission_notify.remove(&id) {
            notify.notify_waiters();
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn restore_removed_runtime(
        &mut self,
        checkout: CheckoutKey,
        mut saved: RetainedSessionState,
    ) -> Result<u64> {
        if let Some(id) = self.runtime_checkouts.get(&checkout).copied() {
            return Ok(id);
        }
        let lifecycle = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .map(|saved| saved.lifecycle().clone())
            .ok_or_else(|| anyhow!("checkout {} is missing", checkout.get()))?;
        match lifecycle {
            CheckoutLifecycle::Inactive => {
                self.drive_lifecycle_effect(
                    checkout,
                    lifecycle::LifecycleEvent::RequestActivation,
                    |_, _| Ok(()),
                )?;
                self.drive_lifecycle_effect(
                    checkout,
                    lifecycle::LifecycleEvent::ActivationVerified,
                    |_, _| Ok(()),
                )?;
            }
            CheckoutLifecycle::Protected(UnavailableCause::RemovalTombstone(_)) => {
                self.drive_lifecycle_effect(
                    checkout,
                    lifecycle::LifecycleEvent::RestoreRemovalAuthority,
                    |_, _| Ok(()),
                )?;
            }
            CheckoutLifecycle::Active => {}
            other => bail!("checkout cannot restore a runtime from {other:?}"),
        }
        let mode =
            lifecycle::retained_resume_mode(&mut self.repository_state, checkout, &mut saved);
        let id = self.spawn_retained_with_mode(
            checkout,
            saved.cwd.to_path_buf(),
            saved.repo_root.to_path_buf(),
            saved.branch.clone(),
            saved.is_worktree,
            Some(&saved.name),
            mode,
            saved.shell_open,
        )?;
        if let Ok(runtime) = self.session_mut(id) {
            runtime.archived = saved.archived;
            runtime.archived_by_user = saved.archived_by_user;
        }
        self.runtime_checkouts.insert(checkout, id);
        Ok(id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn compensate_failed_removal(
        &mut self,
        checkout: CheckoutKey,
        runtime: Option<RetainedSessionState>,
        failure: lifecycle::RemovalFailure,
    ) -> std::result::Result<LifecycleOutcome, lifecycle::RemovalFailure> {
        let original = failure.to_string();
        if let Some(saved) = runtime {
            if let Err(error) = self.restore_removed_runtime(checkout, saved) {
                return Err(lifecycle::RemovalFailure::Compensation {
                    original,
                    recovery: error.to_string(),
                });
            }
        }
        Err(failure)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn prepare_remove_worktree(
        &self,
        checkout: CheckoutKey,
    ) -> std::result::Result<lifecycle::RemovalConfirmation, lifecycle::RemovalFailure> {
        if self.persistence_dirty || self.repository_state.has_pending_activation() {
            return Err(lifecycle::RemovalFailure::Inspection(
                "repository lifecycle is blocked by unresolved persistence recovery".into(),
            ));
        }
        let saved = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .ok_or(lifecycle::RemovalFailure::CheckoutMissing(checkout))?;
        let _reservation = self
            .repository_reservations
            .reserve(saved.repository_key)
            .map_err(|outcome| lifecycle::RemovalFailure::Inspection(format!("{outcome:?}")))?;
        lifecycle::prepare_removal(&self.repository_state, checkout)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn confirm_remove_worktree(
        &mut self,
        confirmation: lifecycle::RemovalConfirmation,
    ) -> std::result::Result<LifecycleOutcome, lifecycle::RemovalFailure> {
        let _reservation = self
            .repository_reservations
            .reserve(confirmation.repository())
            .map_err(|outcome| lifecycle::RemovalFailure::Inspection(format!("{outcome:?}")))?;
        let checkout = confirmation.checkout();
        let runtime_id = self.runtime_checkouts.get(&checkout).copied();
        let runtime = runtime_id
            .map(|id| self.retained_runtime_snapshot(id))
            .transpose()
            .map_err(|error| lifecycle::RemovalFailure::Inspection(error.to_string()))?;
        if let Some(saved) = runtime.clone() {
            if let Some(retained) = self
                .repository_state
                .checkouts
                .iter_mut()
                .find(|retained| retained.key == checkout)
            {
                retained.session = saved;
            }
        }
        if let Some(id) = runtime_id {
            let retry = matches!(
                self.repository_state
                    .checkouts
                    .iter()
                    .find(|saved| saved.key == checkout)
                    .map(SavedCheckout::lifecycle),
                Some(CheckoutLifecycle::Protected(
                    UnavailableCause::TeardownPending { .. }
                ))
            );
            if retry {
                self.teardown_retained_runtime(checkout, id)
                    .map_err(|error| {
                        lifecycle::RemovalFailure::Inspection(format!(
                            "runtime stop failed: {error}"
                        ))
                    })?;
            } else {
                self.drive_lifecycle_effect(
                    checkout,
                    lifecycle::LifecycleEvent::RequestClose,
                    move |manager, _| manager.teardown_retained_runtime(checkout, id),
                )
                .map_err(|error| {
                    lifecycle::RemovalFailure::Inspection(format!("runtime stop failed: {error}"))
                })?;
            }
            self.drive_lifecycle_effect(
                checkout,
                lifecycle::LifecycleEvent::RuntimeExtinct,
                |_, _| Ok(()),
            )
            .map_err(|error| {
                lifecycle::RemovalFailure::Inspection(format!(
                    "runtime extinction persistence failed: {error}"
                ))
            })?;
            self.forget_stopped_runtime(checkout, id);
        }

        let target =
            match lifecycle::inspect_confirmed_removal(&self.repository_state, &confirmation) {
                Ok(target) => target,
                Err(failure) => {
                    return self.compensate_failed_removal(checkout, runtime, failure);
                }
            };
        if let Some(saved) = runtime.clone() {
            if let Some(retained) = self
                .repository_state
                .checkouts
                .iter_mut()
                .find(|retained| retained.key == checkout)
            {
                retained.session = saved;
            }
        }
        // Rollback may restore management authority, but must never roll back
        // the freshly captured conversation and shell context.
        let before_revocation = self.repository_state.clone();
        lifecycle::revoke_removal_authority(&mut self.repository_state, checkout)
            .map_err(|error| lifecycle::RemovalFailure::Inspection(error.to_string()))?;
        if let Err(error) = self.save_removal_revocation() {
            self.persistence_dirty = true;
            let original = format!("could not durably revoke removal authority: {error}");
            self.repository_state = before_revocation;
            if let Err(restoration) = self.save_removal_revocation() {
                self.persistence_dirty = true;
                return Err(lifecycle::RemovalFailure::Compensation {
                    original,
                    recovery: format!(
                        "authority restoration persistence failed before runtime compensation: {restoration}"
                    ),
                });
            }
            self.persistence_dirty = false;
            return self.compensate_failed_removal(
                checkout,
                runtime,
                lifecycle::RemovalFailure::Inspection(original),
            );
        }
        let revoked_state = self.repository_state.clone();
        let removal = match lifecycle::execute_verified_removal(&target) {
            Ok(removal) => removal,
            Err(git::RemoveVerifiedError::Postcondition(failure)) => {
                if let Some(saved) = runtime {
                    if let Some(retained) = self
                        .repository_state
                        .checkouts
                        .iter_mut()
                        .find(|retained| retained.key == checkout)
                    {
                        retained.session = saved;
                    }
                }
                let detail =
                    format!("Git topology committed but postconditions degraded: {failure:?}");
                lifecycle::mark_removed_checkout_unavailable(
                    &mut self.repository_state,
                    checkout,
                    detail.clone(),
                );
                if self.save_checked().is_err() {
                    self.persistence_dirty = true;
                }
                return Ok(LifecycleOutcome::TopologyCommittedStateDegraded { checkout, detail });
            }
            Err(error) => {
                self.repository_state = before_revocation;
                if self.save_removal_revocation().is_err() {
                    self.repository_state = revoked_state;
                    self.persistence_dirty = true;
                }
                return self.compensate_failed_removal(
                    checkout,
                    runtime,
                    lifecycle::RemovalFailure::GitRefused(error.to_string()),
                );
            }
        };

        let before_deletion = self.repository_state.clone();
        let outcome =
            lifecycle::commit_removed_checkout(&mut self.repository_state, &confirmation, &removal)
                .map_err(|error| lifecycle::RemovalFailure::Inspection(error.to_string()))?;
        self.runtime_checkouts.remove(&checkout);
        match self.save_checked() {
            Ok(()) => {
                self.persistence_dirty = false;
                Ok(outcome)
            }
            Err(error) if !error.replacement_committed() => {
                self.repository_state = before_deletion;
                lifecycle::mark_removed_checkout_unavailable(
                    &mut self.repository_state,
                    checkout,
                    format!("Git removal committed before persistence replacement: {error}"),
                );
                self.persistence_dirty = true;
                Ok(LifecycleOutcome::TopologyCommittedStateDegraded {
                    checkout,
                    detail: error.to_string(),
                })
            }
            Err(error) => {
                self.persistence_dirty = true;
                Ok(LifecycleOutcome::TopologyCommittedStateDegraded {
                    checkout,
                    detail: error.to_string(),
                })
            }
        }
    }

    pub fn remove(&mut self, id: u64) -> MutationResult<()> {
        if let Some(busy) = self.session_busy(id) {
            return Err(busy);
        }
        self.session(id)?;
        // A folder session is closed, never dropped: its row stays and must
        // stop recording the runtime.
        if let Some(key) = self.standalone_owner(id) {
            return self.close_standalone(key, id);
        }
        let session = self.session(id)?;
        let checkout_key = self
            .runtime_checkouts
            .iter()
            .find_map(|(key, runtime_id)| (*runtime_id == id).then_some(*key));
        let Some(checkout_key) = checkout_key else {
            self.session_mut(id)?.kill();
            self.sessions.retain(|session| session.id != id);
            if let Some(notify) = self.permission_notify.remove(&id) {
                notify.notify_waiters();
            }
            return Ok(());
        };
        let snapshot = RetainedSessionState {
            name: session.name.clone(),
            cwd: PersistedPath::from_path(&session.cwd),
            repo_root: PersistedPath::from_path(&session.repo_root),
            branch: session.branch.clone(),
            is_worktree: session.is_worktree,
            shell_open: session.shell_open,
            archived: session.archived,
            archived_by_user: session.archived_by_user,
            resume_id: session.meta.session_id.clone().or_else(|| {
                self.repository_state
                    .checkouts
                    .iter()
                    .find(|checkout| checkout.key == checkout_key)
                    .and_then(|checkout| checkout.session.resume_id.clone())
            }),
        };
        if let Some(saved) = self
            .repository_state
            .checkouts
            .iter_mut()
            .find(|saved| saved.key == checkout_key)
        {
            saved.session = snapshot;
        }
        if matches!(
            self.repository_state
                .checkouts
                .iter()
                .find(|saved| saved.key == checkout_key)
                .map(SavedCheckout::lifecycle),
            Some(CheckoutLifecycle::Protected(
                UnavailableCause::TeardownPending { .. }
            ))
        ) {
            self.teardown_retained_runtime(checkout_key, id)
                .map_err(MutationError::from_lifecycle)?;
            self.drive_lifecycle_effect(
                checkout_key,
                lifecycle::LifecycleEvent::RuntimeExtinct,
                move |manager, _| {
                    manager.forget_stopped_runtime(checkout_key, id);
                    Ok(())
                },
            )
            .map_err(MutationError::from_lifecycle)?;
            return Ok(());
        }
        self.drive_lifecycle_effect(
            checkout_key,
            lifecycle::LifecycleEvent::RequestClose,
            move |manager, _| manager.teardown_retained_runtime(checkout_key, id),
        )
        .map_err(MutationError::from_lifecycle)?;
        self.drive_lifecycle_effect(
            checkout_key,
            lifecycle::LifecycleEvent::RuntimeExtinct,
            move |manager, _| {
                manager.forget_stopped_runtime(checkout_key, id);
                Ok(())
            },
        )
        .map_err(MutationError::from_lifecycle)?;
        if self.persistence_dirty {
            return Err(MutationError::Persistence(
                persist::SaveError::after_replacement(anyhow!(
                    "lifecycle state committed but directory durability failed"
                )),
            ));
        }
        Ok(())
    }

    /// How long to wait between pasting the text and pressing Enter. Claude
    /// Code coalesces input arriving in one burst into a single paste, which
    /// swallows the CR — verified live; a same-write `text + \r` never
    /// submits. The submit must arrive as a distinct later keypress.
    const SUBMIT_DELAY: std::time::Duration = std::time::Duration::from_millis(150);

    /// Inject a message into the session's PTY: paste the text, then press
    /// Enter. Multiline-safe via bracketed paste. If Claude is busy it queues
    /// the message natively (visible as `queue-operation` transcript records).
    pub fn post_message(&mut self, id: u64, text: &str) -> Result<()> {
        let s = self.session_mut(id)?;
        if s.claude.is_exited() {
            bail!("claude has exited");
        }
        // Input written before the CLI's TUI is up gets drained, not queued.
        // Claude readiness = its session file resolved a session_id; opencode
        // readiness = its server answered a poll (`backend_ready` — its
        // session_id only exists AFTER the first prompt, so gating on it
        // would deadlock the first message).
        if s.meta.session_id.is_none() && !s.meta.backend_ready {
            bail!("claude is still starting — retry shortly");
        }
        let bracketed = s
            .claude
            .parser
            .lock()
            .map(|p| p.screen().bracketed_paste())
            .unwrap_or(false);
        let mut bytes = Vec::with_capacity(text.len() + 12);
        if bracketed {
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend_from_slice(text.as_bytes());
            bytes.extend_from_slice(b"\x1b[201~");
        } else {
            bytes.extend_from_slice(text.as_bytes());
        }
        s.claude.write_input(&bytes);
        std::thread::sleep(Self::SUBMIT_DELAY);
        s.claude.write_input(b"\r");
        if s.unarchive_on_input() {
            self.save_checked()?;
        }
        Ok(())
    }

    /// Ingest one hook event line POSTed to `POST /sessions/{id}/event` onto
    /// the same `/tmp/baude-events-<sid>.jsonl` consume path the poll loop
    /// tails — converging the daemon (POST) transport with the TUI-local
    /// (file-tail) transport onto one event model.
    ///
    /// Resolves the target Claude `session_id` by preferring the one embedded in
    /// the POSTed event line itself (`baude hook` builds the line with the
    /// authoritative `session_id` from the hook payload, and the file is keyed by
    /// that same id), falling back to the session's poll-resolved
    /// `meta.session_id`. Preferring the body id means a real session's earliest
    /// hook events land in the correct file immediately, instead of being
    /// rejected until the first poll cycle resolves `meta.session_id` (~1s race).
    /// Errors (never panics) only on an unknown baude id or when neither source
    /// yields a session_id. `event_path` sanitizes the id, so a body-supplied id
    /// cannot traverse paths (single-user loopback model).
    pub fn ingest_event(&mut self, id: u64, body: &str) -> Result<()> {
        let s = self.session(id)?;
        let body_sid = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v["session_id"].as_str().map(str::to_string))
            .filter(|s| !s.is_empty());
        let sid = body_sid
            .or_else(|| s.meta.session_id.clone())
            .ok_or_else(|| anyhow!("session {id} has no claude session_id yet"))?;
        baude_core::hook::append_event(&sid, body.trim_end())
            .map_err(|e| anyhow!("append event for session {id}: {e}"))
    }

    // ===== PERM-02: pending-permission set/resolve =======================
    //
    // The bridge POSTs a pending request (set_pending), the PWA/phone POSTs the
    // decision (resolve_pending), and the bridge's GET poll reads it (decision).
    // All route `Err -> 404` via `self.session(id)?`/`session_mut(id)?` exactly
    // like `ingest_event`. Pitfall 4: these only touch state UNDER the lock; the
    // actual wait happens OUTSIDE via the Arc<Notify> from `permission_notify`.

    /// Store a fresh pending permission request on a known session, clearing any
    /// stale decision from a previous turn so the bridge can't read it. Err →
    /// 404 on an unknown id.
    pub fn set_pending(&mut self, id: u64, p: PendingPermission) -> Result<()> {
        let s = self.session_mut(id)?;
        s.pending_permission = Some(serde_json::to_value(&p).unwrap_or(serde_json::Value::Null));
        s.permission_decision = None; // a new request supersedes any prior decision
        Ok(())
    }

    /// The pending permission request, if one is awaiting a decision. `Ok(None)`
    /// when nothing is pending; Err → 404 on an unknown id.
    pub fn pending(&self, id: u64) -> Result<Option<PendingPermission>> {
        let s = self.session(id)?;
        Ok(s.pending_permission
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok()))
    }

    /// The recorded decision for the most recent request, if resolved. `Ok(None)`
    /// while pending/idle; Err → 404 on an unknown id. The bridge's GET poll
    /// reads this to unblock.
    pub fn decision(&self, id: u64) -> Result<Option<PermissionDecision>> {
        let s = self.session(id)?;
        Ok(s.permission_decision.as_ref().map(|v| PermissionDecision {
            request_id: v["request_id"].as_str().unwrap_or_default().to_string(),
            decision: v["decision"].as_str().unwrap_or("deny").to_string(),
            ts: v["ts"].as_u64().unwrap_or_default(),
        }))
    }

    /// Resolve the pending permission with a `allow`/`deny` decision: clear the
    /// pending request, record the decision for the bridge poll, and wake any
    /// registered waiter (Pitfall 4 — the wake fires here, the await is outside
    /// the lock). Err → 404 on an unknown id. The caller (`post_permission`)
    /// validates `decision ∈ {allow,deny}` BEFORE calling — but as defense in
    /// depth any non-`allow` value is stored as `deny` (deny-default).
    /// WR-03: `scope` is intentionally NOT a parameter — scope is accepted by the
    /// POST handler for forward-compat but is not enforced or stored in v0.7
    /// (every `tools/call` mints a fresh request, so a "session" scope would have
    /// nothing to attach to). Enforcement is deferred to a later milestone.
    pub fn resolve_pending(&mut self, id: u64, decision: &str) -> Result<()> {
        let request_id = {
            let s = self.session_mut(id)?;
            let request_id = s
                .pending_permission
                .as_ref()
                .and_then(|v| v["request_id"].as_str().map(str::to_string))
                .unwrap_or_default();
            let verdict = if decision == "allow" { "allow" } else { "deny" };
            s.permission_decision = Some(serde_json::json!({
                "request_id": request_id,
                "decision": verdict,
                "ts": now_unix_ms(),
            }));
            s.pending_permission = None;
            request_id
        };
        let _ = request_id;
        // Wake the bridge/handler waiting outside the lock.
        if let Some(n) = self.permission_notify.get(&id) {
            n.notify_waiters();
        }
        Ok(())
    }

    /// The per-session wake handle for the permission long-poll. A waiter clones
    /// this Arc, registers `notified()` BEFORE re-checking `decision`, then
    /// `await`s OUTSIDE the manager lock (Pitfall 4). `resolve_pending` fires it.
    /// Err → 404 on an unknown id.
    pub fn permission_notify(&mut self, id: u64) -> Result<Arc<Notify>> {
        // Validate the id first so an unknown session is a clean 404.
        self.session(id)?;
        Ok(Arc::clone(self.permission_notify.entry(id).or_default()))
    }

    /// Respawn the CLI in an exited session's PTY (same cwd, fresh process,
    /// the backend's resume form to pick the conversation back up).
    ///
    /// Routes through the SAME backend composition as `spawn` — previously
    /// this hand-rolled `{claude_cmd} --continue || exec {claude_cmd}`, so a
    /// restarted session silently lost its permission flag AND the
    /// `$BAUDE_EVENT_URL` export (hook events regressed to the /tmp file
    /// path with no daemon POST). Now a restart is spawn-equivalent, and for
    /// opencode it re-rolls the pinned server port.
    pub fn reopen_checkout(
        &mut self,
        checkout_key: CheckoutKey,
    ) -> MutationResult<LifecycleOutcome> {
        if self.persistence_blocked {
            return Err(persist::SaveError::before_replacement(anyhow!(
                "daemon persistence is blocked after a state load failure"
            ))
            .into());
        }
        let checkout = self
            .repository_state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == checkout_key)
            .cloned()
            .ok_or_else(|| anyhow!("retained checkout {} is missing", checkout_key.get()))?;
        let _reservation = match self
            .repository_reservations
            .reserve_reopen(checkout.repository_key, checkout_key)
        {
            Ok(reservation) => reservation,
            Err(outcome) => return Ok(outcome),
        };
        let runtime = self.runtime_checkouts.get(&checkout_key).and_then(|id| {
            self.sessions
                .iter()
                .find(|session| session.id == *id)
                .map(|session| (*id, session.claude.is_exited()))
        });
        let runtime_fact = match runtime {
            Some((id, false)) => lifecycle::ReopenRuntime::Live { id },
            Some((id, true)) => lifecycle::ReopenRuntime::Exited { id },
            None => lifecycle::ReopenRuntime::Absent,
        };
        let repository = self
            .repository_state
            .repositories
            .iter()
            .find(|repository| repository.key == checkout.repository_key)
            .ok_or_else(|| anyhow!("retained checkout repository is missing"))?;
        let reconciliation = git::reconcile_checkout(
            &repository.observed_common_dir.to_path_buf(),
            &checkout.observed_path.to_path_buf(),
            checkout.observed_branch.as_deref(),
        )
        .map(|_| ());
        let state_before = self.repository_state.clone();
        let plan = match lifecycle::plan_reopen(
            &mut self.repository_state,
            lifecycle::ReopenRequest {
                checkout: checkout_key,
                reconciliation,
                runtime: runtime_fact,
            },
        ) {
            Ok(plan) => plan,
            Err(blocked) => {
                if let Err(error) = self.save_checked() {
                    self.persistence_dirty = true;
                    if !error.replacement_committed() {
                        self.repository_state = state_before;
                    }
                    return Err(MutationError::Persistence(error));
                }
                self.persistence_dirty = false;
                return Err(anyhow!(
                    "retained checkout {} is unavailable: {:?}",
                    blocked.checkout().get(),
                    blocked.cause
                )
                .into());
            }
        };
        if let Err(error) = self.save_checked() {
            self.persistence_dirty = true;
            if !error.replacement_committed() {
                self.repository_state = state_before;
            }
            return Err(MutationError::Persistence(error));
        }
        self.persistence_dirty = false;

        match plan.dispatch {
            lifecycle::ReopenDispatch::Focus { id } => Ok(LifecycleOutcome::Focused {
                checkout: checkout_key,
                runtime: id,
            }),
            lifecycle::ReopenDispatch::Restart { id } => {
                self.restart_with_mode(id, plan.mode)?;
                Ok(LifecycleOutcome::Reopened {
                    checkout: checkout_key,
                    runtime: id,
                })
            }
            lifecycle::ReopenDispatch::Spawn => {
                let saved = checkout.session;
                let id = self.spawn_retained_with_mode(
                    checkout_key,
                    checkout.observed_path.to_path_buf(),
                    saved.repo_root.to_path_buf(),
                    saved.branch.clone(),
                    saved.is_worktree,
                    Some(&saved.name),
                    plan.mode,
                    saved.shell_open,
                )?;
                if let Ok(session) = self.session_mut(id) {
                    session.archived = saved.archived;
                    session.archived_by_user = saved.archived_by_user;
                }
                self.runtime_checkouts.insert(checkout_key, id);
                Ok(LifecycleOutcome::Reopened {
                    checkout: checkout_key,
                    runtime: id,
                })
            }
        }
    }

    pub fn restart(&mut self, id: u64) -> MutationResult<()> {
        if self.persistence_blocked {
            return Err(persist::SaveError::before_replacement(anyhow!(
                "daemon persistence is blocked after a state load failure"
            ))
            .into());
        }
        if let Some(busy) = self.session_busy(id) {
            return Err(busy);
        }
        if !self.session(id)?.claude.is_exited() {
            return Err(anyhow!("claude is still running").into());
        }
        if let Some(checkout_key) = self
            .runtime_checkouts
            .iter()
            .find_map(|(key, runtime_id)| (*runtime_id == id).then_some(*key))
        {
            self.reopen_checkout(checkout_key)?;
            return Ok(());
        }
        // A folder session never reaches `restart_with_mode`: that path
        // launches without an owner and would leave the row recording the
        // exited runtime.
        if let Some(key) = self.standalone_owner(id) {
            self.reopen_standalone(key)?;
            return Ok(());
        }
        let targeted = self
            .runtime_checkouts
            .iter()
            .find_map(|(key, runtime_id)| (*runtime_id == id).then_some(*key))
            .and_then(|key| {
                self.repository_state
                    .checkouts
                    .iter()
                    .find(|checkout| checkout.key == key)
                    .and_then(|checkout| checkout.session.resume_id.clone())
            })
            .or_else(|| {
                self.session(id)
                    .ok()
                    .and_then(|session| session.meta.session_id.clone())
            });
        // A targeted resume of a conversation with no transcript dies at
        // spawn ("No conversation found"), so a dead id continues instead.
        let mut targeted = targeted;
        let cwd = self.session(id)?.cwd.clone();
        let mode = lifecycle::resume_mode(&mut targeted, |resume| {
            backend::active().resume_target_exists(&cwd, resume)
        });
        self.restart_with_mode(id, mode)
    }

    fn restart_with_mode(&mut self, id: u64, mode: backend::SpawnMode) -> MutationResult<()> {
        let be = backend::active();
        let resolved = be.resolve_cmd(&self.claude_cmd);
        let targeted_resume = matches!(&mode, backend::SpawnMode::ResumeId(_));
        let plan = be.spawn_plan(&resolved.cmd, Some(&event_url(id)), mode);
        let checkout = self
            .runtime_checkouts
            .iter()
            .find_map(|(key, runtime_id)| (*runtime_id == id).then_some(*key));
        let cwd = self.session(id)?.cwd.clone();
        // HREG-03/D-02: same stderr surface as spawn_with_mode_internal —
        // restart is a spawn path too; warnings never block (D-04).
        for warning in be.prepare_cwd(&cwd) {
            eprintln!("seed warning: {warning}");
        }
        let mut replacement = if let Some(checkout) = checkout {
            let generation = self
                .repository_state
                .checkouts
                .iter()
                .find(|saved| saved.key == checkout)
                .and_then(SavedCheckout::owned_runtime)
                .and_then(|runtime| runtime.generation.successor())
                .unwrap_or(RuntimeGeneration::initial());
            let shell = self
                .session(id)?
                .shell
                .as_ref()
                .filter(|shell| !shell.is_exited())
                .map(|shell| ShellOwnership::Owned(shell.process_identity().clone()))
                .unwrap_or(ShellOwnership::Closed);
            let mut replacement = Pty::spawn_registered_with(
                Some(&plan.cmd),
                &plan.env,
                &cwd,
                ROWS,
                COLS,
                |agent| {
                    self.drive_lifecycle_effect(
                        checkout,
                        lifecycle::LifecycleEvent::LaunchRegistered(OwnedRuntime {
                            generation,
                            agent: agent.clone(),
                            shell,
                        }),
                        |_, _| Ok(()),
                    )?;
                    Ok(())
                },
            )?;
            if let Err(error) = self.drive_lifecycle_effect(
                checkout,
                lifecycle::LifecycleEvent::LaunchReleased,
                |_, _| Ok(()),
            ) {
                let _ = replacement.kill_and_wait();
                return Err(error.into());
            }
            replacement
        } else {
            Pty::spawn_with_env(Some(&plan.cmd), &plan.env, &cwd, ROWS, COLS)?
        };
        let s = self.session_mut(id)?;
        std::mem::swap(&mut s.claude, &mut replacement);
        s.spawn_unix_ms = now_unix_ms();
        s.targeted_resume = targeted_resume;
        s.meta = ClaudeMeta::default();
        s.meta.backend_port = plan.server_port;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn track_runtime_for_test(&mut self, id: u64) {
        self.record_runtime(id).unwrap();
    }

    /// Local port of the session's backend server (opencode), if it runs one.
    /// Err → 404 on an unknown id. Read by the daemon's permission bridge.
    pub fn backend_port(&self, id: u64) -> Result<Option<u16>> {
        Ok(self.session(id)?.meta.backend_port)
    }

    /// Attach for raw terminal streaming: a redraw snapshot plus a receiver
    /// of every output chunk after it. See `Pty::subscribe`.
    pub fn attach(&self, id: u64) -> Result<(Vec<u8>, std::sync::mpsc::Receiver<Vec<u8>>)> {
        let s = self.session(id)?;
        if s.claude.is_exited() {
            bail!("claude has exited");
        }
        Ok(s.claude.subscribe())
    }

    /// Raw input bytes from an attached client.
    pub fn write_raw(&mut self, id: u64, bytes: &[u8]) -> Result<()> {
        let s = self.session_mut(id)?;
        if s.claude.is_exited() {
            bail!("claude has exited");
        }
        s.claude.write_input(bytes);
        if s.unarchive_on_input() {
            self.save_checked()?;
        }
        Ok(())
    }

    /// Resize from an attached client. Multiple clients: last write wins.
    pub fn resize_pty(&mut self, id: u64, rows: u16, cols: u16) -> Result<()> {
        let s = self.session_mut(id)?;
        s.claude.resize(rows, cols);
        Ok(())
    }

    /// Send Esc — stops Claude's current work without killing the session.
    pub fn interrupt(&mut self, id: u64) -> Result<()> {
        let s = self.session_mut(id)?;
        if s.claude.is_exited() {
            bail!("claude has exited");
        }
        s.claude.write_input(b"\x1b");
        Ok(())
    }

    /// Transcript path for a session: Err = no such session, Ok(None) = the
    /// transcript hasn't been resolved yet (session just spawned).
    pub fn transcript_path(&self, id: u64) -> Result<Option<PathBuf>> {
        let s = self.session(id)?;
        Ok(s.meta.transcript_path().map(Path::to_path_buf))
    }

    /// Per-session hook-event file path: Err = no such session, Ok(None) = the
    /// Claude session_id hasn't been resolved yet (so no event file exists).
    /// The sid is sanitized by `baude_core::hook::event_path` (T-03-05).
    /// Analog of `transcript_path` — the SSE existence guard maps Err → 404.
    pub fn event_path(&self, id: u64) -> Result<Option<PathBuf>> {
        let s = self.session(id)?;
        Ok(s.meta
            .session_id
            .as_ref()
            .map(|sid| PathBuf::from(baude_core::hook::event_path(sid))))
    }

    /// The session's recent hook events, newest-at-back, bounded to `limit`.
    /// Reads the in-memory `ClaudeMeta` ring (the single source of truth).
    /// Err = no such session (→ 404 upstream).
    pub fn activity(&self, id: u64, limit: usize) -> Result<Vec<HookEvent>> {
        let s = self.session(id)?;
        let act = s.meta.activity();
        let start = act.len().saturating_sub(limit);
        Ok(act.iter().skip(start).cloned().collect())
    }

    /// Plain-text snapshot of the session's terminal — the escape hatch for
    /// the rare interactive menu that the chat surface can't represent.
    pub fn screen(&self, id: u64) -> Result<Screenshot> {
        let s = self.session(id)?;
        let parser = s
            .claude
            .parser
            .lock()
            .map_err(|_| anyhow!("screen unavailable"))?;
        let screen = parser.screen();
        let (rows, cols) = screen.size();
        let (cur_row, cur_col) = screen.cursor_position();
        Ok(Screenshot {
            text: screen.contents(),
            rows,
            cols,
            cursor: [cur_row, cur_col],
        })
    }

    /// Send named keys (and literal text) straight into the PTY — pairs with
    /// `screen` to drive menus. Small gaps between keys so Claude's input
    /// coalescing treats each as a distinct keypress.
    pub fn send_keys(&mut self, id: u64, keys: &[String]) -> Result<()> {
        let s = self.session_mut(id)?;
        if s.claude.is_exited() {
            bail!("claude has exited");
        }
        let app_cursor = s
            .claude
            .parser
            .lock()
            .map(|p| p.screen().application_cursor())
            .unwrap_or(false);
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            s.claude.write_input(&key_bytes(key, app_cursor));
        }
        if s.unarchive_on_input() {
            self.save_checked()?;
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions.iter().map(session_info).collect()
    }

    pub fn info(&self, id: u64) -> Option<SessionInfo> {
        self.sessions.iter().find(|s| s.id == id).map(session_info)
    }

    /// Reconcile an immediate `--resume` rejection through the ordinary
    /// retained remove boundary so child reaping and retryable lifecycle state
    /// stay identical to an explicit close.
    fn reconcile_early_targeted_resume_exits(&mut self) {
        let exited: Vec<_> = self
            .sessions
            .iter_mut()
            .filter_map(|session| {
                session
                    .early_targeted_resume_exit()
                    .then(|| {
                        self.runtime_checkouts
                            .iter()
                            .find_map(|(key, runtime)| {
                                (*runtime == session.id).then_some(RuntimeOwner::Checkout(*key))
                            })
                            .or_else(|| {
                                self.runtime_standalones.iter().find_map(|(key, runtime)| {
                                    (*runtime == session.id)
                                        .then_some(RuntimeOwner::Standalone(*key))
                                })
                            })
                            .map(|owner| (owner, session.id))
                    })
                    .flatten()
            })
            .collect();
        for (owner, id) in exited {
            match owner {
                RuntimeOwner::Checkout(checkout) => {
                    if let Some(saved) = self
                        .repository_state
                        .checkouts
                        .iter_mut()
                        .find(|saved| saved.key == checkout)
                    {
                        saved.session.resume_id = None;
                    }
                }
                RuntimeOwner::Standalone(key) => {
                    if let Some(row) = self.repository_state.standalone_session_mut(key) {
                        row.session.resume_id = None;
                    }
                }
            }
            if let Err(error) = self.remove(id) {
                eprintln!("failed to reconcile early targeted resume: {error}");
            }
        }
    }

    pub fn poll(&mut self) {
        self.reconcile_early_targeted_resume_exits();
        let mut changed = false;
        let idle = self.auto_archive_ms;
        let policy = self.idle_child_policy;
        let mut notes = Vec::new();
        for s in &mut self.sessions {
            // PERF-06: archived and exited rows are never polled for metadata;
            // they still tick so an auto-archived row can re-engage.
            if s.archived || s.claude.is_exited() {
                changed |= s.auto_archive_tick(idle);
                continue;
            }
            s.poll_meta();
            let was_archived = s.archived;
            changed |= s.auto_archive_tick(idle);
            // PERF-07: a fresh auto-archive applies the idle-child policy,
            // exactly as the TUI tick does.
            if !was_archived && s.archived && !s.archived_by_user {
                if let Some(note) = s.apply_idle_child_policy(policy) {
                    notes.push(note);
                }
            }
        }
        for note in notes {
            eprintln!("idle child policy: {note}");
        }
        if changed {
            self.save();
        }
    }

    /// Park or unpark a session. Archived sessions sort last in clients and
    /// stop sending notifications. A manual archive sticks until unarchived
    /// or re-engaged; an automatic one also lifts when a new turn starts.
    pub fn set_archived(&mut self, id: u64, archived: bool) -> MutationResult<()> {
        if let Some(busy) = self.session_busy(id) {
            return Err(busy);
        }
        let s = self.session_mut(id)?;
        let archived_before = (s.archived, s.archived_by_user);
        s.set_archived(archived);
        match self.save_checked() {
            Ok(()) => {
                // Persist the intent first, then apply the idle-child policy
                // (PERF-07); a failed save leaves the child untouched.
                let policy = self.idle_child_policy;
                if let Some(note) = self.session_mut(id)?.apply_idle_child_policy(policy) {
                    eprintln!("idle child policy: {note}");
                }
                Ok(())
            }
            Err(error) if error.replacement_committed() => Err(MutationError::Persistence(error)),
            Err(error) => {
                let s = self.session_mut(id)?;
                s.archived = archived_before.0;
                s.archived_by_user = archived_before.1;
                Err(MutationError::Persistence(error))
            }
        }
    }

    /// Test-only: override the idle-child policy without touching config.json
    /// or the process environment.
    #[cfg(test)]
    pub fn set_idle_child_policy_for_test(&mut self, policy: persist::IdleChildPolicy) {
        self.idle_child_policy = policy;
    }

    /// Test-only: pin a session's resolved Claude `session_id` so handlers
    /// that resolve baude id -> sid (e.g. `ingest_event`) can be exercised
    /// without a live Claude writing `sessions/<pid>.json`.
    #[cfg(test)]
    pub fn session_id_for_test(&mut self, id: u64, sid: &str) {
        if let Ok(s) = self.session_mut(id) {
            s.meta.session_id = Some(sid.to_string());
            // Keep unrelated real Claude session files for the test cwd from
            // replacing the explicitly pinned id during the next metadata poll.
            s.spawn_unix_ms = u64::MAX;
        }
    }

    #[cfg(test)]
    pub(crate) fn persist_at_for_test(
        &mut self,
        root: &Path,
        workspace: &baude_core::workspace::Workspace,
        failure: Option<persist::AtomicFailure>,
    ) {
        self.persistence_target_for_test =
            Some((root.to_path_buf(), workspace.state_file(STATE_BASE)));
        self.atomic_failure_for_test = failure;
        self.atomic_failure_window_for_test = (0, usize::MAX);
    }

    /// Test-only: let the next `healthy` saves through, fail the `failing`
    /// saves after them with `failure`, and let every later save through.
    /// `usize::MAX` failing saves is a disk that went bad and stayed bad.
    /// This is how a test reaches a save in the middle of one request, such
    /// as the one that marks a started launch as running.
    #[cfg(test)]
    pub(crate) fn fail_saves_after_for_test(
        &mut self,
        healthy: usize,
        failing: usize,
        failure: persist::AtomicFailure,
    ) {
        self.atomic_failure_for_test = Some(failure);
        self.atomic_failure_window_for_test = (healthy, failing);
    }

    /// The failure the save about to run is given, counted against the
    /// window `fail_saves_after_for_test` set.
    #[cfg(test)]
    fn scheduled_atomic_failure_for_test(&mut self) -> Option<persist::AtomicFailure> {
        let (healthy, failing) = &mut self.atomic_failure_window_for_test;
        if *healthy > 0 {
            *healthy -= 1;
            return None;
        }
        if *failing == 0 {
            return None;
        }
        if *failing != usize::MAX {
            *failing -= 1;
        }
        self.atomic_failure_for_test
    }

    /// Test-only: make the next confirmed stop of session `id`'s agent PTY
    /// fail, so API tests can drive the runtime-teardown failure path.
    #[cfg(test)]
    pub(crate) fn fail_next_agent_teardown_for_test(&self, id: u64, detail: &str) {
        self.session(id)
            .expect("session exists")
            .claude
            .fail_next_teardown_for_test(detail);
    }

    /// Test-only deterministic Claude metadata poll. The process running the
    /// suite may itself select the OpenCode workspace, which must not redirect
    /// fixtures that explicitly seed Claude hook-event files.
    #[cfg(test)]
    pub fn poll_claude_meta_for_test(&mut self, id: u64) {
        if let Ok(session) = self.session_mut(id) {
            let pid = session.claude.pid();
            let (cwd, spawn, root) = (
                session.cwd.clone(),
                session.spawn_unix_ms,
                session.repo_root.clone(),
            );
            session.meta.poll(&cwd, pid, spawn, &root);
        }
    }

    pub fn kill_all(&mut self) {
        for s in &mut self.sessions {
            s.kill();
        }
    }

    /// Test-only: a reaper for the agent and, when present, the shell of
    /// every session this manager lists, recorded now.
    ///
    /// A fixture agent runs under an interactive shell, whose job control
    /// can give the command a process group of its own. Killing the PTY
    /// leader does not always reach that job, which is reparented to pid 1
    /// but stays in the session the leader created. Each reaper sweeps that
    /// recorded session when it drops. Bind these before a test kills,
    /// removes or restarts a runtime itself: once the manager forgets or
    /// replaces it, `kill_all_and_reap_for_test` no longer sees its identity.
    #[cfg(test)]
    pub(crate) fn runtime_reapers_for_test(&self) -> Vec<baude_core::testing::RuntimeReaper> {
        self.sessions
            .iter()
            .flat_map(|session| {
                std::iter::once(session.claude.process_identity())
                    .chain(session.shell.as_ref().map(|shell| shell.process_identity()))
            })
            .map(baude_core::testing::RuntimeReaper::for_identity)
            .collect()
    }

    /// Test-only: `kill_all`, then reap the recorded session of every runtime
    /// this manager listed before it, so a job that outlived its PTY leader
    /// does not outlive the test.
    #[cfg(test)]
    pub(crate) fn kill_all_and_reap_for_test(&mut self) {
        let reapers = self.runtime_reapers_for_test();
        self.kill_all();
        drop(reapers);
    }

    fn session(&self, id: u64) -> Result<&Session> {
        self.sessions
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| anyhow!("no session {id}"))
    }

    fn session_mut(&mut self, id: u64) -> Result<&mut Session> {
        self.sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| anyhow!("no session {id}"))
    }
}

/// `GET /sessions/{id}/screen` payload.
#[derive(Serialize)]
pub struct Screenshot {
    pub text: String,
    pub rows: u16,
    pub cols: u16,
    /// (row, col), 0-based.
    pub cursor: [u16; 2],
}

/// Map a key name to the bytes a terminal would send. Unrecognized names are
/// sent literally, so `["y"]` types y and `["down","enter"]` drives a menu.
fn key_bytes(key: &str, app_cursor: bool) -> Vec<u8> {
    let arrow = |c: u8| {
        if app_cursor {
            vec![0x1b, b'O', c]
        } else {
            vec![0x1b, b'[', c]
        }
    };
    match key {
        "up" => arrow(b'A'),
        "down" => arrow(b'B'),
        "right" => arrow(b'C'),
        "left" => arrow(b'D'),
        "enter" => vec![b'\r'],
        "esc" => vec![0x1b],
        "tab" => vec![b'\t'],
        "shift+tab" => b"\x1b[Z".to_vec(),
        "space" => vec![b' '],
        "backspace" => vec![0x7f],
        k => match k.strip_prefix("ctrl+").and_then(|r| {
            let mut chars = r.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_lowercase() => Some(c as u8 - b'a' + 1),
                _ => None,
            }
        }) {
            Some(b) => vec![b],
            None => k.as_bytes().to_vec(),
        },
    }
}

fn session_info(s: &Session) -> SessionInfo {
    let (status, source) = s.status_with_source();
    SessionInfo {
        id: s.id,
        name: s.name.clone(),
        title: s.meta.title.clone(),
        status: status_str(status),
        state_source: source_str(source),
        last_tool: s.meta.last_tool.as_ref().map(|(t, _)| t.clone()),
        // Populated for both idle flavors: Waiting's yellow "waiting Xm" timer
        // and Completed's dim "done Xm ago" both read this same duration.
        waiting_for_ms: matches!(status, Status::Waiting | Status::Completed)
            .then(|| s.waiting_for_ms()),
        // A live pending_permission IS a permission wait regardless of what the
        // hook stream said — claude's Notification hook usually agrees, but
        // opencode has no hooks, so the daemon-set pending (via the SSE
        // permission bridge) is the only signal there. This is what routes the
        // distinct `notified_permission` push and the PWA approve/deny card.
        waiting_reason: if s.pending_permission.is_some() {
            Some("permission".to_string())
        } else {
            match baude_core::permission::waiting_reason(
                s.meta.last_notification.as_ref(),
                status == Status::Waiting,
                status == Status::Completed,
            ) {
                // "none" carries no signal — omit it so the JSON stays lean and
                // the PWA/push key off "permission"/"input"/"completed".
                "none" => None,
                reason => Some(reason.to_string()),
            }
        },
        model: s.meta.model.clone(),
        // BL-02: fall back to baude's spawn-intended mode (skip→bypassPermissions)
        // when the transcript hasn't reported one yet, so the mode is shown for
        // every session — not just those past their first permissionMode record.
        permission_mode: s
            .meta
            .permission_mode
            .clone()
            .or_else(|| baude_core::permission::spawn_permission_mode().map(str::to_string)),
        context_used_pct: s.meta.context_used_pct,
        rate_5h_used_pct: s
            .meta
            .rate_5h
            .and_then(|w| w.used_pct)
            .map(|p| (p.round() as u64).min(100) as u8),
        rate_5h_resets_at_unix_s: s.meta.rate_5h.and_then(|w| w.resets_at_unix_s),
        branch: s.meta.git_branch.clone().or_else(|| s.branch.clone()),
        cwd: s.cwd.display().to_string(),
        repo_root: s.repo_root.display().to_string(),
        is_worktree: s.is_worktree,
        gsd_milestone: s.meta.gsd.as_ref().and_then(|g| g.milestone.clone()),
        gsd_phase: s.meta.gsd.as_ref().and_then(|g| g.phase_line.clone()),
        gsd_active_phase: s.meta.gsd.as_ref().and_then(|g| g.active_phase.clone()),
        session_cost_usd: s.meta.session_cost_usd,
        claude_session_id: s.meta.session_id.clone(),
        archived: s.archived,
        suspended: s.child_suspended,
        activity: {
            // Bounded recent set (~30) for the remote TUI overlay; the full
            // ring is served by GET /sessions/{id}/activity.
            let act = s.meta.activity();
            let start = act.len().saturating_sub(30);
            act.iter().skip(start).cloned().collect()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baude_core::lifecycle::LifecycleOutcome;
    use std::process::Command;
    use std::time::{Duration, Instant};

    /// The plain admission router over values, with no process: each of the
    /// four routings the daemon can reach without a discovered repository.
    #[test]
    fn plain_admission_routes_by_what_git_reports() {
        type Discovery =
            std::result::Result<git::RepositorySnapshot, git::RepositoryDiscoveryError>;
        let path = Path::new("/tmp/baude-21-10-router/notes");
        let not_repository = || -> Discovery {
            Err(git::RepositoryDiscoveryError::NotRepository(
                path.to_path_buf(),
            ))
        };

        let empty = RepositoryState::default();
        assert_eq!(
            route_plain_admission(&empty, path, &not_repository()),
            PlainAdmission::Standalone
        );

        let mut with_standalone = RepositoryState::default();
        let (standalone, order) = Manager::allocate_standalone_slot(&mut with_standalone).unwrap();
        with_standalone
            .standalone_sessions
            .push(SavedStandaloneSession::new(
                standalone,
                PersistedPath::from_path(path),
                order,
                StandaloneLifecycle::Inactive,
                None,
                RetainedStandaloneSessionState {
                    name: "notes".into(),
                    shell_open: false,
                    archived: false,
                    archived_by_user: false,
                    resume_id: None,
                    ever_launched: false,
                },
            ));
        assert_eq!(
            route_plain_admission(&with_standalone, path, &not_repository()),
            PlainAdmission::ReuseStandalone(standalone)
        );

        let mut with_checkout = RepositoryState::default();
        let repository = with_checkout.allocate_repository_key().unwrap();
        let checkout = with_checkout.allocate_checkout_key().unwrap();
        let order = with_checkout.allocate_first_seen_order().unwrap();
        with_checkout.checkouts.push(SavedCheckout::new(
            checkout,
            repository,
            CheckoutRole::ManagedBranch,
            false,
            PersistedPath::from_path(path),
            None,
            order,
            CheckoutLifecycle::Active,
            RetainedSessionState {
                name: "notes".into(),
                cwd: PersistedPath::from_path(path),
                repo_root: PersistedPath::from_path(path),
                branch: None,
                is_worktree: false,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
        ));
        assert_eq!(
            route_plain_admission(&with_checkout, path, &not_repository()),
            PlainAdmission::JoinStored(repository)
        );

        let malformed = git::RepositoryDiscoveryError::MalformedTopology(
            "worktree list named no main worktree".into(),
        );
        let display = malformed.to_string();
        let refused: Discovery = Err(malformed);
        let PlainAdmission::Refuse(refusal) = route_plain_admission(&empty, path, &refused) else {
            panic!("a discovery error that is not a missing repository must be refused");
        };
        assert_eq!(refusal, format!("repository discovery failed: {display}"));
        assert!(refusal.contains(&display), "{refusal}");
    }

    #[test]
    fn lifecycle_protocol_contract_manager_vectors() {
        for vector in baude_core::lifecycle::canonical_lifecycle_contract_vectors() {
            let actual = Manager::lifecycle_contract_trace(
                *vector,
                baude_core::lifecycle::AdapterFailureScript::None,
            );
            assert_eq!(
                baude_core::lifecycle::normalize_lifecycle_trace(&actual.trace),
                baude_core::lifecycle::canonical_lifecycle_trace(*vector).trace
            );
            assert_eq!(
                actual.final_lifecycle,
                baude_core::lifecycle::canonical_lifecycle_trace(*vector).final_lifecycle
            );
        }
        for failure in [
            baude_core::lifecycle::AdapterFailureScript::Persist(1),
            baude_core::lifecycle::AdapterFailureScript::Effect(1),
        ] {
            let actual = Manager::lifecycle_contract_trace(
                baude_core::lifecycle::CanonicalLifecycleVector::Launch,
                failure,
            );
            assert_eq!(
                actual,
                baude_core::lifecycle::run_canonical_lifecycle_contract(
                    baude_core::lifecycle::CanonicalLifecycleVector::Launch,
                    failure,
                )
            );
        }
    }

    /// The deliberate escape: config resolution with NO redirect must abort
    /// this binary's test, not quietly reach the developer's real
    /// `~/.config/baude`.
    ///
    /// `cfg(test)` is set per crate by `rustc --test`, so a `#[cfg(test)]`-only
    /// guard inside `baude-core` would be absent from exactly the two binaries
    /// that leaked (#72). This is the direct evidence that the `test-support`
    /// feature carried the gate into `bauded`'s test binary: it fails to COMPILE
    /// if the dev-dependency feature wiring is removed, and passes WITHOUT a
    /// panic if the guard itself vanished — the warning sign the phase exists
    /// to make visible.
    ///
    /// It mutates no environment variable. `BAUDE_TEST_FIXTURE_ROOT` is
    /// process-wide, so clearing it would change what every concurrently
    /// running test sees; the `NoFixtureRoot` probe is thread-local, so this
    /// needs no serial flag and no `--test-threads=1`.
    #[test]
    #[should_panic(expected = "resolved to the real user path")]
    fn unguarded_resolution_panics() {
        let _no_root = baude_core::testing::NoFixtureRoot::new();
        let _escaped = baude_core::persist::config_dir();
    }

    /// The `~` expansion behind `create_session`: a `POST /sessions` body
    /// naming `~/repo` used to stat the developer's real home from inside this
    /// binary. Same shape as the sibling above, one resolver down.
    #[test]
    #[should_panic(expected = "resolved to the real user path")]
    fn unguarded_tilde_expansion_panics() {
        let _no_root = baude_core::testing::NoFixtureRoot::new();
        let _escaped = persist::expand_tilde("~/repo");
    }

    fn mgr() -> Manager {
        Manager::new("sh -c 'sleep 30'".into(), false)
    }

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn pid_is_live(pid: u32) -> bool {
        Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .split_whitespace()
                        .next()
                        .is_some_and(|state| !state.starts_with('Z'))
            })
    }

    /// A literal fixture config: no file is read, no environment variable is
    /// consulted, and the workspace/backend choice is written down in the
    /// source of the test that made it (D-06).
    fn fixture_config(workspace_name: &str) -> baude_core::persist::Config {
        baude_core::persist::Config {
            workspace: Some(workspace_name.to_string()),
            ..baude_core::persist::Config::default()
        }
    }

    /// One construction that establishes everything a `bauded` manager fixture
    /// needs: a process-unique root, the unified redirect guard bound to that
    /// root, and a literal per-fixture workspace identity.
    ///
    /// It replaces ten copies of the same twelve-line preamble. The copies were
    /// how the original leak (#72) stayed easy to reintroduce: an eleventh copy
    /// that forgot the guard reads and writes the developer's real roots while
    /// looking exactly like its neighbours.
    struct ManagerFixture {
        /// Struct fields drop in DECLARATION order (the inverse of locals), so
        /// the identity is restored while the root redirect it was resolved
        /// under is still installed — the 08-03 owner convention.
        _identity: baude_core::testing::TestRedirect,
        /// Held as a FIELD, never constructed and let go. An unbound guard arms
        /// and disarms in the same statement, so the fixture would run entirely
        /// unredirected while still compiling and still reading correctly at the
        /// call site. Tying the redirect's lifetime to the fixture's is the
        /// whole reason the guard is RAII (D-01).
        _redirect: baude_core::testing::TestRedirect,
        workspace: baude_core::workspace::Workspace,
        /// Declared last, so the directory is removed after both redirects are
        /// restored, also when the test panicked.
        root: baude_core::testing::FixtureRoot,
    }

    impl ManagerFixture {
        /// The default (`claude`) workspace — what all ten preambles resolved.
        #[must_use = "the fixture owns this case's root, redirects and identity; bind it to a \
                      named local that outlives every Manager, session and PTY it isolates"]
        fn new(label: &str) -> Self {
            Self::with_workspace_name(label, baude_core::workspace::DEFAULT)
        }

        #[must_use = "the fixture owns this case's root, redirects and identity; bind it to a \
                      named local that outlives every Manager, session and PTY it isolates"]
        fn with_workspace_name(label: &str, workspace_name: &str) -> Self {
            let root = baude_core::testing::FixtureRoot::new(&format!("bauded-{label}"));
            let config = fixture_config(workspace_name);
            // Acquisition order is root FIRST, identity SECOND, so the identity
            // is resolved with this fixture's roots already installed.
            let redirect = baude_core::testing::TestRedirect::new(&root);
            let identity = baude_core::workspace::override_for_test(&config, None);
            let workspace =
                baude_core::workspace::resolve(Some(workspace_name), None, &config, |_| {});
            Self {
                _identity: identity,
                _redirect: redirect,
                root,
                workspace,
            }
        }

        fn root(&self) -> &Path {
            &self.root
        }

        fn workspace(&self) -> &baude_core::workspace::Workspace {
            &self.workspace
        }

        /// A created subdirectory of the fixture root, for the scenario-specific
        /// `repo` / `origin.git` / `state` trees the preambles built by hand.
        fn subdir(&self, relative: impl AsRef<Path>) -> PathBuf {
            let path = self.root.join(relative);
            std::fs::create_dir_all(&path).expect("create manager fixture subdirectory");
            path
        }
    }

    #[test]
    fn fixture_isolation_two_fixtures_have_distinct_roots() {
        let first = ManagerFixture::new("distinct-roots");
        let second = ManagerFixture::new("distinct-roots");
        assert_ne!(first.root(), second.root());
        assert!(first.root().is_dir());
        assert!(second.root().is_dir());
    }

    #[test]
    fn fixture_isolation_redirect_outlives_the_construction_statement() {
        let fixture = ManagerFixture::new("redirect-lifetime");
        // Resolved AFTER the constructor returned. A guard that was constructed
        // and dropped inside `new` arms and disarms in the same statement, so
        // this line would run entirely unredirected while the fixture still
        // looks correct at the call site.
        let config_dir = baude_core::persist::config_dir();
        assert!(
            config_dir.starts_with(fixture.root()),
            "config dir {config_dir:?} escaped the fixture root {:?}",
            fixture.root()
        );
        let claude_dir = baude_core::meta::claude_config_dir();
        assert!(
            claude_dir.starts_with(fixture.root()),
            "Claude config dir {claude_dir:?} escaped the fixture root {:?}",
            fixture.root()
        );
    }

    #[test]
    fn fixture_isolation_installs_a_literal_workspace_identity() {
        let fixture = ManagerFixture::new("identity");
        assert_eq!(
            baude_core::workspace::active().name,
            fixture.workspace().name
        );
    }

    #[test]
    fn fixture_isolation_nested_drop_restores_the_enclosing_fixture() {
        let outer = ManagerFixture::new("nested-outer");
        {
            let inner = ManagerFixture::with_workspace_name("nested-inner", "opencode");
            assert!(baude_core::persist::config_dir().starts_with(inner.root()));
            assert_eq!(baude_core::workspace::active().name, "opencode");
        }
        assert!(baude_core::persist::config_dir().starts_with(outer.root()));
        assert_eq!(baude_core::workspace::active().name, outer.workspace().name);
    }

    #[test]
    fn fixture_isolation_drop_removes_its_root() {
        let root = {
            let fixture = ManagerFixture::new("drop-cleanup");
            let root = fixture.root().to_path_buf();
            assert!(root.is_dir());
            root
        };
        assert!(!root.exists(), "fixture root {root:?} survived its owner");
    }

    #[test]
    fn manager_restore_reconciles_current_git_before_spawn_and_persists_failure() {
        let fixture = ManagerFixture::new("manager-reconcile");
        let repo = fixture.subdir("repo");
        let state_root = fixture.subdir("state");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        let snapshot = git::discover_repository(&repo).unwrap();

        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key().unwrap();
        let checkout_key = state.allocate_checkout_key().unwrap();
        let repository_order = state.allocate_first_seen_order().unwrap();
        let checkout_order = state.allocate_first_seen_order().unwrap();
        state.repositories.push(SavedRepository {
            key: repository_key,
            observed_common_dir: PersistedPath::from_path(&snapshot.common_dir),
            observed_main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
            first_seen_order: repository_order,
            health: RepositoryHealth::Available,
            physical_key: repository_key.get().to_string(),
        });
        state.checkouts.push(SavedCheckout::new(
            checkout_key,
            repository_key,
            CheckoutRole::ManagedBranch,
            false,
            PersistedPath::from_path(&snapshot.selected_worktree.path),
            snapshot.selected_worktree.branch.clone(),
            checkout_order,
            CheckoutLifecycle::Active,
            RetainedSessionState {
                name: "changed checkout".into(),
                cwd: PersistedPath::from_path(&snapshot.selected_worktree.path),
                repo_root: PersistedPath::from_path(&snapshot.main_worktree),
                branch: Some("main".into()),
                is_worktree: false,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
        ));
        let workspace = fixture.workspace();
        persist::save_current_at(
            &state_root,
            &workspace.state_file(STATE_BASE),
            &StateFile::new(state),
        )
        .unwrap();
        git(&repo, &["checkout", "-b", "changed"]);

        let mut manager = Manager::new("true".into(), true);
        assert_eq!(manager.restore_at(&state_root, workspace), 0);
        assert!(manager.sessions.is_empty());
        assert!(matches!(
            manager.repository_state.checkouts[0].health(),
            CheckoutHealth::Unavailable(_)
        ));
        let persisted =
            persist::load_current_at(&state_root, &workspace.state_file(STATE_BASE)).unwrap();
        assert!(matches!(
            persisted.state.checkouts[0].health(),
            CheckoutHealth::Unavailable(_)
        ));
    }

    #[test]
    fn manager_persistence_blocks_malformed_state_and_later_saves() {
        let fixture = ManagerFixture::new("manager-persistence");
        let root = fixture.root().to_path_buf();
        let workspace = fixture.workspace();
        let path = root.join(workspace.state_file(STATE_BASE));
        let original = b"{truncated";
        std::fs::write(&path, original).unwrap();

        let mut manager = Manager::new("true".into(), true);
        assert_eq!(manager.restore_at(&root, workspace), 0);
        assert!(manager.persistence_blocked);
        manager.save_at(&root, workspace);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(manager.create("/tmp", None, None).is_err());
        assert!(manager.sessions.is_empty());
    }

    #[test]
    fn manager_persistence_migrates_selected_legacy_once_without_field_loss() {
        let fixture = ManagerFixture::new("manager-legacy");
        let root = fixture.root().to_path_buf();
        let workspace = fixture.workspace();
        let legacy_path = root.join(workspace.legacy_state_file(STATE_BASE).unwrap());
        let legacy = baude_core::persist::State {
            sessions: vec![baude_core::persist::SavedSession {
                name: "retained daemon".into(),
                cwd: PathBuf::from("/missing/checkout"),
                repo_root: PathBuf::from("/missing/repository"),
                branch: Some("feature/retained".into()),
                is_worktree: true,
                shell_open: true,
                archived: true,
                archived_by_user: true,
            }],
        };
        std::fs::write(&legacy_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let mut manager = Manager::new("true".into(), true);
        assert_eq!(manager.restore_at(&root, workspace), 0);
        assert!(!manager.persistence_blocked);
        assert_eq!(manager.repository_state.checkouts.len(), 1);
        let retained = &manager.repository_state.checkouts[0].session;
        assert_eq!(retained.name, "retained daemon");
        assert_eq!(retained.branch.as_deref(), Some("feature/retained"));
        assert!(retained.is_worktree);
        assert!(retained.shell_open);
        assert!(retained.archived);
        assert!(retained.archived_by_user);
        let first = manager.repository_state.clone();
        assert_eq!(manager.restore_at(&root, workspace), 0);
        assert_eq!(manager.repository_state, first);
        assert_eq!(
            std::fs::read(&legacy_path).unwrap(),
            serde_json::to_vec_pretty(&legacy).unwrap()
        );
    }

    #[test]
    fn create_list_info_remove() {
        let _fixture = ManagerFixture::new("create-list-info-remove");
        let mut m = mgr();
        let info = m.create("/tmp", None, Some("t1")).unwrap();
        assert_eq!(info.name, "t1");
        assert_eq!(m.list().len(), 1);
        // macOS canonicalizes /tmp to /private/tmp
        assert!(m.info(info.id).unwrap().cwd.ends_with("/tmp"));
        assert!(m.info(99).is_none());
        let _reapers = m.runtime_reapers_for_test();
        m.remove(info.id).unwrap();
        assert!(m.list().is_empty());
        assert!(m.remove(info.id).is_err());
    }

    #[test]
    fn exhausted_durable_counter_rejects_create_before_spawn() {
        let _fixture = ManagerFixture::new("exhausted-durable-counter-rejects-create-before-spawn");
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.repository_state.next_standalone_key = u64::MAX - 1;

        let error = match manager.create("/tmp", None, None) {
            Ok(_) => panic!("counter exhaustion unexpectedly spawned a session"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("StandaloneKeysExhausted"), "got: {error}");
        assert!(manager.sessions.is_empty());
        assert!(manager.runtime_checkouts.is_empty());
        assert!(manager.runtime_standalones.is_empty());
        assert!(manager.repository_state.standalone_sessions.is_empty());
        assert_eq!(manager.next_id, 1);
    }

    /// The transaction-scenario fixture.
    ///
    /// It returns the OWNER, not `(root, workspace)`. The previous shape
    /// constructed a `TestRedirect` and dropped it at the `return`, so every
    /// line in every caller ran unredirected while the call site still read
    /// like an isolated fixture — the exact failure mode this helper now makes
    /// impossible to express.
    #[must_use = "the returned fixture owns this case's root, redirects and identity; bind it to a \
                  named local that outlives the Manager it isolates"]
    fn persistence_fixture(label: &str) -> ManagerFixture {
        ManagerFixture::new(&format!("transaction-{label}"))
    }

    /// Returns its `ManagerFixture` in place of the bare root and workspace it
    /// used to hand back, for the same reason as [`persistence_fixture`]: the
    /// guard has to outlive the call.
    #[must_use = "the returned fixture owns this case's root, redirects and identity; bind it to a \
                  named local that outlives the Manager, its sessions and its worktrees"]
    fn removal_manager(
        label: &str,
        offset: u64,
    ) -> (
        Manager,
        PathBuf,
        ManagerFixture,
        PathBuf,
        CheckoutKey,
        u64,
        PathBuf,
    ) {
        // Contains managed worktree allocation (issue #72) before the manager
        // can activate a branch and create one under the real data dir.
        let fixture = ManagerFixture::new(label);
        let repo = fixture.subdir("repo");
        let origin = fixture.subdir("origin.git");
        let state_root = fixture.subdir("state");
        git(&origin, &["init", "--bare", "-b", "main"]);
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        // Stops at `push -u`, the shape `gh repo create` leaves behind. Writing
        // refs/remotes/origin/HEAD here would hide whether admission works for a repo
        // that never went through `git clone`.
        git(&repo, &["push", "-u", "origin", "main"]);
        let workspace = fixture.workspace();
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id()) + offset;
        manager.persist_at_for_test(&state_root, workspace, None);
        let created = manager
            .activate_branch_worktree(&repo, &format!("feature/{label}"), None)
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        let path = manager.repository_state.checkouts[0]
            .observed_path
            .to_path_buf();
        (manager, state_root, fixture, repo, checkout, runtime, path)
    }

    /// A real repository with one commit under `root`, for tests whose subject
    /// is checkout lifecycle and so need a path git reports as a repository.
    fn committed_repo(root: &Path) -> PathBuf {
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        repo
    }

    fn persisted_at(root: &Path, workspace: &baude_core::workspace::Workspace) -> RepositoryState {
        persist::load_current_at(root, &workspace.state_file(STATE_BASE))
            .unwrap()
            .state
    }

    /// WLOCK-01/03 test debt (D-12): bauded never claims the workspace lock at
    /// startup, so its existing contention surface is the FIRST SAVE. A held
    /// lock must surface `StateLockError::Held`'s pid + lock-path diagnostic
    /// through `save_checked`, and the refused save must leave the other
    /// owner's lock file untouched (WLOCK-02).
    #[test]
    fn held_lock_save_refuses_with_pid_diagnostic() {
        use std::io::Write as _;

        let fixture = ManagerFixture::new("held-lock");
        let state_root = fixture.subdir("state");
        let mut manager = Manager::new("true".into(), true);
        manager.persist_at_for_test(&state_root, fixture.workspace(), None);

        // Mirror persist's private lock_path shape: ".{file_name}.lock"
        // sibling of the destination the save will contend on.
        let destination = state_root.join(fixture.workspace().state_file(STATE_BASE));
        let file_name = destination
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let lock_path = destination.with_file_name(format!(".{file_name}.lock"));
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();

        // Simulate ANOTHER owner with a raw handle: two file descriptions in
        // one process DO conflict under try_lock. The handle stays alive
        // across the save attempt.
        let mut holder = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        holder.try_lock().unwrap();
        writeln!(holder, "424242").unwrap();
        holder.flush().unwrap();

        let error = manager.save_checked().unwrap_err();
        assert!(
            !error.replacement_committed(),
            "refusal must happen before any replacement"
        );
        let message = error.to_string();
        assert!(
            message.contains("already owns this workspace"),
            "message must carry the ownership phrase: {message}"
        );
        assert!(
            message.contains("pid 424242"),
            "message must carry the holder pid: {message}"
        );
        assert!(
            message.contains(lock_path.file_name().unwrap().to_str().unwrap()),
            "message must name the lock file: {message}"
        );

        // WLOCK-02: the refused save removed nothing — the other owner's lock
        // file survives with its stamp intact.
        assert!(lock_path.exists());
        assert_eq!(
            std::fs::read_to_string(&lock_path).unwrap().trim(),
            "424242"
        );

        drop(holder);
        // ManagerFixture::drop removes the root.
    }

    #[test]
    fn create_persistence_failure_keeps_memory_process_and_disk_consistent() {
        let fixture = persistence_fixture("create");
        let root = fixture.root().to_path_buf();
        let workspace = fixture.workspace();
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.persist_at_for_test(&root, workspace, Some(persist::AtomicFailure::Rename));

        assert!(manager.create("/tmp", None, Some("pre")).is_err());
        assert!(manager.repository_state.standalone_sessions.is_empty());
        assert!(manager.sessions.is_empty());
        assert!(!root.join(workspace.state_file(STATE_BASE)).exists());

        manager.persist_at_for_test(
            &root,
            workspace,
            Some(persist::AtomicFailure::DirectorySync),
        );
        assert!(manager.create("/tmp", None, Some("post")).is_err());
        assert_eq!(manager.repository_state.standalone_sessions.len(), 1);
        assert!(manager.sessions.is_empty());
        assert!(manager.runtime_checkouts.is_empty());
        assert!(manager.runtime_standalones.is_empty());
        assert_eq!(persisted_at(&root, workspace), manager.repository_state);
    }

    #[test]
    fn lifecycle_create_activate_manager_persists_once_and_reuses_runtime() {
        let fixture = ManagerFixture::new("lifecycle-create-activate");
        let root = fixture.root().to_path_buf();
        let repo = fixture.subdir("repo");
        let state_root = fixture.subdir("state");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "-b", "main"]);
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        // Stops at `push -u`, the shape `gh repo create` leaves behind. Writing
        // refs/remotes/origin/HEAD here would hide whether admission works for a repo
        // that never went through `git clone`.
        git(&repo, &["push", "-u", "origin", "main"]);
        let workspace = fixture.workspace();
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id());
        manager.persist_at_for_test(&state_root, workspace, None);

        let created = manager
            .activate_branch_worktree(&repo, "feature/manager-contract", None)
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        assert_eq!(manager.repository_state.checkouts.len(), 1);
        assert!(manager.repository_state.checkouts[0].managed_by_baude);
        assert_eq!(
            manager.runtime_checkouts,
            HashMap::from([(checkout, runtime)])
        );
        assert_eq!(
            persisted_at(&state_root, workspace),
            manager.repository_state
        );

        assert_eq!(
            manager
                .activate_branch_worktree(&repo, "feature/manager-contract", None)
                .unwrap(),
            LifecycleOutcome::Focused { checkout, runtime }
        );
        assert_eq!(manager.repository_state.checkouts.len(), 1);
        assert_eq!(manager.runtime_checkouts.len(), 1);

        let _reapers = manager.runtime_reapers_for_test();
        manager
            .session_mut(runtime)
            .unwrap()
            .kill_and_wait()
            .unwrap();
        assert_eq!(
            manager
                .activate_branch_worktree(&repo, "feature/manager-contract", None)
                .unwrap(),
            LifecycleOutcome::Reopened { checkout, runtime }
        );
        assert!(!manager.session(runtime).unwrap().claude.is_exited());

        let different = manager
            .activate_branch_worktree(&repo, "feature/manager-distinct", None)
            .unwrap();
        assert!(matches!(
            different,
            LifecycleOutcome::Created {
                checkout: other_checkout,
                runtime: Some(other_runtime),
            } if other_checkout != checkout && other_runtime != runtime
        ));
        assert_eq!(manager.repository_state.checkouts.len(), 2);
        assert_eq!(manager.runtime_checkouts.len(), 2);
        manager.kill_all_and_reap_for_test();
        let linked: Vec<_> = manager
            .repository_state
            .checkouts
            .iter()
            .map(|checkout| checkout.observed_path.to_path_buf())
            .filter(|path| path != &repo)
            .collect();
        for path in linked {
            let _ = Command::new("git")
                .args(["worktree", "remove", "--force", "--"])
                .arg(path)
                .current_dir(&repo)
                .status();
        }
    }

    #[test]
    fn lifecycle_creation_rollback_manager_precommit_save_failure_has_no_partial_child() {
        let fixture = ManagerFixture::new("lifecycle-create-rollback");
        let root = fixture.root().to_path_buf();
        let repo = fixture.subdir("repo");
        let state_root = fixture.subdir("state");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "-b", "main"]);
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        // Stops at `push -u`, the shape `gh repo create` leaves behind. Writing
        // refs/remotes/origin/HEAD here would hide whether admission works for a repo
        // that never went through `git clone`.
        git(&repo, &["push", "-u", "origin", "main"]);
        let workspace = fixture.workspace();
        let mut manager = Manager::new("true".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id()) + 20_000;
        manager.persist_at_for_test(&state_root, workspace, Some(persist::AtomicFailure::Rename));
        let before = manager.repository_state.clone();

        let result = manager.activate_branch_worktree(&repo, "feature/manager-rollback", None);
        let after = git::discover_repository(&repo).unwrap();
        let partial: Vec<_> = after
            .worktrees
            .iter()
            .filter(|record| {
                record.branch.as_deref() == Some("refs/heads/feature/manager-rollback")
            })
            .map(|record| record.path.clone())
            .collect();
        for path in &partial {
            let _ = Command::new("git")
                .args(["worktree", "remove", "--force", "--"])
                .arg(path)
                .current_dir(&repo)
                .status();
        }
        let branch_retained = Command::new("git")
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                "--",
                "refs/heads/feature/manager-rollback",
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success();

        assert!(result.is_err());
        assert!(partial.is_empty(), "save failure left a linked worktree");
        assert_eq!(manager.repository_state, before);
        assert!(manager.runtime_checkouts.is_empty());
        assert!(manager.sessions.is_empty());
        assert!(!branch_retained);
    }

    #[test]
    fn activation_recovery_manager_reuses_occupied_owner_after_pending_save_crash() {
        let fixture = ManagerFixture::new("occupied-pending-recovery");
        let root = fixture.root().to_path_buf();
        let repo = fixture.subdir("repo");
        let state_root = fixture.subdir("state");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        git(&repo, &["branch", "occupied-before-crash"]);
        let occupied = root.join("occupied-before-crash");
        git(
            &repo,
            &[
                "worktree",
                "add",
                occupied.to_str().unwrap(),
                "occupied-before-crash",
            ],
        );
        let workspace = fixture.workspace();
        let snapshot = git::discover_repository(&repo).unwrap();
        let mut crashed = Manager::new("sh -c 'sleep 30'".into(), true);
        crashed.persist_at_for_test(&state_root, workspace, None);
        let prepared = lifecycle::prepare_activation(
            &mut crashed.repository_state,
            &snapshot,
            "occupied-before-crash",
        )
        .unwrap();
        lifecycle::record_pending_activation(&mut crashed.repository_state, &snapshot, &prepared)
            .unwrap();
        crashed.save_checked().unwrap();

        let mut restarted = Manager::new("sh -c 'sleep 30'".into(), true);
        assert_eq!(restarted.restore_at(&state_root, workspace), 1);
        assert_eq!(restarted.repository_state.checkouts.len(), 1);
        let recovered = &restarted.repository_state.checkouts[0];
        assert_eq!(
            recovered.observed_path.to_path_buf(),
            occupied.canonicalize().unwrap()
        );
        assert!(!recovered.managed_by_baude);
        assert!(recovered.active_intent());
        assert_eq!(recovered.health(), &CheckoutHealth::Available);
        assert!(restarted.runtime_checkouts.contains_key(&recovered.key));

        restarted.kill_all_and_reap_for_test();
        git(
            &repo,
            &[
                "worktree",
                "remove",
                "--force",
                "--",
                occupied.to_str().unwrap(),
            ],
        );
    }

    #[test]
    fn lifecycle_creation_rollback_manager_committed_save_and_spawn_failures_retain_retry_child() {
        let fixture = ManagerFixture::new("lifecycle-create-stages");
        let root = fixture.root().to_path_buf();
        let repo = fixture.subdir("repo");
        let state_root = fixture.subdir("state");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "-b", "main"]);
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        // Stops at `push -u`, the shape `gh repo create` leaves behind. Writing
        // refs/remotes/origin/HEAD here would hide whether admission works for a repo
        // that never went through `git clone`.
        git(&repo, &["push", "-u", "origin", "main"]);
        let workspace = fixture.workspace();
        let mut manager = Manager::new("true".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id()) + 40_000;
        manager.persist_at_for_test(
            &state_root,
            workspace,
            Some(persist::AtomicFailure::DirectorySync),
        );

        assert!(manager
            .activate_branch_worktree(&repo, "feature/manager-postcommit", None)
            .is_err());
        assert_eq!(manager.repository_state.checkouts.len(), 1);
        assert_eq!(
            persisted_at(&state_root, workspace),
            manager.repository_state
        );
        assert!(manager.runtime_checkouts.is_empty());
        assert!(manager.repository_state.has_pending_activation());

        let mut reloaded = Manager::new("true".into(), true);
        assert_eq!(reloaded.restore_at(&state_root, workspace), 0);
        assert!(!reloaded.repository_state.has_pending_activation());
        assert!(reloaded.repository_state.checkouts.is_empty());
        assert!(reloaded
            .activate_branch_worktree(&repo, "feature/manager-spawn", None)
            .is_ok());

        let linked: Vec<_> = manager
            .repository_state
            .checkouts
            .iter()
            .map(|checkout| checkout.observed_path.to_path_buf())
            .collect();
        for path in linked {
            let _ = Command::new("git")
                .args(["worktree", "remove", "--force", "--"])
                .arg(path)
                .current_dir(&repo)
                .status();
        }
    }

    #[test]
    fn lifecycle_close_manager_persistence_failure_retains_child_and_parent() {
        for (label, failure, committed) in [
            ("remove-pre", persist::AtomicFailure::Rename, false),
            ("remove-post", persist::AtomicFailure::DirectorySync, true),
        ] {
            let fixture = persistence_fixture(label);
            let root = fixture.root().to_path_buf();
            let workspace = fixture.workspace();
            let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
            manager.persist_at_for_test(&root, workspace, None);
            let repo = committed_repo(&root);
            let id = manager
                .create(repo.to_str().unwrap(), None, Some(label))
                .unwrap()
                .id;
            manager.session_id_for_test(id, &format!("opaque-{label}"));
            manager.session_mut(id).unwrap().open_shell(5, 40).unwrap();
            let before = manager.repository_state.clone();
            let original_pid = manager.session(id).unwrap().claude.pid().unwrap();
            let original_shell_pid = manager
                .session(id)
                .unwrap()
                .shell
                .as_ref()
                .unwrap()
                .pid()
                .unwrap();
            assert!(pid_is_live(original_pid));
            assert!(pid_is_live(original_shell_pid));
            manager.persist_at_for_test(&root, workspace, Some(failure));

            let _reapers = manager.runtime_reapers_for_test();
            let close_error = manager.remove(id).unwrap_err().to_string();
            assert_eq!(manager.repository_state.repositories.len(), 1);
            assert_eq!(manager.repository_state.checkouts.len(), 1);
            assert_eq!(
                manager.repository_state.checkouts[0].active_intent(),
                !committed
            );
            assert_eq!(
                persisted_at(&root, workspace).checkouts[0].active_intent(),
                !committed
            );
            if committed {
                assert!(!pid_is_live(original_pid));
                assert!(!pid_is_live(original_shell_pid));
                assert!(manager.sessions.is_empty());
                assert!(manager.runtime_checkouts.is_empty());
                assert_eq!(
                    manager.repository_state.checkouts[0]
                        .session
                        .resume_id
                        .as_deref(),
                    Some(format!("opaque-{label}").as_str())
                );
                assert!(manager.persistence_dirty);
            } else {
                assert!(!close_error.is_empty());
                assert_eq!(manager.repository_state.repositories, before.repositories);
                assert!(matches!(
                    manager.repository_state.checkouts[0].lifecycle(),
                    CheckoutLifecycle::Running(_)
                ));
                let compensated = *manager.runtime_checkouts.keys().next().unwrap();
                let compensated = manager.runtime_checkouts[&compensated];
                assert_eq!(compensated, id);
                assert!(pid_is_live(original_pid));
                assert!(pid_is_live(original_shell_pid));
                assert!(!manager.session(compensated).unwrap().claude.is_exited());
                assert!(pid_is_live(
                    manager.session(compensated).unwrap().claude.pid().unwrap()
                ));
                assert!(manager.session(compensated).unwrap().shell_open);
                assert!(pid_is_live(
                    manager
                        .session(compensated)
                        .unwrap()
                        .shell
                        .as_ref()
                        .unwrap()
                        .pid()
                        .unwrap()
                ));
                manager.kill_all_and_reap_for_test();
            }
        }
    }

    #[test]
    fn lifecycle_close_manager_success_retains_exact_child_context() {
        let fixture = persistence_fixture("close-success");
        let root = fixture.root().to_path_buf();
        let workspace = fixture.workspace();
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.persist_at_for_test(&root, workspace, None);
        let repo = committed_repo(&root);
        let id = manager
            .create(repo.to_str().unwrap(), None, Some("retained daemon"))
            .unwrap()
            .id;
        manager.repository_state.checkouts[0].session.resume_id =
            Some("opaque-daemon-before-poll".into());
        manager.session_mut(id).unwrap().meta.session_id = None;
        manager.session_mut(id).unwrap().archived = true;
        manager.session_mut(id).unwrap().archived_by_user = true;
        manager.session_mut(id).unwrap().open_shell(5, 40).unwrap();
        manager
            .session_mut(id)
            .unwrap()
            .shell
            .as_ref()
            .unwrap()
            .fail_next_teardown_for_test("shell stop refused once");
        let before = manager.repository_state.clone();

        let _reapers = manager.runtime_reapers_for_test();
        let partial = manager.remove(id).unwrap_err().to_string();
        assert!(partial.contains("shell stopped: false"), "got: {partial}");
        assert!(manager.session(id).unwrap().claude.is_exited());
        assert!(!manager
            .session(id)
            .unwrap()
            .shell
            .as_ref()
            .unwrap()
            .is_exited());
        assert!(manager.repository_state.checkouts[0].active_intent());
        assert!(matches!(
            manager.repository_state.checkouts[0].health(),
            CheckoutHealth::Unavailable(UnavailableCause::TeardownPending {
                agent_stopped: true,
                shell_stopped: false,
                ..
            })
        ));

        let mut restarted = Manager::new("sh -c 'sleep 30'".into(), true);
        assert_eq!(restarted.restore_at(&root, workspace), 0);
        assert!(restarted.sessions.is_empty());
        assert!(restarted.runtime_checkouts.is_empty());
        assert!(!restarted.repository_state.checkouts[0].active_intent());
        assert_eq!(
            restarted.repository_state.checkouts[0].health(),
            &CheckoutHealth::Available
        );
        assert!(manager
            .session(id)
            .unwrap()
            .shell
            .as_ref()
            .unwrap()
            .is_exited());

        manager.remove(id).unwrap();

        assert!(manager.sessions.is_empty());
        assert!(manager.runtime_checkouts.is_empty());
        assert_eq!(manager.repository_state.repositories, before.repositories);
        assert_eq!(manager.repository_state.checkouts.len(), 1);
        let retained = &manager.repository_state.checkouts[0];
        let original = &before.checkouts[0];
        assert_eq!(retained.key, original.key);
        assert_eq!(retained.repository_key, original.repository_key);
        assert_eq!(retained.role, original.role);
        assert_eq!(retained.managed_by_baude, original.managed_by_baude);
        assert_eq!(retained.observed_path, original.observed_path);
        assert_eq!(retained.observed_branch, original.observed_branch);
        assert_eq!(retained.first_seen_order, original.first_seen_order);
        assert!(!retained.active_intent());
        assert_eq!(retained.session.name, "retained daemon");
        assert!(retained.session.archived);
        assert!(retained.session.archived_by_user);
        assert_eq!(
            retained.session.resume_id.as_deref(),
            Some("opaque-daemon-before-poll")
        );
        assert_eq!(persisted_at(&root, workspace), manager.repository_state);
    }

    #[test]
    fn poll_clears_an_immediately_dead_targeted_resume_and_authorizes_reopen() {
        let fixture = persistence_fixture("early-targeted-resume");
        let root = fixture.root().to_path_buf();
        let workspace = fixture.workspace();
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);

        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.persist_at_for_test(&root, workspace, None);
        let runtime = manager
            .create(repo.to_str().unwrap(), None, Some("early targeted resume"))
            .unwrap()
            .id;
        let checkout = manager
            .runtime_checkouts
            .iter()
            .find_map(|(key, id)| (*id == runtime).then_some(*key))
            .unwrap();
        let resume_id = "resume-daemon-early";
        manager.session_id_for_test(runtime, resume_id);
        let _reapers = manager.runtime_reapers_for_test();
        manager.remove(runtime).unwrap();
        let transcript_cwd = manager.repository_state.checkouts[0]
            .observed_path
            .to_path_buf();
        let transcript = baude_core::meta::transcript_file(&transcript_cwd, resume_id);
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, b"{\"type\":\"user\"}\n").unwrap();

        manager.claude_cmd = "sh -c 'exit 1'".into();
        let reopened = match manager.reopen_checkout(checkout).unwrap() {
            LifecycleOutcome::Reopened { runtime, .. } => runtime,
            other => panic!("unexpected reopen outcome: {other:?}"),
        };
        assert!(
            manager.session(reopened).unwrap().targeted_resume,
            "real transcript uses --resume"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !manager.session(reopened).unwrap().claude.is_exited() {
            assert!(Instant::now() < deadline, "resumed child never exited");
            std::thread::sleep(Duration::from_millis(25));
        }
        manager.poll();

        let persisted = persisted_at(&root, workspace);
        assert_eq!(persisted.checkouts[0].session.resume_id, None);
        assert_eq!(
            baude_core::lifecycle::lifecycle_capability(persisted.checkouts[0].lifecycle()),
            Some(baude_core::lifecycle::LifecycleCapability::RetryReopen)
        );
        assert!(!manager.runtime_checkouts.contains_key(&checkout));
    }

    /// The folder counterpart of the test above: the same command double and
    /// the same polling entry, with a plain folder admitted through `create`
    /// in place of the repository. The row is kept, closed, with no resume id.
    #[test]
    fn poll_clears_an_immediately_dead_targeted_resume_for_a_folder_session() {
        let fixture = persistence_fixture("early-targeted-resume-folder");
        let root = fixture.root().to_path_buf();
        let workspace = fixture.workspace();
        let folder = root.join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("todo.txt"), b"water the ferns\n").unwrap();

        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.persist_at_for_test(&root, workspace, None);
        let runtime = manager
            .create(folder.to_str().unwrap(), None, Some("early folder resume"))
            .unwrap()
            .id;
        let key = manager
            .runtime_standalones
            .iter()
            .find_map(|(key, id)| (*id == runtime).then_some(*key))
            .expect("a plain folder is admitted as a standalone row");
        let resume_id = "resume-daemon-early-folder";
        manager.session_id_for_test(runtime, resume_id);
        let _reapers = manager.runtime_reapers_for_test();
        manager.remove(runtime).unwrap();
        let closed = persisted_at(&root, workspace);
        assert_eq!(
            closed.standalone_sessions[0].session.resume_id.as_deref(),
            Some(resume_id),
            "a close retains the conversation the session reported"
        );
        let transcript_cwd = closed.standalone_sessions[0].canonical_path.to_path_buf();
        let transcript = baude_core::meta::transcript_file(&transcript_cwd, resume_id);
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, b"{\"type\":\"user\"}\n").unwrap();

        manager.claude_cmd = "sh -c 'exit 1'".into();
        let reopened = manager.reopen_standalone(key).unwrap();
        assert_ne!(reopened, runtime);
        assert!(
            manager.session(reopened).unwrap().targeted_resume,
            "real transcript uses --resume"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !manager.session(reopened).unwrap().claude.is_exited() {
            assert!(Instant::now() < deadline, "resumed child never exited");
            std::thread::sleep(Duration::from_millis(25));
        }
        manager.poll();

        let persisted = persisted_at(&root, workspace);
        assert_eq!(persisted.standalone_sessions.len(), 1);
        let row = &persisted.standalone_sessions[0];
        assert_eq!(row.key, key);
        assert_eq!(row.session.resume_id, None);
        assert_eq!(row.lifecycle(), &StandaloneLifecycle::Inactive);
        assert_eq!(row.owned_runtime(), None);
        assert!(manager.list().is_empty());
        assert!(!manager.runtime_standalones.contains_key(&key));
    }

    #[test]
    fn lifecycle_reopen_manager_reuses_one_runtime_and_preserves_failed_save() {
        let fixture = persistence_fixture("reopen-manager");
        let root = fixture.root().to_path_buf();
        let workspace = fixture.workspace();
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.persist_at_for_test(&root, workspace, None);
        let runtime = manager
            .create(repo.to_str().unwrap(), None, Some("retained daemon"))
            .unwrap()
            .id;
        let checkout = manager
            .runtime_checkouts
            .iter()
            .find_map(|(key, id)| (*id == runtime).then_some(*key))
            .unwrap();
        manager.session_id_for_test(runtime, "opaque-daemon-target");
        let _reapers = manager.runtime_reapers_for_test();
        manager.remove(runtime).unwrap();

        let outcome = manager.reopen_checkout(checkout).unwrap();
        let reopened_runtime = match outcome {
            LifecycleOutcome::Reopened {
                checkout: key,
                runtime,
            } if key == checkout => runtime,
            other => panic!("unexpected reopen outcome: {other:?}"),
        };
        assert_eq!(manager.runtime_checkouts.len(), 1);
        assert!(manager.repository_state.checkouts[0].active_intent());
        assert_eq!(
            manager.reopen_checkout(checkout).unwrap(),
            LifecycleOutcome::Focused {
                checkout,
                runtime: reopened_runtime,
            }
        );
        assert_eq!(manager.runtime_checkouts.len(), 1);

        let _reopened_reapers = manager.runtime_reapers_for_test();
        manager.remove(reopened_runtime).unwrap();
        manager.persist_at_for_test(&root, workspace, Some(persist::AtomicFailure::Rename));
        assert!(manager.reopen_checkout(checkout).is_err());
        assert!(!manager.repository_state.checkouts[0].active_intent());
        assert!(manager.runtime_checkouts.is_empty());
        assert!(manager.sessions.is_empty());
    }

    #[test]
    fn lifecycle_remove_clean_manager_uses_the_shared_child_only_transaction() {
        let fixture = ManagerFixture::new("lifecycle-safe-remove");
        let repo = fixture.subdir("repo");
        let origin = fixture.subdir("origin.git");
        let state_root = fixture.subdir("state");
        git(&origin, &["init", "--bare", "-b", "main"]);
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        // Stops at `push -u`, the shape `gh repo create` leaves behind. Writing
        // refs/remotes/origin/HEAD here would hide whether admission works for a repo
        // that never went through `git clone`.
        git(&repo, &["push", "-u", "origin", "main"]);
        let workspace = fixture.workspace();
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id()) + 110_000;
        manager.persist_at_for_test(&state_root, workspace, None);
        let created = manager
            .activate_branch_worktree(&repo, "feature/safe-remove-manager", None)
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        manager.session_id_for_test(runtime, "fresh-manager-removal-target");
        let path = manager.repository_state.checkouts[0]
            .observed_path
            .to_path_buf();
        // A real conversation has a transcript; a restore must not drop it as
        // a dead id (SQ-4).
        let transcript = baude_core::meta::transcript_file(&path, "fresh-manager-removal-target");
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, b"{\"type\":\"user\"}\n").unwrap();

        manager.persist_at_for_test(&state_root, workspace, Some(persist::AtomicFailure::Rename));
        let _reapers = manager.runtime_reapers_for_test();
        let confirmation = manager.prepare_remove_worktree(checkout).unwrap();
        let error = manager
            .confirm_remove_worktree(confirmation)
            .unwrap_err()
            .to_string();
        assert!(!error.is_empty());
        assert!(path.exists());
        assert_eq!(manager.runtime_checkouts.len(), 1);
        assert_eq!(manager.sessions.len(), 1);
        assert_eq!(
            manager.repository_state.checkouts[0]
                .session
                .resume_id
                .as_deref(),
            Some("fresh-manager-removal-target")
        );
        assert_eq!(
            persisted_at(&state_root, workspace).checkouts[0]
                .session
                .resume_id
                .as_deref(),
            None
        );
        manager.persist_at_for_test(&state_root, workspace, None);
        manager.save_checked().unwrap();
        let before = manager.repository_state.clone();

        let confirmation = manager.prepare_remove_worktree(checkout).unwrap();
        let removed = manager.confirm_remove_worktree(confirmation).unwrap();

        assert!(
            matches!(removed, LifecycleOutcome::Removed { checkout: key, .. } if key == checkout)
        );
        assert!(manager.repository_state.checkouts.is_empty());
        assert_eq!(manager.repository_state.repositories, before.repositories);
        assert!(manager.runtime_checkouts.is_empty());
        assert!(manager.sessions.is_empty());
        assert_eq!(
            persisted_at(&state_root, workspace),
            manager.repository_state
        );
        assert!(Command::new("git")
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                "--",
                "refs/heads/feature/safe-remove-manager"
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn lifecycle_remove_manager_partial_teardown_is_durable_and_retryable() {
        for (label, fail_agent, offset) in [
            ("remove-agent-partial", true, 120_000),
            ("remove-shell-partial", false, 130_000),
        ] {
            let (mut manager, state_root, fixture, _repo, checkout, runtime, path) =
                removal_manager(label, offset);
            let workspace = fixture.workspace();
            manager
                .session_mut(runtime)
                .unwrap()
                .open_shell(5, 40)
                .unwrap();
            if fail_agent {
                manager
                    .session(runtime)
                    .unwrap()
                    .claude
                    .fail_next_teardown_for_test("agent stop refused once");
            } else {
                manager
                    .session(runtime)
                    .unwrap()
                    .shell
                    .as_ref()
                    .unwrap()
                    .fail_next_teardown_for_test("shell stop refused once");
            }
            let _reapers = manager.runtime_reapers_for_test();
            let confirmation = manager.prepare_remove_worktree(checkout).unwrap();

            let error = manager
                .confirm_remove_worktree(confirmation.clone())
                .unwrap_err()
                .to_string();
            assert!(error.contains("runtime stop failed"), "got: {error}");
            assert!(path.exists());
            assert!(manager.runtime_checkouts.contains_key(&checkout));
            assert!(matches!(
                manager.repository_state.checkouts[0].health(),
                CheckoutHealth::Unavailable(UnavailableCause::TeardownPending { .. })
            ));
            assert_eq!(
                persisted_at(&state_root, workspace),
                manager.repository_state
            );

            assert!(matches!(
                manager.confirm_remove_worktree(confirmation).unwrap(),
                LifecycleOutcome::Removed { checkout: key, .. } if key == checkout
            ));
            assert!(!path.exists());
            assert!(manager.sessions.is_empty());
            assert!(manager.runtime_checkouts.is_empty());
        }
    }

    #[test]
    fn archive_persistence_failure_rolls_back_or_keeps_the_replacement() {
        for (label, failure, committed) in [
            ("archive-pre", persist::AtomicFailure::Rename, false),
            ("archive-post", persist::AtomicFailure::DirectorySync, true),
        ] {
            let fixture = persistence_fixture(label);
            let root = fixture.root().to_path_buf();
            let workspace = fixture.workspace();
            let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
            manager.persist_at_for_test(&root, workspace, None);
            let repo = committed_repo(&root);
            let id = manager
                .create(repo.to_str().unwrap(), None, Some(label))
                .unwrap()
                .id;
            manager.persist_at_for_test(&root, workspace, Some(failure));

            assert!(manager.set_archived(id, true).is_err());
            assert_eq!(manager.info(id).unwrap().archived, committed);
            assert_eq!(
                persisted_at(&root, workspace).checkouts[0].session.archived,
                committed
            );
            manager.kill_all_and_reap_for_test();
        }
    }

    #[test]
    fn event_path_resolves_per_sid_and_404s_unknown() {
        let _fixture = ManagerFixture::new("event-path-resolves-per-sid-and-404s-unknown");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // No sid resolved yet → Ok(None).
        assert!(matches!(m.event_path(id), Ok(None)));
        // Pin a sid → Ok(Some(the /tmp event path)).
        let sid = format!("mgr-evpath-{}", std::process::id());
        m.session_id_for_test(id, &sid);
        let p = m.event_path(id).unwrap().unwrap();
        assert_eq!(
            p,
            std::path::PathBuf::from(baude_core::hook::event_path(&sid))
        );
        // Unknown id → Err (→ 404 upstream).
        assert!(m.event_path(9999).is_err());
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn activity_returns_recent_slice_and_404s_unknown() {
        let _fixture = ManagerFixture::new("activity-returns-recent-slice-and-404s-unknown");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        let sid = format!("mgr-activity-{}", std::process::id());
        let path = baude_core::hook::event_path(&sid);
        let _ = std::fs::remove_file(&path);
        std::fs::write(
            &path,
            concat!(
                r#"{"event":"UserPromptSubmit","ts":1}"#,
                "\n",
                r#"{"event":"PostToolUse","tool":"Read","ts":2}"#,
                "\n",
                r#"{"event":"Stop","ts":3}"#,
                "\n",
            ),
        )
        .unwrap();
        m.session_id_for_test(id, &sid);
        // Drive read_event_tail so the ring fills from the on-disk file.
        m.poll_claude_meta_for_test(id);

        // The last 2 events, newest at back.
        let recent = m.activity(id, 2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].event, "PostToolUse");
        assert_eq!(recent[1].event, "Stop");
        assert_eq!(recent[1].ts, 3);

        // SessionInfo carries a bounded recent set too.
        let info = m.info(id).unwrap();
        assert_eq!(info.activity.len(), 3);
        assert_eq!(info.activity.last().unwrap().event, "Stop");

        // Unknown id → Err (→ 404 upstream).
        assert!(m.activity(9999, 10).is_err());

        let _ = std::fs::remove_file(&path);
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn duplicate_names_get_suffixed() {
        let _fixture = ManagerFixture::new("duplicate-names-get-suffixed");
        let mut m = mgr();
        let a = m.create("/tmp", None, None).unwrap();
        let b = m.create("/tmp", None, None).unwrap();
        assert_eq!(a.name, "tmp");
        assert_eq!(b.name, "tmp (2)");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn message_rejected_while_starting() {
        let _fixture = ManagerFixture::new("message-rejected-while-starting");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // The stub never writes a sessions/<pid>.json, so the daemon must
        // refuse rather than write into a not-yet-listening PTY.
        let err = m.post_message(id, "hello").unwrap_err().to_string();
        assert!(err.contains("starting"), "got: {err}");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn keys_drive_a_shell_and_screen_reads_back() {
        let _fixture = ManagerFixture::new("keys-drive-a-shell-and-screen-reads-back");
        // Wrap the shell so the spawn-site permission flag (appended to the
        // base cmd by `spawn`, default `--dangerously-skip-permissions`) lands
        // as the harmless `$0` of `sh -c` instead of breaking bash's arg
        // parsing. Production uses `claude`, which accepts the flag.
        let mut m = Manager::new("sh -c 'exec bash --norc -i'".into(), false);
        let id = m.create("/tmp", None, None).unwrap().id;
        // Let the shell come up, type a command, read it off the screen.
        std::thread::sleep(Duration::from_millis(800));
        m.send_keys(id, &["echo peek-ok".into(), "enter".into()])
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let shot = m.screen(id).unwrap();
            if shot.text.contains("peek-ok") {
                assert_eq!((shot.rows, shot.cols), (40, 120));
                break;
            }
            assert!(Instant::now() < deadline, "screen never showed output");
            std::thread::sleep(Duration::from_millis(300));
        }
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn restart_requires_exited() {
        let _fixture = ManagerFixture::new("restart-requires-exited");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        let err = m.restart(id).unwrap_err().to_string();
        assert!(err.contains("still running"), "got: {err}");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn restart_respawns_an_exited_session() {
        let _fixture = ManagerFixture::new("restart-respawns-an-exited-session");
        let mut m = Manager::new("true".into(), false);
        let id = m.create("/tmp", None, None).unwrap().id;
        let deadline = Instant::now() + Duration::from_secs(10);
        while m.info(id).unwrap().status != "exited" {
            assert!(Instant::now() < deadline, "stub never exited");
            std::thread::sleep(Duration::from_millis(100));
        }
        let _reapers = m.runtime_reapers_for_test();
        m.restart(id).unwrap();
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn archive_toggles() {
        let _fixture = ManagerFixture::new("archive-toggles");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        assert!(!m.info(id).unwrap().archived);
        m.set_archived(id, true).unwrap();
        assert!(m.info(id).unwrap().archived);
        m.set_archived(id, false).unwrap();
        assert!(!m.info(id).unwrap().archived);
        assert!(m.set_archived(99, true).is_err());
        m.kill_all_and_reap_for_test();
    }

    fn proc_state(pid: u32) -> String {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Poll `ps` until the child reaches the wanted state. The whole workspace
    /// suite runs these PTY tests in parallel, so the budget is generous; on
    /// timeout the panic carries the full `ps` row (state, start time, command)
    /// because a wrong-looking state usually means a recycled pid, not a missed
    /// signal.
    fn wait_for_state(pid: u32, pred: impl Fn(&str) -> bool, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let state = proc_state(pid);
            if pred(&state) {
                return;
            }
            if Instant::now() >= deadline {
                let row = std::process::Command::new("ps")
                    .args(["-o", "pid=,stat=,lstart=,command=", "-p", &pid.to_string()])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|error| format!("<ps failed: {error}>"));
                panic!("timed out waiting for pid {pid} to be {what}; last state {state:?}; ps row: {row}");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn child_pid(m: &Manager, id: u64) -> u32 {
        m.sessions
            .iter()
            .find(|s| s.id == id)
            .expect("session")
            .claude
            .process_identity()
            .pid
    }

    #[test]
    fn daemon_archive_endpoint_suspends_and_unarchive_resumes() {
        let _fixture = ManagerFixture::new("daemon-archive-suspends");
        let mut m = mgr();
        m.set_idle_child_policy_for_test(baude_core::persist::IdleChildPolicy::Suspend);
        let id = m.create("/tmp", None, None).unwrap().id;
        let pid = child_pid(&m, id);
        wait_for_state(pid, |st| !st.is_empty() && !st.starts_with('T'), "running");
        m.set_archived(id, true).unwrap();
        let info = m.info(id).unwrap();
        assert!(
            info.archived && info.suspended,
            "archive suspends under policy=suspend"
        );
        wait_for_state(pid, |st| st.starts_with('T'), "stopped");
        m.set_archived(id, false).unwrap();
        let info = m.info(id).unwrap();
        assert!(
            !info.archived && !info.suspended,
            "unarchive resumes the child"
        );
        wait_for_state(
            pid,
            |st| !st.is_empty() && !st.starts_with('T'),
            "running again",
        );
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn daemon_archive_keeps_child_under_default_policy() {
        let _fixture = ManagerFixture::new("daemon-archive-keep");
        let mut m = mgr();
        m.set_idle_child_policy_for_test(baude_core::persist::IdleChildPolicy::Keep);
        let id = m.create("/tmp", None, None).unwrap().id;
        let pid = child_pid(&m, id);
        wait_for_state(pid, |st| !st.is_empty() && !st.starts_with('T'), "running");
        m.set_archived(id, true).unwrap();
        let info = m.info(id).unwrap();
        assert!(
            info.archived && !info.suspended,
            "keep leaves the child alone"
        );
        std::thread::sleep(Duration::from_millis(100));
        assert!(!proc_state(pid).starts_with('T'), "keep never SIGSTOPs");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn daemon_auto_archive_applies_idle_child_policy() {
        let _fixture = ManagerFixture::new("daemon-auto-archive-policy");
        let mut m = mgr();
        m.set_idle_child_policy_for_test(baude_core::persist::IdleChildPolicy::Suspend);
        let idle = 60_000;
        m.auto_archive_ms = idle;
        let id = m.create("/tmp", None, None).unwrap().id;
        let pid = child_pid(&m, id);
        wait_for_state(pid, |st| !st.is_empty() && !st.starts_with('T'), "running");
        // Fake a session that went idle well past the threshold.
        let s = m.sessions.iter_mut().find(|s| s.id == id).unwrap();
        s.meta.claude_status = Some((false, now_unix_ms() - 2 * idle));
        m.poll();
        let info = m.info(id).unwrap();
        assert!(info.archived, "long-waiting session should auto-archive");
        assert!(info.suspended, "auto-archive applies policy=suspend");
        wait_for_state(pid, |st| st.starts_with('T'), "stopped");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn session_info_reports_suspended() {
        let _fixture = ManagerFixture::new("session-info-suspended");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        assert!(!m.info(id).unwrap().suspended);
        let s = m.sessions.iter_mut().find(|s| s.id == id).unwrap();
        s.archived = true;
        s.suspend_idle_child().expect("suspend");
        assert!(m.info(id).unwrap().suspended);
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn manual_unarchive_survives_the_auto_archive_tick() {
        let _fixture = ManagerFixture::new("manual-unarchive-survives-the-auto-archive-tick");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // Fake a session that went idle well past the threshold.
        let idle = 60_000;
        let s = m.sessions.iter_mut().find(|s| s.id == id).unwrap();
        s.meta.claude_status = Some((false, now_unix_ms() - 2 * idle));
        assert!(
            s.auto_archive_tick(idle),
            "long-waiting session should park"
        );
        // Unarchiving must grant a fresh grace period — the waiting clock is
        // still past the threshold, so without it the next tick re-parks.
        m.set_archived(id, false).unwrap();
        let s = m.sessions.iter_mut().find(|s| s.id == id).unwrap();
        assert!(!s.auto_archive_tick(idle), "tick undid a manual unarchive");
        assert!(!m.info(id).unwrap().archived);
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn event_url_is_loopback_default_bind() {
        // The injected $BAUDE_EVENT_URL points at the daemon's own loopback
        // event route for the session (DEFAULT_BIND = 127.0.0.1:8642).
        assert_eq!(
            event_url(7),
            "http://127.0.0.1:8642/sessions/7/event",
            "spawn command must carry BAUDE_EVENT_URL= for the loopback route"
        );
    }

    #[test]
    fn ingest_event_appends_to_resolved_tmp_file() {
        let _fixture = ManagerFixture::new("ingest-event-appends-to-resolved-tmp-file");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // Pin a deterministic claude session_id so the /tmp path is isolated.
        let sid = format!("ingest-test-{}", std::process::id());
        let path = baude_core::hook::event_path(&sid);
        let _ = std::fs::remove_file(&path);
        m.sessions
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .meta
            .session_id = Some(sid.clone());

        m.ingest_event(id, r#"{"schema":1,"event":"UserPromptSubmit"}"#)
            .unwrap();
        m.ingest_event(id, r#"{"schema":1,"event":"Stop"}"#)
            .unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "two posts -> two appended lines");
        assert!(lines[0].contains("UserPromptSubmit"));
        assert!(lines[1].contains("Stop"));

        let _ = std::fs::remove_file(&path);
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn ingest_event_errors_on_unknown_id_and_missing_session_id() {
        let _fixture =
            ManagerFixture::new("ingest-event-errors-on-unknown-id-and-missing-session-id");
        let mut m = mgr();
        // Unknown id -> Err (not panic).
        let err = m.ingest_event(999, "{}").unwrap_err().to_string();
        assert!(err.contains("no session"), "got: {err}");
        // Known id but session_id not resolved yet AND no session_id in the
        // body -> Err (not panic).
        let id = m.create("/tmp", None, None).unwrap().id;
        let err = m.ingest_event(id, "{}").unwrap_err().to_string();
        assert!(err.contains("session_id"), "got: {err}");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn ingest_event_uses_body_session_id_before_meta_resolves() {
        let _fixture =
            ManagerFixture::new("ingest-event-uses-body-session-id-before-meta-resolves");
        // A real session's earliest hook events arrive before the poll loop has
        // resolved meta.session_id. The POSTed line carries the authoritative
        // session_id, so ingest must use it and land the event in the correct
        // /tmp file immediately (no 404 / no loss).
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        assert!(
            m.sessions
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .meta
                .session_id
                .is_none(),
            "precondition: meta.session_id not resolved for a sleep session"
        );
        let sid = format!("ingest-body-sid-{}", std::process::id());
        let path = baude_core::hook::event_path(&sid);
        let _ = std::fs::remove_file(&path);

        let line = format!(r#"{{"schema":1,"event":"UserPromptSubmit","session_id":"{sid}"}}"#);
        m.ingest_event(id, &line).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("UserPromptSubmit"), "got: {contents}");
        assert!(contents.contains(&sid));

        let _ = std::fs::remove_file(&path);
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn session_info_carries_state_source_and_last_tool() {
        let _fixture = ManagerFixture::new("session-info-carries-state-source-and-last-tool");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        let info = m.info(id).unwrap();
        // A freshly spawned stub has no hook/session-file state -> silence.
        assert_eq!(info.state_source, "silence");
        assert!(info.last_tool.is_none());
        // Populate last_tool from the hook event stream and re-read.
        m.sessions
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .meta
            .last_tool = Some(("Bash".to_string(), 1));
        assert_eq!(m.info(id).unwrap().last_tool.as_deref(), Some("Bash"));
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn session_info_sets_waiting_reason_permission() {
        let _fixture = ManagerFixture::new("session-info-sets-waiting-reason-permission");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // No notification yet -> no permission signal (a fresh stub is not
        // in a permission wait).
        assert_ne!(
            m.info(id).unwrap().waiting_reason.as_deref(),
            Some("permission")
        );
        // A permission_prompt notification -> waiting_reason == "permission"
        // (the distinct push + PWA card key off this), regardless of the
        // silence-derived status.
        m.sessions
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .meta
            .last_notification = Some(("permission_prompt".to_string(), 1));
        assert_eq!(
            m.info(id).unwrap().waiting_reason.as_deref(),
            Some("permission")
        );
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn key_encoding() {
        assert_eq!(key_bytes("up", false), b"\x1b[A");
        assert_eq!(key_bytes("up", true), b"\x1bOA");
        assert_eq!(key_bytes("enter", false), b"\r");
        assert_eq!(key_bytes("shift+tab", false), b"\x1b[Z");
        assert_eq!(key_bytes("ctrl+c", false), vec![3]);
        assert_eq!(key_bytes("plain text", false), b"plain text");
    }

    // ==== 04-02 Task 2: pending-permission state + set/resolve ============

    fn pending(req: &str, tool: &str) -> PendingPermission {
        PendingPermission {
            request_id: req.to_string(),
            tool: tool.to_string(),
            input: serde_json::json!({"command": "ls"}),
            ts: now_unix_ms(),
        }
    }

    #[test]
    fn set_pending_and_read_round_trip() {
        let _fixture = ManagerFixture::new("set-pending-and-read-round-trip");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // No pending initially.
        assert!(m.pending(id).unwrap().is_none());
        // Set -> readable.
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        let p = m.pending(id).unwrap().expect("pending present");
        assert_eq!(p.request_id, "r1");
        assert_eq!(p.tool, "Bash");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn set_and_pending_404_on_unknown_id() {
        let _fixture = ManagerFixture::new("set-and-pending-404-on-unknown-id");
        let mut m = mgr();
        assert!(m.set_pending(9999, pending("x", "Bash")).is_err());
        assert!(m.pending(9999).is_err());
        assert!(m.resolve_pending(9999, "allow").is_err());
    }

    #[test]
    fn resolve_clears_pending_and_records_decision() {
        let _fixture = ManagerFixture::new("resolve-clears-pending-and-records-decision");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        // WR-03: scope is no longer a parameter; an "allow" simply resolves the
        // single in-flight request (scope enforcement is deferred).
        m.resolve_pending(id, "allow").unwrap();
        // Pending cleared.
        assert!(m.pending(id).unwrap().is_none());
        // The decision is readable by a waiter (the bridge's poll).
        let d = m.decision(id).unwrap().expect("decision recorded");
        assert_eq!(d.decision, "allow");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn resolve_deny_records_deny() {
        let _fixture = ManagerFixture::new("resolve-deny-records-deny");
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r2", "Write")).unwrap();
        m.resolve_pending(id, "deny").unwrap();
        assert_eq!(m.decision(id).unwrap().unwrap().decision, "deny");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn setting_new_pending_clears_a_stale_decision() {
        let _fixture = ManagerFixture::new("setting-new-pending-clears-a-stale-decision");
        // A fresh permission request must not read the previous turn's decision.
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        m.resolve_pending(id, "allow").unwrap();
        assert!(m.decision(id).unwrap().is_some());
        // New request resets the decision slot.
        m.set_pending(id, pending("r2", "Edit")).unwrap();
        assert!(m.decision(id).unwrap().is_none());
        assert_eq!(m.pending(id).unwrap().unwrap().request_id, "r2");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn timeout_with_no_decision_resolves_to_deny() {
        // SECURITY-CRITICAL (T-04-04 / V4): when the deadline passes with no
        // POSTed decision, the resolution is DENY — never allow. The pure rule
        // lives in baude-core so both binaries' bridges share it.
        use baude_core::permission::decide_with_timeout;
        let none: Option<&str> = None;
        assert_eq!(decide_with_timeout(none, true), "deny"); // deadline passed, no decision
        assert_eq!(decide_with_timeout(Some("allow"), true), "allow"); // decision wins even at deadline
        assert_eq!(decide_with_timeout(Some("deny"), true), "deny");
        // An unknown decision value also coerces to deny (deny-default).
        assert_eq!(decide_with_timeout(Some("bogus"), false), "deny");
        // Before the deadline with no decision yet: keep waiting sentinel.
        assert_eq!(decide_with_timeout(none, false), "");
    }

    #[test]
    fn permission_timeout_s_reads_env_with_safe_default() {
        // Default ~120s; an explicit env value is honored; a garbage value
        // falls back to the default (never 0 / never panics).
        assert!(baude_core::permission::permission_timeout_s() >= 1);
    }

    #[test]
    fn resolve_notifies_a_registered_waiter() {
        let _fixture = ManagerFixture::new("resolve-notifies-a-registered-waiter");
        // Pitfall 4: a waiter registered before the resolve observes the wake.
        // The per-session Notify fires on resolve so a bounded poll/await is
        // promptly woken (the await happens OUTSIDE the manager lock).
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        let notify = m.permission_notify(id).unwrap();
        // Register interest BEFORE resolving.
        let waiter = notify.notified();
        tokio::pin!(waiter);
        // Build a tiny runtime to drive the await deterministically.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            // Not yet resolved: the waiter is pending.
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(50), &mut waiter)
                    .await
                    .is_err(),
                "waiter must block until resolve"
            );
            m.resolve_pending(id, "allow").unwrap();
            // After resolve, the waiter completes promptly.
            tokio::time::timeout(std::time::Duration::from_millis(500), &mut waiter)
                .await
                .expect("resolve must wake the waiter");
        });
        assert_eq!(m.decision(id).unwrap().unwrap().decision, "allow");
        m.kill_all_and_reap_for_test();
    }

    #[test]
    fn info_reports_collisions() {
        // Verify that the /info endpoint returns collisions when manager has recorded them
        let fixture = ManagerFixture::new("info-reports-collisions");
        let mut m = mgr();

        // Pre-seed a collision report into the manager
        let collision_report = baude_core::lifecycle::CollisionReport {
            requested_path: std::path::PathBuf::from("/some/requested/path"),
            allocated_path: std::path::PathBuf::from("/some/allocated-2"),
            owner_common_dir: Some(std::path::PathBuf::from("/tmp/owner/.git")),
            owner_display: "owner-display".to_string(),
            requester_common_dir: std::path::PathBuf::from("/tmp/requester/.git"),
            requester_display: "requester-display".to_string(),
            reason: baude_core::lifecycle::CollisionReason::ForeignMarker,
        };
        m.collisions.push(collision_report.clone());

        // Build the info payload
        let json = serde_json::json!({
            "workspace": baude_core::workspace::active().name,
            "backend": baude_core::workspace::active().backend.name(),
            "collision_count": m.collisions.len(),
            "collisions": m.collisions,
        });

        // Verify collision is reported
        assert_eq!(json["collision_count"], 1);
        assert_eq!(
            json["collisions"][0]["owner_display"].as_str(),
            Some("owner-display")
        );
        assert_eq!(
            json["collisions"][0]["reason"].as_str(),
            Some("ForeignMarker")
        );

        m.kill_all_and_reap_for_test();
        drop(fixture);
    }

    #[test]
    fn manager_admission_collision_recorded() {
        // Verify that when manager admits a repository and a collision is detected,
        // it records the collision report and can be queried via /info
        use std::os::unix::ffi::OsStrExt;
        let fixture = ManagerFixture::new("manager-admission-collision-recorded");
        let mut m = Manager::new("sh -c 'sleep 30'".into(), true); // Use persistence
        let state_root = fixture.subdir("state");
        m.persist_at_for_test(&state_root, fixture.workspace(), None);

        // Create a real git repository with proper remote setup
        let repo = fixture.subdir("test-repo");
        let origin = fixture.subdir("test-repo-origin.git");
        git(&origin, &["init", "--bare", "-b", "main"]);
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.name", "Baude Test"]);
        git(&repo, &["config", "user.email", "baude@example.invalid"]);
        std::fs::write(repo.join("file.txt"), b"test\n").expect("write file");
        git(&repo, &["add", "file.txt"]);
        git(&repo, &["commit", "-q", "-m", "test"]);
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&repo, &["push", "-q", "-u", "origin", "main"]);

        // Pre-seed a foreign marker in the repository's allocated digest directory
        // ManagerFixture defaults to "claude" workspace
        let workspace = "claude";
        let worktrees_base = baude_core::testing::worktrees_base_override()
            .expect("test redirect should set worktrees base")
            .join("baude/worktrees");
        let base = worktrees_base.join(workspace);
        std::fs::create_dir_all(&base).expect("create worktrees base");

        // Compute digest from the git common dir (what ensure_repository will use)
        let snapshot = baude_core::git::discover_repository(&repo).expect("discover repository");
        let digest = baude_core::repository::compute_repository_digest(
            snapshot.common_dir.as_os_str().as_bytes(),
        );
        let target_dir = base.join(format!("repository-{digest}"));
        std::fs::create_dir_all(&target_dir).expect("create collision directory");

        // Write a foreign marker (belonging to a different repo)
        let foreign_path = std::path::PathBuf::from("/tmp/foreign-repo/.git");
        let foreign_marker = baude_core::marker::MarkerMetadata {
            canonical_common_dir: foreign_path.as_os_str().as_bytes().to_vec(),
            scheme_version: 1,
            recorded_at_ms: 1000,
        };
        baude_core::marker::write_marker(&target_dir, &foreign_marker)
            .expect("write foreign marker");

        // Activate a branch worktree - this is where collision detection happens
        let result = m.activate_branch_worktree(&repo, "feature/test", None);

        // The activation should succeed despite the collision
        assert!(
            result.is_ok(),
            "activation should succeed despite collision: {result:?}"
        );

        // Verify collision was recorded
        assert_eq!(
            m.collisions.len(),
            1,
            "manager should have recorded one collision"
        );
        assert_eq!(
            m.collisions[0].reason,
            baude_core::lifecycle::CollisionReason::ForeignMarker
        );
        assert!(
            m.collisions[0]
                .allocated_path
                .to_string_lossy()
                .contains("-2"),
            "allocated path should have -2 suffix: {}",
            m.collisions[0].allocated_path.display()
        );

        m.kill_all_and_reap_for_test();
        drop(fixture);
    }

    /// BL-02, daemon side: a resumed daemon pane runs
    /// `claude --continue 2>/dev/null || exec claude`, so its claude is a
    /// FORKED child that writes `sessions/<child pid>.json`, never the pane
    /// pid's file. The stand-in below behaves that way through a real PTY, and
    /// the continued conversation's transcript predates the spawn by an hour,
    /// so only the session-member lookup can find the model.
    #[test]
    fn resumed_daemon_session_reports_model_from_forked_claude() {
        let fixture = ManagerFixture::new("resumed-daemon-session-reports-model");
        let claude = fixture.root().join("claude");
        let repo = fixture.subdir("repo");
        let physical = repo.canonicalize().unwrap();
        let sid = format!("daemon-continued-{}", std::process::id());

        let transcript = baude_core::meta::transcript_file(&physical, &sid);
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(
            &transcript,
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{sid}\"}}\n\
                 {{\"type\":\"assistant\",\"message\":{{\"model\":\"claude-opus-4-8\"}}}}\n"
            ),
        )
        .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&transcript)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(3600))
            .unwrap();

        let fake = fixture.root().join("fake-claude.sh");
        std::fs::write(
            &fake,
            format!(
                "mkdir -p '{sessions}'\n\
                 now=$(( $(date +%s) * 1000 ))\n\
                 printf '{{\"pid\":%s,\"sessionId\":\"{sid}\",\"cwd\":\"{cwd}\",\"startedAt\":%s,\
                 \"version\":\"2.1.310\",\"kind\":\"interactive\",\"entrypoint\":\"cli\",\
                 \"status\":\"idle\"}}' $$ \"$now\" > '{sessions}'/$$.json\n\
                 exec sleep 30\n",
                sessions = claude.join("sessions").display(),
                cwd = physical.display(),
            ),
        )
        .unwrap();

        let mut m = Manager::new(format!("sh {}", fake.display()), false);
        let id = m
            .spawn(repo.clone(), repo.clone(), None, false, None, true)
            .unwrap();
        let pane_pid = m.session(id).unwrap().claude.pid();

        let deadline = Instant::now() + Duration::from_secs(10);
        let model = loop {
            m.poll_claude_meta_for_test(id);
            let info = m.info(id).unwrap();
            if info.model.is_some() || Instant::now() > deadline {
                break info.model;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let own_file = pane_pid.map(|p| claude.join("sessions").join(format!("{p}.json")));

        m.kill_all_and_reap_for_test();
        drop(fixture);

        assert!(
            own_file.is_some_and(|f| !f.exists()),
            "the forked claude must not have written the pane pid's file"
        );
        assert_eq!(model.as_deref(), Some("claude-opus-4-8"));
    }
}
