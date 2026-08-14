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

/// Default idle window before a session auto-archives: it sinks to the
/// bottom of lists and stops demanding attention until it's active again.
/// Overridable via config `auto_archive_minutes` / BAUDED_AUTO_ARCHIVE_MIN.
pub const AUTO_ARCHIVE_IDLE_MS: u64 = 30 * 60 * 1000;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Status {
    /// Idle, blocked on the user — Claude asked a question or needs a
    /// permission decision. The urgent idle state: flashes, has a wait timer.
    Waiting,
    /// Idle, turn ended cleanly (a `Stop` event, no pending `Notification`) —
    /// your move, but not urgent. Calm sibling of `Waiting`; recolors the same
    /// sidebar row in place rather than reordering it. See [`idle_kind`].
    Completed,
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
    /// Parked: sorts last, excluded from counters/notifications.
    /// Set manually or after the auto-archive idle window (config
    /// `auto_archive_minutes` / BAUDED_AUTO_ARCHIVE_MIN, default
    /// `AUTO_ARCHIVE_IDLE_MS`) of waiting.
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
    /// PERM-02: the in-flight tool-permission request the `permission-mcp`
    /// bridge POSTed, awaiting a human `allow`/`deny`. `None` when nothing is
    /// pending. The bridge long-polls until [`Self::permission_decision`] is
    /// recorded (or its own deadline denies). Daemon-mediated state — the
    /// `Manager` owns set/resolve. Held as an opaque JSON `Value` so baude-core
    /// carries no permission-request type (the shape lives in the daemon).
    pub pending_permission: Option<serde_json::Value>,
    /// PERM-02: the decision the human POSTed for the most recent request
    /// (`{request_id, decision, scope?, ts}` as JSON). The bridge's GET poll
    /// reads this to unblock; cleared when a new request supersedes it.
    pub permission_decision: Option<serde_json::Value>,
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
            // Both idle flavors still park after AUTO_ARCHIVE_IDLE_MS — a
            // Completed session left unattended is just as "done being
            // watched" as a Waiting one, it just didn't shout on the way in.
            Status::Waiting | Status::Completed
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

/// Which flavor of "idle" a [`Status::Waiting`] session is really in. Output
/// of [`idle_kind`] — see there for the derivation rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IdleKind {
    /// Claude asked a question / needs a permission decision — urgent.
    NeedsInput,
    /// Claude finished its turn cleanly — calm, your move.
    Completed,
}

/// Refine the *idle* bucket `decide_status` already produced into
/// [`IdleKind::Completed`] vs [`IdleKind::NeedsInput`]. Deliberately NOT part
/// of `decide_status`/`decide_live` — those own busy-vs-idle precedence and
/// stay untouched; this is a separate, pure classifier layered on top, fed the
/// two terminal hook-event timestamps `ClaudeMeta` already tracks
/// (`last_stop`, `last_notification`).
///
/// Rules, in order:
/// - a `last_notification` whose type CONTAINS `"permission"` is
///   `NeedsInput` regardless of recency — a pending permission is itself a
///   kind of waiting (mirrors [`crate::permission::waiting_reason`]'s own
///   rule, and guarantees a session can never read `Completed` while a
///   permission is outstanding).
/// - otherwise, whichever of `last_stop`/`last_notification` is the more
///   recent terminal event wins: a `Stop` at or after the last `Notification`
///   is `Completed`; a `Notification` strictly after the last `Stop` is
///   `NeedsInput`.
/// - fail-safe: idle with NEITHER known (a session that never fired hooks —
///   pure silence/session-file fallback) is `NeedsInput`, never `Completed`.
///   A false "completed" could hide a session actually blocked on you; a
///   false "needs input" only costs one harmless extra flash. Never trade
///   away the attention guarantee.
pub fn idle_kind(last_stop: Option<u64>, last_notification: Option<&(String, u64)>) -> IdleKind {
    if let Some((notification_type, _)) = last_notification {
        if notification_type.contains("permission") {
            return IdleKind::NeedsInput;
        }
    }
    match (last_stop, last_notification.map(|(_, ts)| *ts)) {
        (Some(stop), Some(notif)) if notif > stop => IdleKind::NeedsInput,
        (Some(_), _) => IdleKind::Completed,
        (None, _) => IdleKind::NeedsInput, // fail-safe: no Stop ever seen
    }
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
        let (status, source) = decide_status(
            self.claude.is_exited(),
            self.meta.hook_status,
            now_unix_ms(),
            self.meta.claude_status,
            self.claude.last_output_ms.load(Ordering::Relaxed),
            now_ms(),
        );
        // decide_status/decide_live are untouched (they own busy-vs-idle
        // precedence); the idle bucket they hand back as Waiting is refined
        // here into Completed vs NeedsInput(=Waiting) via the new pure
        // classifier. Busy and Exited pass through unchanged.
        if status == Status::Waiting {
            let refined = match idle_kind(self.meta.last_stop, self.meta.last_notification.as_ref())
            {
                IdleKind::Completed => Status::Completed,
                IdleKind::NeedsInput => Status::Waiting,
            };
            return (refined, source);
        }
        (status, source)
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
        crate::backend::active().poll_meta(&mut self.meta, &cwd, pid, spawn, &root);
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

    // --- idle_kind: the new Completed/NeedsInput classifier ---
    //
    // Pure over the two terminal-event timestamps ClaudeMeta already tracks;
    // does not touch decide_status/decide_live (asserted unchanged above).

    #[test]
    fn idle_kind_stop_newer_is_completed() {
        assert_eq!(
            idle_kind(Some(200), Some(&("idle".to_string(), 100))),
            IdleKind::Completed
        );
        // Stop with no notification at all is also Completed.
        assert_eq!(idle_kind(Some(200), None), IdleKind::Completed);
        // Tied timestamps: Stop counts as "the most recent" -> Completed.
        assert_eq!(
            idle_kind(Some(100), Some(&("idle".to_string(), 100))),
            IdleKind::Completed
        );
    }

    #[test]
    fn idle_kind_notification_newer_is_needs_input() {
        assert_eq!(
            idle_kind(Some(100), Some(&("idle".to_string(), 200))),
            IdleKind::NeedsInput
        );
    }

    #[test]
    fn idle_kind_permission_type_wins_regardless_of_recency() {
        // A permission-type notification is NeedsInput even when it is OLDER
        // than the last Stop — mirrors permission::waiting_reason's own rule
        // and guarantees Completed never coexists with a pending permission.
        assert_eq!(
            idle_kind(Some(500), Some(&("permission_prompt".to_string(), 100))),
            IdleKind::NeedsInput
        );
    }

    #[test]
    fn idle_kind_neither_known_fails_safe_to_needs_input() {
        // No hook history at all (pure silence/session-file fallback) must
        // NOT read as Completed — a false "completed" could hide a session
        // genuinely blocked on the user.
        assert_eq!(idle_kind(None, None), IdleKind::NeedsInput);
        // No Stop but SOME notification: still NeedsInput (the notification
        // itself is the reason to wait, and there's no Stop to out-rank it).
        assert_eq!(
            idle_kind(None, Some(&("idle".to_string(), 100))),
            IdleKind::NeedsInput
        );
    }
}
