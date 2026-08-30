use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Result;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use baude_core::backend;
use baude_core::git;
use baude_core::meta::{now_unix_ms, ClaudeMeta, RateWindow};
use baude_core::persist::{self, Config, SavedSession, State};
use baude_core::pty::{now_ms, Pty};
use baude_core::session::{Session, Status};

use crate::keys::encode_key;
use crate::notify_desktop::{self, DesktopNotifier, Row};
use crate::remote::{RemoteAttach, RemoteInfo, RemotePoller, RemoteSnapshot};
use crate::usage::{UsageCosts, UsagePoller};

const MESSAGE_TTL_MS: u64 = 5000;
const META_POLL_MS: u64 = 1000;

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

/// Join renderer-inserted line breaks at the terminal edge. vt100 already
/// omits genuine terminal soft wraps, but TUIs may hard-wrap text one cell
/// before the edge and indent the continuation.
fn unwrap_visual_linebreaks(text: &str, start_col: u16, cols: u16) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut output = String::with_capacity(text.len());
    let mut trim_continuation = false;

    for (index, line) in lines.iter().enumerate() {
        let line = if trim_continuation {
            line.trim_start()
        } else {
            line
        };
        output.push_str(line);

        if index + 1 == lines.len() {
            break;
        }

        let available = if index == 0 {
            cols.saturating_sub(start_col)
        } else {
            cols
        };
        trim_continuation =
            available > 0 && line.chars().count() >= usize::from(available.saturating_sub(1));
        if !trim_continuation {
            output.push('\n');
        }
    }

    output
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
        }
    }

    /// Cached today/week costs from the ccusage background poller.
    pub fn usage_costs(&self) -> UsageCosts {
        self.usage.costs()
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
        let state = persist::load();
        for saved in &state.sessions {
            if !saved.cwd.exists() {
                continue;
            }
            match self.add_session(
                saved.cwd.clone(),
                Some(saved.repo_root.clone()),
                saved.branch.clone(),
                saved.is_worktree,
                true,
                saved.shell_open,
            ) {
                Ok(id) => {
                    if saved.archived {
                        if let Some(s) = self.session_mut(id) {
                            s.archived = true;
                            s.archived_by_user = saved.archived_by_user;
                        }
                    }
                }
                Err(e) => self.set_message(format!("restore {}: {e}", saved.name)),
            }
        }
        // Premise: baude is started from a repo folder. Auto-add it if new.
        let launch = self.launch_dir.clone();
        let already = self.sessions.iter().any(|s| s.cwd == launch);
        if !already && git::repo_root(&launch).is_some() {
            if let Err(e) = self.add_session(launch, None, None, false, false, false) {
                self.set_message(format!("start session: {e}"));
            }
        }
        self.selected_id = self.ordered_ids().first().copied();
        self.save();
    }

    pub fn save(&self) {
        let state = State {
            sessions: self
                .sessions
                .iter()
                .map(|s| SavedSession {
                    name: s.name.clone(),
                    cwd: s.cwd.clone(),
                    repo_root: s.repo_root.clone(),
                    branch: s.branch.clone(),
                    is_worktree: s.is_worktree,
                    shell_open: s.shell_open,
                    archived: s.archived,
                    archived_by_user: s.archived_by_user,
                })
                .collect(),
        };
        let _ = persist::save(&state);
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
        let plan = be.spawn_plan(&base, None, resume);

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
        let claude = Pty::spawn(Some(&plan.cmd), &cwd, rows, cols)?;
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

    fn remove_session(&mut self, id: u64) {
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
                        self.remove_session(id);
                        self.set_message("session closed — worktree kept".into());
                    }
                    KeyCode::Char('r') => {
                        self.modal = Modal::None;
                        let info = self
                            .session(id)
                            .map(|s| (s.repo_root.clone(), s.cwd.clone()));
                        self.remove_session(id);
                        if let Some((repo, wt)) = info {
                            if git::is_dirty(&wt) {
                                self.set_message(
                                    "worktree has uncommitted changes — kept on disk".into(),
                                );
                            } else {
                                match git::remove_worktree(&repo, &wt) {
                                    Ok(()) => self.set_message("worktree removed".into()),
                                    Err(e) => self.set_message(format!("worktree kept: {e}")),
                                }
                            }
                        }
                    }
                    KeyCode::Esc => self.modal = Modal::None,
                    _ => {}
                }
            }
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
                match git::create_worktree(&repo_root, &value) {
                    Ok(dir) => match self.add_session(
                        dir,
                        Some(repo_root),
                        Some(value),
                        true,
                        false,
                        false,
                    ) {
                        Ok(_) => {
                            self.focus = Focus::Claude;
                            self.save();
                        }
                        Err(e) => self.set_message(format!("spawn failed: {e}")),
                    },
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
        if let Some(remote) = &self.remote {
            match remote.create(&path.to_string_lossy(), None, None) {
                Ok(()) => self.set_message("session queued on daemon".into()),
                Err(e) => self.set_message(format!("daemon: {e}")),
            }
            return;
        }
        match self.add_session(path, None, None, false, false, false) {
            Ok(_) => {
                self.focus = Focus::Claude;
                self.save();
            }
            Err(e) => self.set_message(format!("spawn failed: {e}")),
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
                    self.open_repo_session(pc.dest);
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
        let (rows, cols) = {
            let Some(s) = self.session(id) else { return };
            if !s.claude.is_exited() {
                self.set_message("claude is still running".into());
                return;
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
        let plan = be.spawn_plan(&base, None, false);
        be.prepare_cwd(&cwd);
        match Pty::spawn(Some(&plan.cmd), &cwd, rows, cols) {
            Ok(pty) => {
                if let Some(s) = self.session_mut(id) {
                    s.claude = pty;
                    s.spawn_unix_ms = now_unix_ms();
                    s.meta = ClaudeMeta::default();
                    s.meta.backend_port = plan.server_port;
                }
                self.focus = Focus::Claude;
            }
            Err(e) => self.set_message(format!("restart failed: {e}")),
        }
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
                            let text = screen.contents_between(sr, sc, er, ec + 1);
                            let text = unwrap_visual_linebreaks(&text, sc, screen.size().1);
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
    use super::unwrap_visual_linebreaks;

    #[test]
    fn joins_hard_breaks_that_fill_terminal_width() {
        assert_eq!(
            unwrap_visual_linebreaks("123456789\n  abcdefghi\n  tail", 0, 10),
            "123456789abcdefghitail"
        );
    }

    #[test]
    fn preserves_genuine_multiline_text() {
        assert_eq!(
            unwrap_visual_linebreaks("short line\nsecond line", 0, 80),
            "short line\nsecond line"
        );
    }

    #[test]
    fn accounts_for_selection_start_column() {
        assert_eq!(
            unwrap_visual_linebreaks("1234567\n  continued", 12, 20),
            "1234567continued"
        );
    }
}
