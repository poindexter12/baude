use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::Result;

use crate::meta::{now_unix_ms, ClaudeMeta};
use crate::pty::{now_ms, Pty};
use crate::repository::ProcessIdentity;

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

    /// Stop and reap every process owned by this session. The agent is waited
    /// first so callers never begin destructive worktree inspection while it
    /// may still be writing into the checkout.
    pub fn kill_and_wait(&mut self) -> std::result::Result<(), SessionTeardownError> {
        // Attempt each owned process independently. A partially successful
        // stop remains retryable because Pty::kill_and_wait is idempotent.
        let claude = self.claude.kill_and_wait().err();
        let shell = self
            .shell
            .as_mut()
            .and_then(|shell| shell.kill_and_wait().err());
        match (claude, shell) {
            (None, None) => Ok(()),
            (claude, shell) => {
                let agent_stopped = claude.is_none();
                let shell_stopped = shell.is_none();
                let mut failures = Vec::new();
                if let Some(error) = claude {
                    failures.push(format!("agent: {error}"));
                }
                if let Some(error) = shell {
                    failures.push(format!("shell: {error}"));
                }
                Err(SessionTeardownError {
                    agent_pid: self.claude.pid(),
                    shell_pid: self.shell.as_ref().and_then(Pty::pid),
                    agent_identity: Some(self.claude.process_identity().clone()),
                    shell_identity: self
                        .shell
                        .as_ref()
                        .map(|shell| shell.process_identity().clone()),
                    agent_stopped,
                    shell_stopped,
                    detail: failures.join("; "),
                })
            }
        }
    }
}

#[derive(Debug)]
pub struct SessionTeardownError {
    pub agent_pid: Option<u32>,
    pub shell_pid: Option<u32>,
    pub agent_identity: Option<ProcessIdentity>,
    pub shell_identity: Option<ProcessIdentity>,
    pub agent_stopped: bool,
    pub shell_stopped: bool,
    pub detail: String,
}

/// Typed result of attempting to restore a retained agent/shell pair after a
/// rolled-back close. Owners supply process observations; this pure decision
/// keeps their definition of a complete restoration identical.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeRestorationFailure {
    pub agent_restarted: bool,
    pub shell_restarted: bool,
    pub detail: String,
}

impl std::fmt::Display for RuntimeRestorationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for RuntimeRestorationFailure {}

pub fn verify_runtime_restoration(
    agent_live: bool,
    shell_required: bool,
    shell_live: bool,
    detail: impl Into<String>,
) -> std::result::Result<(), RuntimeRestorationFailure> {
    let shell_restarted = !shell_required || shell_live;
    if agent_live && shell_restarted {
        Ok(())
    } else {
        Err(RuntimeRestorationFailure {
            agent_restarted: agent_live,
            shell_restarted,
            detail: detail.into(),
        })
    }
}

impl std::fmt::Display for SessionTeardownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "session process teardown incomplete (agent stopped: {}, shell stopped: {}; {})",
            self.agent_stopped, self.shell_stopped, self.detail
        )
    }
}

impl std::error::Error for SessionTeardownError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedTeardownError {
    pub agent_pid: Option<u32>,
    pub shell_pid: Option<u32>,
    pub agent_identity: Option<ProcessIdentity>,
    pub shell_identity: Option<ProcessIdentity>,
    pub agent_stopped: bool,
    pub shell_stopped: bool,
    pub detail: String,
}

impl std::fmt::Display for RecordedTeardownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "recorded process teardown remains unresolved: {}",
            self.detail
        )
    }
}

impl std::error::Error for RecordedTeardownError {}

#[cfg(target_os = "linux")]
pub(crate) fn inspect_process_identity(
    pid: u32,
) -> std::result::Result<Option<ProcessIdentity>, String> {
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not inspect pid {pid}: {error}")),
    };
    let end = stat
        .rfind(')')
        .ok_or_else(|| format!("malformed process stat for pid {pid}"))?;
    let fields: Vec<_> = stat[end + 1..].split_whitespace().collect();
    if fields.len() < 20 {
        return Err(format!("incomplete process stat for pid {pid}"));
    }
    if fields[0] == "Z" {
        return Ok(None);
    }
    let parse = |index: usize, name: &str| {
        fields[index]
            .parse()
            .map_err(|error| format!("invalid {name} for pid {pid}: {error}"))
    };
    Ok(Some(ProcessIdentity {
        pid,
        process_group: parse(2, "process group")?,
        session: parse(3, "session")?,
        start_time: parse(19, "start time")?,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn inspect_process_identity(
    pid: u32,
) -> std::result::Result<Option<ProcessIdentity>, String> {
    #[repr(C)]
    #[derive(Default)]
    struct ProcBsdInfo {
        flags: u32,
        status: u32,
        xstatus: u32,
        pid: u32,
        ppid: u32,
        uid: u32,
        gid: u32,
        ruid: u32,
        rgid: u32,
        svuid: u32,
        svgid: u32,
        rfu_1: u32,
        comm: [u8; 16],
        name: [u8; 32],
        nfiles: u32,
        pgid: u32,
        pjobc: u32,
        e_tdev: u32,
        e_tpgid: u32,
        nice: i32,
        start_tvsec: u64,
        start_tvusec: u64,
    }
    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut libc::c_void,
            buffersize: i32,
        ) -> i32;
    }
    const PROC_PIDTBSDINFO: i32 = 3;
    let mut info = ProcBsdInfo::default();
    let size = std::mem::size_of::<ProcBsdInfo>();
    // SAFETY: `info` is writable for exactly `size` bytes and proc_pidinfo
    // initializes PROC_PIDTBSDINFO on success.
    let read = unsafe {
        proc_pidinfo(
            pid as i32,
            PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut ProcBsdInfo).cast(),
            size as i32,
        )
    };
    if read == 0 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(None),
            _ => Err(format!("could not inspect pid {pid}: {error}")),
        };
    }
    if read as usize != size || info.pid != pid {
        return Err(format!("incomplete process identity for pid {pid}"));
    }
    // SAFETY: getsid only reads kernel process metadata for the supplied pid.
    let session = unsafe { libc::getsid(pid as i32) };
    if session < 0 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(None),
            _ => Err(format!("could not inspect session for pid {pid}: {error}")),
        };
    }
    Ok(Some(ProcessIdentity {
        pid,
        start_time: info
            .start_tvsec
            .saturating_mul(1_000_000)
            .saturating_add(info.start_tvusec),
        process_group: info.pgid as i32,
        session,
    }))
}

fn signal_process_group(
    identity: &ProcessIdentity,
    signal: i32,
) -> std::result::Result<(), String> {
    // SAFETY: a negative pgid addresses the exact process group. The caller
    // verifies the durable leader identity immediately before every signal.
    let result = unsafe { libc::kill(-identity.process_group, signal) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

fn identity_still_matches(
    expected: &ProcessIdentity,
    inspect: &mut impl FnMut(u32) -> std::result::Result<Option<ProcessIdentity>, String>,
) -> std::result::Result<bool, String> {
    Ok(inspect(expected.pid)?.as_ref() == Some(expected))
}

fn finish_recorded_process(
    identity: Option<ProcessIdentity>,
    already_stopped: bool,
) -> std::result::Result<(), String> {
    finish_recorded_process_with(
        identity,
        already_stopped,
        inspect_process_identity,
        signal_process_group,
    )
}

fn finish_recorded_process_with(
    identity: Option<ProcessIdentity>,
    already_stopped: bool,
    mut inspect: impl FnMut(u32) -> std::result::Result<Option<ProcessIdentity>, String>,
    mut signal: impl FnMut(&ProcessIdentity, i32) -> std::result::Result<(), String>,
) -> std::result::Result<(), String> {
    if already_stopped || identity.is_none() {
        return Ok(());
    }
    let identity = identity.expect("checked above");
    if !identity_still_matches(&identity, &mut inspect)? {
        return Ok(());
    }
    if let Err(error) = signal(&identity, libc::SIGTERM) {
        if identity_still_matches(&identity, &mut inspect)? {
            return Err(format!(
                "process group {} refused termination: {error}",
                identity.process_group
            ));
        }
        return Ok(());
    }
    for _ in 0..25 {
        if !identity_still_matches(&identity, &mut inspect)? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if !identity_still_matches(&identity, &mut inspect)? {
        return Ok(());
    }
    if let Err(error) = signal(&identity, libc::SIGKILL) {
        if identity_still_matches(&identity, &mut inspect)? {
            return Err(format!(
                "process group {} refused forced termination: {error}",
                identity.process_group
            ));
        }
        return Ok(());
    }
    for _ in 0..25 {
        if !identity_still_matches(&identity, &mut inspect)? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err(format!(
        "process group {} remained live after termination",
        identity.process_group
    ))
}

/// Idempotently finish teardown recorded before an owner crash. Already
/// stopped or absent children are accepted; live recorded children must be
/// confirmed gone before recovery can transition to inactive.
pub fn finish_recorded_teardown(
    agent_pid: Option<u32>,
    shell_pid: Option<u32>,
    agent_identity: Option<ProcessIdentity>,
    shell_identity: Option<ProcessIdentity>,
    agent_stopped: bool,
    shell_stopped: bool,
) -> std::result::Result<(), RecordedTeardownError> {
    let agent = finish_recorded_process(agent_identity.clone(), agent_stopped).err();
    let shell = finish_recorded_process(shell_identity.clone(), shell_stopped).err();
    if agent.is_none() && shell.is_none() {
        return Ok(());
    }
    let mut details = Vec::new();
    if let Some(error) = agent.as_ref() {
        details.push(format!("agent: {error}"));
    }
    if let Some(error) = shell.as_ref() {
        details.push(format!("shell: {error}"));
    }
    Err(RecordedTeardownError {
        agent_pid,
        shell_pid,
        agent_identity,
        shell_identity,
        agent_stopped: agent.is_none(),
        shell_stopped: shell.is_none(),
        detail: details.join("; "),
    })
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
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    #[test]
    fn human_duration_buckets() {
        assert_eq!(human_duration(5_000), "5s");
        assert_eq!(human_duration(120_000), "2m");
        assert_eq!(human_duration(3_660_000), "1h1m");
    }

    #[test]
    fn recorded_teardown_pid_reuse_never_signals_new_occupant() {
        let recorded = ProcessIdentity {
            pid: 4242,
            start_time: 100,
            process_group: 4242,
            session: 4242,
        };
        let reused = ProcessIdentity {
            start_time: 200,
            ..recorded.clone()
        };
        let signals = AtomicUsize::new(0);

        finish_recorded_process_with(
            Some(recorded),
            false,
            |_| Ok(Some(reused.clone())),
            |_, _| {
                signals.fetch_add(1, AtomicOrdering::Relaxed);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(signals.load(AtomicOrdering::Relaxed), 0);
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
