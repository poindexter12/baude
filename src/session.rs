use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::Result;

use crate::meta::{now_unix_ms, ClaudeMeta};
use crate::pty::{now_ms, Pty};

/// Output silence longer than this means Claude is waiting on the user.
/// While working, Claude Code streams spinner/progress output continuously.
const BUSY_WINDOW_MS: u64 = 2000;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Status {
    /// Idle and (presumably) waiting for user input.
    Waiting,
    /// Producing output — Claude is thinking/working.
    Busy,
    /// The claude process has exited.
    Exited,
}

pub struct Session {
    pub id: u64,
    pub name: String,
    pub cwd: PathBuf,
    pub repo_root: PathBuf,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub claude: Pty,
    pub shell: Option<Pty>,
    pub shell_open: bool,
    pub spawn_unix_ms: u64,
    pub meta: ClaudeMeta,
}

impl Session {
    pub fn status(&self) -> Status {
        if self.claude.is_exited() {
            return Status::Exited;
        }
        // Claude's own session file is authoritative when we found it;
        // otherwise fall back to the output-silence heuristic.
        if let Some((busy, _)) = self.meta.claude_status {
            return if busy { Status::Busy } else { Status::Waiting };
        }
        let last = self.claude.last_output_ms.load(Ordering::Relaxed);
        if now_ms().saturating_sub(last) < BUSY_WINDOW_MS {
            Status::Busy
        } else {
            Status::Waiting
        }
    }

    /// How long this session has been waiting for input, in ms.
    pub fn waiting_for_ms(&self) -> u64 {
        if let Some((false, since)) = self.meta.claude_status {
            return now_unix_ms().saturating_sub(since);
        }
        now_ms().saturating_sub(self.claude.last_output_ms.load(Ordering::Relaxed))
    }

    pub fn poll_meta(&mut self) {
        if self.claude.is_exited() {
            return;
        }
        let pid = self.claude.pid();
        let (cwd, spawn, root) = (self.cwd.clone(), self.spawn_unix_ms, self.repo_root.clone());
        self.meta.poll(&cwd, pid, spawn, &root);
    }

    pub fn open_shell(&mut self, rows: u16, cols: u16) -> Result<()> {
        let needs_spawn = match &self.shell {
            None => true,
            Some(p) => p.is_exited(),
        };
        if needs_spawn {
            self.shell = Some(Pty::spawn(None, &self.cwd, rows, cols)?);
        }
        self.shell_open = true;
        Ok(())
    }

    pub fn kill(&mut self) {
        self.claude.kill();
        if let Some(shell) = &mut self.shell {
            shell.kill();
        }
    }
}

pub fn human_duration(ms: u64) -> String {
    let secs = ms / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}
