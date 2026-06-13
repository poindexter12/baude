use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::Result;

use crate::meta::{now_unix_ms, ClaudeMeta};
use crate::pty::{now_ms, Pty};

/// Output silence longer than this means Claude is waiting on the user.
/// While working, Claude Code streams spinner/progress output continuously.
const BUSY_WINDOW_MS: u64 = 2000;

/// Waiting this long unattended auto-archives a session: it sinks to the
/// bottom of lists and stops demanding attention until it's active again.
pub const AUTO_ARCHIVE_IDLE_MS: u64 = 30 * 60 * 1000;

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
    /// Parked: sorts last, excluded from cycling/counters/notifications.
    /// Set manually or after `AUTO_ARCHIVE_IDLE_MS` of waiting.
    pub archived: bool,
    /// A manual archive sticks until unarchived or re-engaged (input sent);
    /// an automatic one also lifts when a new turn starts.
    pub archived_by_user: bool,
    /// Busy state at the previous archive tick — auto-unarchiving triggers
    /// on the *edge* into busy (fresh activity), not on busy level.
    pub was_busy: bool,
    /// Monotonic ms of the last manual unarchive. The waiting clock keeps
    /// running across an unarchive, so without a fresh grace period the very
    /// next tick would re-park a still-long-waiting session.
    pub unarchived_at_ms: Option<u64>,
}

impl Session {
    /// Apply the auto-archive rules; returns true when the flag flipped.
    pub fn auto_archive_tick(&mut self, idle_ms: u64) -> bool {
        let status = self.status();
        let busy_now = status == Status::Busy;
        let was_busy = std::mem::replace(&mut self.was_busy, busy_now);
        if idle_ms == 0 {
            return false;
        }
        match status {
            Status::Waiting
                if !self.archived
                    && self.waiting_for_ms() >= idle_ms
                    && self
                        .unarchived_at_ms
                        .is_none_or(|t| now_ms().saturating_sub(t) >= idle_ms) =>
            {
                self.archived = true;
                self.archived_by_user = false;
                true
            }
            Status::Busy if self.archived && !self.archived_by_user && !was_busy => {
                self.archived = false;
                true
            }
            _ => false,
        }
    }

    /// Park or unpark by explicit user action. Unparking grants a fresh
    /// idle grace period so `auto_archive_tick` can't immediately undo it.
    pub fn set_archived(&mut self, archived: bool) {
        self.archived = archived;
        self.archived_by_user = archived;
        if !archived {
            self.unarchived_at_ms = Some(now_ms());
        }
    }

    /// Input headed into the session = re-engagement; lift any archive.
    /// Returns true when the flag flipped.
    pub fn unarchive_on_input(&mut self) -> bool {
        if self.archived {
            self.archived = false;
            self.archived_by_user = false;
            self.unarchived_at_ms = Some(now_ms());
            true
        } else {
            false
        }
    }
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
