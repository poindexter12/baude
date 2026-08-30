//! Session ownership for the daemon. Unlike the TUI, the daemon never kills
//! sessions when a client goes away — only on explicit DELETE or daemon
//! shutdown. State persists to its own file (`daemon-state.json`) so a daemon
//! restart restores every session via `claude --continue`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use tokio::sync::Notify;

use baude_core::backend;
use baude_core::git;
use baude_core::lifecycle::{self, LifecycleOutcome, RepositoryReservations};
use baude_core::meta::{now_unix_ms, ClaudeMeta, HookEvent};
use baude_core::persist::{self, LegacyReconciliation, LoadOutcome, StateFile};
use baude_core::pty::Pty;
use baude_core::repository::{
    CheckoutHealth, CheckoutKey, CheckoutRole, PersistedPath, RepositoryHealth, RepositoryState,
    RetainedSessionState, SavedCheckout, SavedRepository, UnavailableCause,
};
use baude_core::session::{Session, StateSource, Status};

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
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(f),
            Self::Persistence(error) => error.fmt(f),
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
    /// PERM-02: per-session wake handle for the permission long-poll. Set/clear
    /// pending state happens UNDER the manager lock; the bridge/handler then
    /// `notified().await`s on this Arc OUTSIDE the lock so one pending
    /// permission never stalls other sessions (Pitfall 4 — "decide under the
    /// lock, act outside it"). `resolve_pending` fires `notify_waiters()`.
    permission_notify: HashMap<u64, Arc<Notify>>,
    repository_state: RepositoryState,
    runtime_checkouts: HashMap<CheckoutKey, u64>,
    repository_reservations: RepositoryReservations,
    persistence_blocked: bool,
    /// True after a failed save so API owners can surface degraded durability.
    pub persistence_dirty: bool,
    persistence_error: Option<String>,
    #[cfg(test)]
    persistence_target_for_test: Option<(PathBuf, String)>,
    #[cfg(test)]
    atomic_failure_for_test: Option<persist::AtomicFailure>,
    #[cfg(test)]
    spawn_error_for_test: Option<String>,
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

/// One row of `GET /sessions`.
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

fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(rest)
    } else if s == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    } else {
        PathBuf::from(s)
    }
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

/// BAUDED_AUTO_ARCHIVE_MIN env (minutes, 0 disables), then config.json
/// `auto_archive_minutes`, then 30.
pub fn default_auto_archive_ms() -> u64 {
    persist::load_config().auto_archive_ms()
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
    pub fn new(claude_cmd: String, persist: bool) -> Manager {
        Manager {
            sessions: Vec::new(),
            next_id: 1,
            claude_cmd,
            persist,
            auto_archive_ms: default_auto_archive_ms(),
            permission_notify: HashMap::new(),
            repository_state: RepositoryState::default(),
            runtime_checkouts: HashMap::new(),
            repository_reservations: RepositoryReservations::default(),
            persistence_blocked: false,
            persistence_dirty: false,
            persistence_error: None,
            #[cfg(test)]
            persistence_target_for_test: None,
            #[cfg(test)]
            atomic_failure_for_test: None,
            #[cfg(test)]
            spawn_error_for_test: None,
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
    fn restore_at(&mut self, root: &Path, workspace: &baude_core::workspace::Workspace) -> usize {
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
        let checkouts: Vec<_> = self
            .repository_state
            .checkouts
            .iter()
            .filter(|checkout| checkout.active_intent)
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
        restored
    }

    fn reconcile_checkout(&mut self, checkout_key: CheckoutKey) -> bool {
        let Some(checkout_index) = self
            .repository_state
            .checkouts
            .iter()
            .position(|checkout| checkout.key == checkout_key)
        else {
            return false;
        };
        let repository_key = self.repository_state.checkouts[checkout_index].repository_key;
        let Some(repository_index) = self
            .repository_state
            .repositories
            .iter()
            .position(|repository| repository.key == repository_key)
        else {
            return false;
        };
        let checkout = &self.repository_state.checkouts[checkout_index];
        let expected_path = checkout.observed_path.to_path_buf();
        let expected_branch = checkout.observed_branch.clone();
        let expected_common = self.repository_state.repositories[repository_index]
            .observed_common_dir
            .to_path_buf();

        match git::reconcile_checkout(&expected_common, &expected_path, expected_branch.as_deref())
        {
            Ok(_) => {
                self.repository_state.checkouts[checkout_index].health = CheckoutHealth::Available;
                self.repository_state.repositories[repository_index].health =
                    RepositoryHealth::Available;
                true
            }
            Err(error) => {
                let cause = if matches!(error, git::ReconciliationUnavailable::Missing { .. }) {
                    UnavailableCause::Missing
                } else {
                    UnavailableCause::Other(error.to_string())
                };
                self.repository_state.checkouts[checkout_index].health =
                    CheckoutHealth::Unavailable(cause.clone());
                self.repository_state.repositories[repository_index].health =
                    RepositoryHealth::Unavailable(cause);
                false
            }
        }
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
        let saved = if let Some((root, file)) = &self.persistence_target_for_test {
            persist::save_current_at_test(root, file, &state, self.atomic_failure_for_test, None)
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
        for checkout in &mut state.checkouts {
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
            checkout.session = RetainedSessionState {
                name: session.name.clone(),
                cwd: PersistedPath::from_path(&session.cwd),
                repo_root: PersistedPath::from_path(&session.repo_root),
                branch: session.branch.clone(),
                is_worktree: session.is_worktree,
                shell_open: false,
                archived: session.archived,
                archived_by_user: session.archived_by_user,
                resume_id: session.meta.session_id.clone(),
            };
        }
        state
    }

    /// `POST /sessions` — spawn a fresh session in `repo`, optionally in a
    /// managed worktree for `worktree` (branch name).
    pub fn create(
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
        let repo = expand_tilde(repo);
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
                LifecycleOutcome::Busy { .. } => {
                    return Err(anyhow!("repository lifecycle is busy; retry the action").into())
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
            self.ensure_runtime_capacity(&cwd, &repo_root)?;
        }
        if self.persist {
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
            )?;
            if let Err(error) = self.save_checked() {
                if !error.replacement_committed() {
                    self.repository_state = state_before;
                }
                return Err(MutationError::Persistence(error));
            }
            let id = self.spawn(
                cwd,
                repo_root,
                branch,
                is_worktree,
                Some(&session_name),
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
        let snapshot = git::discover_repository(repository_child).map_err(anyhow::Error::new)?;
        let mut next = self.repository_state.clone();
        let prepared = lifecycle::prepare_activation(&mut next, &snapshot, branch)
            .map_err(anyhow::Error::new)?;
        let repository = prepared.request.repository;
        let _reservation = match self.repository_reservations.reserve(repository) {
            Ok(reservation) => reservation,
            Err(busy) => return Ok(busy),
        };
        let activation = lifecycle::execute_activation(&mut next, repository_child, prepared)
            .map_err(anyhow::Error::new)?;
        if let Some(name) = name {
            if let Some(checkout) = next
                .checkouts
                .iter_mut()
                .find(|checkout| checkout.key == activation.checkout)
            {
                checkout.session.name = name.to_owned();
            }
        }
        let state_before = self.repository_state.clone();
        self.repository_state = next;
        if let Err(error) = self.save_checked() {
            if !error.replacement_committed() {
                lifecycle::compensate_uncommitted_activation(&activation).map_err(
                    |compensation| {
                        anyhow!(
                            "{} failed: {error}; {} failed: {compensation}",
                            lifecycle::CreationFailureStage::PersistenceBeforeReplacement,
                            lifecycle::CreationFailureStage::Compensation
                        )
                    },
                )?;
                self.repository_state = state_before;
            }
            return Err(MutationError::Persistence(error));
        }

        if let Some(runtime) = self.runtime_checkouts.get(&activation.checkout).copied() {
            if self.sessions.iter().any(|session| session.id == runtime) {
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
            .spawn(
                activation.path.clone(),
                activation.main_worktree.clone(),
                Some(activation.branch.clone()),
                activation.path != activation.main_worktree,
                Some(&checkout.session.name),
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

    fn record_checkout_intent(
        &mut self,
        cwd: &Path,
        repo_root: &Path,
        branch: Option<String>,
        is_worktree: bool,
        name: String,
    ) -> Result<CheckoutKey> {
        let snapshot = git::discover_repository(cwd).ok();
        let common = snapshot
            .as_ref()
            .map(|snapshot| PersistedPath::from_path(&snapshot.common_dir))
            .unwrap_or_else(|| PersistedPath::from_path(repo_root));
        let repository_key = if let Some(key) = self
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
                observed_main_worktree: PersistedPath::from_path(
                    snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.main_worktree.as_path())
                        .unwrap_or(repo_root),
                ),
                first_seen_order,
                health: if snapshot.is_some() {
                    RepositoryHealth::Available
                } else {
                    RepositoryHealth::Unavailable(UnavailableCause::NotRepository)
                },
            });
            key
        };
        let checkout_key = self.repository_state.allocate_checkout_key()?;
        let first_seen_order = self.repository_state.allocate_first_seen_order()?;
        self.repository_state.checkouts.push(SavedCheckout {
            key: checkout_key,
            repository_key,
            role: CheckoutRole::ManagedBranch,
            managed_by_baude: is_worktree,
            observed_path: PersistedPath::from_path(cwd),
            observed_branch: snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.selected_worktree.branch.clone()),
            first_seen_order,
            active_intent: true,
            session: RetainedSessionState {
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
            health: if snapshot.is_some() {
                CheckoutHealth::Available
            } else {
                CheckoutHealth::Unavailable(UnavailableCause::NotRepository)
            },
        });
        Ok(checkout_key)
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
        self.spawn_with_mode(cwd, repo_root, branch, is_worktree, name, mode)
    }

    fn spawn_with_mode(
        &mut self,
        cwd: PathBuf,
        repo_root: PathBuf,
        branch: Option<String>,
        is_worktree: bool,
        name: Option<&str>,
        mode: backend::SpawnMode,
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
        // each daemon startup.
        be.prepare_cwd(&cwd);

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

        let plan = be.spawn_plan(&resolved.cmd, Some(&event_url(id)), mode);
        let claude = Pty::spawn_with_env(Some(&plan.cmd), &plan.env, &cwd, ROWS, COLS)?;
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
            shell: None,
            shell_open: false,
            spawn_unix_ms: now_unix_ms(),
            meta,
            archived: false,
            archived_by_user: false,
            was_busy: false,
            unarchived_at_ms: None,
            pending_permission: None,
            permission_decision: None,
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
                });
                key
            }
        };
        let checkout_key = self.repository_state.allocate_checkout_key()?;
        let first_seen_order = self.repository_state.allocate_first_seen_order()?;
        self.repository_state.checkouts.push(SavedCheckout {
            key: checkout_key,
            repository_key,
            role: CheckoutRole::ManagedBranch,
            managed_by_baude: session.is_worktree,
            observed_path: PersistedPath::from_path(&session.cwd),
            observed_branch: snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.selected_worktree.branch.clone()),
            first_seen_order,
            active_intent: true,
            session: RetainedSessionState {
                name: session.name.clone(),
                cwd: PersistedPath::from_path(&session.cwd),
                repo_root: PersistedPath::from_path(&session.repo_root),
                branch: session.branch.clone(),
                is_worktree: session.is_worktree,
                shell_open: false,
                archived: session.archived,
                archived_by_user: session.archived_by_user,
                resume_id: None,
            },
            health: if snapshot.is_some() {
                CheckoutHealth::Available
            } else {
                CheckoutHealth::Unavailable(UnavailableCause::NotRepository)
            },
        });
        self.runtime_checkouts.insert(checkout_key, id);
        Ok(())
    }

    fn ensure_runtime_capacity(&self, cwd: &Path, repo_root: &Path) -> Result<()> {
        let mut state = self.repository_state.clone();
        let snapshot = git::discover_repository(cwd).ok();
        let common = snapshot
            .as_ref()
            .map(|snapshot| PersistedPath::from_path(&snapshot.common_dir))
            .unwrap_or_else(|| PersistedPath::from_path(repo_root));
        if !state
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
        Ok(RetainedSessionState {
            name: session.name.clone(),
            cwd: PersistedPath::from_path(&session.cwd),
            repo_root: PersistedPath::from_path(&session.repo_root),
            branch: session.branch.clone(),
            is_worktree: session.is_worktree,
            shell_open: false,
            archived: session.archived,
            archived_by_user: session.archived_by_user,
            resume_id: session.meta.session_id.clone(),
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn stop_removed_runtime(&mut self, checkout: CheckoutKey, id: u64) -> Result<()> {
        self.session_mut(id)?.kill_and_wait()?;
        self.sessions.retain(|session| session.id != id);
        self.runtime_checkouts.remove(&checkout);
        if let Some(notify) = self.permission_notify.remove(&id) {
            notify.notify_waiters();
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn restore_removed_runtime(
        &mut self,
        checkout: CheckoutKey,
        saved: RetainedSessionState,
    ) -> Result<u64> {
        if let Some(id) = self.runtime_checkouts.get(&checkout).copied() {
            return Ok(id);
        }
        let mode = saved
            .resume_id
            .clone()
            .map(backend::SpawnMode::ResumeId)
            .unwrap_or(backend::SpawnMode::ContinueLatest);
        let id = self.spawn_with_mode(
            saved.cwd.to_path_buf(),
            saved.repo_root.to_path_buf(),
            saved.branch.clone(),
            saved.is_worktree,
            Some(&saved.name),
            mode,
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
        if let Some(id) = runtime_id {
            self.stop_removed_runtime(checkout, id).map_err(|error| {
                lifecycle::RemovalFailure::Inspection(format!("runtime stop failed: {error}"))
            })?;
        }

        let target =
            match lifecycle::inspect_confirmed_removal(&self.repository_state, &confirmation) {
                Ok(target) => target,
                Err(failure) => {
                    return self.compensate_failed_removal(checkout, runtime, failure);
                }
            };
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
            shell_open: false,
            archived: session.archived,
            archived_by_user: session.archived_by_user,
            resume_id: session.meta.session_id.clone(),
        };
        let state_before = self.repository_state.clone();
        lifecycle::plan_close(
            &mut self.repository_state,
            lifecycle::CloseRequest {
                checkout: checkout_key,
                runtime: snapshot,
            },
        )
        .map_err(anyhow::Error::new)?;
        let save_error = match self.save_checked() {
            Err(error) if !error.replacement_committed() => {
                self.repository_state = state_before;
                return Err(MutationError::Persistence(error));
            }
            Err(error) => Some(error),
            Ok(()) => None,
        };
        self.session_mut(id)?.kill_and_wait()?;
        self.sessions.retain(|s| s.id != id);
        self.runtime_checkouts.remove(&checkout_key);
        // Wake any lingering permission waiter (it will re-check, find the
        // session gone, and bail) and drop its handle so the map doesn't leak.
        if let Some(n) = self.permission_notify.remove(&id) {
            n.notify_waiters();
        }
        save_error.map_or(Ok(()), |error| Err(MutationError::Persistence(error)))
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
                self.save_checked().map_err(MutationError::Persistence)?;
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
                let id = self.spawn_with_mode(
                    checkout.observed_path.to_path_buf(),
                    saved.repo_root.to_path_buf(),
                    saved.branch.clone(),
                    saved.is_worktree,
                    Some(&saved.name),
                    plan.mode,
                )?;
                if let Ok(session) = self.session_mut(id) {
                    session.archived = saved.archived;
                    session.archived_by_user = saved.archived_by_user;
                    if saved.shell_open {
                        let _ = session.open_shell(ROWS, COLS);
                    }
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
        if !self.session(id)?.claude.is_exited() {
            return Err(anyhow!("claude is still running").into());
        }
        if let Some(checkout_key) = self
            .runtime_checkouts
            .iter()
            .find_map(|(key, runtime_id)| (*runtime_id == id).then_some(*key))
        {
            let reconciled = self.reconcile_checkout(checkout_key);
            self.save_checked().map_err(MutationError::Persistence)?;
            if !reconciled {
                return Err(anyhow!("checkout changed since admission; restart refused").into());
            }
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
        let mode = targeted
            .map(backend::SpawnMode::ResumeId)
            .unwrap_or(backend::SpawnMode::ContinueLatest);
        self.restart_with_mode(id, mode)
    }

    fn restart_with_mode(&mut self, id: u64, mode: backend::SpawnMode) -> MutationResult<()> {
        let be = backend::active();
        let resolved = be.resolve_cmd(&self.claude_cmd);
        let plan = be.spawn_plan(&resolved.cmd, Some(&event_url(id)), mode);
        let s = self.session_mut(id)?;
        be.prepare_cwd(&s.cwd);
        s.claude = Pty::spawn_with_env(Some(&plan.cmd), &plan.env, &s.cwd, ROWS, COLS)?;
        s.spawn_unix_ms = now_unix_ms();
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

    pub fn poll(&mut self) {
        let mut changed = false;
        let idle = self.auto_archive_ms;
        for s in &mut self.sessions {
            s.poll_meta();
            changed |= s.auto_archive_tick(idle);
        }
        if changed {
            self.save();
        }
    }

    /// Park or unpark a session. Archived sessions sort last in clients and
    /// stop sending notifications. A manual archive sticks until unarchived
    /// or re-engaged; an automatic one also lifts when a new turn starts.
    pub fn set_archived(&mut self, id: u64, archived: bool) -> MutationResult<()> {
        let s = self.session_mut(id)?;
        let archived_before = (s.archived, s.archived_by_user);
        s.set_archived(archived);
        match self.save_checked() {
            Ok(()) => Ok(()),
            Err(error) if error.replacement_committed() => Err(MutationError::Persistence(error)),
            Err(error) => {
                let s = self.session_mut(id)?;
                s.archived = archived_before.0;
                s.archived_by_user = archived_before.1;
                Err(MutationError::Persistence(error))
            }
        }
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

    fn mgr() -> Manager {
        Manager::new("sleep 30".into(), false)
    }

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn manager_restore_reconciles_current_git_before_spawn_and_persists_failure() {
        let root =
            std::env::temp_dir().join(format!("bauded-manager-reconcile-{}", std::process::id()));
        let repo = root.join("repo");
        let state_root = root.join("state");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&state_root).unwrap();
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
        });
        state.checkouts.push(SavedCheckout {
            key: checkout_key,
            repository_key,
            role: CheckoutRole::ManagedBranch,
            managed_by_baude: false,
            observed_path: PersistedPath::from_path(&snapshot.selected_worktree.path),
            observed_branch: snapshot.selected_worktree.branch.clone(),
            first_seen_order: checkout_order,
            active_intent: true,
            session: RetainedSessionState {
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
            health: CheckoutHealth::Available,
        });
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        persist::save_current_at(
            &state_root,
            &workspace.state_file(STATE_BASE),
            &StateFile::new(state),
        )
        .unwrap();
        git(&repo, &["checkout", "-b", "changed"]);

        let mut manager = Manager::new("true".into(), true);
        assert_eq!(manager.restore_at(&state_root, &workspace), 0);
        assert!(manager.sessions.is_empty());
        assert!(matches!(
            manager.repository_state.checkouts[0].health,
            CheckoutHealth::Unavailable(_)
        ));
        let persisted =
            persist::load_current_at(&state_root, &workspace.state_file(STATE_BASE)).unwrap();
        assert!(matches!(
            persisted.state.checkouts[0].health,
            CheckoutHealth::Unavailable(_)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manager_persistence_blocks_malformed_state_and_later_saves() {
        let root =
            std::env::temp_dir().join(format!("bauded-manager-persistence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        let path = root.join(workspace.state_file(STATE_BASE));
        let original = b"{truncated";
        std::fs::write(&path, original).unwrap();

        let mut manager = Manager::new("true".into(), true);
        assert_eq!(manager.restore_at(&root, &workspace), 0);
        assert!(manager.persistence_blocked);
        manager.save_at(&root, &workspace);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(manager.create("/tmp", None, None).is_err());
        assert!(manager.sessions.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manager_persistence_migrates_selected_legacy_once_without_field_loss() {
        let root =
            std::env::temp_dir().join(format!("bauded-manager-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
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
        assert_eq!(manager.restore_at(&root, &workspace), 0);
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
        assert_eq!(manager.restore_at(&root, &workspace), 0);
        assert_eq!(manager.repository_state, first);
        assert_eq!(
            std::fs::read(&legacy_path).unwrap(),
            serde_json::to_vec_pretty(&legacy).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_list_info_remove() {
        let mut m = mgr();
        let info = m.create("/tmp", None, Some("t1")).unwrap();
        assert_eq!(info.name, "t1");
        assert_eq!(m.list().len(), 1);
        // macOS canonicalizes /tmp to /private/tmp
        assert!(m.info(info.id).unwrap().cwd.ends_with("/tmp"));
        assert!(m.info(99).is_none());
        m.remove(info.id).unwrap();
        assert!(m.list().is_empty());
        assert!(m.remove(info.id).is_err());
    }

    #[test]
    fn exhausted_durable_counter_rejects_create_before_spawn() {
        let mut manager = Manager::new("sleep 30".into(), true);
        manager.repository_state.next_repository_key = u64::MAX - 1;

        let error = match manager.create("/tmp", None, None) {
            Ok(_) => panic!("counter exhaustion unexpectedly spawned a session"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("RepositoryKeysExhausted"), "got: {error}");
        assert!(manager.sessions.is_empty());
        assert!(manager.runtime_checkouts.is_empty());
        assert_eq!(manager.next_id, 1);
    }

    fn persistence_fixture(label: &str) -> (PathBuf, baude_core::workspace::Workspace) {
        let root =
            std::env::temp_dir().join(format!("bauded-transaction-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        (root, workspace)
    }

    fn persisted_at(root: &Path, workspace: &baude_core::workspace::Workspace) -> RepositoryState {
        persist::load_current_at(root, &workspace.state_file(STATE_BASE))
            .unwrap()
            .state
    }

    #[test]
    fn create_persistence_failure_keeps_memory_process_and_disk_consistent() {
        let (root, workspace) = persistence_fixture("create");
        let mut manager = Manager::new("sleep 30".into(), true);
        manager.persist_at_for_test(&root, &workspace, Some(persist::AtomicFailure::Rename));

        assert!(manager.create("/tmp", None, Some("pre")).is_err());
        assert!(manager.repository_state.checkouts.is_empty());
        assert!(manager.sessions.is_empty());
        assert!(!root.join(workspace.state_file(STATE_BASE)).exists());

        manager.persist_at_for_test(
            &root,
            &workspace,
            Some(persist::AtomicFailure::DirectorySync),
        );
        assert!(manager.create("/tmp", None, Some("post")).is_err());
        assert_eq!(manager.repository_state.checkouts.len(), 1);
        assert!(manager.sessions.is_empty());
        assert!(manager.runtime_checkouts.is_empty());
        assert_eq!(persisted_at(&root, &workspace), manager.repository_state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_create_activate_manager_persists_once_and_reuses_runtime() {
        let root = std::env::temp_dir().join(format!(
            "bauded-lifecycle-create-activate-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let state_root = root.join("state");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&state_root).unwrap();
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
        git(&repo, &["push", "-u", "origin", "main"]);
        git(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id());
        manager.persist_at_for_test(&state_root, &workspace, None);

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
            persisted_at(&state_root, &workspace),
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
        manager.kill_all();
        let linked: Vec<_> = manager
            .repository_state
            .checkouts
            .iter()
            .map(|checkout| checkout.observed_path.to_path_buf())
            .filter(|path| path != &repo)
            .collect();
        for path in linked {
            let _ = Command::new("git")
                .args(["worktree", "remove", "--"])
                .arg(path)
                .current_dir(&repo)
                .status();
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_creation_rollback_manager_precommit_save_failure_has_no_partial_child() {
        let root = std::env::temp_dir().join(format!(
            "bauded-lifecycle-create-rollback-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let state_root = root.join("state");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&state_root).unwrap();
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
        git(&repo, &["push", "-u", "origin", "main"]);
        git(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        let mut manager = Manager::new("true".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id()) + 20_000;
        manager.persist_at_for_test(
            &state_root,
            &workspace,
            Some(persist::AtomicFailure::Rename),
        );
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
                .args(["worktree", "remove", "--"])
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
        assert!(branch_retained);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_creation_rollback_manager_committed_save_and_spawn_failures_retain_retry_child() {
        let root = std::env::temp_dir().join(format!(
            "bauded-lifecycle-create-stages-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let state_root = root.join("state");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&state_root).unwrap();
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
        git(&repo, &["push", "-u", "origin", "main"]);
        git(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        let mut manager = Manager::new("true".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id()) + 40_000;
        manager.persist_at_for_test(
            &state_root,
            &workspace,
            Some(persist::AtomicFailure::DirectorySync),
        );

        assert!(manager
            .activate_branch_worktree(&repo, "feature/manager-postcommit", None)
            .is_err());
        assert_eq!(manager.repository_state.checkouts.len(), 1);
        assert_eq!(
            persisted_at(&state_root, &workspace),
            manager.repository_state
        );
        assert!(manager.runtime_checkouts.is_empty());

        manager.persist_at_for_test(&state_root, &workspace, None);
        manager.spawn_error_for_test = Some("pty unavailable".into());
        let spawn_error = manager
            .activate_branch_worktree(&repo, "feature/manager-spawn", None)
            .unwrap_err()
            .to_string();
        assert!(spawn_error.contains("pty unavailable"));
        assert_eq!(manager.repository_state.checkouts.len(), 2);
        assert_eq!(
            persisted_at(&state_root, &workspace),
            manager.repository_state
        );
        assert!(manager.runtime_checkouts.is_empty());
        assert!(manager.sessions.is_empty());

        let linked: Vec<_> = manager
            .repository_state
            .checkouts
            .iter()
            .map(|checkout| checkout.observed_path.to_path_buf())
            .collect();
        for path in linked {
            let _ = Command::new("git")
                .args(["worktree", "remove", "--"])
                .arg(path)
                .current_dir(&repo)
                .status();
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_close_manager_persistence_failure_retains_child_and_parent() {
        for (label, failure, committed) in [
            ("remove-pre", persist::AtomicFailure::Rename, false),
            ("remove-post", persist::AtomicFailure::DirectorySync, true),
        ] {
            let (root, workspace) = persistence_fixture(label);
            let mut manager = Manager::new("sleep 30".into(), true);
            manager.persist_at_for_test(&root, &workspace, None);
            let id = manager.create("/tmp", None, Some(label)).unwrap().id;
            manager.session_id_for_test(id, &format!("opaque-{label}"));
            let before = manager.repository_state.clone();
            manager.persist_at_for_test(&root, &workspace, Some(failure));

            assert!(manager.remove(id).is_err());
            assert_eq!(manager.sessions.is_empty(), committed);
            assert_eq!(manager.repository_state.repositories.len(), 1);
            assert_eq!(manager.repository_state.checkouts.len(), 1);
            assert_eq!(
                manager.repository_state.checkouts[0].active_intent,
                !committed
            );
            assert_eq!(
                persisted_at(&root, &workspace).checkouts[0].active_intent,
                !committed
            );
            if committed {
                assert_eq!(
                    manager.repository_state.checkouts[0]
                        .session
                        .resume_id
                        .as_deref(),
                    Some(format!("opaque-{label}").as_str())
                );
                assert!(manager.persistence_dirty);
            } else {
                assert_eq!(manager.repository_state, before);
                manager.kill_all();
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn lifecycle_close_manager_success_retains_exact_child_context() {
        let (root, workspace) = persistence_fixture("close-success");
        let mut manager = Manager::new("sleep 30".into(), true);
        manager.persist_at_for_test(&root, &workspace, None);
        let id = manager
            .create("/tmp", None, Some("retained daemon"))
            .unwrap()
            .id;
        manager.session_id_for_test(id, "opaque-daemon-resume");
        manager.session_mut(id).unwrap().archived = true;
        manager.session_mut(id).unwrap().archived_by_user = true;
        let before = manager.repository_state.clone();

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
        assert!(!retained.active_intent);
        assert_eq!(retained.session.name, "retained daemon");
        assert!(retained.session.archived);
        assert!(retained.session.archived_by_user);
        assert_eq!(
            retained.session.resume_id.as_deref(),
            Some("opaque-daemon-resume")
        );
        assert_eq!(persisted_at(&root, &workspace), manager.repository_state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_reopen_manager_reuses_one_runtime_and_preserves_failed_save() {
        let (root, workspace) = persistence_fixture("reopen-manager");
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        let mut manager = Manager::new("sleep 30".into(), true);
        manager.persist_at_for_test(&root, &workspace, None);
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
        assert!(manager.repository_state.checkouts[0].active_intent);
        assert_eq!(
            manager.reopen_checkout(checkout).unwrap(),
            LifecycleOutcome::Focused {
                checkout,
                runtime: reopened_runtime,
            }
        );
        assert_eq!(manager.runtime_checkouts.len(), 1);

        manager.remove(reopened_runtime).unwrap();
        manager.persist_at_for_test(&root, &workspace, Some(persist::AtomicFailure::Rename));
        assert!(manager.reopen_checkout(checkout).is_err());
        assert!(!manager.repository_state.checkouts[0].active_intent);
        assert!(manager.runtime_checkouts.is_empty());
        assert!(manager.sessions.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_remove_clean_manager_uses_the_shared_child_only_transaction() {
        let root = std::env::temp_dir().join(format!(
            "bauded-lifecycle-safe-remove-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let origin = root.join("origin.git");
        let state_root = root.join("state");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&origin).unwrap();
        std::fs::create_dir_all(&state_root).unwrap();
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
        git(&repo, &["push", "-u", "origin", "main"]);
        git(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        let mut manager = Manager::new("sh -c 'sleep 30'".into(), true);
        manager.repository_state.next_repository_key = u64::from(std::process::id()) + 110_000;
        manager.persist_at_for_test(&state_root, &workspace, None);
        let created = manager
            .activate_branch_worktree(&repo, "feature/safe-remove-manager", None)
            .unwrap();
        let (checkout, _) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
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
            persisted_at(&state_root, &workspace),
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
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_persistence_failure_rolls_back_or_keeps_the_replacement() {
        for (label, failure, committed) in [
            ("archive-pre", persist::AtomicFailure::Rename, false),
            ("archive-post", persist::AtomicFailure::DirectorySync, true),
        ] {
            let (root, workspace) = persistence_fixture(label);
            let mut manager = Manager::new("sleep 30".into(), true);
            manager.persist_at_for_test(&root, &workspace, None);
            let id = manager.create("/tmp", None, Some(label)).unwrap().id;
            manager.persist_at_for_test(&root, &workspace, Some(failure));

            assert!(manager.set_archived(id, true).is_err());
            assert_eq!(manager.info(id).unwrap().archived, committed);
            assert_eq!(
                persisted_at(&root, &workspace).checkouts[0]
                    .session
                    .archived,
                committed
            );
            manager.kill_all();
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn event_path_resolves_per_sid_and_404s_unknown() {
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
        m.kill_all();
    }

    #[test]
    fn activity_returns_recent_slice_and_404s_unknown() {
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
        m.kill_all();
    }

    #[test]
    fn duplicate_names_get_suffixed() {
        let mut m = mgr();
        let a = m.create("/tmp", None, None).unwrap();
        let b = m.create("/tmp", None, None).unwrap();
        assert_eq!(a.name, "tmp");
        assert_eq!(b.name, "tmp (2)");
        m.kill_all();
    }

    #[test]
    fn message_rejected_while_starting() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // The stub never writes a sessions/<pid>.json, so the daemon must
        // refuse rather than write into a not-yet-listening PTY.
        let err = m.post_message(id, "hello").unwrap_err().to_string();
        assert!(err.contains("starting"), "got: {err}");
        m.kill_all();
    }

    #[test]
    fn keys_drive_a_shell_and_screen_reads_back() {
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
        m.kill_all();
    }

    #[test]
    fn restart_requires_exited() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        let err = m.restart(id).unwrap_err().to_string();
        assert!(err.contains("still running"), "got: {err}");
        m.kill_all();
    }

    #[test]
    fn restart_respawns_an_exited_session() {
        let mut m = Manager::new("true".into(), false);
        let id = m.create("/tmp", None, None).unwrap().id;
        let deadline = Instant::now() + Duration::from_secs(10);
        while m.info(id).unwrap().status != "exited" {
            assert!(Instant::now() < deadline, "stub never exited");
            std::thread::sleep(Duration::from_millis(100));
        }
        m.restart(id).unwrap();
        m.kill_all();
    }

    #[test]
    fn archive_toggles() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        assert!(!m.info(id).unwrap().archived);
        m.set_archived(id, true).unwrap();
        assert!(m.info(id).unwrap().archived);
        m.set_archived(id, false).unwrap();
        assert!(!m.info(id).unwrap().archived);
        assert!(m.set_archived(99, true).is_err());
        m.kill_all();
    }

    #[test]
    fn manual_unarchive_survives_the_auto_archive_tick() {
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
        m.kill_all();
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
        m.kill_all();
    }

    #[test]
    fn ingest_event_errors_on_unknown_id_and_missing_session_id() {
        let mut m = mgr();
        // Unknown id -> Err (not panic).
        let err = m.ingest_event(999, "{}").unwrap_err().to_string();
        assert!(err.contains("no session"), "got: {err}");
        // Known id but session_id not resolved yet AND no session_id in the
        // body -> Err (not panic).
        let id = m.create("/tmp", None, None).unwrap().id;
        let err = m.ingest_event(id, "{}").unwrap_err().to_string();
        assert!(err.contains("session_id"), "got: {err}");
        m.kill_all();
    }

    #[test]
    fn ingest_event_uses_body_session_id_before_meta_resolves() {
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
        m.kill_all();
    }

    #[test]
    fn session_info_carries_state_source_and_last_tool() {
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
        m.kill_all();
    }

    #[test]
    fn session_info_sets_waiting_reason_permission() {
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
        m.kill_all();
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
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // No pending initially.
        assert!(m.pending(id).unwrap().is_none());
        // Set -> readable.
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        let p = m.pending(id).unwrap().expect("pending present");
        assert_eq!(p.request_id, "r1");
        assert_eq!(p.tool, "Bash");
        m.kill_all();
    }

    #[test]
    fn set_and_pending_404_on_unknown_id() {
        let mut m = mgr();
        assert!(m.set_pending(9999, pending("x", "Bash")).is_err());
        assert!(m.pending(9999).is_err());
        assert!(m.resolve_pending(9999, "allow").is_err());
    }

    #[test]
    fn resolve_clears_pending_and_records_decision() {
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
        m.kill_all();
    }

    #[test]
    fn resolve_deny_records_deny() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r2", "Write")).unwrap();
        m.resolve_pending(id, "deny").unwrap();
        assert_eq!(m.decision(id).unwrap().unwrap().decision, "deny");
        m.kill_all();
    }

    #[test]
    fn setting_new_pending_clears_a_stale_decision() {
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
        m.kill_all();
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
        m.kill_all();
    }
}
