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
    /// false in tests — never touch the real daemon-state.json.
    persist: bool,
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

/// The command run per session: BAUDE_CLAUDE_CMD env, then config.json
/// `claude_cmd`, then plain `claude`.
pub fn default_claude_cmd() -> String {
    std::env::var("BAUDE_CLAUDE_CMD")
        .ok()
        .or_else(|| persist::load_config().claude_cmd)
        .unwrap_or_else(|| "claude".to_string())
}

impl Manager {
    pub fn new(claude_cmd: String, persist: bool) -> Manager {
        Manager {
            sessions: Vec::new(),
            next_id: 1,
            claude_cmd,
            persist,
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
        if !self.persist {
            return;
        }
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
        // Input written before Claude's TUI is up gets drained, not queued.
        if s.meta.session_id.is_none() {
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
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn mgr() -> Manager {
        Manager::new("sleep 30".into(), false)
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
        let mut m = Manager::new("bash --norc -i".into(), false);
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
    fn key_encoding() {
        assert_eq!(key_bytes("up", false), b"\x1b[A");
        assert_eq!(key_bytes("up", true), b"\x1bOA");
        assert_eq!(key_bytes("enter", false), b"\r");
        assert_eq!(key_bytes("shift+tab", false), b"\x1b[Z");
        assert_eq!(key_bytes("ctrl+c", false), vec![3]);
        assert_eq!(key_bytes("plain text", false), b"plain text");
    }
}
