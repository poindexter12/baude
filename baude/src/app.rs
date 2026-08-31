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
    CheckoutHealth, CheckoutKey, CheckoutLifecycle, CheckoutRole, OwnedRuntime, PersistedPath,
    RepositoryHealth, RepositoryKey, RepositoryState, RetainedSessionState, RuntimeGeneration,
    SavedCheckout, SavedRepository, ShellOwnership, UnavailableCause,
};
use baude_core::session::{Session, Status};

use crate::hierarchy::{
    self, ActionKind, ActionView, CheckoutDecoration, LocalRow, LocalRowId, SelectionTarget,
};
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
        .filter(|checkout| {
            matches!(
                checkout.lifecycle(),
                CheckoutLifecycle::Active | CheckoutLifecycle::Launching(_)
            ) || (checkout.active_intent() && checkout.health() == &CheckoutHealth::Available)
        })
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

/// Sidebar selection uses durable identity for local topology and runtime
/// identity only for the separate flat remote compatibility section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelId {
    Repository(RepositoryKey),
    Checkout(CheckoutKey),
    Remote(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SidebarRefusal {
    RepositoryClose,
    RepositoryRemove,
    RepositoryShell,
    RepositoryArchive,
    AlreadyClosed,
    MainRemove,
    UnmanagedRemove,
    NoLiveRuntime,
    UnavailableReopen,
    UnavailableBranch,
    UnavailableClose,
    UnavailableRemove,
    UnavailableArchive,
    RecoveryReopen,
    RecoveryBranch,
    RecoveryClose,
    RecoveryRemove,
    RetryNotAuthorized,
    RemoteShell,
    RemoteEditor,
    RemoteGsd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveryKind {
    Activation,
    Teardown,
    Removal,
    StoppedActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SidebarAction {
    None,
    Open,
    Branch,
    Close,
    RetryReopen,
    RetryRecovery,
    Remove,
    Shell,
    Editor,
    Info,
    Activity,
    Gsd,
    Archive,
    RemoteOpen,
    RemoteClose,
    RemoteRestart,
    RemoteArchive,
    Refuse(SidebarRefusal),
}

fn sidebar_action(view: ActionView, key: KeyEvent) -> SidebarAction {
    let remove_key = matches!(key.code, KeyCode::Char('X'))
        || matches!(key.code, KeyCode::Char('x')) && key.modifiers.contains(KeyModifiers::SHIFT);
    if remove_key {
        return match view.kind {
            ActionKind::Repository => SidebarAction::Refuse(SidebarRefusal::RepositoryRemove),
            ActionKind::Main => SidebarAction::Refuse(SidebarRefusal::MainRemove),
            ActionKind::Managed if view.can_remove => SidebarAction::Remove,
            ActionKind::External => SidebarAction::Refuse(SidebarRefusal::UnmanagedRemove),
            ActionKind::Unavailable
                if view.capability == Some(lifecycle::LifecycleCapability::RetryRecovery) =>
            {
                SidebarAction::Refuse(SidebarRefusal::RecoveryRemove)
            }
            ActionKind::Unavailable => SidebarAction::Refuse(SidebarRefusal::UnavailableRemove),
            ActionKind::Remote => SidebarAction::None,
            ActionKind::Managed => SidebarAction::Refuse(SidebarRefusal::UnmanagedRemove),
        };
    }
    match key.code {
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => match view.kind {
            ActionKind::Remote => SidebarAction::RemoteOpen,
            ActionKind::Unavailable
                if view.capability == Some(lifecycle::LifecycleCapability::RetryRecovery) =>
            {
                SidebarAction::Refuse(SidebarRefusal::RecoveryReopen)
            }
            ActionKind::Unavailable => SidebarAction::Refuse(SidebarRefusal::UnavailableReopen),
            _ => SidebarAction::Open,
        },
        KeyCode::Char('w') => match view.kind {
            ActionKind::Remote => SidebarAction::None,
            ActionKind::Unavailable
                if view.capability == Some(lifecycle::LifecycleCapability::RetryRecovery) =>
            {
                SidebarAction::Refuse(SidebarRefusal::RecoveryBranch)
            }
            ActionKind::Unavailable => SidebarAction::Refuse(SidebarRefusal::UnavailableBranch),
            _ => SidebarAction::Branch,
        },
        KeyCode::Char('x') => match view.kind {
            ActionKind::Remote => SidebarAction::RemoteClose,
            ActionKind::Repository => SidebarAction::Refuse(SidebarRefusal::RepositoryClose),
            ActionKind::Unavailable
                if view.capability == Some(lifecycle::LifecycleCapability::RetryRecovery) =>
            {
                SidebarAction::Refuse(SidebarRefusal::RecoveryClose)
            }
            ActionKind::Unavailable => SidebarAction::Refuse(SidebarRefusal::UnavailableClose),
            _ if view.can_close => SidebarAction::Close,
            _ => SidebarAction::Refuse(SidebarRefusal::AlreadyClosed),
        },
        KeyCode::Char('r') => match view.kind {
            ActionKind::Remote => SidebarAction::RemoteRestart,
            _ => match view.capability {
                Some(lifecycle::LifecycleCapability::RetryReopen) => SidebarAction::RetryReopen,
                Some(lifecycle::LifecycleCapability::RetryRecovery) => SidebarAction::RetryRecovery,
                None => SidebarAction::Refuse(SidebarRefusal::RetryNotAuthorized),
            },
        },
        KeyCode::Char('t') => match view.kind {
            ActionKind::Remote => SidebarAction::Refuse(SidebarRefusal::RemoteShell),
            ActionKind::Repository => SidebarAction::Refuse(SidebarRefusal::RepositoryShell),
            _ if view.has_runtime => SidebarAction::Shell,
            _ => SidebarAction::Refuse(SidebarRefusal::NoLiveRuntime),
        },
        KeyCode::Char('e') => match view.kind {
            ActionKind::Remote => SidebarAction::Refuse(SidebarRefusal::RemoteEditor),
            _ => SidebarAction::Editor,
        },
        KeyCode::Char('i') => SidebarAction::Info,
        KeyCode::Char('v') => match view.kind {
            ActionKind::Repository => SidebarAction::None,
            _ => SidebarAction::Activity,
        },
        KeyCode::Char('g') => match view.kind {
            ActionKind::Remote => SidebarAction::Refuse(SidebarRefusal::RemoteGsd),
            _ => SidebarAction::Gsd,
        },
        KeyCode::Char('a') => match view.kind {
            ActionKind::Remote => SidebarAction::RemoteArchive,
            ActionKind::Repository => SidebarAction::Refuse(SidebarRefusal::RepositoryArchive),
            ActionKind::Unavailable => SidebarAction::Refuse(SidebarRefusal::UnavailableArchive),
            _ => SidebarAction::Archive,
        },
        _ => SidebarAction::None,
    }
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
    if shell_open && content.height >= 12 {
        let shell_h = (content.height * 30 / 100)
            .max(4)
            .min(content.height.saturating_sub(4));
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

struct AppLifecycleEffects<'a, F> {
    app: &'a mut App,
    effect: Option<F>,
}

impl<F> lifecycle::LifecycleEffects for AppLifecycleEffects<'_, F>
where
    F: FnOnce(&mut App, &lifecycle::LifecycleTraceEntry) -> Result<()>,
{
    type Error = anyhow::Error;

    fn persist_lifecycle(&mut self, candidate: &lifecycle::LifecycleCandidate) -> Result<()> {
        let before = self.app.repository_state.clone();
        candidate.apply(&mut self.app.repository_state)?;
        if let Err(error) = self.app.save_durable_status() {
            self.app.persistence_dirty = true;
            if error.replacement_committed() {
                return Ok(());
            } else {
                self.app.repository_state = before;
            }
            return Err(anyhow::Error::new(error));
        }
        self.app.persistence_dirty = false;
        Ok(())
    }

    fn apply_lifecycle_effect(&mut self, effect: &lifecycle::LifecycleTraceEntry) -> Result<()> {
        self.effect
            .take()
            .ok_or_else(|| anyhow::anyhow!("lifecycle effect invoked more than once"))?(
            self.app, effect,
        )
    }
}

impl App {
    fn drive_lifecycle_effect<F>(
        &mut self,
        checkout: CheckoutKey,
        event: lifecycle::LifecycleEvent,
        effect: F,
    ) -> Result<lifecycle::LifecycleTransition>
    where
        F: FnOnce(&mut App, &lifecycle::LifecycleTraceEntry) -> Result<()>,
    {
        let mut saved = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("checkout {} is missing", checkout.get()))?;
        let adapter = AppLifecycleEffects {
            app: self,
            effect: Some(effect),
        };
        let mut engine = lifecycle::LifecycleEngine::new(adapter);
        let transition = engine.drive(&mut saved, event)?;
        if transition.effects.is_empty() {
            anyhow::bail!(
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
            PersistedPath::from_path(Path::new("/app-contract")),
            Some("refs/heads/contract".into()),
            1,
            initial,
            RetainedSessionState {
                name: "app-contract".into(),
                cwd: PersistedPath::from_path(Path::new("/app-contract")),
                repo_root: PersistedPath::from_path(Path::new("/app-contract")),
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

    pub fn hierarchy_rows(&self) -> Vec<LocalRow> {
        let decorations = self
            .repository_state
            .checkouts
            .iter()
            .filter_map(|checkout| {
                let runtime_id = self.runtime_checkouts.get(&checkout.key).copied()?;
                let session = self.session(runtime_id)?;
                Some((
                    checkout.key,
                    CheckoutDecoration {
                        runtime_id: Some(runtime_id),
                        status: Some(session.status()),
                        waiting_for_ms: session.waiting_for_ms(),
                        archived: session.archived,
                    },
                ))
            })
            .collect();
        hierarchy::project_local(&self.repository_state, &decorations)
    }

    pub(crate) fn selected_action_view(&self) -> Option<ActionView> {
        match self.selected_id? {
            SelId::Repository(key) => self.hierarchy_rows().into_iter().find_map(|row| match row {
                LocalRow::Repository(parent) if parent.key == key => Some(parent.actions),
                _ => None,
            }),
            SelId::Checkout(key) => self.hierarchy_rows().into_iter().find_map(|row| match row {
                LocalRow::Checkout(checkout) if checkout.key == key => Some(checkout.actions),
                _ => None,
            }),
            SelId::Remote(_) => Some(hierarchy::action_view(
                hierarchy::ActionSelection::Remote,
                true,
                None,
            )),
        }
    }

    fn selected_repository(&self) -> Option<&SavedRepository> {
        let key = match self.selected_id? {
            SelId::Repository(key) => key,
            SelId::Checkout(key) => {
                self.repository_state
                    .checkouts
                    .iter()
                    .find(|checkout| checkout.key == key)?
                    .repository_key
            }
            SelId::Remote(_) => return None,
        };
        self.repository_state
            .repositories
            .iter()
            .find(|repository| repository.key == key)
    }

    fn selected_checkout(&self) -> Option<&SavedCheckout> {
        let SelId::Checkout(key) = self.selected_id? else {
            return None;
        };
        self.repository_state
            .checkouts
            .iter()
            .find(|checkout| checkout.key == key)
    }

    fn repository_label(repository: &SavedRepository) -> String {
        repository
            .observed_main_worktree
            .to_path_buf()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                repository
                    .observed_main_worktree
                    .to_path_buf()
                    .display()
                    .to_string()
            })
    }

    fn selected_target_label(&self) -> String {
        self.selected_checkout()
            .map(|checkout| checkout.session.name.clone())
            .or_else(|| self.selected_repository().map(Self::repository_label))
            .or_else(|| self.selected_remote().map(|remote| remote.name.clone()))
            .unwrap_or_else(|| "selected target".into())
    }

    fn selected_unavailable_cause(&self) -> Option<&UnavailableCause> {
        let checkout = self.selected_checkout()?;
        match checkout.lifecycle() {
            CheckoutLifecycle::Protected(cause) => Some(cause),
            CheckoutLifecycle::RemovalCommitted => None,
            _ => match checkout.health() {
                CheckoutHealth::Unavailable(cause) => Some(cause),
                CheckoutHealth::Available => None,
            },
        }
    }

    fn selected_recovery_kind(&self) -> Option<RecoveryKind> {
        match self.selected_checkout()?.lifecycle() {
            CheckoutLifecycle::Activating
            | CheckoutLifecycle::Protected(
                UnavailableCause::PendingActivation { .. }
                | UnavailableCause::ActivationRecovery { .. },
            ) => Some(RecoveryKind::Activation),
            CheckoutLifecycle::Stopping(_)
            | CheckoutLifecycle::Protected(UnavailableCause::TeardownPending { .. }) => {
                Some(RecoveryKind::Teardown)
            }
            CheckoutLifecycle::RemovalCommitted
            | CheckoutLifecycle::Protected(UnavailableCause::RemovalTombstone(_)) => {
                Some(RecoveryKind::Removal)
            }
            CheckoutLifecycle::Protected(UnavailableCause::StoppedActiveRecovery { .. }) => {
                Some(RecoveryKind::StoppedActive)
            }
            _ => None,
        }
    }

    fn unavailable_cause_label(cause: Option<&UnavailableCause>) -> &'static str {
        match cause {
            Some(UnavailableCause::Missing) => "missing",
            Some(UnavailableCause::NotRepository) => "not a repository",
            Some(UnavailableCause::IdentityChanged) => "identity changed",
            Some(UnavailableCause::RemovalTombstone(_)) => "protected removal state",
            Some(UnavailableCause::PendingActivation { .. })
            | Some(UnavailableCause::ActivationRecovery { .. }) => "activation recovery",
            Some(UnavailableCause::TeardownPending { .. }) => "teardown recovery",
            Some(UnavailableCause::StoppedActiveRecovery { .. }) => "runtime ownership recovery",
            Some(UnavailableCause::Io(_)) => "I/O failure",
            Some(UnavailableCause::Other(_)) | None => "indeterminate topology",
        }
    }

    fn refuse_sidebar_action(&mut self, refusal: SidebarRefusal) {
        let target = self.selected_target_label();
        let repository = self
            .selected_repository()
            .map(Self::repository_label)
            .unwrap_or_else(|| target.clone());
        let checkout = self.selected_checkout();
        let worktree = checkout.is_some_and(|saved| saved.role != CheckoutRole::Main);
        let path = checkout
            .map(|saved| saved.observed_path.to_path_buf())
            .unwrap_or_default();
        let cause = Self::unavailable_cause_label(self.selected_unavailable_cause());
        let recovery = self.selected_recovery_kind();
        let message = match refusal {
            SidebarRefusal::RepositoryClose => format!(
                "Cannot close “{repository}”: a repository parent is not a session. Select a running checkout, then press x."
            ),
            SidebarRefusal::RepositoryRemove => format!(
                "Cannot remove “{repository}”: repository parents are never removed by this action. Select a baude-managed worktree, then press X."
            ),
            SidebarRefusal::RepositoryShell => format!(
                "Cannot open a shell for “{repository}”: a repository parent has no checkout directory or live session. Select a live checkout child under “{repository}”, then press t."
            ),
            SidebarRefusal::RepositoryArchive => format!(
                "Cannot archive “{repository}”: a repository parent is a durable container, not checkout session state. Select a checkout child under “{repository}” with applicable session state, then press a."
            ),
            SidebarRefusal::AlreadyClosed => format!(
                "Cannot close “{target}”: its session is already closed and the {} is kept. Press enter to reopen it.",
                if worktree { "worktree" } else { "checkout" }
            ),
            SidebarRefusal::MainRemove => format!(
                "Cannot remove “{target}”: the main checkout is never removable from baude. Keep it in Git and select a baude-managed linked worktree if removal is intended."
            ),
            SidebarRefusal::UnmanagedRemove => format!(
                "Cannot remove “{target}”: it is not a baude-managed linked worktree. Keep it unchanged or remove it manually with Git if intended; nothing was removed."
            ),
            SidebarRefusal::NoLiveRuntime => format!(
                "Cannot open a shell for “{target}”: no live local runtime is associated with this checkout. Press enter to reopen it first."
            ),
            SidebarRefusal::UnavailableBranch if recovery == Some(RecoveryKind::Removal) => format!(
                "Cannot create or activate a branch in “{repository}”: “{target}” has protected removal state. Open details with i and let lifecycle recovery reconcile the committed Git facts first."
            ),
            SidebarRefusal::UnavailableBranch => format!(
                "Cannot create or activate a branch in “{repository}”: checkout “{target}” no longer matches the recorded Git topology ({cause}). Repair or restore the checkout at “{}”, open details with i, then use only the authorized action shown there.",
                path.display()
            ),
            SidebarRefusal::UnavailableClose if recovery == Some(RecoveryKind::Removal) => format!(
                "Cannot close “{target}”: protected removal recovery owns this checkout. Open details with i; no process or persisted child was changed."
            ),
            SidebarRefusal::UnavailableClose => format!(
                "Cannot close “{target}”: its recorded runtime/topology state is unavailable ({cause}). Open details with i and repair the checkout; no session or checkout was changed."
            ),
            SidebarRefusal::UnavailableReopen if recovery == Some(RecoveryKind::Removal) => format!(
                "Cannot reopen “{target}”: protected removal recovery may represent already-committed Git topology. Open details with i; baude will not recreate or launch it."
            ),
            SidebarRefusal::UnavailableReopen => format!(
                "Cannot reopen “{target}”: Git topology is unavailable ({cause}) and this state is not retryable from the TUI. Open details with i and repair the checkout; no runtime was started."
            ),
            SidebarRefusal::UnavailableRemove if recovery == Some(RecoveryKind::Removal) => format!(
                "Cannot start another removal for “{target}”: protected removal recovery is already in progress. Open details with i; nothing else was removed."
            ),
            SidebarRefusal::UnavailableRemove => format!(
                "Cannot remove “{target}”: its Git topology is unavailable ({cause}), so safe removal cannot be proven. Repair or reconcile it with Git, then press X for a fresh preflight; nothing was removed."
            ),
            SidebarRefusal::RecoveryBranch => match recovery {
                Some(RecoveryKind::Activation) => format!("Cannot create or activate a branch in “{repository}”: “{target}” has an unfinished activation. Open details with i and complete the lifecycle-authorized recovery before starting another branch action."),
                Some(RecoveryKind::Teardown) => format!("Cannot create or activate a branch in “{repository}”: “{target}” still has teardown ownership to resolve. Open details with i and complete the authorized teardown recovery first."),
                _ => format!("Cannot create or activate a branch in “{repository}”: “{target}” has unresolved runtime ownership or unsaved lifecycle state. Repair persistence, open details with i, and complete the authorized recovery first."),
            },
            SidebarRefusal::RecoveryClose => match recovery {
                Some(RecoveryKind::Activation) => format!("Cannot close “{target}”: activation recovery must finish before close is legal. Open details with i; no runtime or checkout was changed."),
                Some(RecoveryKind::Teardown) => format!("Cannot start a new close for “{target}”: teardown is already pending. Press r to continue the authorized teardown recovery."),
                _ => format!("Cannot close “{target}”: runtime ownership recovery is unresolved. Repair persistence and open details with i; no process ownership was discarded."),
            },
            SidebarRefusal::RecoveryReopen => match recovery {
                Some(RecoveryKind::Activation) => format!("Cannot reopen “{target}” until activation recovery completes. Press r to continue the authorized recovery."),
                Some(RecoveryKind::Teardown) => format!("Cannot reopen “{target}”: teardown recovery must reach a stable closed state first. Open details with i and use r only if retry is shown."),
                _ => format!("Cannot reopen “{target}” as a new runtime while ownership recovery is unresolved. Repair persistence, then press r to continue the authorized recovery."),
            },
            SidebarRefusal::RecoveryRemove => match recovery {
                Some(RecoveryKind::Activation) => format!("Cannot remove “{target}”: activation recovery must finish before removal can be inspected. Open details with i; nothing was removed."),
                Some(RecoveryKind::Teardown) => format!("Cannot remove “{target}”: teardown recovery must complete before a fresh removal preflight. Open details with i; nothing was removed."),
                _ => format!("Cannot remove “{target}”: runtime ownership or persistence recovery is unresolved. Repair persistence and complete the authorized recovery before pressing X; nothing was removed."),
            },
            SidebarRefusal::RetryNotAuthorized => format!(
                "Cannot retry “{target}”: this state has no lifecycle-authorized manual retry. Open details with i; no runtime or checkout was changed."
            ),
            SidebarRefusal::UnavailableArchive => format!(
                "Cannot archive “{target}”: lifecycle or topology recovery is unresolved. Open details with i; no checkout state was changed."
            ),
            SidebarRefusal::RemoteShell => "no shell pane for remote sessions".into(),
            SidebarRefusal::RemoteEditor => {
                "remote session — folder lives on the daemon host".into()
            }
            SidebarRefusal::RemoteGsd => "GSD view is for local sessions".into(),
        };
        self.set_message(message);
    }

    fn selection_target(&self) -> Option<SelectionTarget> {
        self.selected_id.map(|selected| match selected {
            SelId::Repository(key) => SelectionTarget::Local(LocalRowId::Repository(key)),
            SelId::Checkout(key) => SelectionTarget::Local(LocalRowId::Checkout(key)),
            SelId::Remote(id) => SelectionTarget::Remote(id),
        })
    }

    fn set_selection_target(&mut self, selected: Option<SelectionTarget>) {
        self.selected_id = selected.map(|selected| match selected {
            SelectionTarget::Local(LocalRowId::Repository(key)) => SelId::Repository(key),
            SelectionTarget::Local(LocalRowId::Checkout(key)) => SelId::Checkout(key),
            SelectionTarget::Remote(id) => SelId::Remote(id),
        });
    }

    #[cfg(test)]
    pub(crate) fn install_hierarchy_state_for_test(
        &mut self,
        state: RepositoryState,
        runtime_checkouts: HashMap<CheckoutKey, u64>,
    ) {
        self.repository_state = state;
        self.runtime_checkouts = runtime_checkouts;
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
        if let Err(error) = self.reconcile_teardown_recoveries() {
            self.set_message(format!("teardown recovery: {error}"));
        }
        if let Err(error) = self.reconcile_activation_recoveries() {
            self.set_message(format!("activation recovery: {error}"));
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
        if let Some(index) = self
            .repository_state
            .checkouts
            .iter_mut()
            .position(|checkout| checkout.key == checkout_key)
        {
            let checkout = &mut self.repository_state.checkouts[index];
            require_same_checkout_path(checkout, &record.path)?;
            checkout.observed_branch = Some(default.local_ref.clone());
            checkout.session.cwd = PersistedPath::from_path(&record.path);
            checkout.session.branch = Some(default.local_branch.clone());
            checkout.session.is_worktree = is_worktree;
            lifecycle::mark_checkout_active(&mut self.repository_state, checkout_key)?;
        } else {
            let first_seen_order = self.repository_state.allocate_first_seen_order()?;
            let name = snapshot
                .main_worktree
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| snapshot.main_worktree.display().to_string());
            self.repository_state.checkouts.push(SavedCheckout::new(
                checkout_key,
                repository_key,
                CheckoutRole::PrimaryDefault,
                managed_by_baude,
                PersistedPath::from_path(&record.path),
                Some(default.local_ref.clone()),
                first_seen_order,
                CheckoutLifecycle::Active,
                RetainedSessionState {
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
            ));
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
                self.repository_state = state_before.clone();
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
                    drop(_reservation);
                    return self.reopen_checkout(activation.checkout);
                }
                self.selected_id = Some(SelId::Checkout(activation.checkout));
                self.focus = Focus::Claude;
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
            .add_retained_session_with_mode(
                activation.checkout,
                activation.path.clone(),
                Some(activation.main_worktree.clone()),
                Some(activation.branch.clone()),
                activation.path != activation.main_worktree,
                backend::SpawnMode::Fresh,
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
        self.selected_id = Some(SelId::Checkout(activation.checkout));
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
                self.selected_id = Some(SelId::Checkout(checkout_key));
                self.focus = Focus::Claude;
                Ok(LifecycleOutcome::Focused {
                    checkout: checkout_key,
                    runtime: id,
                })
            }
            lifecycle::ReopenDispatch::Restart { id } => {
                self.restart_session_with_mode(id, plan.mode)?;
                self.selected_id = Some(SelId::Checkout(checkout_key));
                Ok(LifecycleOutcome::Reopened {
                    checkout: checkout_key,
                    runtime: id,
                })
            }
            lifecycle::ReopenDispatch::Spawn => {
                let cwd = checkout.observed_path.to_path_buf();
                let session = checkout.session;
                let id = self.add_retained_session_with_mode(
                    checkout_key,
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
                self.selected_id = Some(SelId::Checkout(checkout_key));
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
                let _ = lifecycle::record_checkout_reconciliation(
                    &mut self.repository_state,
                    checkout_key,
                    Some(cause.clone()),
                );
                self.repository_state.repositories[repository_index].health =
                    RepositoryHealth::Unavailable(cause);
                return false;
            }
        };
        let _ = lifecycle::record_checkout_reconciliation(
            &mut self.repository_state,
            checkout_key,
            None,
        );
        self.repository_state.repositories[repository_index].health = RepositoryHealth::Available;
        true
    }

    // ---- session bookkeeping ----

    /// Selection order: durable local parents and children in structural
    /// hierarchy order, followed by the existing flat remote compatibility
    /// order. Volatile local status never participates in ordering.
    pub fn ordered_ids(&self) -> Vec<SelId> {
        let mut active_remote: Vec<(String, SelId)> = Vec::new();
        let mut archived_remote: Vec<(String, SelId)> = Vec::new();
        for r in &self.remote_snap.sessions {
            let entry = (r.name.to_lowercase(), SelId::Remote(r.id));
            if r.archived {
                archived_remote.push(entry);
            } else {
                active_remote.push(entry);
            }
        }
        for group in [&mut active_remote, &mut archived_remote] {
            group.sort_by(|a, b| a.0.cmp(&b.0));
        }
        self.hierarchy_rows()
            .into_iter()
            .map(|row| match row.id() {
                LocalRowId::Repository(key) => SelId::Repository(key),
                LocalRowId::Checkout(key) => SelId::Checkout(key),
            })
            .chain(active_remote.into_iter().map(|(_, id)| id))
            .chain(archived_remote.into_iter().map(|(_, id)| id))
            .collect()
    }

    pub fn is_archived(&self, id: SelId) -> bool {
        match id {
            SelId::Repository(_) => false,
            SelId::Checkout(key) => self
                .runtime_checkouts
                .get(&key)
                .and_then(|id| self.session(*id))
                .map(|session| session.archived)
                .or_else(|| {
                    self.repository_state
                        .checkouts
                        .iter()
                        .find(|checkout| checkout.key == key)
                        .map(|checkout| checkout.session.archived)
                })
                .unwrap_or(false),
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
            Some(SelId::Checkout(key)) => self
                .runtime_checkouts
                .get(&key)
                .and_then(|id| self.session(*id)),
            _ => None,
        }
    }

    fn selected_mut(&mut self) -> Option<&mut Session> {
        match self.selected_id {
            Some(SelId::Checkout(key)) => {
                let id = self.runtime_checkouts.get(&key).copied()?;
                self.sessions.iter_mut().find(|session| session.id == id)
            }
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

    #[allow(clippy::too_many_arguments)]
    fn add_retained_session_with_mode(
        &mut self,
        checkout: CheckoutKey,
        cwd: PathBuf,
        repo_root: Option<PathBuf>,
        branch: Option<String>,
        is_worktree: bool,
        mode: backend::SpawnMode,
        shell_open: bool,
    ) -> Result<u64> {
        self.add_session_with_mode_internal(
            Some(checkout),
            cwd,
            repo_root,
            branch,
            is_worktree,
            mode,
            shell_open,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn add_session_with_mode_internal(
        &mut self,
        checkout: Option<CheckoutKey>,
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
        let (_, shell_rect) = pane_rects(self.content_rect, shell_open);
        let shell_size = shell_rect.map(inner).map(|rect| (rect.height, rect.width));
        let generation = checkout
            .and_then(|key| {
                self.repository_state
                    .checkouts
                    .iter()
                    .find(|saved| saved.key == key)
                    .and_then(SavedCheckout::owned_runtime)
                    .and_then(|runtime| runtime.generation.successor())
            })
            .unwrap_or(RuntimeGeneration::initial());
        let mut registered_shell = None;
        let mut claude = if let Some(checkout) = checkout {
            Pty::spawn_registered_with(Some(&plan.cmd), &plan.env, &cwd, rows, cols, |agent| {
                if let Some((shell_rows, shell_cols)) = shell_size {
                    let shell = Pty::spawn_registered_with(
                        None,
                        &[],
                        &cwd,
                        shell_rows,
                        shell_cols,
                        |identity| {
                            let runtime = OwnedRuntime {
                                generation,
                                agent: agent.clone(),
                                shell: ShellOwnership::Owned(identity.clone()),
                            };
                            self.drive_lifecycle_effect(
                                checkout,
                                lifecycle::LifecycleEvent::LaunchRegistered(runtime),
                                |_, _| Ok(()),
                            )?;
                            Ok(())
                        },
                    )?;
                    registered_shell = Some(shell);
                } else {
                    let runtime = OwnedRuntime {
                        generation,
                        agent: agent.clone(),
                        shell: ShellOwnership::Closed,
                    };
                    self.drive_lifecycle_effect(
                        checkout,
                        lifecycle::LifecycleEvent::LaunchRegistered(runtime),
                        |_, _| Ok(()),
                    )?;
                }
                Ok(())
            })?
        } else {
            Pty::spawn_with_env(Some(&plan.cmd), &plan.env, &cwd, rows, cols)?
        };
        if let Some(checkout) = checkout {
            if let Err(error) = self.drive_lifecycle_effect(
                checkout,
                lifecycle::LifecycleEvent::LaunchReleased,
                |_, _| Ok(()),
            ) {
                let _ = claude.kill_and_wait();
                if let Some(shell) = &mut registered_shell {
                    let _ = shell.kill_and_wait();
                }
                return Err(error);
            }
        }
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
            shell: registered_shell,
            shell_open: shell_size.is_some(),
            spawn_unix_ms: now_unix_ms(),
            meta,
            archived: false,
            archived_by_user: false,
            was_busy: false,
            unarchived_at_ms: None,
            pending_permission: None,
            permission_decision: None,
        };
        if shell_open && checkout.is_none() {
            let (_, shell_rect) = pane_rects(self.content_rect, true);
            if let Some(sr) = shell_rect {
                let r = inner(sr);
                let _ = session.open_shell(r.height, r.width);
            }
        }
        self.sessions.push(session);
        if let Some(checkout) = checkout {
            self.selected_id = Some(SelId::Checkout(checkout));
        }
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
        // Retained close removes only the runtime decoration; the durable
        // checkout remains selected and visible in the hierarchy.
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
            self.selected_id = Some(SelId::Checkout(checkout));
            self.focus = Focus::Claude;
            return Ok(id);
        }
        let lifecycle = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .map(|saved| saved.lifecycle().clone())
            .ok_or_else(|| anyhow::anyhow!("checkout {} is missing", checkout.get()))?;
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
            other => anyhow::bail!("checkout cannot restore a runtime from {other:?}"),
        }
        let mode = saved
            .resume_id
            .clone()
            .map(backend::SpawnMode::ResumeId)
            .unwrap_or(backend::SpawnMode::ContinueLatest);
        let id = self.add_retained_session_with_mode(
            checkout,
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
        self.selected_id = Some(SelId::Checkout(checkout));
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
        let rows_before_removal = self.hierarchy_rows();
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
        #[cfg(test)]
        if let Some(error) = &self.remove_stop_error_for_test {
            return Err(lifecycle::RemovalFailure::Inspection(format!(
                "runtime stop failed: {error}"
            )));
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
                    move |app, _| app.teardown_retained_runtime(checkout, id),
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
        let result = match self.save_durable_status() {
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
        };
        if !self
            .repository_state
            .checkouts
            .iter()
            .any(|saved| saved.key == checkout)
        {
            let rows_after_removal = self.hierarchy_rows();
            if let Some(selected) = self.selection_target() {
                let fallback = hierarchy::reconcile_after_removal(
                    selected,
                    &rows_before_removal,
                    &rows_after_removal,
                );
                self.set_selection_target(fallback);
            }
        }
        result
    }

    fn close_retained_session(&mut self, id: u64) -> Result<LifecycleOutcome> {
        let checkout_key = checkout_for_runtime(&self.runtime_checkouts, id)
            .ok_or_else(|| anyhow::anyhow!("runtime {id} has no retained checkout"))?;
        let snapshot = self.retained_runtime_snapshot(id)?;
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
            self.teardown_retained_runtime(checkout_key, id)?;
            self.drive_lifecycle_effect(
                checkout_key,
                lifecycle::LifecycleEvent::RuntimeExtinct,
                move |app, _| {
                    app.forget_stopped_runtime(checkout_key, id);
                    Ok(())
                },
            )?;
            return Ok(LifecycleOutcome::Closed {
                checkout: checkout_key,
            });
        }
        self.drive_lifecycle_effect(
            checkout_key,
            lifecycle::LifecycleEvent::RequestClose,
            move |app, _| app.teardown_retained_runtime(checkout_key, id),
        )?;
        self.drive_lifecycle_effect(
            checkout_key,
            lifecycle::LifecycleEvent::RuntimeExtinct,
            move |app, _| {
                app.forget_stopped_runtime(checkout_key, id);
                Ok(())
            },
        )?;
        if self.persistence_dirty {
            anyhow::bail!("lifecycle state committed but directory durability failed");
        }
        Ok(LifecycleOutcome::Closed {
            checkout: checkout_key,
        })
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
        if matches!(
            self.selected_id,
            Some(SelId::Repository(_) | SelId::Checkout(_))
        ) {
            let rows = self.hierarchy_rows();
            let remote_ids: Vec<_> = self
                .ordered_ids()
                .into_iter()
                .filter_map(|id| match id {
                    SelId::Remote(id) => Some(id),
                    _ => None,
                })
                .collect();
            let selected =
                hierarchy::reconcile_selection(self.selection_target(), &rows, &remote_ids);
            self.set_selection_target(selected);
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
                    Some(SelId::Checkout(_)) => self
                        .selected()
                        .map(|s| !s.claude.is_exited())
                        .unwrap_or(false),
                    Some(SelId::Remote(id)) => self
                        .attach
                        .as_ref()
                        .map(|a| a.remote_id == id && !a.is_closed())
                        .unwrap_or(false),
                    Some(SelId::Repository(_)) => false,
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
        if area.height < 13 && self.focus == Focus::Shell {
            let live_agent = self
                .selected()
                .is_some_and(|session| !session.claude.is_exited());
            if live_agent {
                self.focus = Focus::Claude;
                self.set_message(
                    "shell hidden at this terminal height — resize to 13+ rows or press ctrl+\\ to close it"
                        .into(),
                );
            } else {
                self.focus = Focus::Sidebar;
                self.set_message(
                    "shell hidden at this terminal height — resize to 13+ rows; session input is paused"
                        .into(),
                );
            }
        }
        let rects = crate::ui::layout(area, self.focus);
        let content = rects.content;
        if !rects.content_visible {
            return;
        }
        let content_inner = inner(content);
        if content_inner.height == 0 || content_inner.width == 0 {
            return;
        }
        self.content_rect = content;
        for s in &mut self.sessions {
            let (claude_rect, shell_rect) = pane_rects(content, s.shell_open);
            let c = inner(claude_rect);
            if c.height > 0 && c.width > 0 {
                s.claude.resize(c.height, c.width);
            }
            if rects.shell_visible {
                if let (Some(sr), Some(shell)) = (shell_rect, s.shell.as_mut()) {
                    let r = inner(sr);
                    if r.height > 0 && r.width > 0 {
                        shell.resize(r.height, r.width);
                    }
                }
            }
        }
        if let Some(a) = &mut self.attach {
            let (claude_rect, _) = pane_rects(content, false);
            let r = inner(claude_rect);
            if r.height > 0 && r.width > 0 {
                a.resize(r.height.max(2), r.width.max(10));
            }
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
            Some(SelId::Checkout(key)) => {
                if let Some(id) = self.runtime_checkouts.get(&key).copied() {
                    self.modal = Modal::ConfirmCloseWorktree { id };
                }
            }
            Some(SelId::Repository(_)) => {}
            None => {}
        }
    }

    fn default_checkout(&self, repository: RepositoryKey) -> Option<CheckoutKey> {
        self.repository_state
            .checkouts
            .iter()
            .filter(|checkout| {
                checkout.repository_key == repository
                    && matches!(
                        checkout.role,
                        CheckoutRole::PrimaryDefault | CheckoutRole::Main
                    )
            })
            .min_by_key(|checkout| {
                let priority = match checkout.role {
                    CheckoutRole::PrimaryDefault => 0,
                    CheckoutRole::Main => 1,
                    CheckoutRole::ManagedBranch => 2,
                };
                (priority, checkout.first_seen_order, checkout.key)
            })
            .map(|checkout| checkout.key)
    }

    fn action_checkout(&self) -> Option<CheckoutKey> {
        match self.selected_id? {
            SelId::Checkout(key) => Some(key),
            SelId::Repository(repository) => self.default_checkout(repository),
            SelId::Remote(_) => None,
        }
    }

    fn open_local_target(&mut self) {
        let Some(checkout) = self.action_checkout() else {
            let repository = self.selected_target_label();
            self.set_message(format!(
                "Cannot reopen “{repository}”: its default checkout is unavailable. Open details with i, repair the reported Git topology, then use the action authorized there."
            ));
            return;
        };
        if let Some(runtime) = self.runtime_checkouts.get(&checkout).copied() {
            if self
                .session(runtime)
                .is_some_and(|session| !session.claude.is_exited())
            {
                self.selected_id = Some(SelId::Checkout(checkout));
                self.focus = Focus::Claude;
                return;
            }
        }
        let capability = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .and_then(|saved| lifecycle::lifecycle_capability(saved.lifecycle()));
        if capability != Some(lifecycle::LifecycleCapability::RetryReopen) {
            if matches!(self.selected_id, Some(SelId::Repository(_))) {
                let repository = self.selected_target_label();
                self.set_message(format!(
                    "Cannot reopen “{repository}”: its default checkout is unavailable. Open details with i, repair the reported Git topology, then use the action authorized there."
                ));
            } else {
                self.refuse_sidebar_action(SidebarRefusal::RetryNotAuthorized);
            }
            return;
        }
        if let Err(error) = self.reopen_checkout(checkout) {
            self.set_message(format!("reopen blocked: {error}"));
        }
    }

    fn open_branch_modal(&mut self) {
        let Some(repository) = self.selected_repository() else {
            return;
        };
        let repo_root = repository.observed_main_worktree.to_path_buf();
        let label = Self::repository_label(repository);
        self.modal = Modal::Input {
            kind: InputKind::NewWorktreeBranch { repo_root },
            title: format!("create or activate branch in {label} — local branch name"),
            buf: String::new(),
            candidates: Vec::new(),
        };
    }

    fn retry_selected_recovery(&mut self) {
        let Some(SelId::Checkout(checkout)) = self.selected_id else {
            self.refuse_sidebar_action(SidebarRefusal::RetryNotAuthorized);
            return;
        };
        let lifecycle = self
            .repository_state
            .checkouts
            .iter()
            .find(|saved| saved.key == checkout)
            .map(|saved| saved.lifecycle().clone());
        let result = match lifecycle {
            Some(CheckoutLifecycle::Activating)
            | Some(CheckoutLifecycle::Protected(
                UnavailableCause::PendingActivation { .. }
                | UnavailableCause::ActivationRecovery { .. },
            )) => self
                .retry_activation_recovery(checkout)
                .map(|outcome| format!("activation recovery: {outcome:?}")),
            Some(CheckoutLifecycle::Protected(UnavailableCause::TeardownPending { .. })) => self
                .retry_teardown_recovery(checkout)
                .map(|outcome| format!("teardown recovery: {outcome:?}")),
            Some(CheckoutLifecycle::Protected(UnavailableCause::StoppedActiveRecovery {
                ..
            })) => self
                .drive_lifecycle_effect(
                    checkout,
                    lifecycle::LifecycleEvent::RuntimeExtinct,
                    |_, _| Ok(()),
                )
                .map(|_| "runtime ownership recovery completed".into()),
            _ => {
                self.refuse_sidebar_action(SidebarRefusal::RetryNotAuthorized);
                return;
            }
        };
        match result {
            Ok(message) => self.set_message(message),
            Err(error) => self.set_message(format!("lifecycle recovery blocked: {error}")),
        }
    }

    fn removal_refusal(&self, failure: &lifecycle::RemovalFailure) -> String {
        let target = self.selected_target_label();
        match failure {
            lifecycle::RemovalFailure::Blocked(blockers)
                if blockers
                    .iter()
                    .any(|blocker| matches!(blocker, git::RemovalBlocker::Conflict { .. })) =>
            {
                format!("Cannot remove “{target}”: unresolved Git conflicts are present. Resolve or abort the Git operation yourself, then press X to run a new safety check; nothing was removed.")
            }
            lifecycle::RemovalFailure::Blocked(blockers)
                if blockers
                    .iter()
                    .any(|blocker| matches!(blocker, git::RemovalBlocker::Locked)) =>
            {
                format!("Cannot remove “{target}”: Git reports this worktree as locked. Review and unlock it with Git if safe, then press X to run a new safety check; nothing was removed.")
            }
            lifecycle::RemovalFailure::Blocked(blockers)
                if blockers.iter().any(|blocker| {
                    matches!(
                        blocker,
                        git::RemovalBlocker::SubmoduleChange { .. }
                            | git::RemovalBlocker::SubmodulePresent { .. }
                    )
                }) => format!("Cannot remove “{target}”: recursive submodule state makes non-force removal unsafe. Resolve the submodule worktrees yourself, then press X to run a new safety check; nothing was removed."),
            lifecycle::RemovalFailure::Blocked(blockers)
                if blockers.iter().any(|blocker| {
                    matches!(
                        blocker,
                        git::RemovalBlocker::StagedAdd { .. }
                            | git::RemovalBlocker::StagedDelete { .. }
                            | git::RemovalBlocker::StagedRename { .. }
                            | git::RemovalBlocker::StagedModification { .. }
                            | git::RemovalBlocker::UnstagedModification { .. }
                            | git::RemovalBlocker::UnstagedDelete { .. }
                            | git::RemovalBlocker::Untracked { .. }
                            | git::RemovalBlocker::Ignored { .. }
                    )
                }) => format!("Cannot remove “{target}”: dirty tracked or untracked files are present. Commit, move, or clean those files yourself, then press X to run a new safety check; nothing was removed."),
            _ => format!("Cannot remove “{target}”: baude could not conclusively verify clean Git status and topology. Inspect the repository with Git, repair the reported error, then press X to run a new safety check; nothing was removed."),
        }
    }

    fn prepare_selected_removal(&mut self) {
        let Some(SelId::Checkout(checkout)) = self.selected_id else {
            self.refuse_sidebar_action(SidebarRefusal::RepositoryRemove);
            return;
        };
        match self.prepare_remove_worktree(checkout) {
            Ok(confirmation) => self.modal = Modal::ConfirmRemoveWorktree { confirmation },
            Err(failure) => {
                let message = self.removal_refusal(&failure);
                self.set_message(message);
            }
        }
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('n') => self.open_new_session_modal(),
            KeyCode::Char('c') => {
                self.modal = Modal::Input {
                    kind: InputKind::CloneUrl,
                    title: "clone repo — github url or owner/repo".into(),
                    buf: String::new(),
                    candidates: Vec::new(),
                };
            }
            KeyCode::Char('?') => self.modal = Modal::Help,
            _ => {
                let Some(view) = self.selected_action_view() else {
                    return;
                };
                match sidebar_action(view, key) {
                    SidebarAction::None => {}
                    SidebarAction::Open => self.open_local_target(),
                    SidebarAction::Branch => self.open_branch_modal(),
                    SidebarAction::Close => self.confirm_close_selected(),
                    SidebarAction::RetryReopen => self.open_local_target(),
                    SidebarAction::RetryRecovery => self.retry_selected_recovery(),
                    SidebarAction::Remove => self.prepare_selected_removal(),
                    SidebarAction::Shell => self.toggle_shell(true),
                    SidebarAction::Editor => self.open_editor_for_selection(),
                    SidebarAction::Info => self.modal = Modal::Info,
                    SidebarAction::Activity => self.modal = Modal::Activity,
                    SidebarAction::Gsd => self.modal = Modal::Gsd,
                    SidebarAction::Archive => self.toggle_archive(),
                    SidebarAction::RemoteOpen => self.attach_selected_remote(),
                    SidebarAction::RemoteClose => self.confirm_close_selected(),
                    SidebarAction::RemoteRestart => {
                        if let Some(SelId::Remote(id)) = self.selected_id {
                            self.restart_remote(id);
                        }
                    }
                    SidebarAction::RemoteArchive => self.toggle_archive(),
                    SidebarAction::Refuse(refusal) => self.refuse_sidebar_action(refusal),
                }
            }
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
                            SelId::Checkout(key) => {
                                if let Some(id) = self.runtime_checkouts.get(&key).copied() {
                                    self.remove_session(id);
                                }
                            }
                            SelId::Repository(_) => {}
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
                    KeyCode::Char('y') | KeyCode::Enter => {
                        self.modal = Modal::None;
                        match self.close_retained_session(id) {
                            Ok(_) => self.set_message("session closed — checkout kept".into()),
                            Err(error) => self
                                .set_message(format!("session close degraded or blocked: {error}")),
                        }
                    }
                    KeyCode::Char('n') | KeyCode::Esc => self.modal = Modal::None,
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
                        Err(error) => {
                            let message = self.removal_refusal(&error);
                            self.set_message(message);
                        }
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
                let repository = repo_root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| repo_root.display().to_string());
                match self.activate_branch_worktree(&repo_root, &value) {
                    Ok(LifecycleOutcome::Busy { .. }) => self.set_message(format!(
                        "Cannot create or activate a branch in “{repository}”: another lifecycle action is in progress. Wait for it to finish, then press w to retry."
                    )),
                    Ok(LifecycleOutcome::Created { .. }) => {
                        self.set_message(format!("created worktree for {value}"))
                    }
                    Ok(LifecycleOutcome::Activated { .. }) => {
                        self.set_message(format!("activated {value}"))
                    }
                    Ok(LifecycleOutcome::Reused { .. } | LifecycleOutcome::Focused { .. }) => {
                        self.set_message(format!("focused existing {value}"))
                    }
                    Ok(other) => self.set_message(format!(
                        "unexpected branch activation outcome for {value}: {other:?}"
                    )),
                    Err(error) => {
                        let message = self.activation_refusal(&error, &value, &repository);
                        self.set_message(message);
                    }
                }
            }
        }
    }

    fn activation_refusal(&self, error: &anyhow::Error, branch: &str, repository: &str) -> String {
        if let Some(lifecycle::LifecycleError::OccupiedProtected { checkout, cause }) =
            error.downcast_ref::<lifecycle::LifecycleError>()
        {
            let target = self
                .repository_state
                .checkouts
                .iter()
                .find(|saved| saved.key == *checkout)
                .map(|saved| saved.session.name.clone())
                .unwrap_or_else(|| format!("checkout {}", checkout.get()));
            let recovery = Self::unavailable_cause_label(Some(cause));
            return format!(
                "Cannot activate “{branch}” in “{repository}”: checkout “{target}” is in protected {recovery} state. Open details with i and complete the lifecycle-authorized recovery before pressing w again."
            );
        }
        if let Some(lifecycle::LifecycleError::Git(git_error)) =
            error.downcast_ref::<lifecycle::LifecycleError>()
        {
            return match git_error {
                git::BranchActivationError::InvalidLiteral { .. } => format!(
                    "Cannot create or activate “{branch}” in “{repository}”: “{branch}” is not a valid literal local branch name. Press w and enter a name accepted by Git."
                ),
                git::BranchActivationError::RemoteOnly { .. } => format!(
                    "Cannot activate “{branch}” in “{repository}”: only a remote-tracking branch exists. Create an explicit local branch outside baude, then press w to activate it."
                ),
                git::BranchActivationError::PathCollision(path) => format!(
                    "Cannot create or activate “{branch}” in “{repository}”: the managed worktree path “{}” collides with existing filesystem or Git state. Move or reconcile that path, then press w to retry.",
                    path.display()
                ),
                _ => format!(
                    "Cannot create or activate “{branch}” in “{repository}”: Git refused the literal local branch request ({git_error}). Inspect the repository with Git, then press w to retry."
                ),
            };
        }
        format!(
            "Cannot create or activate “{branch}” in “{repository}”: {error}. Open details with i, repair the reported lifecycle or Git state, then press w to retry."
        )
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
            Some(SelId::Checkout(key)) => {
                if let Some(id) = self.runtime_checkouts.get(&key).copied() {
                    let Some(s) = self.session_mut(id) else {
                        return;
                    };
                    s.set_archived(!s.archived);
                    let msg = if s.archived { "archived" } else { "unarchived" };
                    self.set_message(msg.into());
                    self.save();
                    return;
                }
                let before = self.repository_state.clone();
                let Some(saved) = self
                    .repository_state
                    .checkouts
                    .iter_mut()
                    .find(|saved| saved.key == key)
                else {
                    return;
                };
                saved.session.archived = !saved.session.archived;
                saved.session.archived_by_user = saved.session.archived;
                let archived = saved.session.archived;
                if let Err(error) = self.save_durable_status() {
                    self.persistence_dirty = true;
                    if !error.replacement_committed() {
                        self.repository_state = before;
                    }
                    self.set_message(format!("archive state not saved: {error}"));
                    return;
                }
                self.persistence_dirty = false;
                self.set_message(if archived {
                    "archived".into()
                } else {
                    "unarchived".into()
                });
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
            Some(SelId::Repository(_)) => {}
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
            .map(|index| index as i64)
            .unwrap_or_else(|| if delta < 0 { 0 } else { -1 });
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
        let ids: Vec<_> = self
            .ordered_ids()
            .into_iter()
            .filter(|id| !matches!(id, SelId::Repository(_)))
            .collect();
        if ids.is_empty() {
            return;
        }
        let len = ids.len() as i64;
        let cur = self
            .selected_id
            .and_then(|id| ids.iter().position(|&x| x == id))
            .map(|index| index as i64)
            .unwrap_or_else(|| if delta < 0 { 0 } else { -1 });
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
        let cwd = match self.selected_id {
            Some(SelId::Repository(key)) => self
                .repository_state
                .repositories
                .iter()
                .find(|repository| repository.key == key)
                .map(|repository| repository.observed_main_worktree.to_path_buf()),
            Some(SelId::Checkout(key)) => self
                .repository_state
                .checkouts
                .iter()
                .find(|checkout| checkout.key == key)
                .map(|checkout| checkout.observed_path.to_path_buf()),
            _ => None,
        };
        let Some(cwd) = cwd else {
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
        let checkout = checkout_for_runtime(&self.runtime_checkouts, id);
        let mut pty = if let Some(checkout) = checkout {
            let generation = self
                .repository_state
                .checkouts
                .iter()
                .find(|saved| saved.key == checkout)
                .and_then(SavedCheckout::owned_runtime)
                .and_then(|runtime| runtime.generation.successor())
                .unwrap_or(RuntimeGeneration::initial());
            let shell = self
                .session(id)
                .and_then(|session| session.shell.as_ref())
                .filter(|shell| !shell.is_exited())
                .map(|shell| ShellOwnership::Owned(shell.process_identity().clone()))
                .unwrap_or(ShellOwnership::Closed);
            let mut replacement = Pty::spawn_registered_with(
                Some(&plan.cmd),
                &plan.env,
                &cwd,
                rows,
                cols,
                |agent| {
                    self.drive_lifecycle_effect(
                        checkout,
                        baude_core::lifecycle::LifecycleEvent::LaunchRegistered(OwnedRuntime {
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
                baude_core::lifecycle::LifecycleEvent::LaunchReleased,
                |_, _| Ok(()),
            ) {
                let _ = replacement.kill_and_wait();
                return Err(error);
            }
            replacement
        } else {
            Pty::spawn_with_env(Some(&plan.cmd), &plan.env, &cwd, rows, cols)?
        };
        if let Some(s) = self.session_mut(id) {
            std::mem::swap(&mut s.claude, &mut pty);
            s.spawn_unix_ms = now_unix_ms();
            s.meta = ClaudeMeta::default();
            s.meta.backend_port = plan.server_port;
        }
        self.focus = Focus::Claude;
        Ok(())
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
mod tests {
    use super::{
        active_restore_checkouts, checkout_for_runtime, local_admission_route,
        require_same_checkout_path, App, LocalAdmissionRoute, Modal,
    };
    use baude_core::lifecycle::{
        canonical_lifecycle_contract_vectors, canonical_lifecycle_trace, normalize_lifecycle_trace,
        LifecycleOutcome, RepositoryReservations,
    };
    use baude_core::repository::{
        CheckoutHealth, CheckoutKey, CheckoutLifecycle, CheckoutRole, PersistedPath,
        RepositoryHealth, RepositoryState, RetainedSessionState, SavedCheckout, SavedRepository,
        UnavailableCause,
    };
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    #[test]
    fn lifecycle_protocol_contract_app_vectors() {
        for vector in canonical_lifecycle_contract_vectors() {
            let actual = App::lifecycle_contract_trace(
                *vector,
                baude_core::lifecycle::AdapterFailureScript::None,
            );
            assert_eq!(
                normalize_lifecycle_trace(&actual.trace),
                canonical_lifecycle_trace(*vector).trace
            );
            assert_eq!(
                actual.final_lifecycle,
                canonical_lifecycle_trace(*vector).final_lifecycle
            );
        }
        for failure in [
            baude_core::lifecycle::AdapterFailureScript::Persist(1),
            baude_core::lifecycle::AdapterFailureScript::Effect(1),
        ] {
            let actual = App::lifecycle_contract_trace(
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

    fn assert_runtime_replaced_preserving_checkout(
        actual: &RepositoryState,
        before: &RepositoryState,
    ) {
        assert_eq!(actual.repositories, before.repositories);
        assert_eq!(actual.checkouts.len(), before.checkouts.len());
        let actual = &actual.checkouts[0];
        let before = &before.checkouts[0];
        assert_eq!(actual.key, before.key);
        assert_eq!(actual.repository_key, before.repository_key);
        assert_eq!(actual.observed_path, before.observed_path);
        assert_eq!(actual.observed_branch, before.observed_branch);
        assert_eq!(actual.session, before.session);
        assert!(matches!(actual.lifecycle(), CheckoutLifecycle::Running(_)));
        assert!(actual.owned_runtime().is_some());
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
        state.checkouts.push(SavedCheckout::new(
            key,
            repository_key,
            role,
            false,
            path.clone(),
            Some("refs/heads/main".into()),
            order,
            if active_intent {
                CheckoutLifecycle::Active
            } else {
                CheckoutLifecycle::Inactive
            },
            RetainedSessionState {
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
        ));
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
    fn hierarchy_navigation_visits_parents_cycles_children_and_retains_selection_on_ctrl_q() {
        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        let path = PersistedPath::from_path(Path::new("/repo"));
        state.repositories.push(SavedRepository {
            key: repository_key,
            observed_common_dir: PersistedPath::from_path(Path::new("/repo/.git")),
            observed_main_worktree: path,
            first_seen_order: order,
            health: RepositoryHealth::Available,
        });
        add_checkout(&mut state, CheckoutRole::ManagedBranch, false);
        state.checkouts[0].observed_path = PersistedPath::from_path(Path::new("/repo/one"));
        state.checkouts[0].session.cwd = PersistedPath::from_path(Path::new("/repo/one"));
        add_checkout(&mut state, CheckoutRole::ManagedBranch, false);
        state.checkouts[1].observed_path = PersistedPath::from_path(Path::new("/repo/two"));
        state.checkouts[1].session.cwd = PersistedPath::from_path(Path::new("/repo/two"));
        let first = state.checkouts[0].key;
        let second = state.checkouts[1].key;

        let mut app = App::new(PathBuf::from("/not-a-repository"));
        app.remote = None;
        app.install_hierarchy_state_for_test(state, HashMap::new());
        app.selected_id = Some(super::SelId::Repository(repository_key));

        app.move_selection(1);
        assert_eq!(app.selected_id, Some(super::SelId::Checkout(first)));
        app.move_selection(-1);
        assert_eq!(
            app.selected_id,
            Some(super::SelId::Repository(repository_key))
        );
        app.cycle_session(1);
        assert_eq!(app.selected_id, Some(super::SelId::Checkout(first)));
        app.cycle_session(1);
        assert_eq!(app.selected_id, Some(super::SelId::Checkout(second)));
        app.cycle_session(1);
        assert_eq!(app.selected_id, Some(super::SelId::Checkout(first)));

        app.focus = super::Focus::Claude;
        let selected = app.selected_id;
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(matches!(app.focus, super::Focus::Sidebar));
        assert_eq!(app.selected_id, selected);
    }

    #[test]
    fn hierarchy_action_matrix_dispatches_only_authorized_local_actions() {
        use super::{SidebarAction, SidebarRefusal};
        use crate::hierarchy::{action_view, ActionSelection};
        use baude_core::lifecycle::LifecycleCapability;

        let keys = [
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        ];
        let cases = [
            (
                "repository",
                action_view(
                    ActionSelection::Repository { available: true },
                    false,
                    Some(LifecycleCapability::RetryReopen),
                ),
                [
                    SidebarAction::Open,
                    SidebarAction::Branch,
                    SidebarAction::Refuse(SidebarRefusal::RepositoryClose),
                    SidebarAction::RetryReopen,
                    SidebarAction::Refuse(SidebarRefusal::RepositoryRemove),
                    SidebarAction::Refuse(SidebarRefusal::RepositoryShell),
                    SidebarAction::Editor,
                    SidebarAction::Info,
                    SidebarAction::None,
                    SidebarAction::Gsd,
                    SidebarAction::Refuse(SidebarRefusal::RepositoryArchive),
                ],
            ),
            (
                "main",
                action_view(
                    ActionSelection::Checkout {
                        role: CheckoutRole::Main,
                        managed_by_baude: false,
                        available: true,
                    },
                    false,
                    Some(LifecycleCapability::RetryReopen),
                ),
                [
                    SidebarAction::Open,
                    SidebarAction::Branch,
                    SidebarAction::Refuse(SidebarRefusal::AlreadyClosed),
                    SidebarAction::RetryReopen,
                    SidebarAction::Refuse(SidebarRefusal::MainRemove),
                    SidebarAction::Refuse(SidebarRefusal::NoLiveRuntime),
                    SidebarAction::Editor,
                    SidebarAction::Info,
                    SidebarAction::Activity,
                    SidebarAction::Gsd,
                    SidebarAction::Archive,
                ],
            ),
            (
                "managed",
                action_view(
                    ActionSelection::Checkout {
                        role: CheckoutRole::ManagedBranch,
                        managed_by_baude: true,
                        available: true,
                    },
                    false,
                    Some(LifecycleCapability::RetryReopen),
                ),
                [
                    SidebarAction::Open,
                    SidebarAction::Branch,
                    SidebarAction::Refuse(SidebarRefusal::AlreadyClosed),
                    SidebarAction::RetryReopen,
                    SidebarAction::Remove,
                    SidebarAction::Refuse(SidebarRefusal::NoLiveRuntime),
                    SidebarAction::Editor,
                    SidebarAction::Info,
                    SidebarAction::Activity,
                    SidebarAction::Gsd,
                    SidebarAction::Archive,
                ],
            ),
            (
                "external",
                action_view(
                    ActionSelection::Checkout {
                        role: CheckoutRole::ManagedBranch,
                        managed_by_baude: false,
                        available: true,
                    },
                    false,
                    Some(LifecycleCapability::RetryReopen),
                ),
                [
                    SidebarAction::Open,
                    SidebarAction::Branch,
                    SidebarAction::Refuse(SidebarRefusal::AlreadyClosed),
                    SidebarAction::RetryReopen,
                    SidebarAction::Refuse(SidebarRefusal::UnmanagedRemove),
                    SidebarAction::Refuse(SidebarRefusal::NoLiveRuntime),
                    SidebarAction::Editor,
                    SidebarAction::Info,
                    SidebarAction::Activity,
                    SidebarAction::Gsd,
                    SidebarAction::Archive,
                ],
            ),
            (
                "unavailable",
                action_view(
                    ActionSelection::Checkout {
                        role: CheckoutRole::ManagedBranch,
                        managed_by_baude: true,
                        available: false,
                    },
                    false,
                    None,
                ),
                [
                    SidebarAction::Refuse(SidebarRefusal::UnavailableReopen),
                    SidebarAction::Refuse(SidebarRefusal::UnavailableBranch),
                    SidebarAction::Refuse(SidebarRefusal::UnavailableClose),
                    SidebarAction::Refuse(SidebarRefusal::RetryNotAuthorized),
                    SidebarAction::Refuse(SidebarRefusal::UnavailableRemove),
                    SidebarAction::Refuse(SidebarRefusal::NoLiveRuntime),
                    SidebarAction::Editor,
                    SidebarAction::Info,
                    SidebarAction::Activity,
                    SidebarAction::Gsd,
                    SidebarAction::Refuse(SidebarRefusal::UnavailableArchive),
                ],
            ),
            (
                "recovery",
                action_view(
                    ActionSelection::Checkout {
                        role: CheckoutRole::ManagedBranch,
                        managed_by_baude: true,
                        available: false,
                    },
                    false,
                    Some(LifecycleCapability::RetryRecovery),
                ),
                [
                    SidebarAction::Refuse(SidebarRefusal::RecoveryReopen),
                    SidebarAction::Refuse(SidebarRefusal::RecoveryBranch),
                    SidebarAction::Refuse(SidebarRefusal::RecoveryClose),
                    SidebarAction::RetryRecovery,
                    SidebarAction::Refuse(SidebarRefusal::RecoveryRemove),
                    SidebarAction::Refuse(SidebarRefusal::NoLiveRuntime),
                    SidebarAction::Editor,
                    SidebarAction::Info,
                    SidebarAction::Activity,
                    SidebarAction::Gsd,
                    SidebarAction::Refuse(SidebarRefusal::UnavailableArchive),
                ],
            ),
            (
                "remote",
                action_view(ActionSelection::Remote, true, None),
                [
                    SidebarAction::RemoteOpen,
                    SidebarAction::None,
                    SidebarAction::RemoteClose,
                    SidebarAction::RemoteRestart,
                    SidebarAction::None,
                    SidebarAction::Refuse(SidebarRefusal::RemoteShell),
                    SidebarAction::Refuse(SidebarRefusal::RemoteEditor),
                    SidebarAction::Info,
                    SidebarAction::Activity,
                    SidebarAction::Refuse(SidebarRefusal::RemoteGsd),
                    SidebarAction::RemoteArchive,
                ],
            ),
        ];

        for (selection, view, expected) in cases {
            for (index, key) in keys.iter().copied().enumerate() {
                assert_eq!(
                    super::sidebar_action(view, key),
                    expected[index],
                    "{selection} × {key:?}"
                );
            }
        }
        let managed = action_view(
            ActionSelection::Checkout {
                role: CheckoutRole::ManagedBranch,
                managed_by_baude: true,
                available: true,
            },
            true,
            None,
        );
        assert_eq!(
            super::sidebar_action(
                managed,
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)
            ),
            SidebarAction::Close
        );
        assert_eq!(
            super::sidebar_action(
                managed,
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::SHIFT)
            ),
            SidebarAction::Remove
        );

        // Defensive refusals are pure: stale hidden-key dispatch cannot alter
        // durable state, process association, row order, or selection.
        let mut app = App::new(PathBuf::from("/not-a-repository"));
        app.remote = None;
        let before_state = app.repository_state.clone();
        let before_runtimes = app.runtime_checkouts.clone();
        let before_rows = app.ordered_ids();
        let before_selection = app.selected_id;
        for refusal in [
            SidebarRefusal::RepositoryClose,
            SidebarRefusal::RepositoryRemove,
            SidebarRefusal::RetryNotAuthorized,
            SidebarRefusal::UnmanagedRemove,
            SidebarRefusal::UnavailableRemove,
        ] {
            app.refuse_sidebar_action(refusal);
            assert_eq!(app.repository_state, before_state);
            assert_eq!(app.runtime_checkouts, before_runtimes);
            assert_eq!(app.ordered_ids(), before_rows);
            assert_eq!(app.selected_id, before_selection);
        }

        let mut state = RepositoryState::default();
        let repository = state.allocate_repository_key().unwrap();
        let repository_order = state.allocate_first_seen_order().unwrap();
        state.repositories.push(SavedRepository {
            key: repository,
            observed_common_dir: PersistedPath::from_path(Path::new("/repo/project/.git")),
            observed_main_worktree: PersistedPath::from_path(Path::new("/repo/project")),
            first_seen_order: repository_order,
            health: RepositoryHealth::Available,
        });
        add_checkout(&mut state, CheckoutRole::Main, false);
        state.checkouts[0].session.name = "project:main".into();
        state.checkouts[0].session.is_worktree = false;
        state.checkouts[0].observed_path = PersistedPath::from_path(Path::new("/repo/project"));
        state.checkouts[0].session.cwd = PersistedPath::from_path(Path::new("/repo/project"));
        state.checkouts[0].session.repo_root = PersistedPath::from_path(Path::new("/repo/project"));
        let main = state.checkouts[0].key;
        add_checkout(&mut state, CheckoutRole::ManagedBranch, false);
        state.checkouts[1].managed_by_baude = false;
        state.checkouts[1].session.name = "project:external".into();
        state.checkouts[1].session.repo_root = PersistedPath::from_path(Path::new("/repo/project"));
        let external = state.checkouts[1].key;
        add_checkout(&mut state, CheckoutRole::ManagedBranch, false);
        let unavailable = state.checkouts[2].key;
        let unavailable_saved = state.checkouts[2].clone();
        state.checkouts[2] = SavedCheckout::new(
            unavailable_saved.key,
            unavailable_saved.repository_key,
            unavailable_saved.role,
            true,
            PersistedPath::from_path(Path::new("/repo/missing")),
            unavailable_saved.observed_branch,
            unavailable_saved.first_seen_order,
            CheckoutLifecycle::Protected(UnavailableCause::Missing),
            RetainedSessionState {
                name: "project:missing".into(),
                cwd: PersistedPath::from_path(Path::new("/repo/missing")),
                repo_root: PersistedPath::from_path(Path::new("/repo/project")),
                ..unavailable_saved.session
            },
        );
        let mut app = App::new(PathBuf::from("/not-a-repository"));
        app.remote = None;
        let action_state_root = std::env::temp_dir().join(format!(
            "baude-hierarchy-action-state-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&action_state_root);
        std::fs::create_dir_all(&action_state_root).unwrap();
        app.persistence_root_for_test = Some(action_state_root.clone());
        app.install_hierarchy_state_for_test(state, HashMap::new());

        let exact_refusals = [
            (
                super::SelId::Repository(repository),
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
                "Cannot close “project”: a repository parent is not a session. Select a running checkout, then press x.",
            ),
            (
                super::SelId::Repository(repository),
                KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
                "Cannot remove “project”: repository parents are never removed by this action. Select a baude-managed worktree, then press X.",
            ),
            (
                super::SelId::Checkout(main),
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
                "Cannot close “project:main”: its session is already closed and the checkout is kept. Press enter to reopen it.",
            ),
            (
                super::SelId::Checkout(main),
                KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
                "Cannot remove “project:main”: the main checkout is never removable from baude. Keep it in Git and select a baude-managed linked worktree if removal is intended.",
            ),
            (
                super::SelId::Checkout(external),
                KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
                "Cannot remove “project:external”: it is not a baude-managed linked worktree. Keep it unchanged or remove it manually with Git if intended; nothing was removed.",
            ),
            (
                super::SelId::Checkout(unavailable),
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                "Cannot reopen “project:missing”: Git topology is unavailable (missing) and this state is not retryable from the TUI. Open details with i and repair the checkout; no runtime was started.",
            ),
        ];
        let before_state = app.repository_state.clone();
        let before_runtimes = app.runtime_checkouts.clone();
        let before_order = app.ordered_ids();
        for selection in [
            super::SelId::Repository(repository),
            super::SelId::Checkout(main),
            super::SelId::Checkout(external),
        ] {
            app.selected_id = Some(selection);
            app.handle_sidebar_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
            assert!(matches!(
                &app.modal,
                Modal::Input {
                    kind: super::InputKind::NewWorktreeBranch { repo_root },
                    title,
                    ..
                } if repo_root == Path::new("/repo/project")
                    && title == "create or activate branch in project — local branch name"
            ));
            app.handle_modal_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert_eq!(app.repository_state, before_state);
            assert_eq!(app.runtime_checkouts, before_runtimes);
            assert_eq!(app.ordered_ids(), before_order);
            assert_eq!(app.selected_id, Some(selection));
        }
        for (selection, key, expected) in exact_refusals {
            app.selected_id = Some(selection);
            app.handle_sidebar_key(key);
            assert_eq!(app.message.as_ref().unwrap().0, expected);
            assert_eq!(app.repository_state, before_state);
            assert_eq!(app.runtime_checkouts, before_runtimes);
            assert_eq!(app.ordered_ids(), before_order);
            assert_eq!(app.selected_id, Some(selection));
        }
        app.selected_id = Some(super::SelId::Checkout(main));
        app.handle_sidebar_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(
            app.repository_state.checkouts[0].session.archived,
            "archive result: {:?}",
            app.message
        );
        assert_eq!(app.runtime_checkouts, before_runtimes);
        assert_eq!(app.ordered_ids(), before_order);
        assert_eq!(app.selected_id, Some(super::SelId::Checkout(main)));
        std::fs::remove_dir_all(action_state_root).unwrap();

        let (mut app, repo, root, checkout, runtime, worktree_path) =
            removal_app("hierarchy-action-matrix", 180_000);
        let repository = app.repository_state.checkouts[0].repository_key;
        let baseline_state = app.repository_state.clone();
        let baseline_runtimes = app.runtime_checkouts.clone();
        let baseline_order = app.ordered_ids();
        let baseline_selection = app.selected_id;
        let worktree_inventory = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout;

        app.submit_input(
            super::InputKind::NewWorktreeBranch {
                repo_root: repo.clone(),
            },
            "bad ref".into(),
        );
        assert_eq!(
            app.message.as_ref().unwrap().0,
            "Cannot create or activate “bad ref” in “repo”: “bad ref” is not a valid literal local branch name. Press w and enter a name accepted by Git."
        );
        assert_eq!(app.repository_state, baseline_state);
        assert_eq!(app.runtime_checkouts, baseline_runtimes);
        assert_eq!(app.ordered_ids(), baseline_order);
        assert_eq!(app.selected_id, baseline_selection);
        assert_eq!(
            Command::new("git")
                .args(["worktree", "list", "--porcelain"])
                .current_dir(&repo)
                .output()
                .unwrap()
                .stdout,
            worktree_inventory
        );

        git(
            &repo,
            &["update-ref", "refs/remotes/origin/remote-only", "HEAD"],
        );
        app.submit_input(
            super::InputKind::NewWorktreeBranch {
                repo_root: repo.clone(),
            },
            "remote-only".into(),
        );
        assert_eq!(
            app.message.as_ref().unwrap().0,
            "Cannot activate “remote-only” in “repo”: only a remote-tracking branch exists. Create an explicit local branch outside baude, then press w to activate it."
        );
        assert_eq!(app.repository_state, baseline_state);
        assert_eq!(app.runtime_checkouts, baseline_runtimes);
        assert_eq!(app.ordered_ids(), baseline_order);
        assert_eq!(app.selected_id, baseline_selection);
        assert_eq!(
            Command::new("git")
                .args(["worktree", "list", "--porcelain"])
                .current_dir(&repo)
                .output()
                .unwrap()
                .stdout,
            worktree_inventory
        );

        let mut allocation = baseline_state.clone();
        let next_checkout = allocation.allocate_checkout_key().unwrap();
        let collision = baude_core::git::managed_branch_worktree_path(
            repository.get(),
            next_checkout.get(),
            "collision",
        );
        std::fs::create_dir_all(&collision).unwrap();
        app.submit_input(
            super::InputKind::NewWorktreeBranch {
                repo_root: repo.clone(),
            },
            "collision".into(),
        );
        assert_eq!(
            app.message.as_ref().unwrap().0,
            format!(
                "Cannot create or activate “collision” in “repo”: the managed worktree path “{}” collides with existing filesystem or Git state. Move or reconcile that path, then press w to retry.",
                collision.display()
            )
        );
        assert_eq!(app.repository_state, baseline_state);
        assert_eq!(app.runtime_checkouts, baseline_runtimes);
        assert_eq!(app.ordered_ids(), baseline_order);
        assert_eq!(app.selected_id, baseline_selection);
        assert_eq!(
            Command::new("git")
                .args(["worktree", "list", "--porcelain"])
                .current_dir(&repo)
                .output()
                .unwrap()
                .stdout,
            worktree_inventory
        );

        app.session_mut(runtime).unwrap().kill();
        std::fs::remove_dir_all(&collision).unwrap();
        git(
            &repo,
            &["worktree", "remove", "--", worktree_path.to_str().unwrap()],
        );
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(checkout, baseline_state.checkouts[0].key);
    }

    #[test]
    fn hierarchy_resize_never_sends_zero_dimensions_and_transfers_hidden_shell_focus() {
        let mut paused = App::new(PathBuf::from("/not-a-repository"));
        paused.remote = None;
        paused.focus = super::Focus::Shell;
        paused.sync_sizes(ratatui::layout::Rect::new(0, 0, 40, 12));
        assert!(paused.focus == super::Focus::Sidebar);
        assert_eq!(
            paused.message.as_ref().map(|message| message.0.as_str()),
            Some("shell hidden at this terminal height — resize to 13+ rows; session input is paused")
        );

        let (mut app, repo, root, _checkout, runtime, worktree_path) =
            removal_app("tiny-resize", 185_000);
        app.sync_sizes(ratatui::layout::Rect::new(0, 0, 100, 30));
        let before = app
            .session(runtime)
            .unwrap()
            .claude
            .parser
            .lock()
            .unwrap()
            .screen()
            .size();
        app.focus = super::Focus::Sidebar;
        app.sync_sizes(ratatui::layout::Rect::new(0, 0, 40, 12));
        let hidden = app
            .session(runtime)
            .unwrap()
            .claude
            .parser
            .lock()
            .unwrap()
            .screen()
            .size();
        assert_eq!(
            hidden, before,
            "hidden PTY must retain its last visible dimensions"
        );

        app.focus = super::Focus::Shell;
        app.sync_sizes(ratatui::layout::Rect::new(0, 0, 40, 12));
        assert!(app.focus == super::Focus::Claude);
        assert_eq!(
            app.message.as_ref().map(|message| message.0.as_str()),
            Some("shell hidden at this terminal height — resize to 13+ rows or press ctrl+\\ to close it")
        );
        let visible = app
            .session(runtime)
            .unwrap()
            .claude
            .parser
            .lock()
            .unwrap()
            .screen()
            .size();
        assert!(
            visible.0 >= 2 && visible.1 >= 10,
            "PTY minimum regressed: {visible:?}"
        );

        app.session_mut(runtime).unwrap().kill();
        git(
            &repo,
            &["worktree", "remove", "--", worktree_path.to_str().unwrap()],
        );
        std::fs::remove_dir_all(root).unwrap();
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
        assert!(app.repository_state.checkouts[0].active_intent());
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
        assert!(app.repository_state.checkouts[0].active_intent());

        let state_file = baude_core::workspace::active().state_file("state");
        let persisted = baude_core::persist::load_current_at(&state_root, &state_file).unwrap();
        assert_eq!(persisted.state, app.repository_state);

        let mut restarted = App::new(repo.clone());
        restarted.remote = None;
        restarted.persistence_root_for_test = Some(state_root.clone());
        restarted.spawn_error_for_test = Some("pty unavailable after restart".into());
        restarted.restore();

        assert_eq!(restarted.repository_state, persisted.state);
        assert!(restarted.repository_state.checkouts[0].active_intent());
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
        assert!(app.repository_state.checkouts[0].active_intent());
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
        app.drive_lifecycle_effect(
            checkout,
            baude_core::lifecycle::LifecycleEvent::RequestClose,
            |_, _| Ok(()),
        )
        .unwrap();
        app.drive_lifecycle_effect(
            checkout,
            baude_core::lifecycle::LifecycleEvent::RuntimeExtinct,
            |_, _| Ok(()),
        )
        .unwrap();
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
        assert!(recovered.active_intent());
        assert_eq!(recovered.health(), &CheckoutHealth::Available);
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
                app.repository_state.checkouts[0].active_intent(),
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
        assert!(app.repository_state.checkouts[0].active_intent());
        assert!(matches!(
            app.repository_state.checkouts[0].health(),
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
        assert!(!restarted.repository_state.checkouts[0].active_intent());
        assert_eq!(
            restarted.repository_state.checkouts[0].health(),
            &CheckoutHealth::Available
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
        assert!(!retained.active_intent());
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
            assert_eq!(
                app.repository_state.checkouts[0].active_intent(),
                !committed
            );
            if committed {
                assert!(!pid_is_live(original_pid));
                assert!(!pid_is_live(original_shell_pid));
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
                assert!(!close_error.is_empty());
                assert_eq!(app.repository_state.repositories, before.repositories);
                assert_eq!(
                    app.repository_state.checkouts[0].observed_path,
                    before.checkouts[0].observed_path
                );
                assert!(matches!(
                    app.repository_state.checkouts[0].lifecycle(),
                    CheckoutLifecycle::Running(_)
                ));
                assert_eq!(
                    app.repository_state.checkouts[0]
                        .session
                        .resume_id
                        .as_deref(),
                    Some(format!("resume-{label}").as_str())
                );
                assert_eq!(app.runtime_checkouts.get(&checkout), Some(&runtime));
                assert!(pid_is_live(original_pid));
                assert!(pid_is_live(original_shell_pid));
                let restored = app.session(runtime).unwrap();
                assert!(restored.shell_open);
                let restored_shell = restored.shell.as_ref().unwrap();
                assert!(!restored_shell.is_exited());
                app.session_mut(runtime).unwrap().kill();
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
        assert!(app.repository_state.checkouts[0].active_intent());
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
        assert!(!app.repository_state.checkouts[0].active_intent());
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
        assert_runtime_replaced_preserving_checkout(&app.repository_state, &before);
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
            assert!(
                error.to_string().contains("durably revoke")
                    || error.to_string().contains("runtime stop failed")
            );
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
                replacement_committed.then_some(resume_id.as_str())
            );
            assert!(persisted.checkouts[0].managed_by_baude);
            assert_eq!(persisted.checkouts[0].health(), &CheckoutHealth::Available);
            app.atomic_failure_for_test = None;
            app.save();
            let prepared = app.prepare_remove_worktree(checkout);
            assert!(prepared.is_ok(), "got: {prepared:?}");
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
        assert_runtime_replaced_preserving_checkout(&app.repository_state, &before);
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
        assert!(app.repository_state.checkouts[0].active_intent());
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
                app.repository_state.checkouts[0].health(),
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
        app.selected_id = Some(super::SelId::Checkout(checkout));
        app.handle_sidebar_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(app.modal, Modal::ConfirmCloseWorktree { id } if id == runtime));

        // Close confirmation cannot be promoted into destructive removal.
        app.handle_modal_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(matches!(app.modal, Modal::ConfirmCloseWorktree { id } if id == runtime));
        app.handle_modal_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(app.modal, Modal::None));
        assert_eq!(app.repository_state, before);
        assert_eq!(app.runtime_checkouts, HashMap::from([(checkout, runtime)]));

        app.handle_sidebar_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT));

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
        app.handle_sidebar_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT));
        assert!(matches!(app.modal, Modal::None));
        assert_eq!(
            app.message.as_ref().unwrap().0,
            "Cannot remove “repo:feature/remove-confirmation”: dirty tracked or untracked files are present. Commit, move, or clean those files yourself, then press X to run a new safety check; nothing was removed."
        );
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
