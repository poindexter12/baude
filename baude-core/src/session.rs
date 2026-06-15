use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::Result;

use crate::meta::{now_unix_ms, ClaudeMeta};
use crate::pty::{now_ms, Pty};

/// Output silence longer than this means Claude is waiting on the user.
/// While working, Claude Code streams spinner/progress output continuously.
const BUSY_WINDOW_MS: u64 = 2000;

/// A hook event older than this no longer wins precedence — state falls
/// through to the session-file / silence sources. A few seconds is enough to
/// keep a fresh UserPromptSubmit/Stop authoritative against the polled
/// session file without pinning a long-dead event. Tunable; a wrong value
/// only causes brief mislabel, never a crash (research A3 / Pitfall 5).
const HOOK_FRESH_MS: u64 = 5000;

/// Which source decided a session's [`Status`]. Lets the silence fallback be
/// observably "fallback" so a regression to silence-only is visible, not
/// silent. Precedence: `Hook` (fresh) > `SessionFile` > `Silence`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StateSource {
    /// A fresh Claude Code hook event drove the state.
    Hook,
    /// Claude's own session file (`sessions/<pid>.json`) drove the state.
    SessionFile,
    /// The PTY-output-silence heuristic drove the state (the v0.6.1 fallback).
    Silence,
}

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

/// Pure precedence decision shared by [`Session::status`] /
/// [`Session::status_with_source`]. Kept side-effect-free so the precedence
/// tiers are unit-tested without constructing a live `Pty`/`Session`.
///
/// Precedence: exited > fresh hook > session file > silence. The
/// `claude_status` and silence branches are byte-identical to the v0.6.1
/// logic — only the fresh-hook branch is prepended (no-regression, Pitfall 3).
fn decide_status(
    exited: bool,
    hook_status: Option<(bool, u64)>,
    now_unix: u64,
    claude_status: Option<(bool, u64)>,
    last_output_ms: u64,
    now_mono: u64,
) -> (Status, StateSource) {
    // Decide the live (status, source) from the precedence tiers first, then
    // let an exited process override only the Status — never the source. An
    // exited session that never saw a hook must NOT be mislabeled `Hook`; the
    // honest underlying source (SessionFile/Silence) is what the observability
    // field wants to surface (WR-05).
    let (mut st, src) = decide_live(
        hook_status,
        now_unix,
        claude_status,
        last_output_ms,
        now_mono,
    );
    if exited {
        st = Status::Exited;
    }
    (st, src)
}

/// The live (not-exited) precedence decision. Split out of [`decide_status`]
/// so the exited path can override only the `Status` while preserving the
/// honest source label (WR-05): an exited session reports the source that
/// actually decided it, not a fabricated `Hook`.
fn decide_live(
    hook_status: Option<(bool, u64)>,
    now_unix: u64,
    claude_status: Option<(bool, u64)>,
    last_output_ms: u64,
    now_mono: u64,
) -> (Status, StateSource) {
    // Fresh hook event is authoritative — event-driven and the moment-accurate
    // signal. Stale events fall through to the existing sources.
    if let Some((busy, at)) = hook_status {
        if now_unix.saturating_sub(at) < HOOK_FRESH_MS {
            let s = if busy { Status::Busy } else { Status::Waiting };
            return (s, StateSource::Hook);
        }
    }
    // --- below: v0.6.1 logic, byte-identical (prepend-only edit) ---
    // Claude's own session file is authoritative when we found it;
    // otherwise fall back to the output-silence heuristic.
    if let Some((busy, _)) = claude_status {
        let s = if busy { Status::Busy } else { Status::Waiting };
        return (s, StateSource::SessionFile);
    }
    let s = if now_mono.saturating_sub(last_output_ms) < BUSY_WINDOW_MS {
        Status::Busy
    } else {
        Status::Waiting
    };
    (s, StateSource::Silence)
}

impl Session {
    pub fn status(&self) -> Status {
        self.status_with_source().0
    }

    /// Like [`status`](Self::status) but also reports which [`StateSource`]
    /// decided the result. `status()` delegates to `.0` so the public,
    /// total `Status` API and all call sites stay unchanged.
    pub fn status_with_source(&self) -> (Status, StateSource) {
        decide_status(
            self.claude.is_exited(),
            self.meta.hook_status,
            now_unix_ms(),
            self.meta.claude_status,
            self.claude.last_output_ms.load(Ordering::Relaxed),
            now_ms(),
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_duration_buckets() {
        assert_eq!(human_duration(5_000), "5s");
        assert_eq!(human_duration(120_000), "2m");
        assert_eq!(human_duration(3_660_000), "1h1m");
    }

    #[test]
    fn fresh_meta_has_no_status() {
        // Smoke: a default ClaudeMeta carries no Claude-side status, so the
        // precedence tests below start from a known empty baseline.
        let meta = ClaudeMeta::default();
        assert!(meta.claude_status.is_none());
    }

    // --- Precedence tests for decide_status (Hook > SessionFile > Silence) ---
    //
    // decide_status is the pure core of status_with_source: it takes the same
    // raw inputs status_with_source reads off `self` (exited flag, hook_status,
    // now_unix, claude_status, last_output_ms, now_mono) so each precedence
    // tier is asserted without constructing a live Pty/Session.

    const NOW_UNIX: u64 = 1_000_000;
    const NOW_MONO: u64 = 500_000;

    #[test]
    fn fresh_hook_busy_wins() {
        let (st, src) = decide_status(
            false,
            Some((true, NOW_UNIX)), // fresh
            NOW_UNIX,
            Some((false, NOW_UNIX)), // session file disagrees -> hook still wins
            0,
            NOW_MONO,
        );
        assert_eq!((st, src), (Status::Busy, StateSource::Hook));
    }

    #[test]
    fn fresh_hook_waiting_wins() {
        let (st, src) = decide_status(
            false,
            Some((false, NOW_UNIX - 1000)), // within HOOK_FRESH_MS (5000)
            NOW_UNIX,
            None,
            NOW_MONO, // recent output -> silence would say Busy
            NOW_MONO,
        );
        assert_eq!((st, src), (Status::Waiting, StateSource::Hook));
    }

    #[test]
    fn stale_hook_falls_through_to_session_file() {
        let (st, src) = decide_status(
            false,
            Some((true, NOW_UNIX - HOOK_FRESH_MS)), // exactly stale (>= threshold)
            NOW_UNIX,
            Some((false, 0)), // session file -> Waiting
            NOW_MONO,
            NOW_MONO,
        );
        assert_eq!((st, src), (Status::Waiting, StateSource::SessionFile));
    }

    #[test]
    fn no_hook_uses_session_file() {
        let (st, src) = decide_status(false, None, NOW_UNIX, Some((true, 0)), 0, NOW_MONO);
        assert_eq!((st, src), (Status::Busy, StateSource::SessionFile));
    }

    #[test]
    fn silence_recent_output_is_busy() {
        // No hook, no session file, recent output -> Busy via Silence.
        let (st, src) = decide_status(false, None, NOW_UNIX, None, NOW_MONO, NOW_MONO);
        assert_eq!((st, src), (Status::Busy, StateSource::Silence));
    }

    #[test]
    fn silence_old_output_is_waiting() {
        // No hook, no session file, old output -> Waiting via Silence.
        let last = NOW_MONO - BUSY_WINDOW_MS; // exactly at/over the window
        let (st, src) = decide_status(false, None, NOW_UNIX, None, last, NOW_MONO);
        assert_eq!((st, src), (Status::Waiting, StateSource::Silence));
    }

    #[test]
    fn exited_overrides_everything() {
        // Exited overrides the Status, but reports the HONEST underlying source
        // (here a fresh hook drove it) rather than fabricating one (WR-05).
        let (st, src) = decide_status(
            true,
            Some((true, NOW_UNIX)),
            NOW_UNIX,
            Some((true, 0)),
            NOW_MONO,
            NOW_MONO,
        );
        assert_eq!(st, Status::Exited);
        assert_eq!(src, StateSource::Hook);
    }

    #[test]
    fn exited_without_hook_is_not_labeled_hook() {
        // An Exited session that never saw a hook event must NOT report
        // StateSource::Hook — that mislabels the very observability field the
        // source is meant to provide. With no hook and no session file, the
        // honest source is Silence (WR-05).
        let (st, src) = decide_status(true, None, NOW_UNIX, None, 0, NOW_MONO);
        assert_eq!(st, Status::Exited);
        assert_eq!(
            src,
            StateSource::Silence,
            "exited-with-no-hook must carry an honest source, not Hook"
        );

        // And when the session file decided it, exited carries SessionFile.
        let (st, src) = decide_status(true, None, NOW_UNIX, Some((false, 0)), 0, NOW_MONO);
        assert_eq!(st, Status::Exited);
        assert_eq!(src, StateSource::SessionFile);
    }

    #[test]
    fn no_hook_silence_is_byte_identical_to_v0_6_1() {
        // HOOK-02 no-regression: with NO hook events, the (Status, StateSource)
        // result must match the pre-hook dual-source logic exactly, labeled
        // Silence. Recent output -> Busy; old output -> Waiting; and a present
        // session file still wins over silence as before.
        // Recent output, no session file.
        assert_eq!(
            decide_status(false, None, NOW_UNIX, None, NOW_MONO, NOW_MONO),
            (Status::Busy, StateSource::Silence)
        );
        // Old output, no session file.
        assert_eq!(
            decide_status(false, None, NOW_UNIX, None, 0, NOW_MONO),
            (Status::Waiting, StateSource::Silence)
        );
        // Session file present -> SessionFile (still authoritative over silence).
        assert_eq!(
            decide_status(false, None, NOW_UNIX, Some((false, 0)), NOW_MONO, NOW_MONO),
            (Status::Waiting, StateSource::SessionFile)
        );
    }
}
