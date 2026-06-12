//! Session ownership for the daemon. Unlike the TUI, the daemon never kills
//! sessions when a client goes away — only on explicit DELETE or daemon
//! shutdown. State persists to its own file (`daemon-state.json`) so a daemon
//! restart restores every session via `claude --continue`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use baude_core::git;
use baude_core::meta::{now_unix_ms, ClaudeMeta};
use baude_core::persist::{self, SavedSession, State};
use baude_core::pty::Pty;
use baude_core::session::{Session, Status};

/// Headless PTY geometry. Nothing renders it; it only needs to be big enough
/// that Claude Code's TUI lays out sanely in the transcript-driving sense.
const ROWS: u16 = 40;
const COLS: u16 = 120;

const STATE_FILE: &str = "daemon-state.json";

pub type Shared = Arc<Mutex<Manager>>;

/// Lock the manager, recovering from poisoning (a panicked handler must not
/// take the whole daemon's session list with it).
pub fn lock(shared: &Shared) -> MutexGuard<'_, Manager> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct Manager {
    sessions: Vec<Session>,
    next_id: u64,
    claude_cmd: String,
}

/// One row of `GET /sessions`.
#[derive(Serialize, Clone)]
pub struct SessionInfo {
    pub id: u64,
    pub name: String,
    pub title: Option<String>,
    pub status: &'static str,
    /// Only present while waiting — how long Claude has been blocked on us.
    pub waiting_for_ms: Option<u64>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub context_used_pct: Option<u8>,
    pub branch: Option<String>,
    pub cwd: String,
    pub repo_root: String,
    pub is_worktree: bool,
    pub gsd_milestone: Option<String>,
    pub gsd_phase: Option<String>,
    pub session_cost_usd: Option<f64>,
    pub claude_session_id: Option<String>,
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
        Status::Busy => "busy",
        Status::Exited => "exited",
    }
}

impl Manager {
    pub fn new() -> Manager {
        let claude_cmd = std::env::var("BAUDE_CLAUDE_CMD")
            .ok()
            .or_else(|| persist::load_config().claude_cmd)
            .unwrap_or_else(|| "claude".to_string());
        Manager {
            sessions: Vec::new(),
            next_id: 1,
            claude_cmd,
        }
    }

    /// Respawn every saved session with `claude --continue`. Returns how many
    /// came back.
    pub fn restore(&mut self) -> usize {
        let state = persist::load_named(STATE_FILE);
        let mut restored = 0;
        for saved in &state.sessions {
            if !saved.cwd.exists() {
                continue;
            }
            match self.spawn(
                saved.cwd.clone(),
                saved.repo_root.clone(),
                saved.branch.clone(),
                saved.is_worktree,
                Some(&saved.name),
                true,
            ) {
                Ok(_) => restored += 1,
                Err(e) => eprintln!("restore {}: {e}", saved.name),
            }
        }
        self.save();
        restored
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
                    shell_open: false,
                })
                .collect(),
        };
        if let Err(e) = persist::save_named(STATE_FILE, &state) {
            eprintln!("save state: {e}");
        }
    }

    /// `POST /sessions` — spawn a fresh session in `repo`, optionally in a
    /// managed worktree for `worktree` (branch name).
    pub fn create(
        &mut self,
        repo: &str,
        worktree: Option<&str>,
        name: Option<&str>,
    ) -> Result<SessionInfo> {
        let repo = expand_tilde(repo);
        let repo = repo.canonicalize().unwrap_or(repo);
        if !repo.is_dir() {
            bail!("not a directory: {}", repo.display());
        }
        let (cwd, repo_root, branch, is_worktree) = match worktree {
            Some(branch) => {
                let root = git::repo_root(&repo)
                    .ok_or_else(|| anyhow!("not a git repo: {}", repo.display()))?;
                let dir = git::create_worktree(&root, branch)?;
                (dir, root, Some(branch.to_string()), true)
            }
            None => {
                let root = git::repo_root(&repo).unwrap_or_else(|| repo.clone());
                (repo, root, None, false)
            }
        };
        let id = self.spawn(cwd, repo_root, branch, is_worktree, name, false)?;
        self.save();
        Ok(self.info(id).expect("session just spawned"))
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

        // `claude --continue` resumes the most recent conversation in this
        // directory; falls back to a fresh session if there is none.
        let base_cmd = &self.claude_cmd;
        let cmd = if resume {
            format!("{base_cmd} --continue 2>/dev/null || exec {base_cmd}")
        } else {
            format!("exec {base_cmd}")
        };
        let claude = Pty::spawn(Some(&cmd), &cwd, ROWS, COLS)?;

        let id = self.next_id;
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
            meta: ClaudeMeta::default(),
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

    pub fn remove(&mut self, id: u64) -> Result<()> {
        let s = self.session_mut(id)?;
        s.kill();
        self.sessions.retain(|s| s.id != id);
        self.save();
        Ok(())
    }

    /// Inject a message into the session's PTY. Multiline-safe via bracketed
    /// paste; the trailing CR submits. If Claude is busy it queues the message
    /// natively (visible as `queue-operation` transcript records).
    pub fn post_message(&mut self, id: u64, text: &str) -> Result<()> {
        let s = self.session_mut(id)?;
        if s.claude.is_exited() {
            bail!("claude has exited");
        }
        let bracketed = s
            .claude
            .parser
            .lock()
            .map(|p| p.screen().bracketed_paste())
            .unwrap_or(false);
        let mut bytes = Vec::with_capacity(text.len() + 13);
        if bracketed {
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend_from_slice(text.as_bytes());
            bytes.extend_from_slice(b"\x1b[201~");
        } else {
            bytes.extend_from_slice(text.as_bytes());
        }
        bytes.extend_from_slice(b"\r");
        s.claude.write_input(&bytes);
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

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions.iter().map(session_info).collect()
    }

    pub fn info(&self, id: u64) -> Option<SessionInfo> {
        self.sessions.iter().find(|s| s.id == id).map(session_info)
    }

    pub fn poll(&mut self) {
        for s in &mut self.sessions {
            s.poll_meta();
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

fn session_info(s: &Session) -> SessionInfo {
    let status = s.status();
    SessionInfo {
        id: s.id,
        name: s.name.clone(),
        title: s.meta.title.clone(),
        status: status_str(status),
        waiting_for_ms: (status == Status::Waiting).then(|| s.waiting_for_ms()),
        model: s.meta.model.clone(),
        permission_mode: s.meta.permission_mode.clone(),
        context_used_pct: s.meta.context_used_pct,
        branch: s.meta.git_branch.clone().or_else(|| s.branch.clone()),
        cwd: s.cwd.display().to_string(),
        repo_root: s.repo_root.display().to_string(),
        is_worktree: s.is_worktree,
        gsd_milestone: s.meta.gsd.as_ref().and_then(|g| g.milestone.clone()),
        gsd_phase: s.meta.gsd.as_ref().and_then(|g| g.phase_line.clone()),
        session_cost_usd: s.meta.session_cost_usd,
        claude_session_id: s.meta.session_id.clone(),
    }
}
