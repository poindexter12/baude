use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Result;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use baude_core::backend;
use baude_core::git;
use baude_core::lifecycle::{self, LifecycleOutcome, RepositoryReservations};
use baude_core::meta::{now_unix_ms, ClaudeMeta, RateWindow};
use baude_core::persist::{self, Config, LegacyReconciliation, LoadOutcome, StateFile};
use baude_core::pty::{now_ms, Pty};
use baude_core::repository::{
    CheckoutHealth, CheckoutKey, CheckoutLifecycle, CheckoutRole, PersistedPath, RepositoryHealth,
    RepositoryState, RetainedSessionState, SavedCheckout, SavedRepository, UnavailableCause,
};
use baude_core::session::{Session, Status};

use crate::keys::encode_key;
use crate::notify_desktop::{self, DesktopNotifier, Row};
use crate::remote::{RemoteAttach, RemoteInfo, RemotePoller, RemoteSnapshot};
use crate::usage::{UsageCosts, UsagePoller};

const MESSAGE_TTL_MS: u64 = 5000;
const META_POLL_MS: u64 = 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalAdmissionRoute {
    LaunchDirectory,
    Open,
    CloneCompletion,
}

fn local_admission_route(_route: LocalAdmissionRoute, remote_configured: bool) -> bool {
    !remote_configured
}

fn active_restore_checkouts(state: &RepositoryState) -> Vec<CheckoutKey> {
    state
        .checkouts
        .iter()
        .filter(|checkout| checkout.active_intent)
        .map(|checkout| checkout.key)
        .collect()
}

fn require_same_checkout_path(checkout: &SavedCheckout, observed: &Path) -> Result<()> {
    if checkout.observed_path.to_path_buf() != observed {
        anyhow::bail!(
            "refusing to transfer checkout ownership from {} to {}",
            checkout.observed_path.to_path_buf().display(),
            observed.display()
        );
    }
    Ok(())
}

fn checkout_for_runtime(
    runtime_checkouts: &HashMap<CheckoutKey, u64>,
    runtime_id: u64,
) -> Option<CheckoutKey> {
    runtime_checkouts
        .iter()
        .find_map(|(key, id)| (*id == runtime_id).then_some(*key))
}

fn reconcile_legacy_session(
    saved: &persist::SavedSession,
    primary_repositories: &mut std::collections::HashSet<PersistedPath>,
) -> LegacyReconciliation {
    let Ok(snapshot) = git::discover_repository(&saved.cwd) else {
        return LegacyReconciliation::Unavailable {
            repository_cause: UnavailableCause::Missing,
            checkout_cause: UnavailableCause::Missing,
        };
    };
    let common_dir = PersistedPath::from_path(&snapshot.common_dir);
    let role = match git::resolve_default_branch(&snapshot) {
        Ok(default)
            if snapshot.selected_worktree.branch.as_deref() == Some(default.local_ref.as_str()) =>
        {
            if primary_repositories.insert(common_dir.clone()) {
                CheckoutRole::PrimaryDefault
            } else {
                CheckoutRole::ManagedBranch
            }
        }
        _ => CheckoutRole::ManagedBranch,
    };
    LegacyReconciliation::Available {
        common_dir,
        main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
        checkout_path: PersistedPath::from_path(&snapshot.selected_worktree.path),
        observed_branch: snapshot.selected_worktree.branch.clone(),
        checkout_role: role,
        managed_by_baude: false,
    }
}

/// Encode a mouse scroll event for delivery to a PTY.
/// `up` true → scroll up (button 64), false → scroll down (button 65).
/// `col`/`row` are 1-indexed coordinates within the PTY viewport.
/// `sgr` selects SGR encoding (`\x1b[<N;C;RM`) vs X10 (`\x1b[MBxy`).
fn encode_mouse_scroll(up: bool, col: usize, row: usize, sgr: bool) -> Vec<u8> {
    let button = if up { 64u8 } else { 65u8 };
    if sgr {
        format!("\x1b[<{button};{col};{row}M").into_bytes()
    } else {
        vec![
            0x1b,
            b'[',
            b'M',
            button + 32,
            (col as u8).saturating_add(32),
            (row as u8).saturating_add(32),
        ]
    }
}

/// ctrl+\ arrives as raw byte 0x1C, which crossterm reports as ctrl+4
/// (the two are indistinguishable in legacy terminal encoding).
fn is_backslash(code: KeyCode) -> bool {
    matches!(code, KeyCode::Char('\\') | KeyCode::Char('4'))
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

/// Shell-style directory completion: complete the component after the last
/// '/' against directories on disk. Returns the new buffer (if it advanced)
/// and the candidate list when ambiguous. The typed prefix (incl. `~/`) is
/// preserved — only the partial component is rewritten.
fn complete_dir_path(input: &str) -> (Option<String>, Vec<String>) {
    let (dir_part, partial) = match input.rfind('/') {
        Some(i) => (&input[..=i], &input[i + 1..]),
        None => ("", input),
    };
    let search = if dir_part.is_empty() {
        PathBuf::from(".")
    } else {
        expand_tilde(dir_part)
    };
    let Ok(entries) = std::fs::read_dir(&search) else {
        return (None, vec![]);
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with(partial) && (partial.starts_with('.') || !n.starts_with('.')))
        .collect();
    names.sort();
    match names.len() {
        0 => (None, vec![]),
        1 => (Some(format!("{dir_part}{}/", names[0])), vec![]),
        _ => {
            let mut lcp = names[0].clone();
            for n in &names[1..] {
                while !n.starts_with(&lcp) {
                    lcp.pop();
                }
            }
            let advanced = (lcp.len() > partial.len()).then(|| format!("{dir_part}{lcp}"));
            (advanced, names)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Claude,
    Shell,
}

/// Sidebar selection: a local session or one on the remote daemon.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SelId {
    Local(u64),
    Remote(u64),
}

pub enum InputKind {
    NewSessionPath,
    NewWorktreeBranch {
        repo_root: PathBuf,
    },
    /// Step 1 of `c`: a github url (or `owner/repo` shorthand) to clone.
    CloneUrl,
    /// Step 2 of `c`: where to clone it, prefilled from `clone_base_dir`.
    CloneDest {
        url: String,
        /// `owner/repo`, for messages.
        name: String,
    },
}

/// A `git clone` running on a background thread; polled from `tick`.
struct PendingClone {
    name: String,
    dest: PathBuf,
    rx: std::sync::mpsc::Receiver<Result<(), String>>,
}

pub enum Modal {
    None,
    Help,
    /// Session details: model, tokens, context, permission mode.
    Info,
    /// GSD project state for the selected session's repo.
    Gsd,
    /// Recent tool-activity timeline for the selected session (local or remote).
    Activity,
    Input {
        kind: InputKind,
        title: String,
        buf: String,
        /// Tab-completion candidates shown under the input.
        candidates: Vec<String>,
    },
    ConfirmKill {
        id: SelId,
    },
    ConfirmCloseWorktree {
        id: u64,
    },
    ConfirmRemoveWorktree {
        confirmation: lifecycle::RemovalConfirmation,
    },
}

/// Active text selection within a content pane (claude or shell).
/// Coordinates are relative to the pane's inner rect (0-indexed).
pub struct Selection {
    pub start_row: u16,
    pub start_col: u16,
    pub end_row: u16,
    pub end_col: u16,
    /// The terminal-absolute inner rect of the pane the selection lives in,
    /// captured at mouse-down so drags map correctly even if layout shifts.
    pub pane_area: Rect,
    pub is_shell: bool,
}

impl Selection {
    /// Normalize so start <= end (for rendering and extraction).
    pub fn normalized(&self) -> (u16, u16, u16, u16) {
        if self.start_row < self.end_row
            || (self.start_row == self.end_row && self.start_col <= self.end_col)
        {
            (self.start_row, self.start_col, self.end_row, self.end_col)
        } else {
            (self.end_row, self.end_col, self.start_row, self.start_col)
        }
    }

    pub fn contains(&self, row: u16, col: u16) -> bool {
        let (sr, sc, er, ec) = self.normalized();
        if row < sr || row > er {
            return false;
        }
        if sr == er {
            return col >= sc && col <= ec;
        }
        if row == sr {
            return col >= sc;
        }
        if row == er {
            return col <= ec;
        }
        true
    }
}

#[derive(Debug)]
struct RuntimeRestartFailure {
    agent_restarted: bool,
    shell_restarted: bool,
    detail: String,
}

impl std::fmt::Display for RuntimeRestartFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

pub struct App {
    pub sessions: Vec<Session>,
    pub selected_id: Option<SelId>,
    pub focus: Focus,
    pub modal: Modal,
    pub message: Option<(String, u64)>,
    pub should_quit: bool,
    pub launch_dir: PathBuf,
    config: Config,
    /// Resolved once at startup (env / config / 30m default); 0 disables.
    auto_archive_ms: u64,
    content_rect: Rect,
    next_id: u64,
    last_meta_poll: u64,
    usage: UsagePoller,
    /// Remote daemon client (config `daemon_url` / BAUDE_DAEMON_URL).
    pub remote: Option<RemotePoller>,
    pub remote_snap: RemoteSnapshot,
    /// At most one live raw attach to a remote session.
    pub attach: Option<RemoteAttach>,
    /// Scrollback offset for the selected session's claude pane.
    pub claude_scroll: usize,
    /// Scrollback offset for the selected session's shell pane.
    pub shell_scroll: usize,
    /// Active text selection (if any).
    pub selection: Option<Selection>,
    /// Clones in flight (`c` key); sessions open as each one lands.
    pending_clones: Vec<PendingClone>,
    /// macOS banner state machine (waiting/permission/finished/exited).
    desktop_notifier: DesktopNotifier,
    /// Resolved once at startup: BAUDE_NOTIFY env, then config
    /// `desktop_notifications`, then on.
    desktop_notify_enabled: bool,
    repository_state: RepositoryState,
    runtime_checkouts: HashMap<CheckoutKey, u64>,
    repository_reservations: RepositoryReservations,
    persistence_blocked: bool,
    persistence_dirty: bool,
    #[cfg(test)]
    persistence_root_for_test: Option<PathBuf>,
    #[cfg(test)]
    save_attempts_for_test: std::cell::Cell<usize>,
    #[cfg(test)]
    atomic_failure_for_test: Option<persist::AtomicFailure>,
    #[cfg(test)]
    spawn_error_for_test: Option<String>,
    #[cfg(test)]
    spawn_attempts_for_test: usize,
    #[cfg(test)]
    remove_stop_error_for_test: Option<String>,
    #[cfg(test)]
    remove_git_refusal_for_test: bool,
}

/// Outer (bordered) rects for the claude pane and optional shell pane.
pub fn pane_rects(content: Rect, shell_open: bool) -> (Rect, Option<Rect>) {
    if shell_open && content.height >= 14 {
        let shell_h = (content.height * 30 / 100).clamp(8, content.height.saturating_sub(6));
        let claude = Rect {
            height: content.height - shell_h,
            ..content
        };
        let shell = Rect {
            y: content.y + claude.height,
            height: shell_h,
            ..content
        };
        (claude, Some(shell))
    } else {
        (content, None)
    }
}

/// Shrink an outer pane rect to its terminal drawing area (inside the border).
pub fn inner(r: Rect) -> Rect {
    Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: r.width.saturating_sub(2),
        height: r.height.saturating_sub(2),
    }
}

impl App {
    pub fn new(launch_dir: PathBuf) -> App {
        let config = persist::load_config();
        let config_notify = config.desktop_notifications;
        let remote = std::env::var("BAUDE_DAEMON_URL")
            .ok()
            .or_else(|| baude_core::workspace::active().daemon_url.clone())
            .or_else(|| config.daemon_url.clone())
            .filter(|u| !u.trim().is_empty())
            .map(RemotePoller::start);
        App {
            sessions: Vec::new(),
            selected_id: None,
            focus: Focus::Sidebar,
            modal: Modal::None,
            message: None,
            should_quit: false,
            launch_dir,
            auto_archive_ms: config.auto_archive_ms(),
            config,
            content_rect: Rect::new(0, 0, 80, 24),
            next_id: 1,
            last_meta_poll: 0,
            usage: UsagePoller::start(),
            remote,
            remote_snap: RemoteSnapshot::default(),
            attach: None,
            claude_scroll: 0,
            shell_scroll: 0,
            selection: None,
            pending_clones: Vec::new(),
            desktop_notifier: DesktopNotifier::default(),
            desktop_notify_enabled: std::env::var("BAUDE_NOTIFY")
                .ok()
                .map(|v| !matches!(v.as_str(), "0" | "false"))
                .or(config_notify)
                .unwrap_or(true),
            repository_state: RepositoryState::default(),
            runtime_checkouts: HashMap::new(),
            repository_reservations: RepositoryReservations::default(),
            persistence_blocked: false,
            persistence_dirty: false,
            #[cfg(test)]
            persistence_root_for_test: None,
            #[cfg(test)]
            save_attempts_for_test: std::cell::Cell::new(0),
            #[cfg(test)]
            atomic_failure_for_test: None,
            #[cfg(test)]
            spawn_error_for_test: None,
            #[cfg(test)]
            spawn_attempts_for_test: 0,
            #[cfg(test)]
            remove_stop_error_for_test: None,
            #[cfg(test)]
            remove_git_refusal_for_test: false,
        }
    }

    /// Cached today/week costs from the ccusage background poller.
    pub fn usage_costs(&self) -> UsageCosts {
        self.usage.costs()
    }

    pub fn persistence_dirty(&self) -> bool {
        self.persistence_dirty
    }

    /// Freshest account rate-limit windows across all sessions (they're
    /// account-wide; whichever session's bridge file updated last wins).
    pub fn rate_limits(&self) -> (Option<RateWindow>, Option<RateWindow>) {
        let newest = self
            .sessions
            .iter()
            .max_by_key(|s| s.meta.rate_updated_unix_ms)
            .map(|s| &s.meta);
        match newest {
            Some(m) => (m.rate_5h, m.rate_week),
            None => (None, None),
        }
    }

    /// (waiting, busy, completed) session counts for the status bar.
    pub fn status_counts(&self) -> (usize, usize, usize) {
        let mut waiting = 0;
        let mut busy = 0;
        let mut completed = 0;
        for s in &self.sessions {
            if s.archived {
                continue;
            }
            match s.status() {
                Status::Waiting => waiting += 1,
                Status::Busy => busy += 1,
                Status::Completed => completed += 1,
                Status::Exited => {}
            }
        }
        (waiting, busy, completed)
    }

    /// The command run for each session, resolved PER BACKEND (claude:
    /// BAUDE_CLAUDE_CMD/`claude_cmd`; opencode: BAUDE_OPENCODE_CMD/
    /// `opencode_cmd`; then the backend default) — a configured claude_cmd
    /// must never become an opencode spawn command.
    fn claude_cmd(&self) -> String {
        backend::command_for(backend::active(), &self.config)
    }

    /// The editor launched by the sidebar `e` key: BAUDE_EDITOR_CMD env,
    /// then config.json `editor_cmd`, then `code`.
    fn editor_cmd(&self) -> String {
        std::env::var("BAUDE_EDITOR_CMD")
            .ok()
            .or_else(|| self.config.editor_cmd.clone())
            .unwrap_or_else(|| "code".to_string())
    }

    // ---- startup / persistence ----

    pub fn restore(&mut self) {
        let mut primary_repositories = std::collections::HashSet::new();
        #[cfg(test)]
        let loaded = if let Some(root) = &self.persistence_root_for_test {
            persist::load_for_workspace_strict_at(
                root,
                "state",
                baude_core::workspace::active(),
                |saved| reconcile_legacy_session(saved, &mut primary_repositories),
            )
        } else {
            persist::load_for_workspace("state", baude_core::workspace::active(), |saved| {
                reconcile_legacy_session(saved, &mut primary_repositories)
            })
        };
        #[cfg(not(test))]
        let loaded =
            persist::load_for_workspace("state", baude_core::workspace::active(), |saved| {
                reconcile_legacy_session(saved, &mut primary_repositories)
            });
        self.repository_state = match loaded {
            Ok(LoadOutcome::Missing) => RepositoryState::default(),
            Ok(LoadOutcome::Legacy(state) | LoadOutcome::Current(state)) => state.state,
            Err(error) => {
                self.persistence_blocked = true;
                self.set_message(format!(
                    "repository state blocked: {error}; repair or move the named state file, then restart"
                ));
                return;
            }
        };
        if let Err(error) = self.reconcile_activation_recoveries() {
            self.set_message(format!("activation recovery: {error}"));
        }
        if let Err(error) = self.reconcile_teardown_recoveries() {
            self.set_message(format!("teardown recovery: {error}"));
        }
        let active = active_restore_checkouts(&self.repository_state);
        for key in active {
            if let Err(error) = self.ensure_primary(key) {
                self.set_message(format!("restore primary: {error}"));
            }
        }
        // Premise: baude is started from a repo folder. Auto-add it if new.
        let launch = self.launch_dir.clone();
        if local_admission_route(LocalAdmissionRoute::LaunchDirectory, self.remote.is_some())
            && git::discover_repository(&launch).is_ok()
        {
            if let Err(e) = self.admit_repository(&launch) {
                self.set_message(format!("start session: {e}"));
            }
        }
        self.selected_id = self.ordered_ids().first().copied();
    }

    pub fn save(&mut self) {
        match self.save_durable() {
            Ok(()) => self.persistence_dirty = false,
            Err(error) => {
                self.persistence_dirty = true;
                self.set_message(format!(
                    "state not saved: {error}; retry the action after repairing persistence"
                ));
            }
        }
    }

    fn save_durable_status(&self) -> std::result::Result<(), persist::SaveError> {
        if self.persistence_blocked {
            return Err(persist::SaveError::before_replacement(anyhow::anyhow!(
                "automatic persistence is blocked after a state load failure"
            )));
        }
        #[cfg(test)]
        self.save_attempts_for_test
            .set(self.save_attempts_for_test.get() + 1);
        let mut state = self.repository_state.clone();
        for checkout in &mut state.checkouts {
            let Some(runtime_id) = self.runtime_checkouts.get(&checkout.key) else {
                continue;
            };
            let Some(session) = self.session(*runtime_id) else {
                continue;
            };
            checkout.session = RetainedSessionState {
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
            };
        }
        state
            .validate()
            .map_err(persist::SaveError::before_replacement)?;
        #[cfg(test)]
        if let Some(root) = &self.persistence_root_for_test {
            return persist::save_current_at_test(
                root,
                &baude_core::workspace::active().state_file("state"),
                &StateFile::new(state),
                self.atomic_failure_for_test,
                None,
            );
        }
        persist::save_for_workspace_status(
            "state",
            baude_core::workspace::active(),
            &StateFile::new(state),
        )
    }

    fn save_removal_revocation(&mut self) -> std::result::Result<(), persist::SaveError> {
        let result = self.save_durable_status();
        #[cfg(test)]
        if result.is_err() {
            // Failure injection models one atomic boundary, allowing rollback
            // persistence to exercise the same path as a recovered filesystem.
            self.atomic_failure_for_test = None;
        }
        result
    }

    fn save_durable(&self) -> Result<()> {
        self.save_durable_status().map_err(anyhow::Error::new)
    }

    pub fn admit_repository(&mut self, path: &Path) -> Result<Option<u64>> {
        let snapshot = git::discover_repository(path)?;
        let common = PersistedPath::from_path(&snapshot.common_dir);
        let repository_key = match self
            .repository_state
            .repositories
            .iter()
            .find(|repository| repository.observed_common_dir == common)
            .map(|repository| repository.key)
        {
            Some(key) => key,
            None => {
                let key = self.repository_state.allocate_repository_key()?;
                let first_seen_order = self.repository_state.allocate_first_seen_order()?;
                self.repository_state.repositories.push(SavedRepository {
                    key,
                    observed_common_dir: common.clone(),
                    observed_main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
                    first_seen_order,
                    health: RepositoryHealth::Available,
                });
                key
            }
        };
        if let Some(repository) = self
            .repository_state
            .repositories
            .iter_mut()
            .find(|repository| repository.key == repository_key)
        {
            repository.observed_common_dir = common;
            repository.observed_main_worktree = PersistedPath::from_path(&snapshot.main_worktree);
            repository.health = RepositoryHealth::Available;
        }

        let default = match git::resolve_default_branch(&snapshot) {
            Ok(default) => default,
            Err(error) => {
                if let Some(repository) = self
                    .repository_state
                    .repositories
                    .iter_mut()
                    .find(|repository| repository.key == repository_key)
                {
                    repository.health =
                        RepositoryHealth::Unavailable(UnavailableCause::Other(error.to_string()));
                }
                self.save_durable()?;
                self.set_message(format!(
                    "default checkout unavailable: {error}; repair local remote HEAD metadata and reopen"
                ));
                return Ok(None);
            }
        };

        let existing_key = self
            .repository_state
            .checkouts
            .iter()
            .find(|checkout| {
                checkout.repository_key == repository_key
                    && checkout.role == CheckoutRole::PrimaryDefault
            })
            .map(|checkout| checkout.key);
        if let Some(existing_key) = existing_key {
            if !self.reconcile_primary(existing_key) {
                self.save_durable()?;
                self.set_message(
                    "persisted primary changed externally; retained unavailable without transferring ownership"
                        .into(),
                );
                return Ok(None);
            }
        }
        let checkout_key = match existing_key {
            Some(key) => key,
            None => self.repository_state.allocate_checkout_key()?,
        };
        let managed_path =
            git::managed_default_worktree_path(repository_key.get(), checkout_key.get());
        let outcome = git::ensure_default_worktree(&snapshot, &default, &managed_path)?;
        let (record, managed_by_baude) = match outcome {
            git::DefaultWorktreeOutcome::Main(record)
            | git::DefaultWorktreeOutcome::ExistingLinked(record) => (record, false),
            git::DefaultWorktreeOutcome::CreatedManaged(record) => (record, true),
        };
        let fresh = git::discover_repository(&record.path)?;
        if fresh.common_dir != snapshot.common_dir
            || fresh.selected_worktree.path != record.path
            || fresh.selected_worktree.branch.as_deref() != Some(default.local_ref.as_str())
        {
            anyhow::bail!("fresh Git topology did not verify the primary checkout");
        }

        let is_worktree = record.path != fresh.main_worktree;
        if let Some(checkout) = self
            .repository_state
            .checkouts
            .iter_mut()
            .find(|checkout| checkout.key == checkout_key)
        {
            require_same_checkout_path(checkout, &record.path)?;
            checkout.observed_branch = Some(default.local_ref.clone());
            checkout.active_intent = true;
            checkout.health = CheckoutHealth::Available;
            checkout.session.cwd = PersistedPath::from_path(&record.path);
            checkout.session.branch = Some(default.local_branch.clone());
            checkout.session.is_worktree = is_worktree;
        } else {
            let first_seen_order = self.repository_state.allocate_first_seen_order()?;
            let name = snapshot
                .main_worktree
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| snapshot.main_worktree.display().to_string());
            self.repository_state.checkouts.push(SavedCheckout {
                key: checkout_key,
                repository_key,
                role: CheckoutRole::PrimaryDefault,
                managed_by_baude,
                observed_path: PersistedPath::from_path(&record.path),
                observed_branch: Some(default.local_ref.clone()),
                first_seen_order,
                lifecycle: CheckoutLifecycle::Active,
                active_intent: true,
                session: RetainedSessionState {
                    name,
                    cwd: PersistedPath::from_path(&record.path),
                    repo_root: PersistedPath::from_path(&fresh.main_worktree),
                    branch: Some(default.local_branch),
                    is_worktree,
                    shell_open: false,
                    archived: false,
                    archived_by_user: false,
                    resume_id: None,
                },
                health: CheckoutHealth::Available,
            });
        }
        self.repository_state.validate()?;
        self.ensure_primary(checkout_key)
    }

    fn reconcile_activation_recoveries(&mut self) -> Result<()> {
        let recoveries: Vec<_> = self
            .repository_state
            .checkouts
            .iter()
            .filter_map(|checkout| {
                matches!(
                    checkout.health,
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
                .map_err(|busy| anyhow::anyhow!("{busy:?}"))?;
            lifecycle::reconcile_activation_recovery(&mut self.repository_state, checkout)?;
        }
        if let Err(error) = self.save_durable_status() {
            self.persistence_dirty = true;
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(anyhow::Error::new(error));
        }
        self.persistence_dirty = false;
        Ok(())
    }

    fn reconcile_teardown_recoveries(&mut self) -> Result<()> {
        let recoveries: Vec<_> = self
            .repository_state
            .checkouts
            .iter()
            .filter_map(|checkout| {
                matches!(
                    checkout.health,
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
                .map_err(|busy| anyhow::anyhow!("{busy:?}"))?;
            lifecycle::reconcile_teardown_recovery(&mut self.repository_state, checkout)?;
        }
        if let Err(error) = self.save_durable_status() {
            self.persistence_dirty = true;
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(anyhow::Error::new(error));
        }
        self.persistence_dirty = false;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn retry_teardown_recovery(
        &mut self,
        checkout: CheckoutKey,
    ) -> Result<lifecycle::TeardownRecoveryResolution> {
        let repository = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .ok_or_else(|| anyhow::anyhow!("checkout {} is missing", checkout.get()))?
            .repository_key;
        let before = self.repository_state.clone();
        let resolution = {
            let _reservation = self
                .repository_reservations
                .reserve(repository)
                .map_err(|busy| anyhow::anyhow!("{busy:?}"))?;
            lifecycle::reconcile_teardown_recovery(&mut self.repository_state, checkout)?
        };
        if let Err(error) = self.save_durable_status() {
            self.persistence_dirty = true;
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(anyhow::Error::new(error));
        }
        self.persistence_dirty = false;
        Ok(resolution)
    }

    #[allow(dead_code)]
    pub fn retry_activation_recovery(
        &mut self,
        checkout: CheckoutKey,
    ) -> Result<lifecycle::ActivationRecoveryResolution> {
        let repository = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .ok_or_else(|| anyhow::anyhow!("checkout {} is missing", checkout.get()))?
            .repository_key;
        let before = self.repository_state.clone();
        let resolution = {
            let _reservation = self
                .repository_reservations
                .reserve(repository)
                .map_err(|busy| anyhow::anyhow!("{busy:?}"))?;
            lifecycle::reconcile_activation_recovery(&mut self.repository_state, checkout)?
        };
        if let Err(error) = self.save_durable_status() {
            self.persistence_dirty = true;
            if !error.replacement_committed() {
                self.repository_state = before;
            }
            return Err(anyhow::Error::new(error));
        }
        self.persistence_dirty = false;
        Ok(resolution)
    }

    pub fn activate_branch_worktree(
        &mut self,
        repository_child: &Path,
        branch: &str,
    ) -> Result<LifecycleOutcome> {
        if self.persistence_dirty || self.repository_state.has_pending_activation() {
            anyhow::bail!(
                "repository lifecycle is blocked while pending ownership is not durable; repair persistence and save before retrying"
            );
        }
        let snapshot = git::discover_repository(repository_child)?;
        let state_before = self.repository_state.clone();
        let mut next = self.repository_state.clone();
        let prepared = lifecycle::prepare_activation(&mut next, &snapshot, branch)?;
        let pending_checkout = prepared.checkout;
        lifecycle::record_pending_activation(&mut next, &snapshot, &prepared)?;
        let repository = prepared.request.repository;
        let _reservation = match self.repository_reservations.reserve(repository) {
            Ok(reservation) => reservation,
            Err(busy) => return Ok(busy),
        };
        self.repository_state = next.clone();
        if let Err(error) = self.save_durable_status() {
            self.persistence_dirty = true;
            if !error.replacement_committed() {
                self.repository_state = state_before;
            }
            return Err(anyhow::anyhow!(
                "pending activation ownership persistence failed before Git mutation: {error}"
            ));
        }
        self.persistence_dirty = false;
        let activation = match lifecycle::execute_activation(&mut next, repository_child, prepared)
        {
            Ok(activation) => activation,
            Err(error) if error.recovery_child_recorded() => {
                self.repository_state = next;
                if let Err(save_error) = self.save_durable_status() {
                    self.persistence_dirty = true;
                    return Err(anyhow::anyhow!(
                        "{error}; recovery ownership persistence failed: {save_error}"
                    ));
                }
                self.persistence_dirty = false;
                return Err(anyhow::Error::new(error));
            }
            Err(error) => {
                lifecycle::clear_pending_activation(&mut next, pending_checkout);
                self.repository_state = next;
                if let Err(save_error) = self.save_durable_status() {
                    self.persistence_dirty = true;
                    return Err(anyhow::anyhow!(
                        "{error}; clearing pending activation ownership failed: {save_error}"
                    ));
                }
                self.persistence_dirty = false;
                return Err(anyhow::Error::new(error));
            }
        };
        self.repository_state = next;

        if let Some(runtime) = self.runtime_checkouts.get(&activation.checkout).copied() {
            if let Some(exited) = self
                .session(runtime)
                .map(|session| session.claude.is_exited())
            {
                if let Err(error) = self.save_durable_status() {
                    self.persistence_dirty = true;
                    let stage = if error.replacement_committed() {
                        lifecycle::CreationFailureStage::PersistenceAfterReplacement
                    } else {
                        lifecycle::CreationFailureStage::PersistenceBeforeReplacement
                    };
                    if !error.replacement_committed() {
                        self.repository_state = state_before;
                    }
                    return Err(anyhow::anyhow!("{stage} failed: {error}"));
                }
                self.persistence_dirty = false;
                if exited {
                    self.restart_session_with_resume(runtime, true)?;
                }
                self.selected_id = Some(SelId::Local(runtime));
                self.focus = Focus::Claude;
                return Ok(if exited {
                    LifecycleOutcome::Reopened {
                        checkout: activation.checkout,
                        runtime,
                    }
                } else {
                    LifecycleOutcome::Focused {
                        checkout: activation.checkout,
                        runtime,
                    }
                });
            }
        }

        let checkout = self
            .repository_state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == activation.checkout)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("activated checkout is missing"))?;
        if let Err(error) = self.save_durable_status() {
            self.persistence_dirty = true;
            let stage = if error.replacement_committed() {
                lifecycle::CreationFailureStage::PersistenceAfterReplacement
            } else {
                lifecycle::CreationFailureStage::PersistenceBeforeReplacement
            };
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
                    )?;
                    let recovery_save = self.save_durable_status();
                    self.persistence_dirty = recovery_save.is_err();
                    return Err(anyhow::anyhow!(
                        "{stage} failed: {error}; {} failed: {compensation}; recovery persistence: {}",
                        lifecycle::CreationFailureStage::Compensation,
                        recovery_save
                            .map(|()| "saved".to_owned())
                            .unwrap_or_else(|save| save.to_string())
                    ));
                }
                self.repository_state = state_before;
                if let Err(clear_error) = self.save_durable_status() {
                    self.persistence_dirty = true;
                    return Err(anyhow::anyhow!(
                        "{stage} failed: {error}; activation compensated but pending ownership cleanup failed: {clear_error}"
                    ));
                }
            }
            return Err(anyhow::anyhow!("{stage} failed: {error}"));
        }
        self.persistence_dirty = false;
        let id = self
            .add_session(
                activation.path.clone(),
                Some(activation.main_worktree.clone()),
                Some(activation.branch.clone()),
                activation.path != activation.main_worktree,
                false,
                checkout.session.shell_open,
            )
            .map_err(|error| {
                anyhow::anyhow!(
                    "{} failed after durable activation: {error}",
                    lifecycle::CreationFailureStage::Spawn
                )
            })?;
        if let Some(runtime) = self.session_mut(id) {
            runtime.name = checkout.session.name;
            runtime.archived = checkout.session.archived;
            runtime.archived_by_user = checkout.session.archived_by_user;
        }
        self.runtime_checkouts.insert(activation.checkout, id);
        self.selected_id = Some(SelId::Local(id));
        self.focus = Focus::Claude;
        Ok(activation.outcome(Some(id)))
    }

    pub fn ensure_primary(&mut self, checkout_key: CheckoutKey) -> Result<Option<u64>> {
        match self.reopen_checkout(checkout_key)? {
            LifecycleOutcome::Focused { runtime, .. }
            | LifecycleOutcome::Reopened { runtime, .. } => Ok(Some(runtime)),
            LifecycleOutcome::ReopenPending { .. } | LifecycleOutcome::Busy { .. } => Ok(None),
            other => anyhow::bail!("unexpected retained reopen outcome: {other:?}"),
        }
    }

    pub fn reopen_checkout(&mut self, checkout_key: CheckoutKey) -> Result<LifecycleOutcome> {
        let checkout = self
            .repository_state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == checkout_key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("retained checkout is missing"))?;
        let _reservation = match self
            .repository_reservations
            .reserve_reopen(checkout.repository_key, checkout_key)
        {
            Ok(reservation) => reservation,
            Err(outcome) => return Ok(outcome),
        };
        let runtime = self.runtime_checkouts.get(&checkout_key).and_then(|id| {
            self.session(*id)
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
            .ok_or_else(|| anyhow::anyhow!("retained checkout repository is missing"))?;
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
                if let Err(error) = self.save_durable_status() {
                    self.persistence_dirty = true;
                    if !error.replacement_committed() {
                        self.repository_state = state_before;
                    }
                    return Err(anyhow::Error::new(error));
                }
                self.persistence_dirty = false;
                return Err(anyhow::anyhow!(
                    "retained checkout {} is unavailable: {:?}",
                    blocked.checkout().get(),
                    blocked.cause
                ));
            }
        };
        if let Err(error) = self.save_durable_status() {
            self.persistence_dirty = true;
            if !error.replacement_committed() {
                self.repository_state = state_before;
            }
            return Err(anyhow::Error::new(error));
        }
        self.persistence_dirty = false;

        match plan.dispatch {
            lifecycle::ReopenDispatch::Focus { id } => {
                self.selected_id = Some(SelId::Local(id));
                self.focus = Focus::Claude;
                Ok(LifecycleOutcome::Focused {
                    checkout: checkout_key,
                    runtime: id,
                })
            }
            lifecycle::ReopenDispatch::Restart { id } => {
                self.restart_session_with_mode(id, plan.mode)?;
                self.selected_id = Some(SelId::Local(id));
                Ok(LifecycleOutcome::Reopened {
                    checkout: checkout_key,
                    runtime: id,
                })
            }
            lifecycle::ReopenDispatch::Spawn => {
                let cwd = checkout.observed_path.to_path_buf();
                let session = checkout.session;
                let id = self.add_session_with_mode(
                    cwd,
                    Some(session.repo_root.to_path_buf()),
                    session.branch.clone(),
                    session.is_worktree,
                    plan.mode,
                    session.shell_open,
                )?;
                if let Some(runtime) = self.session_mut(id) {
                    runtime.name = session.name;
                    runtime.archived = session.archived;
                    runtime.archived_by_user = session.archived_by_user;
                }
                self.runtime_checkouts.insert(checkout_key, id);
                self.focus = Focus::Claude;
                Ok(LifecycleOutcome::Reopened {
                    checkout: checkout_key,
                    runtime: id,
                })
            }
        }
    }

    fn reconcile_primary(&mut self, checkout_key: CheckoutKey) -> bool {
        let Some(index) = self
            .repository_state
            .checkouts
            .iter()
            .position(|checkout| checkout.key == checkout_key)
        else {
            return false;
        };
        let repository_key = self.repository_state.checkouts[index].repository_key;
        let path = self.repository_state.checkouts[index]
            .observed_path
            .to_path_buf();
        let expected_branch = self.repository_state.checkouts[index]
            .observed_branch
            .clone();
        let Some(repository_index) = self
            .repository_state
            .repositories
            .iter()
            .position(|repository| repository.key == repository_key)
        else {
            return false;
        };
        let expected_common = self.repository_state.repositories[repository_index]
            .observed_common_dir
            .clone();
        match git::reconcile_checkout(
            &expected_common.to_path_buf(),
            &path,
            expected_branch.as_deref(),
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let cause = if matches!(error, git::ReconciliationUnavailable::Missing { .. }) {
                    UnavailableCause::Missing
                } else if matches!(
                    error,
                    git::ReconciliationUnavailable::IdentityChanged { .. }
                        | git::ReconciliationUnavailable::PathChanged { .. }
                        | git::ReconciliationUnavailable::BranchChanged { .. }
                        | git::ReconciliationUnavailable::Detached
                        | git::ReconciliationUnavailable::LockedOrPrunable
                ) {
                    UnavailableCause::IdentityChanged
                } else {
                    UnavailableCause::Other(error.to_string())
                };
                self.repository_state.checkouts[index].health =
                    CheckoutHealth::Unavailable(cause.clone());
                self.repository_state.repositories[repository_index].health =
                    RepositoryHealth::Unavailable(cause);
                return false;
            }
        };
        self.repository_state.checkouts[index].health = CheckoutHealth::Available;
        self.repository_state.repositories[repository_index].health = RepositoryHealth::Available;
        true
    }

    // ---- session bookkeeping ----

    /// Selection order: active local sessions, then the remote daemon's
    /// active sessions, then archived sessions at the end — each group
    /// alphabetical by name (case-insensitive) so the sidebar is predictable.
    /// Sessions that need input flash in place instead of reordering.
    pub fn ordered_ids(&self) -> Vec<SelId> {
        let mut active_local: Vec<(String, SelId)> = Vec::new();
        let mut active_remote: Vec<(String, SelId)> = Vec::new();
        let mut archived: Vec<(String, SelId)> = Vec::new();
        for s in &self.sessions {
            let entry = (s.name.to_lowercase(), SelId::Local(s.id));
            if s.archived {
                archived.push(entry);
            } else {
                active_local.push(entry);
            }
        }
        for r in &self.remote_snap.sessions {
            let entry = (r.name.to_lowercase(), SelId::Remote(r.id));
            if r.archived {
                archived.push(entry);
            } else {
                active_remote.push(entry);
            }
        }
        // Stable sort: equal names keep creation order.
        for group in [&mut active_local, &mut active_remote, &mut archived] {
            group.sort_by(|a, b| a.0.cmp(&b.0));
        }
        active_local
            .into_iter()
            .chain(active_remote)
            .chain(archived)
            .map(|(_, id)| id)
            .collect()
    }

    pub fn is_archived(&self, id: SelId) -> bool {
        match id {
            SelId::Local(lid) => self.session(lid).map(|s| s.archived).unwrap_or(false),
            SelId::Remote(rid) => self.remote_info(rid).map(|r| r.archived).unwrap_or(false),
        }
    }

    pub fn remote_info(&self, id: u64) -> Option<&RemoteInfo> {
        self.remote_snap.sessions.iter().find(|r| r.id == id)
    }

    pub fn selected_remote(&self) -> Option<&RemoteInfo> {
        match self.selected_id {
            Some(SelId::Remote(id)) => self.remote_info(id),
            _ => None,
        }
    }

    pub fn session(&self, id: u64) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn session_mut(&mut self, id: u64) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn selected(&self) -> Option<&Session> {
        match self.selected_id {
            Some(SelId::Local(id)) => self.session(id),
            _ => None,
        }
    }

    fn selected_mut(&mut self) -> Option<&mut Session> {
        match self.selected_id {
            Some(SelId::Local(id)) => self.sessions.iter_mut().find(|s| s.id == id),
            _ => None,
        }
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

    fn claude_spawn_size(&self, shell_open: bool) -> (u16, u16) {
        let (claude, _) = pane_rects(self.content_rect, shell_open);
        let r = inner(claude);
        (r.height, r.width)
    }

    pub fn add_session(
        &mut self,
        cwd: PathBuf,
        repo_root: Option<PathBuf>,
        branch: Option<String>,
        is_worktree: bool,
        resume: bool,
        shell_open: bool,
    ) -> Result<u64> {
        let mode = if resume {
            backend::SpawnMode::ContinueLatest
        } else {
            backend::SpawnMode::Fresh
        };
        self.add_session_with_mode(cwd, repo_root, branch, is_worktree, mode, shell_open)
    }

    fn add_session_with_mode(
        &mut self,
        cwd: PathBuf,
        repo_root: Option<PathBuf>,
        branch: Option<String>,
        is_worktree: bool,
        mode: backend::SpawnMode,
        shell_open: bool,
    ) -> Result<u64> {
        #[cfg(test)]
        {
            self.spawn_attempts_for_test += 1;
            if let Some(error) = &self.spawn_error_for_test {
                anyhow::bail!("test spawn failure: {error}");
            }
        }
        // For worktree sessions the caller passes the main repo root —
        // `rev-parse --show-toplevel` inside a worktree returns the worktree.
        let repo_root =
            repo_root.unwrap_or_else(|| git::repo_root(&cwd).unwrap_or_else(|| cwd.clone()));
        let dir_name = |p: &Path| {
            p.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.to_string_lossy().to_string())
        };
        let base = match &branch {
            Some(b) => format!("{}:{}", dir_name(&repo_root), b),
            None => dir_name(&cwd),
        };
        let name = self.unique_name(&base);

        let be = backend::active();

        // PERM-01: append exactly one permission flag to the base cmd (default
        // skip preserves today's `--dangerously-skip-permissions`; `prompt` is
        // opt-in via BAUDE_PERMISSION_MODE). The flag rides on the base cmd so
        // it survives the `--continue || exec` resume fallback. BL-04: prompt
        // mode strips a conflicting skip flag from claude_cmd so it can't be
        // silently suppressed. The TUI is a ratatui screen, so the `stripped_skip`
        // warning channel (eprintln) the daemon uses is intentionally dropped here.
        // TUI sessions pass NO event URL: the spawned command gets no
        // $BAUDE_EVENT_URL, which routes hook events to the /tmp append path
        // (only the daemon injects that var).
        let base = be.resolve_cmd(&self.claude_cmd()).cmd;
        let plan = be.spawn_plan(&base, None, mode);

        // Wire the session cwd before the CLI starts (for Claude: the
        // settings.local.json hook seed, plus the prompt-mode .mcp.json).
        // Best-effort: a seeding failure must NOT abort the spawn — the session
        // simply falls back to the silence path (no regression).
        be.prepare_cwd(&cwd);

        if baude_core::permission::is_prompt_mode() && be.prompt_mode_needs_daemon() {
            // WR-01: claude's permission approval is inherently daemon+PWA-
            // mediated. A TUI-local session gets NO $BAUDE_EVENT_URL (only the
            // daemon injects it), so the `permission-mcp` bridge fails CLOSED
            // and DENIES every tool with no operator-visible reason. Make that
            // non-silent: warn clearly (once per process to stderr, plus a
            // visible TUI message) that prompt mode requires the daemon and the
            // bare TUI will deny all tools. This is fail-safe (deny, never
            // allow) but no longer a silent footgun. `skip` (the default) is
            // unaffected — and so is opencode, whose own TUI prompts locally.
            self.warn_prompt_mode_without_daemon();
        }

        let (rows, cols) = self.claude_spawn_size(shell_open);
        let claude = Pty::spawn_with_env(Some(&plan.cmd), &plan.env, &cwd, rows, cols)?;
        let mut meta = ClaudeMeta::default();
        meta.backend_port = plan.server_port;

        let id = self.next_id;
        self.next_id += 1;
        let mut session = Session {
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
        };
        if shell_open {
            let (_, shell_rect) = pane_rects(self.content_rect, true);
            if let Some(sr) = shell_rect {
                let r = inner(sr);
                let _ = session.open_shell(r.height, r.width);
            }
        }
        self.sessions.push(session);
        self.selected_id = Some(SelId::Local(id));
        Ok(id)
    }

    fn teardown_retained_runtime(&mut self, checkout_key: CheckoutKey, id: u64) -> Result<()> {
        let session_index = self
            .sessions
            .iter()
            .position(|session| session.id == id)
            .ok_or_else(|| anyhow::anyhow!("runtime {id} is missing"))?;
        if let Err(error) = lifecycle::destructive_teardown(
            &mut self.repository_state,
            checkout_key,
            &mut self.sessions[session_index],
        ) {
            if let Err(save_error) = self.save_durable_status() {
                self.persistence_dirty = true;
                return Err(anyhow::anyhow!(
                    "{error}; could not persist pending teardown recovery: {save_error}"
                ));
            }
            self.persistence_dirty = false;
            return Err(anyhow::Error::new(error));
        }
        Ok(())
    }

    fn forget_stopped_runtime(&mut self, checkout_key: CheckoutKey, id: u64) {
        self.runtime_checkouts.remove(&checkout_key);
        self.sessions.retain(|s| s.id != id);
        if self.selected_id == Some(SelId::Local(id)) {
            self.selected_id = self.ordered_ids().first().copied();
        }
        self.focus = Focus::Sidebar;
    }

    fn retained_runtime_snapshot(&self, id: u64) -> Result<RetainedSessionState> {
        let session = self
            .session(id)
            .ok_or_else(|| anyhow::anyhow!("runtime {id} is missing"))?;
        let durable_resume_id = checkout_for_runtime(&self.runtime_checkouts, id).and_then(|key| {
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

    fn restore_removed_runtime(
        &mut self,
        checkout: CheckoutKey,
        saved: RetainedSessionState,
    ) -> Result<u64> {
        if let Some(id) = self.runtime_checkouts.get(&checkout).copied() {
            self.selected_id = Some(SelId::Local(id));
            self.focus = Focus::Claude;
            return Ok(id);
        }
        let mode = saved
            .resume_id
            .clone()
            .map(backend::SpawnMode::ResumeId)
            .unwrap_or(backend::SpawnMode::ContinueLatest);
        let id = self.add_session_with_mode(
            saved.cwd.to_path_buf(),
            Some(saved.repo_root.to_path_buf()),
            saved.branch.clone(),
            saved.is_worktree,
            mode,
            saved.shell_open,
        )?;
        if let Some(runtime) = self.session_mut(id) {
            runtime.name = saved.name;
            runtime.archived = saved.archived;
            runtime.archived_by_user = saved.archived_by_user;
        }
        self.runtime_checkouts.insert(checkout, id);
        self.selected_id = Some(SelId::Local(id));
        self.focus = Focus::Claude;
        Ok(id)
    }

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
        #[cfg(test)]
        if let Some(error) = &self.remove_stop_error_for_test {
            return Err(lifecycle::RemovalFailure::Inspection(format!(
                "runtime stop failed: {error}"
            )));
        }
        if let Some(id) = runtime_id {
            self.teardown_retained_runtime(checkout, id)
                .map_err(|error| {
                    lifecycle::RemovalFailure::Inspection(format!("runtime stop failed: {error}"))
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
        // This is the authority-restorable state: it intentionally includes
        // the runtime context captured immediately before teardown.
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
        #[cfg(test)]
        if self.remove_git_refusal_for_test {
            std::fs::write(target.path().join("agent-race-after-second"), b"unsaved\n")
                .map_err(|error| lifecycle::RemovalFailure::Inspection(error.to_string()))?;
        }
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
                if self.save_durable_status().is_err() {
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
        match self.save_durable_status() {
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

    fn close_retained_session(&mut self, id: u64) -> Result<LifecycleOutcome> {
        let checkout_key = checkout_for_runtime(&self.runtime_checkouts, id)
            .ok_or_else(|| anyhow::anyhow!("runtime {id} has no retained checkout"))?;
        let snapshot = self.retained_runtime_snapshot(id)?;
        self.teardown_retained_runtime(checkout_key, id)?;
        let before = self.repository_state.clone();
        let plan = lifecycle::plan_close(
            &mut self.repository_state,
            lifecycle::CloseRequest {
                checkout: checkout_key,
                runtime: snapshot.clone(),
            },
        )?;
        match self.save_durable_status() {
            Ok(()) => {
                self.persistence_dirty = false;
                self.forget_stopped_runtime(checkout_key, id);
                Ok(plan.outcome)
            }
            Err(error) if !error.replacement_committed() => {
                self.repository_state = before;
                self.persistence_dirty = true;
                let mode = snapshot
                    .resume_id
                    .clone()
                    .map(backend::SpawnMode::ResumeId)
                    .unwrap_or(backend::SpawnMode::ContinueLatest);
                match self.restore_stopped_runtime(id, mode, snapshot.shell_open) {
                    Ok(()) => Err(anyhow::anyhow!(
                        "{error}; close persistence rolled back and retained runtime {id} restarted (agent and shell restored)"
                    )),
                    Err(restart) => {
                        self.forget_stopped_runtime(checkout_key, id);
                        let detail = format!(
                            "close persistence failed before replacement: {error}; runtime restart compensation failed: {restart}"
                        );
                        lifecycle::mark_stopped_active_recovery(
                            &mut self.repository_state,
                            checkout_key,
                            restart.agent_restarted,
                            restart.shell_restarted,
                            detail.clone(),
                        )?;
                        let recovery_save = self.save_durable_status();
                        self.persistence_dirty = recovery_save.is_err();
                        Err(anyhow::anyhow!(
                            "{detail}; recovery persistence: {}",
                            recovery_save
                                .map(|()| "saved".to_owned())
                                .unwrap_or_else(|save| save.to_string())
                        ))
                    }
                }
            }
            Err(error) => {
                self.persistence_dirty = true;
                self.forget_stopped_runtime(checkout_key, id);
                Err(anyhow::anyhow!(
                    "inactive state committed but directory durability failed: {error}"
                ))
            }
        }
    }

    fn remove_session(&mut self, id: u64) {
        if checkout_for_runtime(&self.runtime_checkouts, id).is_some() {
            if let Err(error) = self.close_retained_session(id) {
                self.set_message(format!("session close degraded or blocked: {error}"));
            }
            return;
        }
        if let Some(s) = self.session_mut(id) {
            s.kill();
        }
        self.sessions.retain(|s| s.id != id);
        if self.selected_id == Some(SelId::Local(id)) {
            self.selected_id = self.ordered_ids().first().copied();
        }
        self.focus = Focus::Sidebar;
        self.save();
    }

    pub fn set_message(&mut self, msg: String) {
        self.message = Some((msg, now_ms() + MESSAGE_TTL_MS));
    }

    /// WR-01: warn — once per process to stderr, and visibly in the TUI — that
    /// `BAUDE_PERMISSION_MODE=prompt` cannot work under the bare TUI. Permission
    /// approval is daemon+PWA-mediated; a TUI-local session has no
    /// `$BAUDE_EVENT_URL`, so the `permission-mcp` bridge fails closed and denies
    /// every tool. This makes the deny-all behaviour discoverable instead of a
    /// silent hang. Fail-safe (deny, never allow); `skip` (default) is untouched.
    fn warn_prompt_mode_without_daemon(&mut self) {
        const MSG: &str = "BAUDE_PERMISSION_MODE=prompt has no approval UI under the bare TUI \
             (no daemon) — every tool will be DENIED. Run via bauded + the PWA to approve.";
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("baude: {MSG}");
        }
        self.set_message(MSG.into());
    }

    /// Feed the desktop-banner state machine one snapshot of every sidebar
    /// row — local sessions and the remote daemon's — and post whatever it
    /// decides. Cheap per frame; osascript runs off-thread on actual events.
    fn tick_desktop_notify(&mut self) {
        if !self.desktop_notify_enabled {
            return;
        }
        let mut rows: Vec<Row> = Vec::with_capacity(self.sessions.len());
        for s in &self.sessions {
            let status = match s.status() {
                Status::Waiting => "waiting",
                Status::Completed => "completed",
                Status::Busy => "busy",
                Status::Exited => "exited",
            };
            rows.push(Row {
                key: format!("l{}", s.id),
                name: s.name.clone(),
                status,
                waiting_for_ms: Some(s.waiting_for_ms()),
                waiting_reason: match baude_core::permission::waiting_reason(
                    s.meta.last_notification.as_ref(),
                    status == "waiting",
                    status == "completed",
                ) {
                    "none" => None,
                    r => Some(r.to_string()),
                },
                archived: s.archived,
            });
        }
        // Remote timers are snapshots — age them by time-since-fetch, exactly
        // like the sidebar rendering does, so the 10s debounce is honest.
        let age = now_ms().saturating_sub(self.remote_snap.fetched_ms);
        for r in &self.remote_snap.sessions {
            rows.push(Row {
                key: format!("r{}", r.id),
                name: r.name.clone(),
                status: match r.status.as_str() {
                    "waiting" => "waiting",
                    "completed" => "completed",
                    "exited" => "exited",
                    _ => "busy",
                },
                waiting_for_ms: r.waiting_for_ms.map(|w| w + age),
                waiting_reason: r.waiting_reason.clone(),
                archived: r.archived,
            });
        }
        for banner in self.desktop_notifier.tick(&rows) {
            notify_desktop::post(banner);
        }
    }

    pub fn tick(&mut self) {
        if let Some((_, expiry)) = &self.message {
            if now_ms() > *expiry {
                self.message = None;
            }
        }
        self.poll_pending_clones();
        if now_ms().saturating_sub(self.last_meta_poll) >= META_POLL_MS {
            self.last_meta_poll = now_ms();
            let mut changed = false;
            for s in &mut self.sessions {
                s.poll_meta();
                changed |= s.auto_archive_tick(self.auto_archive_ms);
            }
            if changed {
                self.save();
            }
        }
        if let Some(r) = &self.remote {
            self.remote_snap = r.snapshot();
        }
        self.tick_desktop_notify();
        // Remote rows can appear after startup (first poll): give an empty
        // selection something to land on.
        if self.selected_id.is_none() {
            self.selected_id = self.ordered_ids().first().copied();
        }
        // Drop a dead attach; if the attached remote session vanished from a
        // healthy listing, it was deleted elsewhere.
        if let Some(a) = &self.attach {
            let gone = self.remote_snap.ok && self.remote_info(a.remote_id).is_none();
            if a.is_closed() || gone {
                self.attach = None;
                self.set_message("remote attach ended".into());
            }
        }
        // A selected remote session that disappeared falls back to the top.
        if let Some(SelId::Remote(id)) = self.selected_id {
            if self.remote_snap.ok && self.remote_info(id).is_none() {
                self.selected_id = self.ordered_ids().first().copied();
                self.focus = Focus::Sidebar;
            }
        }
        // If the focused pane's process died, fall back to the sidebar.
        match self.focus {
            Focus::Claude => {
                let alive = match self.selected_id {
                    Some(SelId::Local(_)) => self
                        .selected()
                        .map(|s| !s.claude.is_exited())
                        .unwrap_or(false),
                    Some(SelId::Remote(id)) => self
                        .attach
                        .as_ref()
                        .map(|a| a.remote_id == id && !a.is_closed())
                        .unwrap_or(false),
                    None => false,
                };
                if !alive {
                    self.focus = Focus::Sidebar;
                }
            }
            Focus::Shell => {
                let dead = self
                    .selected()
                    .and_then(|s| s.shell.as_ref())
                    .map(|p| p.is_exited())
                    .unwrap_or(true);
                if dead {
                    if let Some(s) = self.selected_mut() {
                        s.shell_open = false;
                    }
                    self.focus = Focus::Claude;
                }
            }
            Focus::Sidebar => {}
        }
    }

    /// Resize every session's PTYs to match the geometry they would render at.
    pub fn sync_sizes(&mut self, area: Rect) {
        let rects = crate::ui::layout(area);
        let content = rects.content;
        self.content_rect = content;
        for s in &mut self.sessions {
            let (claude_rect, shell_rect) = pane_rects(content, s.shell_open);
            let c = inner(claude_rect);
            s.claude.resize(c.height, c.width);
            if let (Some(sr), Some(shell)) = (shell_rect, s.shell.as_mut()) {
                let r = inner(sr);
                shell.resize(r.height, r.width);
            }
        }
        if let Some(a) = &mut self.attach {
            let (claude_rect, _) = pane_rects(content, false);
            let r = inner(claude_rect);
            a.resize(r.height, r.width);
        }
    }

    pub fn kill_all(&mut self) {
        for s in &mut self.sessions {
            s.kill();
        }
    }

    // ---- input handling ----

    pub fn handle_event(&mut self, ev: Event) {
        match ev {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.handle_key(key),
            Event::Paste(text) => self.handle_paste(text),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        self.selection = None;
        if !matches!(self.modal, Modal::None) {
            self.handle_modal_key(key);
            return;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        // Global chords — identical in every focus, so muscle memory carries
        // across the sidebar and the panes.
        if ctrl && matches!(key.code, KeyCode::Char('q')) {
            // step out to the sidebar from anywhere
            self.focus = Focus::Sidebar;
            return;
        }
        if ctrl && is_backslash(key.code) {
            // toggle the shell pane from anywhere; opening focuses it
            self.toggle_shell(true);
            return;
        }
        if alt && matches!(key.code, KeyCode::Left) {
            self.cycle_session(-1);
            return;
        }
        if alt && matches!(key.code, KeyCode::Right) {
            self.cycle_session(1);
            return;
        }
        if ctrl && matches!(key.code, KeyCode::Char('e')) {
            self.open_editor_for_selection();
            return;
        }
        if ctrl && matches!(key.code, KeyCode::Char('n')) {
            // Step out to the sidebar so modal paste routing works.
            self.focus = Focus::Sidebar;
            self.open_new_session_modal();
            return;
        }
        if ctrl && matches!(key.code, KeyCode::Char('x')) {
            self.focus = Focus::Sidebar;
            self.confirm_close_selected();
            return;
        }

        match self.focus {
            Focus::Sidebar => self.handle_sidebar_key(key),
            Focus::Claude => self.forward_key(key, false),
            Focus::Shell => self.forward_key(key, true),
        }
    }

    fn forward_key(&mut self, key: KeyEvent, to_shell: bool) {
        if to_shell {
            self.shell_scroll = 0;
        } else {
            self.claude_scroll = 0;
        }
        if !to_shell {
            if let Some(SelId::Remote(id)) = self.selected_id {
                let Some(a) = &self.attach else { return };
                if a.remote_id != id {
                    return;
                }
                let app_cursor = a
                    .parser
                    .lock()
                    .map(|p| p.screen().application_cursor())
                    .unwrap_or(false);
                a.write_input(&encode_key(&key, app_cursor));
                return;
            }
        }
        let Some(s) = self.selected_mut() else { return };
        let pty = if to_shell {
            match s.shell.as_mut() {
                Some(p) => p,
                None => return,
            }
        } else {
            &mut s.claude
        };
        let app_cursor = pty
            .parser
            .lock()
            .map(|p| p.screen().application_cursor())
            .unwrap_or(false);
        let bytes = encode_key(&key, app_cursor);
        pty.write_input(&bytes);
        if !to_shell && s.unarchive_on_input() {
            self.save();
        }
    }

    fn handle_paste(&mut self, text: String) {
        let to_shell = match self.focus {
            Focus::Shell => true,
            Focus::Claude => false,
            Focus::Sidebar => {
                // Paste into an open text input modal.
                if let Modal::Input { buf, .. } = &mut self.modal {
                    buf.push_str(text.trim_end_matches(['\r', '\n']));
                }
                return;
            }
        };
        if !to_shell {
            if let Some(SelId::Remote(id)) = self.selected_id {
                let Some(a) = &self.attach else { return };
                if a.remote_id != id {
                    return;
                }
                let bracketed = a
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
                a.write_input(&bytes);
                return;
            }
        }
        let Some(s) = self.selected_mut() else { return };
        let pty = if to_shell {
            match s.shell.as_mut() {
                Some(p) => p,
                None => return,
            }
        } else {
            &mut s.claude
        };
        let bracketed = pty
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
        pty.write_input(&bytes);
    }

    fn open_editor_for_selection(&mut self) {
        if matches!(self.selected_id, Some(SelId::Remote(_))) {
            self.set_message("remote session — folder lives on the daemon host".into());
        } else {
            self.open_editor();
        }
    }

    fn open_new_session_modal(&mut self) {
        let buf = match &self.config.new_session_dir {
            Some(d) => {
                let d = d.trim_end_matches('/');
                format!("{d}/")
            }
            None => format!("{}", self.launch_dir.display()),
        };
        self.modal = Modal::Input {
            kind: InputKind::NewSessionPath,
            title: "new session — repo path or github url (tab completes)".into(),
            buf,
            candidates: Vec::new(),
        };
    }

    fn confirm_close_selected(&mut self) {
        match self.selected_id {
            Some(SelId::Remote(id)) => {
                self.modal = Modal::ConfirmKill {
                    id: SelId::Remote(id),
                };
            }
            Some(SelId::Local(_)) => {
                if let Some(s) = self.selected() {
                    self.modal = if s.is_worktree {
                        Modal::ConfirmCloseWorktree { id: s.id }
                    } else {
                        Modal::ConfirmKill {
                            id: SelId::Local(s.id),
                        }
                    };
                }
            }
            None => {}
        }
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let remote_selected = matches!(self.selected_id, Some(SelId::Remote(_)));
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                if remote_selected {
                    self.attach_selected_remote();
                } else if let Some(s) = self.selected() {
                    if s.claude.is_exited() {
                        self.set_message("claude exited — press r to restart".into());
                    } else {
                        self.focus = Focus::Claude;
                    }
                }
            }
            KeyCode::Char('t') if remote_selected => {
                self.set_message("no shell pane for remote sessions".into());
            }
            KeyCode::Char('t') => self.toggle_shell(true),
            KeyCode::Char('e') => self.open_editor_for_selection(),
            KeyCode::Char('n') => self.open_new_session_modal(),
            KeyCode::Char('c') => {
                self.modal = Modal::Input {
                    kind: InputKind::CloneUrl,
                    title: "clone repo — github url or owner/repo".into(),
                    buf: String::new(),
                    candidates: Vec::new(),
                };
            }
            KeyCode::Char('w') => {
                if let Some(s) = self.selected() {
                    self.modal = Modal::Input {
                        kind: InputKind::NewWorktreeBranch {
                            repo_root: s.repo_root.clone(),
                        },
                        title: format!(
                            "new worktree in {} — branch name",
                            s.repo_root
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default()
                        ),
                        buf: String::new(),
                        candidates: Vec::new(),
                    };
                } else {
                    self.set_message("no session selected — press n to add a repo first".into());
                }
            }
            KeyCode::Char('a') => self.toggle_archive(),
            KeyCode::Char('r') => match self.selected_id {
                Some(SelId::Local(id)) => self.restart_session(id),
                Some(SelId::Remote(id)) => self.restart_remote(id),
                None => {}
            },
            KeyCode::Char('x') => self.confirm_close_selected(),
            KeyCode::Char('i') => {
                if self.selected().is_some() || self.selected_remote().is_some() {
                    self.modal = Modal::Info;
                }
            }
            KeyCode::Char('v') => {
                if self.selected().is_some() || self.selected_remote().is_some() {
                    self.modal = Modal::Activity;
                }
            }
            KeyCode::Char('g') if remote_selected => {
                self.set_message("GSD view is for local sessions".into());
            }
            KeyCode::Char('g') => {
                if self.selected().is_some() {
                    self.modal = Modal::Gsd;
                }
            }
            KeyCode::Char('?') => self.modal = Modal::Help,
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) {
        match &mut self.modal {
            Modal::Help | Modal::Info | Modal::Gsd | Modal::Activity => {
                self.modal = Modal::None;
            }
            Modal::Input {
                kind,
                buf,
                candidates,
                ..
            } => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Backspace => {
                    buf.pop();
                    candidates.clear();
                }
                // Shell-style clear-line, for replacing a long prefill.
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    buf.clear();
                    candidates.clear();
                }
                KeyCode::Tab => {
                    if matches!(
                        kind,
                        InputKind::NewSessionPath | InputKind::CloneDest { .. }
                    ) {
                        let (completed, names) = complete_dir_path(buf);
                        if let Some(c) = completed {
                            *buf = c;
                        }
                        *candidates = names;
                    }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    buf.push(c);
                    candidates.clear();
                }
                KeyCode::Enter => {
                    let modal = std::mem::replace(&mut self.modal, Modal::None);
                    if let Modal::Input { kind, buf, .. } = modal {
                        self.submit_input(kind, buf.trim().to_string());
                    }
                }
                _ => {}
            },
            Modal::ConfirmKill { id } => {
                let id = *id;
                match key.code {
                    KeyCode::Char('y') | KeyCode::Enter => {
                        self.modal = Modal::None;
                        match id {
                            SelId::Local(id) => self.remove_session(id),
                            SelId::Remote(id) => self.remove_remote(id),
                        }
                    }
                    KeyCode::Char('n') | KeyCode::Esc => self.modal = Modal::None,
                    _ => {}
                }
            }
            Modal::ConfirmCloseWorktree { id } => {
                let id = *id;
                match key.code {
                    KeyCode::Char('k') => {
                        self.modal = Modal::None;
                        match self.close_retained_session(id) {
                            Ok(_) => self.set_message("session closed — worktree kept".into()),
                            Err(error) => self
                                .set_message(format!("session close degraded or blocked: {error}")),
                        }
                    }
                    KeyCode::Char('r') => {
                        let checkout = checkout_for_runtime(&self.runtime_checkouts, id);
                        match checkout
                            .ok_or_else(|| {
                                lifecycle::RemovalFailure::Inspection(format!(
                                    "runtime {id} has no retained checkout"
                                ))
                            })
                            .and_then(|checkout| self.prepare_remove_worktree(checkout))
                        {
                            Ok(confirmation) => {
                                self.modal = Modal::ConfirmRemoveWorktree { confirmation };
                            }
                            Err(error) => {
                                self.modal = Modal::None;
                                self.set_message(error.to_string());
                            }
                        }
                    }
                    KeyCode::Esc => self.modal = Modal::None,
                    _ => {}
                }
            }
            Modal::ConfirmRemoveWorktree { .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let modal = std::mem::replace(&mut self.modal, Modal::None);
                    let Modal::ConfirmRemoveWorktree { confirmation } = modal else {
                        unreachable!();
                    };
                    match self.confirm_remove_worktree(confirmation) {
                        Ok(LifecycleOutcome::Removed { branch_ref, .. }) => self.set_message(
                            format!("worktree removed — local branch {branch_ref} retained"),
                        ),
                        Ok(LifecycleOutcome::TopologyCommittedStateDegraded { detail, .. }) => {
                            self.set_message(format!(
                                "worktree topology changed; retained state needs repair: {detail}"
                            ));
                        }
                        Ok(other) => self
                            .set_message(format!("unexpected worktree removal outcome: {other:?}")),
                        Err(error) => self.set_message(error.to_string()),
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => self.modal = Modal::None,
                _ => {}
            },
            Modal::None => {}
        }
    }

    fn submit_input(&mut self, kind: InputKind, value: String) {
        if value.is_empty() {
            return;
        }
        match kind {
            InputKind::NewSessionPath => {
                let expanded = expand_tilde(&value);
                let expanded = expanded.canonicalize().unwrap_or(expanded);
                if expanded.is_dir() {
                    self.open_repo_session(expanded);
                    return;
                }
                // Not on disk — fall through to the clone flow if the input
                // names a repo: a url / owner-repo shorthand, or a
                // not-yet-cloned path whose tail is <host>/<owner>/<repo>
                // (the ghq-style layout), which clones right where typed.
                if let Some(t) = git::parse_clone_target(&value) {
                    self.prompt_clone_dest(t, None);
                    return;
                }
                let tail: Vec<&str> = value
                    .trim_end_matches('/')
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .collect();
                let tail_target = tail
                    .len()
                    .checked_sub(3)
                    .and_then(|i| git::parse_clone_target(&tail[i..].join("/")));
                if let Some(t) = tail_target {
                    self.prompt_clone_dest(t, Some(value));
                    return;
                }
                self.set_message(format!("not a directory: {}", expanded.display()));
            }
            InputKind::CloneUrl => {
                match git::parse_clone_target(&value) {
                    Some(t) => self.prompt_clone_dest(t, None),
                    None => self.set_message(format!("can't parse repo: {value}")),
                };
            }
            InputKind::CloneDest { url, name } => {
                let dest = expand_tilde(&value);
                // Already cloned there? Just open a session on it.
                if dest.join(".git").exists() {
                    self.set_message(format!("{name} already cloned — opening session"));
                    self.open_repo_session(dest);
                    return;
                }
                let occupied = dest.exists()
                    && std::fs::read_dir(&dest)
                        .map(|mut d| d.next().is_some())
                        .unwrap_or(true);
                if occupied {
                    self.set_message(format!("destination not empty: {}", dest.display()));
                    return;
                }
                let (tx, rx) = std::sync::mpsc::channel();
                let thread_dest = dest.clone();
                std::thread::spawn(move || {
                    let res = git::clone_repo(&url, &thread_dest).map_err(|e| e.to_string());
                    let _ = tx.send(res);
                });
                self.set_message(format!("cloning {name}…"));
                self.pending_clones.push(PendingClone { name, dest, rx });
            }
            InputKind::NewWorktreeBranch { repo_root } => {
                // Daemon mode: daemon creates the worktree and spawns the session.
                if let Some(remote) = &self.remote {
                    match remote.create(repo_root.to_str().unwrap_or(""), Some(&value), None) {
                        Ok(()) => self.set_message("worktree session queued on daemon".into()),
                        Err(e) => self.set_message(format!("daemon: {e}")),
                    }
                    return;
                }
                match self.activate_branch_worktree(&repo_root, &value) {
                    Ok(LifecycleOutcome::Busy { .. }) => {
                        self.set_message("repository lifecycle is busy; retry the action".into())
                    }
                    Ok(_) => self.set_message(format!("activated branch {value}")),
                    Err(e) => self.set_message(format!("worktree: {e}")),
                }
            }
        }
    }

    /// Open the clone-destination prompt for a parsed clone target. The
    /// buffer prefills with `dest` when the user already typed a path (the
    /// `n` fallthrough), else the ghq-style `clone_base_dir` layout.
    fn prompt_clone_dest(&mut self, t: git::CloneTarget, dest: Option<String>) {
        let buf = dest.unwrap_or_else(|| {
            let base = self
                .config
                .clone_base_dir
                .clone()
                .unwrap_or_else(|| "~/Code".into());
            let base = base.trim_end_matches('/');
            format!("{base}/{}/{}/{}", t.host, t.owner, t.repo)
        });
        let name = format!("{}/{}", t.owner, t.repo);
        self.modal = Modal::Input {
            kind: InputKind::CloneDest { url: t.url, name },
            title: format!("clone {}/{} — destination", t.owner, t.repo),
            buf,
            candidates: Vec::new(),
        };
    }

    /// Open a session on an existing repo path — via the daemon when one is
    /// configured (so it survives TUI restarts), locally otherwise. Shared by
    /// the `n` new-session flow and clone completion.
    fn open_repo_session(&mut self, path: PathBuf) {
        self.open_repo_session_via(path, LocalAdmissionRoute::Open);
    }

    fn open_repo_session_via(&mut self, path: PathBuf, route: LocalAdmissionRoute) {
        if let Some(remote) = &self.remote {
            match remote.create(&path.to_string_lossy(), None, None) {
                Ok(()) => self.set_message("session queued on daemon".into()),
                Err(e) => self.set_message(format!("daemon: {e}")),
            }
            return;
        }
        debug_assert!(local_admission_route(route, false));
        match self.admit_repository(&path) {
            Ok(Some(_)) => {
                self.focus = Focus::Claude;
            }
            Ok(None) => {}
            Err(e) => self.set_message(format!("repository admission failed: {e}")),
        }
    }

    /// Poll background clones; open a session for each one that finished.
    /// A completion never steals focus from a pane the user is typing in —
    /// it only auto-selects the new session when the sidebar has focus.
    fn poll_pending_clones(&mut self) {
        use std::sync::mpsc::TryRecvError;
        let mut i = 0;
        while i < self.pending_clones.len() {
            let res = match self.pending_clones[i].rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    i += 1;
                    continue;
                }
                Ok(res) => res,
                Err(TryRecvError::Disconnected) => Err("clone worker died".into()),
            };
            let pc = self.pending_clones.remove(i);
            match res {
                Ok(()) => {
                    let (prev_sel, prev_focus) = (self.selected_id, self.focus);
                    self.set_message(format!("cloned {}", pc.name));
                    self.open_repo_session_via(pc.dest, LocalAdmissionRoute::CloneCompletion);
                    if prev_focus != Focus::Sidebar {
                        self.selected_id = prev_sel;
                        self.focus = prev_focus;
                    }
                }
                Err(e) => self.set_message(format!("clone {}: {e}", pc.name)),
            }
        }
        // Keep a visible heartbeat while clones run (messages expire after 5s).
        if !self.pending_clones.is_empty() && self.message.is_none() {
            let names: Vec<&str> = self
                .pending_clones
                .iter()
                .map(|p| p.name.as_str())
                .collect();
            self.set_message(format!("cloning {}…", names.join(", ")));
        }
    }

    // ---- remote session actions ----

    /// Attach (or re-focus an existing attach) to the selected remote session.
    fn attach_selected_remote(&mut self) {
        let Some(SelId::Remote(id)) = self.selected_id else {
            return;
        };
        if self.remote_info(id).map(|r| r.status == "exited") == Some(true) {
            self.set_message("claude exited — press r to restart".into());
            return;
        }
        if let Some(a) = &self.attach {
            if a.remote_id == id && !a.is_closed() {
                self.focus = Focus::Claude;
                return;
            }
        }
        let Some(r) = &self.remote else { return };
        let (claude_rect, _) = pane_rects(self.content_rect, false);
        let ir = inner(claude_rect);
        match RemoteAttach::connect(&r.base, id, ir.height, ir.width) {
            Ok(a) => {
                self.attach = Some(a);
                self.focus = Focus::Claude;
            }
            Err(e) => self.set_message(format!("attach: {e}")),
        }
    }

    fn restart_remote(&mut self, id: u64) {
        if self.remote_info(id).map(|r| r.status == "exited") != Some(true) {
            self.set_message("claude is still running".into());
            return;
        }
        let Some(r) = &self.remote else { return };
        match r.restart(id) {
            Ok(()) => self.set_message("restarting remote claude…".into()),
            Err(e) => self.set_message(format!("restart: {e}")),
        }
    }

    fn remove_remote(&mut self, id: u64) {
        if let Some(a) = &self.attach {
            if a.remote_id == id {
                self.attach = None;
            }
        }
        let Some(r) = &self.remote else { return };
        match r.delete(id) {
            Ok(()) => {
                self.set_message("remote session killed".into());
                if self.selected_id == Some(SelId::Remote(id)) {
                    self.selected_id = self.ordered_ids().first().copied();
                }
                self.focus = Focus::Sidebar;
            }
            Err(e) => self.set_message(format!("kill: {e}")),
        }
    }

    /// `a` — park/unpark the selected session.
    fn toggle_archive(&mut self) {
        match self.selected_id {
            Some(SelId::Local(id)) => {
                let Some(s) = self.session_mut(id) else {
                    return;
                };
                s.set_archived(!s.archived);
                let msg = if s.archived { "archived" } else { "unarchived" };
                self.set_message(msg.into());
                self.save();
            }
            Some(SelId::Remote(id)) => {
                let archived = self.is_archived(SelId::Remote(id));
                let Some(r) = &self.remote else { return };
                match r.set_archived(id, !archived) {
                    Ok(()) => self.set_message(if archived {
                        "unarchived".into()
                    } else {
                        "archived".into()
                    }),
                    Err(e) => self.set_message(format!("archive: {e}")),
                }
            }
            None => {}
        }
    }

    fn move_selection(&mut self, delta: i64) {
        let ids = self.ordered_ids();
        if ids.is_empty() {
            return;
        }
        let cur = self
            .selected_id
            .and_then(|id| ids.iter().position(|&x| x == id))
            .unwrap_or(0) as i64;
        let next = (cur + delta).clamp(0, ids.len() as i64 - 1) as usize;
        if self.selected_id != Some(ids[next]) {
            self.claude_scroll = 0;
            self.shell_scroll = 0;
            self.selection = None;
        }
        self.selected_id = Some(ids[next]);
    }

    /// Cycle the selection to the next/prev session in sidebar order,
    /// wrapping around. When attached, stays attached to the same kind of
    /// pane — falling back to the claude pane if the new session has no shell.
    fn cycle_session(&mut self, delta: i64) {
        // Cycling reaches the archive too — sending input into an archived
        // session auto-unarchives it, so landing there and typing resurfaces
        // it without an explicit `a`.
        let ids = self.ordered_ids();
        if ids.is_empty() {
            return;
        }
        let len = ids.len() as i64;
        let cur = self
            .selected_id
            .and_then(|id| ids.iter().position(|&x| x == id))
            .unwrap_or(0) as i64;
        let next = (((cur + delta) % len) + len) % len;
        self.claude_scroll = 0;
        self.shell_scroll = 0;
        self.selection = None;
        self.selected_id = Some(ids[next as usize]);
        if self.focus == Focus::Shell {
            let has_shell = self
                .selected()
                .map(|s| s.shell_open && s.shell.is_some())
                .unwrap_or(false);
            if !has_shell {
                self.focus = Focus::Claude;
            }
        }
    }

    /// Launch the configured editor on the selected session's folder.
    /// Detached, with no inherited stdio, so a GUI editor (the `code` default)
    /// doesn't disturb the TUI.
    fn open_editor(&mut self) {
        let Some(cwd) = self.selected().map(|s| s.cwd.clone()) else {
            self.set_message("no session selected".into());
            return;
        };
        let cmd = self.editor_cmd();
        let cwd_str = cwd.to_string_lossy().to_string();
        // `sh -c '<cmd> "$1"' sh <cwd>` keeps args in editor_cmd intact and
        // safely passes a path that may contain spaces.
        match Command::new("sh")
            .arg("-c")
            .arg(format!("{cmd} \"$1\""))
            .arg("sh")
            .arg(&cwd_str)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_) => self.set_message(format!("opening {cmd}")),
            Err(e) => self.set_message(format!("editor: {e}")),
        }
    }

    fn toggle_shell(&mut self, focus_it: bool) {
        let content = self.content_rect;
        let Some(s) = self.selected_mut() else {
            return;
        };
        if s.shell_open {
            s.shell_open = false;
            if self.focus == Focus::Shell {
                self.focus = Focus::Claude;
            }
        } else {
            let (_, shell_rect) = pane_rects(content, true);
            let r = shell_rect.map(inner).unwrap_or(Rect::new(0, 0, 80, 10));
            match s.open_shell(r.height, r.width) {
                Ok(()) => {
                    if focus_it {
                        self.focus = Focus::Shell;
                    }
                }
                Err(e) => self.set_message(format!("shell: {e}")),
            }
        }
        self.save();
    }

    fn restart_session(&mut self, id: u64) {
        let result = (|| -> Result<()> {
            let mut mode = backend::SpawnMode::Fresh;
            if let Some(checkout_key) = checkout_for_runtime(&self.runtime_checkouts, id) {
                if !self.reconcile_primary(checkout_key) {
                    self.save_durable()?;
                    anyhow::bail!("checkout changed externally; restart blocked");
                }
                self.save_durable()?;
                mode = self
                    .repository_state
                    .checkouts
                    .iter()
                    .find(|checkout| checkout.key == checkout_key)
                    .and_then(|checkout| checkout.session.resume_id.clone())
                    .or_else(|| {
                        self.session(id)
                            .and_then(|session| session.meta.session_id.clone())
                    })
                    .map(backend::SpawnMode::ResumeId)
                    .unwrap_or(backend::SpawnMode::ContinueLatest);
            }
            self.restart_session_with_mode(id, mode)
        })();
        if let Err(error) = result {
            self.set_message(format!("restart failed: {error}"));
        }
    }

    fn restart_session_with_resume(&mut self, id: u64, resume: bool) -> Result<()> {
        let mode = if resume {
            backend::SpawnMode::ContinueLatest
        } else {
            backend::SpawnMode::Fresh
        };
        self.restart_session_with_mode(id, mode)
    }

    fn restart_session_with_mode(&mut self, id: u64, mode: backend::SpawnMode) -> Result<()> {
        let (rows, cols) = {
            let Some(s) = self.session(id) else {
                anyhow::bail!("session {id} is missing");
            };
            if !s.claude.is_exited() {
                anyhow::bail!("claude is still running");
            }
            self.claude_spawn_size(s.shell_open)
        };
        let cwd = self.session(id).map(|s| s.cwd.clone()).unwrap();
        // Route through the backend like add_session — previously this
        // hand-rolled `exec {claude_cmd}`, so a restarted session silently
        // lost its permission flag (and, for opencode, needs a fresh pinned
        // server port). TUI restarts stay fresh-start (no resume), matching
        // the prior behavior.
        let be = backend::active();
        let base = be.resolve_cmd(&self.claude_cmd()).cmd;
        let plan = be.spawn_plan(&base, None, mode);
        be.prepare_cwd(&cwd);
        let pty = Pty::spawn_with_env(Some(&plan.cmd), &plan.env, &cwd, rows, cols)?;
        if let Some(s) = self.session_mut(id) {
            s.claude = pty;
            s.spawn_unix_ms = now_unix_ms();
            s.meta = ClaudeMeta::default();
            s.meta.backend_port = plan.server_port;
        }
        self.focus = Focus::Claude;
        Ok(())
    }

    fn restore_stopped_runtime(
        &mut self,
        id: u64,
        mode: backend::SpawnMode,
        shell_open: bool,
    ) -> std::result::Result<(), RuntimeRestartFailure> {
        if let Err(error) = self.restart_session_with_mode(id, mode) {
            return Err(RuntimeRestartFailure {
                agent_restarted: false,
                shell_restarted: !shell_open,
                detail: format!("agent: {error}"),
            });
        }
        let agent_restarted = self
            .session(id)
            .is_some_and(|session| !session.claude.is_exited());
        let shell_restarted = if shell_open {
            let (_, shell_rect) = pane_rects(self.content_rect, true);
            let rect = shell_rect.map(inner).unwrap_or(Rect::new(0, 0, 80, 10));
            match self.session_mut(id) {
                Some(session) => session.open_shell(rect.height, rect.width).is_ok_and(|()| {
                    session
                        .shell
                        .as_ref()
                        .is_some_and(|shell| !shell.is_exited())
                }),
                None => false,
            }
        } else {
            true
        };
        if agent_restarted && shell_restarted {
            return Ok(());
        }
        if let Some(session) = self.session_mut(id) {
            let _ = session.kill_and_wait();
        }
        Err(RuntimeRestartFailure {
            agent_restarted,
            shell_restarted,
            detail: format!(
                "agent live: {agent_restarted}; shell live/restored: {shell_restarted}"
            ),
        })
    }

    // ---- mouse handling ----

    fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
        x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
    }

    fn content_pane_rects(&self) -> (Rect, Option<Rect>) {
        let shell_open = self.selected().map(|s| s.shell_open).unwrap_or(false);
        let (claude_rect, shell_rect) = pane_rects(self.content_rect, shell_open);
        (inner(claude_rect), shell_rect.map(inner))
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(self.modal, Modal::None) {
            return;
        }
        let col = mouse.column;
        let row = mouse.row;
        let (claude_inner, shell_inner) = self.content_pane_rects();

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.selection = None;
                if Self::rect_contains(claude_inner, col, row) {
                    let scroll_state = self.selected().map(|s| s.claude.scroll_info());
                    match scroll_state {
                        Some((true, true, sgr)) => {
                            let px = col.saturating_sub(claude_inner.x) as usize + 1;
                            let py = row.saturating_sub(claude_inner.y) as usize + 1;
                            let bytes = encode_mouse_scroll(true, px, py, sgr);
                            if let Some(s) = self.selected_mut() {
                                s.claude.write_input(&bytes);
                            }
                        }
                        Some((false, _, _)) | None => {
                            self.claude_scroll = self.claude_scroll.saturating_add(3);
                        }
                        _ => {}
                    }
                } else if shell_inner.map(|r| Self::rect_contains(r, col, row)) == Some(true) {
                    self.shell_scroll = self.shell_scroll.saturating_add(3);
                }
            }
            MouseEventKind::ScrollDown => {
                self.selection = None;
                if Self::rect_contains(claude_inner, col, row) {
                    let scroll_state = self.selected().map(|s| s.claude.scroll_info());
                    match scroll_state {
                        Some((true, true, sgr)) => {
                            let px = col.saturating_sub(claude_inner.x) as usize + 1;
                            let py = row.saturating_sub(claude_inner.y) as usize + 1;
                            let bytes = encode_mouse_scroll(false, px, py, sgr);
                            if let Some(s) = self.selected_mut() {
                                s.claude.write_input(&bytes);
                            }
                        }
                        Some((false, _, _)) | None => {
                            self.claude_scroll = self.claude_scroll.saturating_sub(3);
                        }
                        _ => {}
                    }
                } else if shell_inner.map(|r| Self::rect_contains(r, col, row)) == Some(true) {
                    self.shell_scroll = self.shell_scroll.saturating_sub(3);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let (pane_area, is_shell) = if Self::rect_contains(claude_inner, col, row) {
                    (claude_inner, false)
                } else if shell_inner.map(|r| Self::rect_contains(r, col, row)) == Some(true) {
                    (shell_inner.unwrap(), true)
                } else {
                    self.selection = None;
                    return;
                };
                let r = row.saturating_sub(pane_area.y);
                let c = col.saturating_sub(pane_area.x);
                self.selection = Some(Selection {
                    start_row: r,
                    start_col: c,
                    end_row: r,
                    end_col: c,
                    pane_area,
                    is_shell,
                });
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(sel) = &mut self.selection {
                    let r = row
                        .saturating_sub(sel.pane_area.y)
                        .min(sel.pane_area.height.saturating_sub(1));
                    let c = col
                        .saturating_sub(sel.pane_area.x)
                        .min(sel.pane_area.width.saturating_sub(1));
                    sel.end_row = r;
                    sel.end_col = c;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(sel) = self.selection.take() {
                    let (sr, sc, er, ec) = sel.normalized();
                    if sr == er && sc == ec {
                        return;
                    }
                    let scroll = if sel.is_shell {
                        self.shell_scroll
                    } else {
                        self.claude_scroll
                    };
                    let parser = if sel.is_shell {
                        self.selected()
                            .and_then(|s| s.shell.as_ref())
                            .map(|p| &p.parser)
                    } else {
                        match self.selected_id {
                            Some(SelId::Remote(_)) => self.attach.as_ref().map(|a| &a.parser),
                            _ => self.selected().map(|s| &s.claude.parser),
                        }
                    };
                    if let Some(parser) = parser {
                        if let Ok(mut p) = parser.lock() {
                            p.set_scrollback(scroll);
                            let screen = p.screen();
                            // vt100's row wrap metadata is authoritative here:
                            // contents_between omits only newlines after rows marked
                            // as terminal continuations and preserves explicit ones.
                            let text = screen.contents_between(sr, sc, er, ec + 1);
                            p.set_scrollback(0);
                            if !text.is_empty() {
                                Self::copy_to_clipboard(&text);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn copy_to_clipboard(text: &str) {
        use std::io::Write;
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
        }
    }
}

#[cfg(test)]
mod clipboard_tests {
    use baude_core::vt100;

    fn selected(input: &str, rows: u16, cols: u16, end_row: u16, end_col: u16) -> String {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(input.as_bytes());
        parser.screen().contents_between(0, 0, end_row, end_col)
    }

    #[test]
    fn terminal_soft_wrap_pastes_as_one_command() {
        assert_eq!(
            selected("123456789abcdefghi", 3, 10, 1, 8),
            "123456789abcdefghi"
        );
    }

    #[test]
    fn preserves_full_width_explicit_newline_and_indentation() {
        assert_eq!(
            selected("123456789\r\n  second", 3, 10, 1, 8),
            "123456789\n  second"
        );
    }

    #[test]
    fn preserves_tabs_combining_marks_and_wide_glyphs_across_real_newlines() {
        assert_eq!(
            selected("tab\tvalue\r\ne\u{301} and 界\r\nnext", 5, 20, 2, 4),
            "tab     value\ne\u{301} and 界\nnext"
        );
    }
}

#[cfg(test)]
mod repository_admission_tests {
    use super::{
        active_restore_checkouts, checkout_for_runtime, local_admission_route,
        require_same_checkout_path, App, LocalAdmissionRoute, Modal,
    };
    use baude_core::lifecycle::{LifecycleOutcome, RepositoryReservations};
    use baude_core::repository::{
        CheckoutHealth, CheckoutKey, CheckoutRole, PersistedPath, RepositoryHealth,
        RepositoryState, RetainedSessionState, SavedCheckout, SavedRepository, UnavailableCause,
    };
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;

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

    fn git(repo: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
    }

    fn admission_repo(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("baude-admission-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let origin = root.join("origin.git");
        let repo = root.join("repo");
        std::fs::create_dir_all(&origin).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
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
        repo
    }

    fn removal_app(
        label: &str,
        key_offset: u64,
    ) -> (App, PathBuf, PathBuf, CheckoutKey, u64, PathBuf) {
        let repo = admission_repo(label);
        let root = repo.parent().unwrap().to_path_buf();
        let state_root = root.join("state");
        std::fs::create_dir_all(&state_root).unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
        app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
        app.persistence_root_for_test = Some(state_root);
        app.repository_state.next_repository_key = u64::from(std::process::id()) + key_offset;
        let created = app
            .activate_branch_worktree(&repo, &format!("feature/{label}"))
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        let path = app.repository_state.checkouts[0]
            .observed_path
            .to_path_buf();
        (app, repo, root, checkout, runtime, path)
    }

    fn add_checkout(state: &mut RepositoryState, role: CheckoutRole, active_intent: bool) {
        let repository_key = state.repositories[0].key;
        let key = state.allocate_checkout_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        let path = PersistedPath::from_path(Path::new("/repo/checkout"));
        state.checkouts.push(SavedCheckout {
            key,
            repository_key,
            role,
            managed_by_baude: false,
            observed_path: path.clone(),
            observed_branch: Some("refs/heads/main".into()),
            first_seen_order: order,
            active_intent,
            session: RetainedSessionState {
                name: format!("{role:?}"),
                cwd: path.clone(),
                repo_root: path,
                branch: Some("main".into()),
                is_worktree: role != CheckoutRole::Main,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
            health: CheckoutHealth::Available,
        });
    }

    #[test]
    fn restore_includes_active_primary_and_linked_worktree_sessions() {
        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        let path = PersistedPath::from_path(Path::new("/repo"));
        state.repositories.push(SavedRepository {
            key: repository_key,
            observed_common_dir: path.clone(),
            observed_main_worktree: path,
            first_seen_order: order,
            health: RepositoryHealth::Available,
        });
        add_checkout(&mut state, CheckoutRole::PrimaryDefault, true);
        add_checkout(&mut state, CheckoutRole::ManagedBranch, true);
        add_checkout(&mut state, CheckoutRole::ManagedBranch, true);
        add_checkout(&mut state, CheckoutRole::ManagedBranch, false);

        assert_eq!(active_restore_checkouts(&state).len(), 3);
    }

    #[test]
    fn managed_checkout_ownership_cannot_move_to_an_external_path() {
        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        let path = PersistedPath::from_path(Path::new("/managed/default"));
        state.repositories.push(SavedRepository {
            key: repository_key,
            observed_common_dir: path.clone(),
            observed_main_worktree: path,
            first_seen_order: order,
            health: RepositoryHealth::Available,
        });
        add_checkout(&mut state, CheckoutRole::PrimaryDefault, true);
        state.checkouts[0].managed_by_baude = true;
        state.checkouts[0].observed_path = PersistedPath::from_path(Path::new("/managed/default"));

        assert!(
            require_same_checkout_path(&state.checkouts[0], Path::new("/external/default"))
                .is_err()
        );
        assert!(state.checkouts[0].managed_by_baude);
        assert_eq!(
            state.checkouts[0].observed_path.to_path_buf(),
            PathBuf::from("/managed/default")
        );
    }

    #[test]
    fn manual_restart_resolves_managed_runtime_for_reconciliation() {
        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        let path = PersistedPath::from_path(Path::new("/repo"));
        state.repositories.push(SavedRepository {
            key: repository_key,
            observed_common_dir: path.clone(),
            observed_main_worktree: path,
            first_seen_order: order,
            health: RepositoryHealth::Available,
        });
        add_checkout(&mut state, CheckoutRole::PrimaryDefault, true);
        let checkout_key = state.checkouts[0].key;
        let runtimes = HashMap::from([(checkout_key, 41)]);

        assert_eq!(checkout_for_runtime(&runtimes, 41), Some(checkout_key));
        assert_eq!(checkout_for_runtime(&runtimes, 99), None);
    }

    #[test]
    fn production_admission_retains_intent_without_runtime_on_save_failure() {
        let repo = admission_repo("save-failure");
        let root = repo.parent().unwrap().to_path_buf();
        let blocked_root = root.join("persistence-root");
        std::fs::write(&blocked_root, b"not a directory").unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.persistence_root_for_test = Some(blocked_root.clone());

        let error = app.admit_repository(&repo).unwrap_err().to_string();

        assert!(!error.is_empty());
        assert_eq!(app.save_attempts_for_test.get(), 1);
        assert_eq!(app.spawn_attempts_for_test, 0);
        assert!(app.sessions.is_empty());
        assert!(app.runtime_checkouts.is_empty());
        assert_eq!(app.repository_state.checkouts.len(), 1);
        assert!(app.repository_state.checkouts[0].active_intent);
        assert_eq!(std::fs::read(&blocked_root).unwrap(), b"not a directory");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn production_admission_retains_saved_intent_on_spawn_failure() {
        let repo = admission_repo("spawn-failure");
        let root = repo.parent().unwrap().to_path_buf();
        let state_root = root.join("state");
        std::fs::create_dir_all(&state_root).unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.persistence_root_for_test = Some(state_root.clone());
        app.spawn_error_for_test = Some("pty unavailable".into());

        let error = app.admit_repository(&repo).unwrap_err().to_string();

        assert!(error.contains("pty unavailable"), "got: {error}");
        assert_eq!(app.save_attempts_for_test.get(), 1);
        assert_eq!(app.spawn_attempts_for_test, 1);
        assert!(app.sessions.is_empty());
        assert!(app.runtime_checkouts.is_empty());
        assert_eq!(app.repository_state.checkouts.len(), 1);
        assert!(app.repository_state.checkouts[0].active_intent);

        let state_file = baude_core::workspace::active().state_file("state");
        let persisted = baude_core::persist::load_current_at(&state_root, &state_file).unwrap();
        assert_eq!(persisted.state, app.repository_state);

        let mut restarted = App::new(repo.clone());
        restarted.remote = None;
        restarted.persistence_root_for_test = Some(state_root.clone());
        restarted.spawn_error_for_test = Some("pty unavailable after restart".into());
        restarted.restore();

        assert_eq!(restarted.repository_state, persisted.state);
        assert!(restarted.repository_state.checkouts[0].active_intent);
        assert!(restarted.sessions.is_empty());
        assert!(restarted.runtime_checkouts.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn admission_routes_share_one_local_entrypoint_and_preserve_remote_routing() {
        for route in [
            LocalAdmissionRoute::LaunchDirectory,
            LocalAdmissionRoute::Open,
            LocalAdmissionRoute::CloneCompletion,
        ] {
            assert!(local_admission_route(route, false));
            assert!(!local_admission_route(route, true));
        }
    }

    #[test]
    fn lifecycle_create_activate_local_persists_once_and_reuses_runtime() {
        let repo = admission_repo("branch-activation");
        let root = repo.parent().unwrap().to_path_buf();
        let state_root = root.join("state");
        std::fs::create_dir_all(&state_root).unwrap();
        let snapshot = baude_core::git::discover_repository(&repo).unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
        app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
        app.persistence_root_for_test = Some(state_root.clone());
        app.repository_state.next_repository_key = u64::from(std::process::id());
        let repository = app.repository_state.allocate_repository_key().unwrap();
        let order = app.repository_state.allocate_first_seen_order().unwrap();
        app.repository_state.repositories.push(SavedRepository {
            key: repository,
            observed_common_dir: PersistedPath::from_path(&snapshot.common_dir),
            observed_main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
            first_seen_order: order,
            health: RepositoryHealth::Available,
        });

        let created = app
            .activate_branch_worktree(&repo, "feature/local-contract")
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        assert_eq!(app.repository_state.checkouts.len(), 1);
        assert!(app.repository_state.checkouts[0].managed_by_baude);
        assert!(app.repository_state.checkouts[0].active_intent);
        assert_eq!(app.runtime_checkouts, HashMap::from([(checkout, runtime)]));
        let state_file = baude_core::workspace::active().state_file("state");
        assert_eq!(
            baude_core::persist::load_current_at(&state_root, &state_file)
                .unwrap()
                .state,
            app.repository_state
        );

        assert_eq!(
            app.activate_branch_worktree(&repo, "feature/local-contract")
                .unwrap(),
            LifecycleOutcome::Focused { checkout, runtime }
        );
        assert_eq!(app.repository_state.checkouts.len(), 1);
        assert_eq!(app.runtime_checkouts.len(), 1);

        // An occupied activation still has a real persistence commit boundary.
        // If that save fails before replacement, neither memory nor disk may
        // retain the newly activated intent.
        app.repository_state.checkouts[0].active_intent = false;
        app.save_durable_status().unwrap();
        let inactive = app.repository_state.clone();
        app.atomic_failure_for_test = Some(baude_core::persist::AtomicFailure::Rename);
        let error = app
            .activate_branch_worktree(&repo, "feature/local-contract")
            .unwrap_err()
            .to_string();
        assert!(error.contains("pending activation ownership persistence"));
        assert_eq!(app.repository_state, inactive);
        assert!(app.persistence_dirty);
        assert_eq!(
            baude_core::persist::load_current_at(&state_root, &state_file)
                .unwrap()
                .state,
            inactive
        );
        app.atomic_failure_for_test = None;
        app.save();
        assert!(!app.persistence_dirty);
        assert_eq!(
            app.activate_branch_worktree(&repo, "feature/local-contract")
                .unwrap(),
            LifecycleOutcome::Focused { checkout, runtime }
        );

        let different = app
            .activate_branch_worktree(&repo, "feature/local-distinct")
            .unwrap();
        assert!(matches!(
            different,
            LifecycleOutcome::Created {
                checkout: other_checkout,
                runtime: Some(other_runtime),
            } if other_checkout != checkout && other_runtime != runtime
        ));
        assert_eq!(app.repository_state.checkouts.len(), 2);
        assert_eq!(app.runtime_checkouts.len(), 2);

        git(&repo, &["branch", "external-occupied"]);
        let external = root.join("external-occupied");
        git(
            &repo,
            &[
                "worktree",
                "add",
                external.to_str().unwrap(),
                "external-occupied",
            ],
        );
        let reused = app
            .activate_branch_worktree(&repo, "external-occupied")
            .unwrap();
        assert!(matches!(
            reused,
            LifecycleOutcome::Reused {
                managed_by_baude: false,
                runtime: Some(_),
                ..
            }
        ));
        let external = external.canonicalize().unwrap();
        assert!(
            !app.repository_state
                .checkouts
                .iter()
                .find(|checkout| checkout.observed_path.to_path_buf() == external)
                .unwrap()
                .managed_by_baude
        );

        app.repository_reservations = RepositoryReservations::default();
        let reservation = app.repository_reservations.reserve(repository).unwrap();
        assert_eq!(
            app.activate_branch_worktree(&repo, "feature/while-busy")
                .unwrap(),
            LifecycleOutcome::Busy { repository }
        );
        drop(reservation);
        for session in &mut app.sessions {
            session.kill();
        }
        let linked: Vec<_> = app
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
    fn lifecycle_creation_rollback_local_precommit_save_failure_has_no_partial_child() {
        let repo = admission_repo("branch-rollback");
        let root = repo.parent().unwrap().to_path_buf();
        let blocked_root = root.join("blocked-state-root");
        std::fs::write(&blocked_root, b"not a directory").unwrap();
        let snapshot = baude_core::git::discover_repository(&repo).unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.persistence_root_for_test = Some(blocked_root);
        app.repository_state.next_repository_key = u64::from(std::process::id()) + 10_000;
        let repository = app.repository_state.allocate_repository_key().unwrap();
        let order = app.repository_state.allocate_first_seen_order().unwrap();
        app.repository_state.repositories.push(SavedRepository {
            key: repository,
            observed_common_dir: PersistedPath::from_path(&snapshot.common_dir),
            observed_main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
            first_seen_order: order,
            health: RepositoryHealth::Available,
        });
        let before = app.repository_state.clone();

        let result = app.activate_branch_worktree(&repo, "feature/local-rollback");
        let after = baude_core::git::discover_repository(&repo).unwrap();
        let partial: Vec<_> = after
            .worktrees
            .iter()
            .filter(|record| record.branch.as_deref() == Some("refs/heads/feature/local-rollback"))
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
                "refs/heads/feature/local-rollback",
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success();

        assert!(result.is_err());
        assert!(partial.is_empty(), "save failure left a linked worktree");
        assert_eq!(app.repository_state, before);
        assert!(app.runtime_checkouts.is_empty());
        assert!(app.sessions.is_empty());
        assert!(!branch_retained);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn activation_recovery_reuses_unchanged_preexisting_worktree_after_pending_save_crash() {
        let repo = admission_repo("occupied-pending-recovery");
        let root = repo.parent().unwrap().to_path_buf();
        let state_root = root.join("state");
        std::fs::create_dir_all(&state_root).unwrap();
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

        let snapshot = baude_core::git::discover_repository(&repo).unwrap();
        let mut crashed = App::new(repo.clone());
        crashed.remote = None;
        crashed.persistence_root_for_test = Some(state_root.clone());
        let prepared = baude_core::lifecycle::prepare_activation(
            &mut crashed.repository_state,
            &snapshot,
            "occupied-before-crash",
        )
        .unwrap();
        baude_core::lifecycle::record_pending_activation(
            &mut crashed.repository_state,
            &snapshot,
            &prepared,
        )
        .unwrap();
        crashed.save_durable_status().unwrap();

        let mut restarted = App::new(root.join("not-a-repository"));
        restarted.remote = None;
        restarted.config.claude_cmd = Some("sh -c 'sleep 30'".into());
        restarted.persistence_root_for_test = Some(state_root);
        restarted.restore();

        assert_eq!(restarted.repository_state.checkouts.len(), 1);
        let recovered = &restarted.repository_state.checkouts[0];
        assert_eq!(
            recovered.observed_path.to_path_buf(),
            occupied.canonicalize().unwrap()
        );
        assert!(!recovered.managed_by_baude);
        assert!(recovered.active_intent);
        assert_eq!(recovered.health, CheckoutHealth::Available);
        assert!(restarted.runtime_checkouts.contains_key(&recovered.key));

        restarted
            .sessions
            .iter_mut()
            .for_each(|session| session.kill());
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
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_creation_rollback_local_committed_save_and_spawn_failures_retain_retry_child() {
        for (label, failure, spawn_error, expected_stage) in [
            (
                "branch-postcommit",
                Some(baude_core::persist::AtomicFailure::DirectorySync),
                None,
                "persistence after replacement",
            ),
            (
                "branch-spawn",
                None,
                Some("pty unavailable"),
                "runtime spawn",
            ),
        ] {
            let repo = admission_repo(label);
            let root = repo.parent().unwrap().to_path_buf();
            let state_root = root.join("state");
            std::fs::create_dir_all(&state_root).unwrap();
            let snapshot = baude_core::git::discover_repository(&repo).unwrap();
            let mut app = App::new(repo.clone());
            app.remote = None;
            app.persistence_root_for_test = Some(state_root.clone());
            app.atomic_failure_for_test = failure;
            app.spawn_error_for_test = spawn_error.map(str::to_owned);
            app.repository_state.next_repository_key = u64::from(std::process::id()) + 30_000;
            let repository = app.repository_state.allocate_repository_key().unwrap();
            let order = app.repository_state.allocate_first_seen_order().unwrap();
            app.repository_state.repositories.push(SavedRepository {
                key: repository,
                observed_common_dir: PersistedPath::from_path(&snapshot.common_dir),
                observed_main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
                first_seen_order: order,
                health: RepositoryHealth::Available,
            });
            let branch = format!("feature/{label}");

            let error = app
                .activate_branch_worktree(&repo, &branch)
                .unwrap_err()
                .to_string();
            let pending_failure = failure.is_some();
            if pending_failure {
                assert!(
                    error.contains("pending activation ownership persistence"),
                    "got: {error}"
                );
            } else {
                assert!(error.contains(expected_stage), "got: {error}");
            }
            assert_eq!(app.repository_state.checkouts.len(), 1);
            assert_eq!(
                app.repository_state.checkouts[0].active_intent,
                !pending_failure
            );
            assert!(app.runtime_checkouts.is_empty());
            assert!(app.sessions.is_empty());
            let state_file = baude_core::workspace::active().state_file("state");
            assert_eq!(
                baude_core::persist::load_current_at(&state_root, &state_file)
                    .unwrap()
                    .state,
                app.repository_state
            );
            let path = app.repository_state.checkouts[0]
                .observed_path
                .to_path_buf();
            assert_eq!(path.is_dir(), !pending_failure);
            if pending_failure {
                let mut restarted = App::new(root.join("not-a-repository"));
                restarted.remote = None;
                restarted.persistence_root_for_test = Some(state_root.clone());
                restarted.restore();
                assert!(!restarted.repository_state.has_pending_activation());
                assert!(restarted.repository_state.checkouts.is_empty());
                assert!(restarted.sessions.is_empty());
                assert!(restarted
                    .activate_branch_worktree(&repo, "feature/reload-resolved")
                    .is_ok());
            }
            let _ = Command::new("git")
                .args(["worktree", "remove", "--"])
                .arg(path)
                .current_dir(&repo)
                .status();
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn lifecycle_close_local_snapshots_resume_context_and_retains_hierarchy() {
        let repo = admission_repo("retained-close");
        let root = repo.parent().unwrap().to_path_buf();
        let state_root = root.join("state");
        std::fs::create_dir_all(&state_root).unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
        app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
        app.persistence_root_for_test = Some(state_root.clone());
        app.repository_state.next_repository_key = u64::from(std::process::id()) + 60_000;
        let created = app
            .activate_branch_worktree(&repo, "feature/retained-close")
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        let before = app.repository_state.clone();
        let session = app.session_mut(runtime).unwrap();
        session.name = "retained live name".into();
        session.open_shell(5, 40).unwrap();
        session
            .shell
            .as_ref()
            .unwrap()
            .fail_next_teardown_for_test("shell stop refused once");
        session.archived = true;
        session.archived_by_user = true;
        session.meta.session_id = None;
        app.repository_state.checkouts[0].session.resume_id =
            Some("opaque/retained-before-poll".into());
        app.save_durable_status().unwrap();

        let partial = app.close_retained_session(runtime).unwrap_err().to_string();
        assert!(partial.contains("shell stopped: false"), "got: {partial}");
        assert!(app.session(runtime).unwrap().claude.is_exited());
        assert!(!app
            .session(runtime)
            .unwrap()
            .shell
            .as_ref()
            .unwrap()
            .is_exited());
        assert!(app.repository_state.checkouts[0].active_intent);
        assert!(matches!(
            app.repository_state.checkouts[0].health,
            CheckoutHealth::Unavailable(
                baude_core::repository::UnavailableCause::TeardownPending {
                    agent_stopped: true,
                    shell_stopped: false,
                    ..
                }
            )
        ));
        assert!(app.runtime_checkouts.contains_key(&checkout));

        let mut restarted = App::new(root.join("not-a-repository"));
        restarted.remote = None;
        restarted.persistence_root_for_test = Some(state_root.clone());
        restarted.restore();
        assert!(restarted.sessions.is_empty());
        assert!(restarted.runtime_checkouts.is_empty());
        assert!(!restarted.repository_state.checkouts[0].active_intent);
        assert_eq!(
            restarted.repository_state.checkouts[0].health,
            CheckoutHealth::Available
        );
        assert!(app
            .session(runtime)
            .unwrap()
            .shell
            .as_ref()
            .unwrap()
            .is_exited());

        assert_eq!(
            app.close_retained_session(runtime).unwrap(),
            LifecycleOutcome::Closed { checkout }
        );

        assert!(app.sessions.iter().all(|session| session.id != runtime));
        assert!(!app.runtime_checkouts.contains_key(&checkout));
        assert_eq!(app.repository_state.repositories, before.repositories);
        assert_eq!(app.repository_state.checkouts.len(), before.checkouts.len());
        let retained = &app.repository_state.checkouts[0];
        assert_eq!(retained.key, before.checkouts[0].key);
        assert_eq!(retained.repository_key, before.checkouts[0].repository_key);
        assert_eq!(
            retained.first_seen_order,
            before.checkouts[0].first_seen_order
        );
        assert_eq!(retained.observed_path, before.checkouts[0].observed_path);
        assert_eq!(
            retained.observed_branch,
            before.checkouts[0].observed_branch
        );
        assert!(!retained.active_intent);
        assert_eq!(retained.session.name, "retained live name");
        assert!(retained.session.shell_open);
        assert!(retained.session.archived);
        assert!(retained.session.archived_by_user);
        assert_eq!(
            retained.session.resume_id.as_deref(),
            Some("opaque/retained-before-poll")
        );
        let state_file = baude_core::workspace::active().state_file("state");
        assert_eq!(
            baude_core::persist::load_current_at(&state_root, &state_file)
                .unwrap()
                .state,
            app.repository_state
        );
        let path = retained.observed_path.to_path_buf();
        git(&repo, &["worktree", "remove", "--", path.to_str().unwrap()]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_close_local_obeys_persistence_commit_boundary() {
        for (label, failure, committed) in [
            (
                "close-precommit",
                baude_core::persist::AtomicFailure::Rename,
                false,
            ),
            (
                "close-postcommit",
                baude_core::persist::AtomicFailure::DirectorySync,
                true,
            ),
        ] {
            let repo = admission_repo(label);
            let root = repo.parent().unwrap().to_path_buf();
            let state_root = root.join("state");
            std::fs::create_dir_all(&state_root).unwrap();
            let mut app = App::new(repo.clone());
            app.remote = None;
            app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
            app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
            app.persistence_root_for_test = Some(state_root.clone());
            app.repository_state.next_repository_key =
                u64::from(std::process::id()) + if committed { 80_000 } else { 70_000 };
            let created = app
                .activate_branch_worktree(&repo, &format!("feature/{label}"))
                .unwrap();
            let (checkout, runtime) = match created {
                LifecycleOutcome::Created {
                    checkout,
                    runtime: Some(runtime),
                } => (checkout, runtime),
                other => panic!("unexpected activation outcome: {other:?}"),
            };
            app.session_mut(runtime).unwrap().meta.session_id = Some(format!("resume-{label}"));
            app.session_mut(runtime).unwrap().open_shell(5, 40).unwrap();
            let original_pid = app.session(runtime).unwrap().claude.pid().unwrap();
            let original_shell_pid = app
                .session(runtime)
                .unwrap()
                .shell
                .as_ref()
                .unwrap()
                .pid()
                .unwrap();
            assert!(pid_is_live(original_pid));
            assert!(pid_is_live(original_shell_pid));
            let before = app.repository_state.clone();
            app.atomic_failure_for_test = Some(failure);

            let close_error = app.close_retained_session(runtime).unwrap_err().to_string();
            assert_eq!(app.repository_state.checkouts.len(), 1);
            assert_eq!(app.repository_state.repositories.len(), 1);
            assert_eq!(app.repository_state.checkouts[0].active_intent, !committed);
            assert!(!pid_is_live(original_pid));
            assert!(!pid_is_live(original_shell_pid));
            if committed {
                assert!(app.sessions.is_empty());
                assert!(app.runtime_checkouts.is_empty());
                assert_eq!(
                    app.repository_state.checkouts[0]
                        .session
                        .resume_id
                        .as_deref(),
                    Some(format!("resume-{label}").as_str())
                );
                assert!(app.persistence_dirty);
            } else {
                assert!(close_error.contains(&format!("runtime {runtime} restarted")));
                assert_eq!(app.repository_state, before);
                let compensated = *app.runtime_checkouts.get(&checkout).unwrap();
                assert_eq!(compensated, runtime);
                assert!(!app.session(compensated).unwrap().claude.is_exited());
                assert!(pid_is_live(
                    app.session(compensated).unwrap().claude.pid().unwrap()
                ));
                let restored = app.session(compensated).unwrap();
                assert!(restored.shell_open);
                let restored_shell = restored.shell.as_ref().unwrap();
                assert!(!restored_shell.is_exited());
                assert!(pid_is_live(restored_shell.pid().unwrap()));
                assert_eq!(app.runtime_checkouts.get(&checkout), Some(&compensated));
                app.session_mut(compensated).unwrap().kill();
            }
            let path = app.repository_state.checkouts[0]
                .observed_path
                .to_path_buf();
            git(&repo, &["worktree", "remove", "--", path.to_str().unwrap()]);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn lifecycle_reopen_local_targets_retained_checkout_once_and_obeys_save_boundary() {
        let repo = admission_repo("retained-reopen");
        let root = repo.parent().unwrap().to_path_buf();
        let state_root = root.join("state");
        std::fs::create_dir_all(&state_root).unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
        app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
        app.persistence_root_for_test = Some(state_root);
        app.repository_state.next_repository_key = u64::from(std::process::id()) + 90_000;
        let created = app
            .activate_branch_worktree(&repo, "feature/retained-reopen")
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        app.session_mut(runtime).unwrap().meta.session_id = Some("opaque-local-target".into());
        app.close_retained_session(runtime).unwrap();
        let attempts_before = app.spawn_attempts_for_test;

        let reopened = app.reopen_checkout(checkout).unwrap();
        let reopened_runtime = match reopened {
            LifecycleOutcome::Reopened {
                checkout: key,
                runtime,
            } if key == checkout => runtime,
            other => panic!("unexpected reopen outcome: {other:?}"),
        };
        assert!(app.repository_state.checkouts[0].active_intent);
        assert_eq!(
            app.runtime_checkouts,
            HashMap::from([(checkout, reopened_runtime)])
        );
        assert_eq!(app.spawn_attempts_for_test, attempts_before + 1);
        assert_eq!(
            app.reopen_checkout(checkout).unwrap(),
            LifecycleOutcome::Focused {
                checkout,
                runtime: reopened_runtime,
            }
        );
        assert_eq!(app.spawn_attempts_for_test, attempts_before + 1);

        app.close_retained_session(reopened_runtime).unwrap();
        app.atomic_failure_for_test = Some(baude_core::persist::AtomicFailure::Rename);
        let attempts_before_failure = app.spawn_attempts_for_test;
        assert!(app.reopen_checkout(checkout).is_err());
        assert!(!app.repository_state.checkouts[0].active_intent);
        assert!(!app.runtime_checkouts.contains_key(&checkout));
        assert_eq!(app.spawn_attempts_for_test, attempts_before_failure);

        let path = app.repository_state.checkouts[0]
            .observed_path
            .to_path_buf();
        git(&repo, &["worktree", "remove", "--", path.to_str().unwrap()]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_remove_clean_local_rechecks_after_stop_and_compensates_a_race() {
        let repo = admission_repo("safe-remove-local");
        let root = repo.parent().unwrap().to_path_buf();
        let state_root = root.join("state");
        std::fs::create_dir_all(&state_root).unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
        app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
        app.persistence_root_for_test = Some(state_root.clone());
        app.repository_state.next_repository_key = u64::from(std::process::id()) + 100_000;
        let created = app
            .activate_branch_worktree(&repo, "feature/safe-remove-local")
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        let path = app.repository_state.checkouts[0]
            .observed_path
            .to_path_buf();
        let before = app.repository_state.clone();

        let confirmation = app.prepare_remove_worktree(checkout).unwrap();
        assert_eq!(app.repository_state, before);
        assert_eq!(app.runtime_checkouts, HashMap::from([(checkout, runtime)]));
        std::fs::write(path.join("agent-race"), b"unsaved\n").unwrap();

        let blocked = app
            .confirm_remove_worktree(confirmation)
            .unwrap_err()
            .to_string();
        assert!(blocked.contains("Untracked"), "got: {blocked}");
        assert_eq!(app.repository_state, before);
        assert_eq!(app.runtime_checkouts.len(), 1);
        assert_eq!(app.sessions.len(), 1);
        assert!(path.is_dir());

        std::fs::remove_file(path.join("agent-race")).unwrap();
        let confirmation = app.prepare_remove_worktree(checkout).unwrap();
        let removed = app.confirm_remove_worktree(confirmation).unwrap();
        assert!(matches!(
            removed,
            LifecycleOutcome::Removed {
                checkout: key,
                repository: _,
                branch_ref: _
            } if key == checkout
        ));
        assert!(app.repository_state.checkouts.is_empty());
        assert_eq!(app.repository_state.repositories, before.repositories);
        assert!(app.runtime_checkouts.is_empty());
        assert!(app.sessions.is_empty());
        assert!(!path.exists());
        assert!(Command::new("git")
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                "--",
                "refs/heads/feature/safe-remove-local"
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_remove_revocation_failure_retains_fresh_runtime_context() {
        for (label, failure, replacement_committed) in [
            (
                "safe-remove-save-pre",
                baude_core::persist::AtomicFailure::Rename,
                false,
            ),
            (
                "safe-remove-save-post",
                baude_core::persist::AtomicFailure::DirectorySync,
                true,
            ),
        ] {
            let repo = admission_repo(label);
            let root = repo.parent().unwrap().to_path_buf();
            let state_root = root.join("state");
            std::fs::create_dir_all(&state_root).unwrap();
            let mut app = App::new(repo.clone());
            app.remote = None;
            app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
            app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
            app.persistence_root_for_test = Some(state_root.clone());
            app.repository_state.next_repository_key = u64::from(std::process::id())
                + if replacement_committed {
                    130_000
                } else {
                    120_000
                };
            let created = app
                .activate_branch_worktree(&repo, &format!("feature/{label}"))
                .unwrap();
            let (checkout, runtime) = match created {
                LifecycleOutcome::Created {
                    checkout,
                    runtime: Some(runtime),
                } => (checkout, runtime),
                other => panic!("unexpected activation outcome: {other:?}"),
            };
            let resume_id = format!("fresh-{label}");
            app.session_mut(runtime).unwrap().meta.session_id = Some(resume_id.clone());
            let before = app.repository_state.clone();
            let path = before.checkouts[0].observed_path.to_path_buf();
            let confirmation = app.prepare_remove_worktree(checkout).unwrap();
            app.atomic_failure_for_test = Some(failure);

            let error = app.confirm_remove_worktree(confirmation).unwrap_err();
            assert!(error.to_string().contains("durably revoke"));
            assert!(path.exists());
            assert_eq!(app.runtime_checkouts.len(), 1);
            assert_eq!(app.sessions.len(), 1);
            assert_eq!(
                app.repository_state.checkouts[0]
                    .session
                    .resume_id
                    .as_deref(),
                Some(resume_id.as_str())
            );
            assert!(app.repository_state.checkouts[0].managed_by_baude);
            let state_file = baude_core::workspace::active().state_file("state");
            let persisted = baude_core::persist::load_current_at(&state_root, &state_file)
                .unwrap()
                .state;
            assert_eq!(
                persisted.checkouts[0].session.resume_id.as_deref(),
                Some(resume_id.as_str())
            );
            assert!(persisted.checkouts[0].managed_by_baude);
            assert_eq!(persisted.checkouts[0].health, CheckoutHealth::Available);
            assert!(app.prepare_remove_worktree(checkout).is_ok());
            for session in &mut app.sessions {
                session.kill();
            }
            git(
                &repo,
                &[
                    "worktree",
                    "remove",
                    "--force",
                    "--",
                    path.to_str().unwrap(),
                ],
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn lifecycle_remove_clean_local_stop_git_and_compensation_failures_preserve_context() {
        let (mut app, repo, root, checkout, runtime, path) =
            removal_app("safe-remove-stop-refusal", 150_000);
        let before = app.repository_state.clone();
        let confirmation = app.prepare_remove_worktree(checkout).unwrap();
        app.remove_stop_error_for_test = Some("teardown unavailable".into());

        let stop_error = app
            .confirm_remove_worktree(confirmation)
            .unwrap_err()
            .to_string();
        assert!(stop_error.contains("runtime stop failed"));
        assert_eq!(app.repository_state, before);
        assert_eq!(app.runtime_checkouts, HashMap::from([(checkout, runtime)]));

        app.remove_stop_error_for_test = None;
        app.remove_git_refusal_for_test = true;
        let confirmation = app.prepare_remove_worktree(checkout).unwrap();
        let refusal = app
            .confirm_remove_worktree(confirmation)
            .unwrap_err()
            .to_string();
        assert!(
            refusal.contains("plain Git removal refused"),
            "got: {refusal}"
        );
        assert_eq!(app.repository_state, before);
        assert_eq!(app.runtime_checkouts.len(), 1);
        assert_eq!(app.sessions.len(), 1);
        assert!(path.join("agent-race-after-second").is_file());

        let compensated_runtime = *app.runtime_checkouts.get(&checkout).unwrap();
        app.spawn_error_for_test = Some("resume unavailable".into());
        let confirmation = app.prepare_remove_worktree(checkout).unwrap_err();
        assert!(confirmation.to_string().contains("Untracked"));
        app.session_mut(compensated_runtime).unwrap().kill();
        std::fs::remove_file(path.join("agent-race-after-second")).unwrap();
        git(&repo, &["worktree", "remove", "--", path.to_str().unwrap()]);
        std::fs::remove_dir_all(root).unwrap();

        let (mut app, repo, root, checkout, _, path) =
            removal_app("safe-remove-compensation-failure", 160_000);
        let confirmation = app.prepare_remove_worktree(checkout).unwrap();
        app.remove_git_refusal_for_test = true;
        app.spawn_error_for_test = Some("resume unavailable".into());

        let error = app
            .confirm_remove_worktree(confirmation)
            .unwrap_err()
            .to_string();
        assert!(error.contains("compensation also failed"), "got: {error}");
        assert_eq!(app.repository_state.checkouts.len(), 1);
        assert!(app.repository_state.checkouts[0].active_intent);
        assert!(app.runtime_checkouts.is_empty());
        assert!(app.sessions.is_empty());
        std::fs::remove_file(path.join("agent-race-after-second")).unwrap();
        git(&repo, &["worktree", "remove", "--", path.to_str().unwrap()]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_remove_local_partial_teardown_is_durable_and_retryable() {
        for (label, fail_agent, offset) in [
            ("remove-agent-partial", true, 170_000),
            ("remove-shell-partial", false, 180_000),
        ] {
            let (mut app, repo, root, checkout, runtime, path) = removal_app(label, offset);
            app.session_mut(runtime).unwrap().open_shell(5, 40).unwrap();
            if fail_agent {
                app.session(runtime)
                    .unwrap()
                    .claude
                    .fail_next_teardown_for_test("agent stop refused once");
            } else {
                app.session(runtime)
                    .unwrap()
                    .shell
                    .as_ref()
                    .unwrap()
                    .fail_next_teardown_for_test("shell stop refused once");
            }
            let confirmation = app.prepare_remove_worktree(checkout).unwrap();

            let error = app
                .confirm_remove_worktree(confirmation.clone())
                .unwrap_err()
                .to_string();
            assert!(error.contains("runtime stop failed"), "got: {error}");
            assert!(path.exists());
            assert!(app.runtime_checkouts.contains_key(&checkout));
            assert!(matches!(
                app.repository_state.checkouts[0].health,
                CheckoutHealth::Unavailable(UnavailableCause::TeardownPending { .. })
            ));

            assert!(matches!(
                app.confirm_remove_worktree(confirmation).unwrap(),
                LifecycleOutcome::Removed { checkout: key, .. } if key == checkout
            ));
            assert!(!path.exists());
            assert!(app.sessions.is_empty());
            assert!(app.runtime_checkouts.is_empty());
            std::fs::remove_dir_all(root).unwrap();
            drop(repo);
        }
    }

    #[test]
    fn remove_confirmation_is_distinct_targeted_and_cancel_is_non_mutating() {
        let repo = admission_repo("remove-confirmation");
        let root = repo.parent().unwrap().to_path_buf();
        let state_root = root.join("state");
        std::fs::create_dir_all(&state_root).unwrap();
        let mut app = App::new(repo.clone());
        app.remote = None;
        app.config.claude_cmd = Some("sh -c 'sleep 30'".into());
        app.config.opencode_cmd = Some("sh -c 'sleep 30'".into());
        app.persistence_root_for_test = Some(state_root);
        app.repository_state.next_repository_key = u64::from(std::process::id()) + 140_000;
        let created = app
            .activate_branch_worktree(&repo, "feature/remove-confirmation")
            .unwrap();
        let (checkout, runtime) = match created {
            LifecycleOutcome::Created {
                checkout,
                runtime: Some(runtime),
            } => (checkout, runtime),
            other => panic!("unexpected activation outcome: {other:?}"),
        };
        let before = app.repository_state.clone();
        app.modal = Modal::ConfirmCloseWorktree { id: runtime };

        app.handle_modal_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

        let target_path = before.checkouts[0].observed_path.to_path_buf();
        assert!(matches!(
            &app.modal,
            Modal::ConfirmRemoveWorktree { confirmation }
                if confirmation.checkout() == checkout
                    && confirmation.path() == target_path
                    && confirmation.branch_ref() == "refs/heads/feature/remove-confirmation"
        ));
        assert_eq!(app.repository_state, before);
        assert_eq!(app.runtime_checkouts, HashMap::from([(checkout, runtime)]));

        app.handle_modal_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.modal, Modal::None));
        assert_eq!(app.repository_state, before);
        assert_eq!(app.runtime_checkouts, HashMap::from([(checkout, runtime)]));

        std::fs::write(target_path.join("blocked"), b"keep\n").unwrap();
        app.modal = Modal::ConfirmCloseWorktree { id: runtime };
        app.handle_modal_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(matches!(app.modal, Modal::None));
        assert!(app.message.as_ref().unwrap().0.contains("Untracked"));
        assert_eq!(app.repository_state, before);
        assert_eq!(app.runtime_checkouts, HashMap::from([(checkout, runtime)]));
        app.session_mut(runtime).unwrap().kill();
        std::fs::remove_file(target_path.join("blocked")).unwrap();
        git(
            &repo,
            &["worktree", "remove", "--", target_path.to_str().unwrap()],
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
