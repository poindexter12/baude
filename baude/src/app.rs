use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Result;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use baude_core::git;
use baude_core::meta::{now_unix_ms, ClaudeMeta, RateWindow};
use baude_core::persist::{self, Config, SavedSession, State};
use baude_core::pty::{now_ms, Pty};
use baude_core::session::{Session, Status};

use crate::keys::encode_key;
use crate::remote::{RemoteAttach, RemoteInfo, RemotePoller, RemoteSnapshot};
use crate::usage::{UsageCosts, UsagePoller};

const MESSAGE_TTL_MS: u64 = 5000;
const META_POLL_MS: u64 = 1000;

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
    NewWorktreeBranch { repo_root: PathBuf },
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

pub struct App {
    pub sessions: Vec<Session>,
    pub selected_id: Option<SelId>,
    pub focus: Focus,
    pub modal: Modal,
    pub message: Option<(String, u64)>,
    pub should_quit: bool,
    pub launch_dir: PathBuf,
    config: Config,
    content_rect: Rect,
    next_id: u64,
    last_meta_poll: u64,
    usage: UsagePoller,
    /// Remote daemon client (config `daemon_url` / BAUDE_DAEMON_URL).
    pub remote: Option<RemotePoller>,
    pub remote_snap: RemoteSnapshot,
    /// At most one live raw attach to a remote session.
    pub attach: Option<RemoteAttach>,
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

/// Best-effort, non-clobbering seed of a session cwd's `.mcp.json` registering
/// baude's `permission-mcp` stdio server (PERM-01, `prompt` mode only).
///
/// The MCP command is `current_exe()` + ` permission-mcp` (same resolution as
/// `baude_core::hook::baude_hook_command`). Mirrors `seed_settings`: never
/// aborts a spawn on failure, and re-seeding merges `mcpServers.baude` into an
/// existing file via the pure `merge_mcp_config` without discarding a user's
/// sibling MCP servers (idempotent).
fn seed_mcp_config(cwd: &Path) {
    let exe = match std::env::current_exe() {
        Ok(p) => p.display().to_string(),
        Err(_) => return, // can't resolve the bridge command — best-effort skip.
    };
    let path = baude_core::permission::mcp_config_path(cwd);
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let merged = baude_core::permission::merge_mcp_config(&existing, &exe);
    let _ = std::fs::write(&path, merged.to_string());
}

impl App {
    pub fn new(launch_dir: PathBuf) -> App {
        let config = persist::load_config();
        let remote = std::env::var("BAUDE_DAEMON_URL")
            .ok()
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
            config,
            content_rect: Rect::new(0, 0, 80, 24),
            next_id: 1,
            last_meta_poll: 0,
            usage: UsagePoller::start(),
            remote,
            remote_snap: RemoteSnapshot::default(),
            attach: None,
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

    /// (waiting, busy) session counts for the status bar.
    pub fn status_counts(&self) -> (usize, usize) {
        let mut waiting = 0;
        let mut busy = 0;
        for s in &self.sessions {
            if s.archived {
                continue;
            }
            match s.status() {
                Status::Waiting => waiting += 1,
                Status::Busy => busy += 1,
                Status::Exited => {}
            }
        }
        (waiting, busy)
    }

    /// The command run for each session: BAUDE_CLAUDE_CMD env, then
    /// config.json `claude_cmd`, then plain `claude`.
    fn claude_cmd(&self) -> String {
        std::env::var("BAUDE_CLAUDE_CMD")
            .ok()
            .or_else(|| self.config.claude_cmd.clone())
            .unwrap_or_else(|| "claude".to_string())
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

    /// Selection order: local sessions (stable creation order — the sidebar
    /// never reorders, sessions that need input flash in place instead),
    /// followed by the remote daemon's sessions.
    pub fn ordered_ids(&self) -> Vec<SelId> {
        let all = || {
            self.sessions
                .iter()
                .map(|s| (SelId::Local(s.id), s.archived))
                .chain(
                    self.remote_snap
                        .sessions
                        .iter()
                        .map(|r| (SelId::Remote(r.id), r.archived)),
                )
        };
        // Active sessions keep their stable order; archived sink to the end.
        all()
            .filter(|(_, a)| !a)
            .chain(all().filter(|(_, a)| *a))
            .map(|(id, _)| id)
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

        // `claude --continue` resumes the most recent conversation in this
        // directory; falls back to a fresh session if there is none.
        // PERM-01: append exactly one permission flag to the base cmd (default
        // skip preserves today's `--dangerously-skip-permissions`; `prompt` is
        // opt-in via BAUDE_PERMISSION_MODE). The flag rides on the base cmd so
        // it survives the `--continue || exec` resume fallback. No-double-add
        // when the operator already set a permission flag.
        let base = format!(
            "{0}{1}",
            self.claude_cmd(),
            baude_core::permission::permission_flag(&self.claude_cmd())
        );
        let cmd = if resume {
            format!("{base} --continue 2>/dev/null || exec {base}")
        } else {
            format!("exec {base}")
        };
        // Seed baude's hooks into the session cwd's .claude/settings.local.json
        // before claude starts, so Claude Code actually invokes `baude hook`.
        // Best-effort: a seeding failure must NOT abort the spawn — the session
        // simply falls back to the silence path (no regression). TUI sessions
        // get NO $BAUDE_EVENT_URL, which routes the hook to the /tmp append
        // path (only the daemon injects that var).
        baude_core::hook::seed_settings(&cwd);

        // In `prompt` mode only, additionally seed a non-clobbering `.mcp.json`
        // registering the `permission-mcp` stdio server (command =
        // current_exe() + " permission-mcp"). Best-effort, mirrors the hook
        // seed; 04-02 adds the `permission-mcp` arm to both binaries.
        if baude_core::permission::is_prompt_mode() {
            seed_mcp_config(&cwd);
        }

        let (rows, cols) = self.claude_spawn_size(shell_open);
        let claude = Pty::spawn(Some(&cmd), &cwd, rows, cols)?;

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
            meta: ClaudeMeta::default(),
            archived: false,
            archived_by_user: false,
            was_busy: false,
            unarchived_at_ms: None,
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

    pub fn tick(&mut self) {
        if let Some((_, expiry)) = &self.message {
            if now_ms() > *expiry {
                self.message = None;
            }
        }
        if now_ms().saturating_sub(self.last_meta_poll) >= META_POLL_MS {
            self.last_meta_poll = now_ms();
            let mut changed = false;
            for s in &mut self.sessions {
                s.poll_meta();
                changed |= s.auto_archive_tick(baude_core::session::AUTO_ARCHIVE_IDLE_MS);
            }
            if changed {
                self.save();
            }
        }
        if let Some(r) = &self.remote {
            self.remote_snap = r.snapshot();
        }
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
    pub fn sync_sizes(&mut self, content: Rect) {
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
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
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

        match self.focus {
            Focus::Sidebar => self.handle_sidebar_key(key),
            Focus::Claude => self.forward_key(key, false),
            Focus::Shell => self.forward_key(key, true),
        }
    }

    fn forward_key(&mut self, key: KeyEvent, to_shell: bool) {
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
            KeyCode::Char('e') if remote_selected => {
                self.set_message("remote session — folder lives on the daemon host".into());
            }
            KeyCode::Char('e') => self.open_editor(),
            KeyCode::Char('n') => {
                let buf = match &self.config.new_session_dir {
                    Some(d) => {
                        let d = d.trim_end_matches('/');
                        format!("{d}/")
                    }
                    None => format!("{}", self.launch_dir.display()),
                };
                self.modal = Modal::Input {
                    kind: InputKind::NewSessionPath,
                    title: "new session — repo path (tab completes)".into(),
                    buf,
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
            KeyCode::Char('x') => {
                if remote_selected {
                    if let Some(SelId::Remote(id)) = self.selected_id {
                        self.modal = Modal::ConfirmKill {
                            id: SelId::Remote(id),
                        };
                    }
                } else if let Some(s) = self.selected() {
                    self.modal = if s.is_worktree {
                        Modal::ConfirmCloseWorktree { id: s.id }
                    } else {
                        Modal::ConfirmKill {
                            id: SelId::Local(s.id),
                        }
                    };
                }
            }
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
                KeyCode::Tab => {
                    if matches!(kind, InputKind::NewSessionPath) {
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
                if !expanded.is_dir() {
                    self.set_message(format!("not a directory: {}", expanded.display()));
                    return;
                }
                match self.add_session(expanded, None, None, false, false, false) {
                    Ok(_) => {
                        self.focus = Focus::Claude;
                        self.save();
                    }
                    Err(e) => self.set_message(format!("spawn failed: {e}")),
                }
            }
            InputKind::NewWorktreeBranch { repo_root } => {
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
        self.selected_id = Some(ids[next]);
    }

    /// Cycle the selection to the next/prev session in sidebar order,
    /// wrapping around. When attached, stays attached to the same kind of
    /// pane — falling back to the claude pane if the new session has no shell.
    fn cycle_session(&mut self, delta: i64) {
        // Cycling skips the archive — j/k still reaches it.
        let ids: Vec<SelId> = self
            .ordered_ids()
            .into_iter()
            .filter(|&id| !self.is_archived(id))
            .collect();
        if ids.is_empty() {
            return;
        }
        let len = ids.len() as i64;
        let cur = self
            .selected_id
            .and_then(|id| ids.iter().position(|&x| x == id))
            .unwrap_or(0) as i64;
        let next = (((cur + delta) % len) + len) % len;
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
        let cmd = format!("exec {}", self.claude_cmd());
        match Pty::spawn(Some(&cmd), &cwd, rows, cols) {
            Ok(pty) => {
                if let Some(s) = self.session_mut(id) {
                    s.claude = pty;
                    s.spawn_unix_ms = now_unix_ms();
                    s.meta = ClaudeMeta::default();
                }
                self.focus = Focus::Claude;
            }
            Err(e) => self.set_message(format!("restart failed: {e}")),
        }
    }
}
