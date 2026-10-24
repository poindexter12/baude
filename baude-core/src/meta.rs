//! Claude Code session metadata, read from the artifacts Claude writes to
//! disk: `$CLAUDE_CONFIG_DIR/sessions/<pid>.json` (live busy/idle status),
//! the session transcript JSONL (model, token usage, permission mode),
//! `/tmp/claude-ctx-<sessionId>.json` (context %, written by statusline
//! hooks), and `.planning/STATE.md` (GSD project state).

use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The real `~/.claude` root, with no test redirect and no containment check.
///
/// Kept verbatim from the pre-redirect `claude_config_dir`. This is NOT an XDG
/// chain: it consults `CLAUDE_CONFIG_DIR` where [`crate::persist`] and
/// [`crate::git`] consult `XDG_*`, and its terminal fallback (`"."`) differs
/// from `crate::git`'s. The real-root resolvers in this crate share a shape but
/// not their heads or their tails, so they must not be collapsed into one
/// helper — doing so would change production behavior.
///
/// `CLAUDE_CONFIG_DIR` coming first is also why a re-exec'd test child must be
/// given that variable explicitly: a developer who exports it in their shell
/// would otherwise have the child resolve their real Claude directory and trip
/// [`crate::testing::assert_contained`].
fn real_claude_config_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The config dir the spawned claude processes will use (inherited env).
///
/// Under a test redirect this resolves inside the fixture root; with no
/// redirect and no fixture root it aborts the test rather than reaching the
/// developer's real `~/.claude` (#72, D-03). Isolating here rather than by
/// parameter is deliberate: both [`ClaudeMeta::poll`] call sites keep their
/// current signatures and neither gains an `_at` variant.
pub fn claude_config_dir() -> PathBuf {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(dir) = crate::testing::claude_config_dir_override() {
        return dir;
    }
    let real = real_claude_config_dir();
    #[cfg(any(test, feature = "test-support"))]
    crate::testing::assert_contained(&real, "claude config dir");
    real
}

/// Longest encoded project-dir name Claude Code keeps verbatim; longer names
/// are cut here and suffixed with a hash of the full path.
const MAX_ENCODED_PATH: usize = 200;

/// Claude Code's project-dir name for a cwd, matching its JavaScript
/// sanitizer exactly (verified against the CLI v2.1.310 binary):
/// `path.replace(/[^a-zA-Z0-9]/g, "-")`, and past 200 characters the first
/// 200 plus `-` and `Math.abs(hash(path)).toString(36)`, where the hash is
/// the `(h << 5) - h + code | 0` string hash.
///
/// Both steps run over UTF-16 code units, not chars, because that is what a
/// JS regex and `charCodeAt` see: a character outside the BMP is two units,
/// so it becomes TWO dashes and feeds the hash twice.
fn encode_path(p: &Path) -> String {
    let path = p.to_string_lossy();
    let units: Vec<u16> = path.encode_utf16().collect();
    let encoded: String = units
        .iter()
        .map(|&u| match u8::try_from(u) {
            Ok(b) if b.is_ascii_alphanumeric() => b as char,
            _ => '-',
        })
        .collect();
    if encoded.len() <= MAX_ENCODED_PATH {
        return encoded;
    }
    let hash = units.iter().fold(0i32, |h, &u| {
        (h << 5).wrapping_sub(h).wrapping_add(i32::from(u))
    });
    format!(
        "{}-{}",
        &encoded[..MAX_ENCODED_PATH],
        base36(i64::from(hash).unsigned_abs())
    )
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).expect("base36 digits are ASCII")
}

/// Where Claude Code keeps the transcript of conversation `session_id` when
/// it was started in `cwd`: `<config>/projects/<encoded cwd>/<id>.jsonl`.
///
/// Claude writes this file only once the conversation has a first message,
/// so a session id can exist (announced in `sessions/<pid>.json`) with no
/// transcript behind it — and `claude --resume <id>` then dies with "No
/// conversation found".
///
/// This encodes `cwd` as given. Claude itself keys the directory by the
/// PHYSICAL cwd (symlinks resolved, so a macOS `/var/...` cwd lands under
/// `-private-var-...`); [`resolve_transcript_file`] tries both.
pub fn transcript_file(cwd: &Path, session_id: &str) -> PathBuf {
    transcript_file_in(&claude_config_dir(), cwd, session_id)
}

fn transcript_file_in(config: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    config
        .join("projects")
        .join(encode_path(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// The existing transcript of `session_id` for `cwd`, trying the cwd as given
/// and then its canonical path. Claude records the cwd `getcwd()` reports,
/// which has every symlink resolved, so a session opened through a symlinked
/// path (or anywhere under macOS `/tmp` or `/var`) keeps its transcript under
/// the canonical encoding and the as-given lookup never finds it.
pub fn resolve_transcript_file(cwd: &Path, session_id: &str) -> Option<PathBuf> {
    resolve_transcript_file_in(&claude_config_dir(), cwd, session_id)
}

fn resolve_transcript_file_in(config: &Path, cwd: &Path, session_id: &str) -> Option<PathBuf> {
    let given = transcript_file_in(config, cwd, session_id);
    if given.is_file() {
        return Some(given);
    }
    let real = fs::canonicalize(cwd).ok().filter(|real| real != cwd)?;
    let canonical = transcript_file_in(config, &real, session_id);
    canonical.is_file().then_some(canonical)
}

/// The session id that owns process `pid`, or `None` when the process is gone.
fn process_session(pid: u32) -> Option<i32> {
    let pid = i32::try_from(pid).ok()?;
    // SAFETY: getsid only reads kernel process metadata for the given pid.
    let sid = unsafe { libc::getsid(pid) };
    (sid >= 0).then_some(sid)
}

/// The value of `name` in an environment block of `NAME=value` entries.
fn env_entry(entries: &[&[u8]], name: &str) -> Option<std::ffi::OsString> {
    use std::os::unix::ffi::OsStrExt;
    let prefix = format!("{name}=");
    entries.iter().find_map(|entry| {
        entry
            .strip_prefix(prefix.as_bytes())
            .map(|value| std::ffi::OsStr::from_bytes(value).to_os_string())
    })
}

/// The environment entries of a macOS `KERN_PROCARGS2` buffer: a native-endian
/// `argc`, the NUL-terminated exec path, NUL padding, `argc` argument strings,
/// then the environment strings up to the first empty one. Malformed input
/// yields no entries rather than a guess.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn procargs2_env(buf: &[u8]) -> Vec<&[u8]> {
    let Some(argc) = buf.get(..4).and_then(|b| <[u8; 4]>::try_from(b).ok()) else {
        return Vec::new();
    };
    let argc = i32::from_ne_bytes(argc).max(0) as usize;
    let rest = &buf[4..];
    let Some(path_end) = rest.iter().position(|b| *b == 0) else {
        return Vec::new();
    };
    let rest = &rest[path_end..];
    let rest = &rest[rest.iter().position(|b| *b != 0).unwrap_or(rest.len())..];
    let mut fields = rest.split(|b| *b == 0);
    for _ in 0..argc {
        if fields.next().is_none() {
            return Vec::new();
        }
    }
    fields.take_while(|f| !f.is_empty()).collect()
}

/// `name` in process `pid`'s environment as of its last exec, or `None` when
/// the process is gone, unreadable, or does not set it.
#[cfg(target_os = "linux")]
fn process_env_var(pid: u32, name: &str) -> Option<std::ffi::OsString> {
    let raw = fs::read(format!("/proc/{pid}/environ")).ok()?;
    let entries: Vec<&[u8]> = raw.split(|b| *b == 0).collect();
    env_entry(&entries, name)
}

#[cfg(target_os = "macos")]
fn process_env_var(pid: u32, name: &str) -> Option<std::ffi::OsString> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let mut argmax: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    // SAFETY: reads one c_int into `argmax`, whose size `len` describes.
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            (&mut argmax as *mut libc::c_int).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || argmax <= 0 {
        return None;
    }
    let mut buf = vec![0u8; argmax as usize];
    let mut len = buf.len();
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    // SAFETY: the kernel writes at most `len` bytes into `buf` and updates `len`.
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    buf.truncate(len);
    env_entry(&procargs2_env(&buf), name)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_env_var(_pid: u32, _name: &str) -> Option<std::ffi::OsString> {
    None
}

/// The direct children of process `pid`.
#[cfg(target_os = "linux")]
fn child_pids(pid: u32) -> Vec<u32> {
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for task in tasks.flatten() {
        if let Ok(list) = fs::read_to_string(task.path().join("children")) {
            out.extend(
                list.split_whitespace()
                    .filter_map(|p| p.parse::<u32>().ok()),
            );
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn child_pids(pid: u32) -> Vec<u32> {
    const PROC_PPID_ONLY: u32 = 6;
    let mut buf = vec![0 as libc::pid_t; 256];
    let bytes = libc::c_int::try_from(buf.len() * std::mem::size_of::<libc::pid_t>())
        .unwrap_or(libc::c_int::MAX);
    // SAFETY: the kernel fills at most `bytes` bytes of `buf` and returns the
    // number of bytes written.
    let written =
        unsafe { libc::proc_listpids(PROC_PPID_ONLY, pid, buf.as_mut_ptr().cast(), bytes) };
    if written <= 0 {
        return Vec::new();
    }
    let n = (written as usize / std::mem::size_of::<libc::pid_t>()).min(buf.len());
    buf[..n]
        .iter()
        .filter(|p| **p > 0)
        .map(|p| *p as u32)
        .collect()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn child_pids(_pid: u32) -> Vec<u32> {
    Vec::new()
}

/// Whether a config dir read from a child process's environment may be
/// consulted. Production reads whatever the child uses; a test build only
/// admits dirs under the active redirect's fixture root, so an ambient
/// `CLAUDE_CONFIG_DIR` inherited by a test child never reaches the real tree.
fn admissible_child_config_dir(dir: &Path) -> bool {
    #[cfg(any(test, feature = "test-support"))]
    {
        crate::testing::claude_config_dir_override()
            .and_then(|own| own.parent().map(Path::to_path_buf))
            .is_some_and(|root| dir.starts_with(root))
    }
    #[cfg(not(any(test, feature = "test-support")))]
    {
        dir.is_absolute()
    }
}

#[derive(Default, Clone)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
}

/// One account rate-limit window (5-hour block or 7-day) as captured from
/// the statusline payload by `baude statusline`.
#[derive(Default, Clone, Copy)]
pub struct RateWindow {
    pub used_pct: Option<f64>,
    pub resets_at_unix_s: Option<u64>,
}

/// Pull-request info captured from the bridge file's `pr` object.
#[derive(Default, Clone)]
pub struct PrInfo {
    pub number: Option<u64>,
    pub url: Option<String>,
    pub review_state: Option<String>,
}

/// Worktree info captured from the bridge file's `worktree` object.
#[derive(Default, Clone)]
pub struct WorktreeInfo {
    pub name: Option<String>,
    pub path: Option<String>,
    pub branch: Option<String>,
}

#[derive(Default, Clone)]
pub struct GsdState {
    pub milestone: Option<String>,
    pub status: Option<String>,
    pub active_phase: Option<String>,
    pub next_action: Option<String>,
    pub percent: Option<u8>,
    pub phase_line: Option<String>,
}

/// Drop-oldest cap for the per-session tool-activity ring buffer
/// (`ClaudeMeta::activity`). Older events are evicted once the ring exceeds
/// this size, keeping the buffer bounded regardless of event volume (T-03-01).
pub const ACTIVITY_CAP: usize = 200;

/// One Claude Code hook event retained in the per-session activity ring
/// (`ClaudeMeta::activity`). Serialized to JSON for the daemon's activity
/// endpoints and the TUI/PWA timeline views: `{event, tool?,
/// notification_type?, ts}`. `tool` is present for `PostToolUse`,
/// `notification_type` for `Notification`; both are omitted from JSON when
/// `None`. This is the single serde-`Serialize` type produced by `ClaudeMeta`
/// (the struct itself is intentionally NOT serializable).
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Event kind: `PostToolUse` | `UserPromptSubmit` | `Stop` | `Notification`.
    pub event: String,
    /// Tool name, present for `PostToolUse` events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Notification type, present for `Notification` events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,
    /// Event timestamp, unix milliseconds.
    pub ts: u64,
}

#[derive(Default)]
pub struct ClaudeMeta {
    pub session_id: Option<String>,
    transcript: Option<PathBuf>,
    offset: u64,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub last_usage: Option<Usage>,
    pub totals: Usage,
    pub context_used_pct: Option<u8>,
    /// (busy, status_updated_at unix ms) from Claude's own session file.
    pub claude_status: Option<(bool, u64)>,
    /// (busy, event_ts unix ms) derived from the Claude Code hook event
    /// stream (`/tmp/baude-events-<sid>.jsonl`). Higher precedence than
    /// `claude_status` while fresh (see `session.rs::status_with_source`).
    /// Event-driven: only ever set on a new event, never cleared by absence.
    pub hook_status: Option<(bool, u64)>,
    /// (tool_name, event_ts unix ms) of the most recent PostToolUse event.
    pub last_tool: Option<(String, u64)>,
    /// (notification_type, event_ts unix ms) of the most recent Notification
    /// event. Captured for a future Phase 4 `waiting_reason`; intentionally
    /// NOT exposed as a structured SessionInfo field this phase.
    pub last_notification: Option<(String, u64)>,
    /// event_ts unix ms of the most recent Stop event — Claude ended a turn
    /// cleanly. Symmetric to `last_notification`; the two together feed
    /// `session::idle_kind`, which decides whether an idle session reads as
    /// `Status::Completed` (this is the newer terminal event) or
    /// `Status::Waiting`/needs-input (`last_notification` is newer, or is a
    /// permission-type notification).
    pub last_stop: Option<u64>,
    /// Byte offset of the last fully-consumed line in the event stream.
    /// Separate from `offset` (the transcript tail) so the two tails never
    /// interfere.
    offset_events: u64,
    /// Resolved event-file path the `offset_events` cursor belongs to. When the
    /// `session_id` rotates mid-session the event-file path flips; tracking it
    /// here lets `read_event_tail` reset the offset on a path change so a stale
    /// (possibly larger) offset can't permanently short-circuit the new file's
    /// events (WR-03), mirroring the transcript tail's reset in
    /// `resolve_transcript`.
    event_path: Option<PathBuf>,
    /// Capped (drop-oldest) ring of recent hook events for the activity
    /// timeline (ACT-01). Appended to by `read_event_tail` as it tails the
    /// event stream, bounded at `ACTIVITY_CAP`, and cleared on event-path
    /// rotation (WR-03) so it never carries stale cross-session events. Private
    /// like `offset_events`/`event_path`; read via the `activity()` accessor.
    activity: VecDeque<HookEvent>,
    pub gsd: Option<GsdState>,
    /// Live session cost from the `baude statusline` bridge file.
    pub session_cost_usd: Option<f64>,
    /// Account rate-limit windows from the bridge file, with the bridge's
    /// write timestamp so the freshest session wins across the app.
    pub rate_5h: Option<RateWindow>,
    pub rate_week: Option<RateWindow>,
    pub rate_updated_unix_ms: u64,
    /// Current git branch of the session's cwd (read from .git/HEAD).
    pub git_branch: Option<String>,
    /// Claude's AI-generated session title (`ai-title` transcript records).
    pub title: Option<String>,
    /// Reasoning-effort level from the bridge file (e.g. "high").
    pub effort: Option<String>,
    /// Whether extended thinking is enabled, from the bridge file.
    pub thinking: Option<bool>,
    /// Vim editing mode from the bridge file. Captured but not rendered.
    pub vim_mode: Option<String>,
    /// Pull-request info from the bridge file's `pr` object.
    pub pr: Option<PrInfo>,
    /// Worktree info from the bridge file's `worktree` object.
    pub worktree: Option<WorktreeInfo>,
    /// Local port of the per-session backend server, when the backend runs
    /// one (opencode: the TUI's embedded HTTP server, pinned via `--port` at
    /// spawn). `None` for backends without one (claude). Set by the spawn
    /// sites from [`crate::backend::SpawnPlan::server_port`]; read by the
    /// opencode backend's `poll_meta`.
    pub backend_port: Option<u16>,
    /// True once the backend's server has answered a poll — the opencode
    /// equivalent of "the TUI is up, input won't be drained". opencode creates
    /// its session LAZILY on the first prompt, so `session_id` stays `None`
    /// until a message goes in; the daemon's post_message readiness gate
    /// accepts EITHER a resolved session_id (claude) or this flag (opencode),
    /// or the first message could never be delivered. Never set by claude.
    pub backend_ready: bool,
    /// Last recorded modification time of session.json. Used to gate reads:
    /// if mtime unchanged, skip read (cost: one stat call only).
    pub last_session_mtime: Option<SystemTime>,
    /// Last recorded modification time of hook-events.jsonl. Used to gate reads:
    /// if mtime unchanged, skip read (cost: one stat call only).
    pub last_events_mtime: Option<SystemTime>,
    /// Sizes seen alongside the mtimes above. mtime ALONE is not a safe gate:
    /// filesystem timestamp granularity is coarse enough (notably on Linux,
    /// where CI caught this; macOS APFS hides it with nanosecond stamps) that
    /// two writes can land in one tick, and a gate that trusts mtime then
    /// skips a file that really grew. Dropping a `Stop` event that way leaves
    /// a finished session rendered as busy until something else happens to
    /// move the mtime. Both sizes must match too before a read is skipped.
    pub last_session_len: Option<u64>,
    pub last_events_len: Option<u64>,
    /// Pid whose `sessions/<pid>.json` belongs to this pane when it is NOT the
    /// pane's own pid (see [`Self::session_member_file`]), cached so later
    /// polls read that one file instead of rescanning the directory.
    session_file_pid: Option<u32>,
    /// The Claude config dir this pane's claude actually writes to, once its
    /// session file has been found there. It can differ from baude's own: the
    /// child runs under the user's login shell in the session cwd, and a
    /// per-directory env (mise, an account launcher) may set its own
    /// `CLAUDE_CONFIG_DIR`. `None` means baude's own config dir.
    config_dir: Option<PathBuf>,
}

impl ClaudeMeta {
    pub fn poll(&mut self, cwd: &Path, pid: Option<u32>, spawn_unix_ms: u64, repo_root: &Path) {
        self.poll_session_file(cwd, pid, spawn_unix_ms);
        self.resolve_transcript(cwd, spawn_unix_ms);
        self.read_transcript_tail();
        // GSD ctx file first; the baude bridge file overrides when present
        // (it carries Claude's exact used_percentage).
        self.read_context_file();
        self.read_bridge_file();
        self.read_event_tail();
        self.poll_neutral(cwd, repo_root);
    }

    /// The backend-neutral slice of a poll — GSD project state and the git
    /// branch, read from the repo itself rather than any backend artifact.
    /// Called by every backend's `poll_meta` (claude via [`Self::poll`]).
    pub fn poll_neutral(&mut self, cwd: &Path, repo_root: &Path) {
        self.gsd = parse_gsd(repo_root);
        self.git_branch = read_git_branch(cwd);
    }

    /// Find this session's `sessions/<pid>.json`. Exact pid match wins
    /// (sessions spawned with `exec claude` — the child IS claude). When the
    /// pane's pid has no file, claude may still be running as a CHILD of the
    /// pane shell: the continue spawn `claude --continue || exec claude` has
    /// to fork its first command to test the exit status, so that claude
    /// writes its file under its own pid and the exact match never fires.
    /// [`Self::session_member_file`] finds it by process session. With no pid
    /// at all, match by cwd and pick the file whose start time is closest.
    ///
    /// Gate on mtime: if mtime unchanged since last poll, skip read (cost: one stat call).
    fn poll_session_file(&mut self, cwd: &Path, pid: Option<u32>, spawn_unix_ms: u64) {
        let base = claude_config_dir();
        let dir = base.join("sessions");

        // Check mtime before reading (low-cost stat call)
        let session_file = if let Some(pid) = pid {
            let Some(file) = self.find_pid_session_file(&base, pid, spawn_unix_ms) else {
                return;
            };
            file
        } else {
            // Fallback path: we'll need to scan, so mtime gate doesn't apply
            let session_mtime = fs::metadata(&dir).and_then(|m| m.modified()).ok();
            if session_mtime == self.last_session_mtime {
                return; // Unchanged, skip entire scan
            }
            self.last_session_mtime = session_mtime;

            // Claude records the physical cwd, so accept the canonical form too.
            let cwd_str = cwd.to_string_lossy();
            let real = fs::canonicalize(cwd).ok();
            let real_str = real.as_ref().map(|r| r.to_string_lossy());
            let Ok(entries) = fs::read_dir(&dir) else {
                return;
            };
            let mut best: Option<(u64, Value)> = None;
            for entry in entries.flatten() {
                let Some(v) = read_json(&entry.path()) else {
                    continue;
                };
                let recorded = v["cwd"].as_str();
                if recorded != Some(cwd_str.as_ref()) && recorded != real_str.as_deref() {
                    continue;
                }
                let started = v["startedAt"].as_u64().unwrap_or(0);
                // Ignore session files that predate this baude session.
                if started + 20_000 < spawn_unix_ms {
                    continue;
                }
                let dist = started.abs_diff(spawn_unix_ms);
                if best.as_ref().map(|(d, _)| dist < *d).unwrap_or(true) {
                    best = Some((dist, v));
                }
            }
            if let Some((_, v)) = best {
                self.apply_session_file(&v);
            }
            return;
        };

        // Exact pid match path, gated on mtime AND size (see the field docs:
        // mtime alone is unsafe at filesystem timestamp granularity).
        let session_meta = fs::metadata(&session_file).ok();
        let session_mtime = session_meta.as_ref().and_then(|m| m.modified().ok());
        let session_len = session_meta.as_ref().map(|m| m.len());
        if session_mtime.is_some()
            && session_mtime == self.last_session_mtime
            && session_len == self.last_session_len
        {
            return; // Unchanged, skip read
        }

        if let Some(v) = read_json(&session_file) {
            self.apply_session_file(&v);
            self.last_session_mtime = session_mtime;
            self.last_session_len = session_len;
        }
    }

    /// The session file of a claude running INSIDE this pane's process
    /// session when it is not the pane's own pid.
    ///
    /// Every PTY child is a session leader (`setsid` at spawn), so anything
    /// the pane shell forks shares its session id, and no other pane's or
    /// terminal's process does. That makes the session id an exact owner
    /// test, where matching by cwd would cross-wire two panes in one repo.
    /// A file whose `startedAt` predates the pane is a stale file for a
    /// recycled pid: nothing in the pane can have started before the pane.
    /// The session file for pane `pid`, looked up first in the config dir that
    /// matched last time (baude's own until something else has), then in every
    /// `CLAUDE_CONFIG_DIR` set in the environment of the pane process or its
    /// direct children, then in baude's own. Only the miss path reads process
    /// environments, so a resolved session costs one stat per poll as before.
    fn find_pid_session_file(
        &mut self,
        base: &Path,
        pid: u32,
        spawn_unix_ms: u64,
    ) -> Option<PathBuf> {
        let current = self
            .config_dir
            .clone()
            .unwrap_or_else(|| base.to_path_buf());
        if let Some(file) = self.pid_session_file_in(&current, pid, spawn_unix_ms) {
            return Some(file);
        }
        let mut candidates: Vec<PathBuf> = Vec::new();
        for p in std::iter::once(pid).chain(child_pids(pid)) {
            let Some(value) = process_env_var(p, "CLAUDE_CONFIG_DIR") else {
                continue;
            };
            let dir = PathBuf::from(value);
            if dir != current && admissible_child_config_dir(&dir) && !candidates.contains(&dir) {
                candidates.push(dir);
            }
        }
        if current != base && !candidates.iter().any(|d| d == base) {
            candidates.push(base.to_path_buf());
        }
        for dir in candidates {
            if let Some(file) = self.pid_session_file_in(&dir, pid, spawn_unix_ms) {
                self.config_dir = (dir != base).then_some(dir);
                return Some(file);
            }
        }
        None
    }

    fn pid_session_file_in(
        &mut self,
        config: &Path,
        pid: u32,
        spawn_unix_ms: u64,
    ) -> Option<PathBuf> {
        let dir = config.join("sessions");
        let own = dir.join(format!("{pid}.json"));
        if own.is_file() {
            self.session_file_pid = None;
            return Some(own);
        }
        self.session_member_file(&dir, pid, spawn_unix_ms)
    }

    fn session_member_file(&mut self, dir: &Path, pid: u32, spawn_unix_ms: u64) -> Option<PathBuf> {
        let owner = process_session(pid)?;
        if let Some(cached) = self.session_file_pid {
            let file = dir.join(format!("{cached}.json"));
            if process_session(cached) == Some(owner) && file.is_file() {
                return Some(file);
            }
            self.session_file_pid = None;
        }
        let mut best: Option<(u64, u32, PathBuf)> = None;
        for entry in fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Some(candidate) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            if candidate == pid || process_session(candidate) != Some(owner) {
                continue;
            }
            let Some(v) = read_json(&path) else {
                continue;
            };
            let started = v["startedAt"].as_u64().unwrap_or(0);
            if started + 20_000 < spawn_unix_ms {
                continue;
            }
            // Newest wins: a nested claude started later is the live one.
            if best.as_ref().is_none_or(|(s, _, _)| started > *s) {
                best = Some((started, candidate, path));
            }
        }
        let (_, candidate, path) = best?;
        self.session_file_pid = Some(candidate);
        Some(path)
    }

    fn apply_session_file(&mut self, v: &Value) {
        if let Some(sid) = v["sessionId"].as_str() {
            self.session_id = Some(sid.to_string());
        }
        if let (Some(status), Some(at)) = (v["status"].as_str(), v["statusUpdatedAt"].as_u64()) {
            self.claude_status = Some((status == "busy", at));
        }
    }

    fn resolve_transcript(&mut self, cwd: &Path, spawn_unix_ms: u64) {
        let config = self.config_dir.clone().unwrap_or_else(claude_config_dir);
        let path = if let Some(sid) = &self.session_id {
            resolve_transcript_file_in(&config, cwd, sid)
        } else {
            // No session file — fall back to the newest transcript started
            // after this session spawned, under the as-given and then the
            // canonical encoding (Claude writes the canonical one).
            let projects = config.join("projects");
            newest_jsonl(&projects.join(encode_path(cwd)), spawn_unix_ms).or_else(|| {
                let real = fs::canonicalize(cwd).ok().filter(|real| real != cwd)?;
                newest_jsonl(&projects.join(encode_path(&real)), spawn_unix_ms)
            })
        };
        if path != self.transcript {
            self.transcript = path;
            self.offset = 0;
            self.totals = Usage::default();
            self.last_usage = None;
            self.title = None;
            // Transcript-derived values must not survive a transcript switch
            // (e.g. the cwd fallback matched a foreign session before the
            // pid-based session file appeared).
            self.model = None;
            self.permission_mode = None;
        }
    }

    /// Path of the resolved transcript JSONL, if one has been found.
    pub fn transcript_path(&self) -> Option<&Path> {
        self.transcript.as_deref()
    }

    /// The per-session tool-activity ring (oldest at front, newest at back),
    /// bounded at `ACTIVITY_CAP`. The single source of truth for the activity
    /// timeline; the TUI reads it directly and the daemon serves it from the
    /// `ClaudeMeta` it already holds.
    pub fn activity(&self) -> &VecDeque<HookEvent> {
        &self.activity
    }

    /// Incrementally parse new transcript lines since the last poll.
    fn read_transcript_tail(&mut self) {
        let Some(path) = &self.transcript else {
            return;
        };
        let Ok(mut f) = fs::File::open(path) else {
            return;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len <= self.offset {
            return;
        }
        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            return;
        }
        // Only consume complete lines; a partial trailing line is re-read
        // on the next poll.
        let consumed = match buf.rfind('\n') {
            Some(i) => i + 1,
            None => return,
        };
        for line in buf[..consumed].lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(mode) = v["permissionMode"].as_str() {
                self.permission_mode = Some(mode.to_string());
            }
            if v["type"].as_str() == Some("ai-title") {
                if let Some(t) = v["aiTitle"].as_str() {
                    self.title = Some(t.to_string());
                }
            }
            if v["type"].as_str() == Some("assistant") {
                let msg = &v["message"];
                if let Some(model) = msg["model"].as_str() {
                    self.model = Some(model.to_string());
                }
                let u = &msg["usage"];
                if u.is_object() {
                    let usage = Usage {
                        input: u["input_tokens"].as_u64().unwrap_or(0),
                        output: u["output_tokens"].as_u64().unwrap_or(0),
                        cache_read: u["cache_read_input_tokens"].as_u64().unwrap_or(0),
                        cache_create: u["cache_creation_input_tokens"].as_u64().unwrap_or(0),
                    };
                    self.totals.input += usage.input;
                    self.totals.output += usage.output;
                    self.totals.cache_read += usage.cache_read;
                    self.totals.cache_create += usage.cache_create;
                    self.last_usage = Some(usage);
                }
            }
        }
        self.offset += consumed as u64;
    }

    /// Usage bridge file written by `baude statusline`:
    /// /tmp/baude-usage-<sessionId>.json — session cost, context %, and the
    /// account rate-limit windows (the only local source for those).
    fn read_bridge_file(&mut self) {
        let Some(sid) = &self.session_id else {
            return;
        };
        let Some(v) = read_json(&PathBuf::from(crate::bridge::bridge_path(sid))) else {
            return;
        };
        if let Some(cost) = v["cost_usd"].as_f64() {
            self.session_cost_usd = Some(cost);
        }
        if let Some(pct) = v["context_used_pct"].as_f64() {
            self.context_used_pct = Some((pct.round() as u64).min(100) as u8);
        }
        self.rate_updated_unix_ms = v["updated_unix_ms"].as_u64().unwrap_or(0);
        let window = |w: &Value| -> Option<RateWindow> {
            w.is_object().then(|| RateWindow {
                used_pct: w["used_pct"].as_f64(),
                resets_at_unix_s: w["resets_at"].as_u64(),
            })
        };
        if let Some(w) = window(&v["five_hour"]) {
            self.rate_5h = Some(w);
        }
        if let Some(w) = window(&v["seven_day"]) {
            self.rate_week = Some(w);
        }
        // STL-02 reader half: additive fields. Read every field optionally —
        // never branch on `schema`. Type mismatches yield None (no panic).
        //
        // These bridge-derived fields are assigned UNCONDITIONALLY: the bridge
        // is the source of truth for session state that comes and goes (a PR
        // merges, effort/vim toggle off). An absent/null/wrong-type value
        // yields None and clears the field, so the overlay never shows stale
        // state after Claude stops emitting it (WR-01).
        self.effort = v["effort"].as_str().map(str::to_string);
        self.thinking = v["thinking"].as_bool();
        self.vim_mode = v["vim_mode"].as_str().map(str::to_string);
        // Model precedence is the deliberate exception: the bridge value wins
        // when present, but the guard preserves the transcript-derived model
        // when the bridge omits it (poll() reads the transcript before the
        // bridge — load-bearing). Do NOT make this unconditional.
        if let Some(m) = v["model"].as_str() {
            self.model = Some(m.to_string());
        }
        let p = &v["pr"];
        self.pr = p.is_object().then(|| PrInfo {
            number: p["number"].as_u64(),
            url: p["url"].as_str().map(str::to_string),
            review_state: p["review_state"].as_str().map(str::to_string),
        });
        let w = &v["worktree"];
        self.worktree = w.is_object().then(|| WorktreeInfo {
            name: w["name"].as_str().map(str::to_string),
            path: w["path"].as_str().map(str::to_string),
            branch: w["branch"].as_str().map(str::to_string),
        });
    }

    /// Incrementally parse new hook-event lines since the last poll. Mirrors
    /// `read_transcript_tail`'s offset-tracked, complete-lines-only machinery
    /// but tracks a separate `offset_events` so the transcript tail is never
    /// disturbed. Events drive `hook_status`/`last_tool`/`last_notification`.
    /// Untyped `Value` accessors throughout — a malformed line is skipped and
    /// never panics (T-02-05).
    fn read_event_tail(&mut self) {
        let Some(sid) = &self.session_id else {
            return;
        };
        let path = PathBuf::from(crate::hook::event_path(sid));
        // The session_id can rotate mid-session (a resumed/rotated Claude
        // session), flipping the resolved event-file path. Reset the offset and
        // the event-derived state when that happens so a stale (possibly larger)
        // offset cannot permanently short-circuit the new file's events (WR-03).
        // Mirrors the transcript tail's reset in `resolve_transcript`.
        if self.event_path.as_ref() != Some(&path) {
            self.event_path = Some(path.clone());
            self.offset_events = 0;
            self.hook_status = None;
            self.last_tool = None;
            self.last_notification = None;
            self.last_stop = None;
            self.activity.clear();
        }

        // Gate on mtime AND size. The size is the load-bearing half: this file
        // is append-only, so a growth the mtime failed to register is exactly
        // the case that must still be read (see the field docs).
        let events_meta = fs::metadata(&path).ok();
        let events_mtime = events_meta.as_ref().and_then(|m| m.modified().ok());
        let events_len = events_meta.as_ref().map(|m| m.len());
        if events_mtime.is_some()
            && events_mtime == self.last_events_mtime
            && events_len == self.last_events_len
        {
            return; // Unchanged, skip read
        }

        let Ok(mut f) = fs::File::open(&path) else {
            return;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len <= self.offset_events {
            return;
        }
        if f.seek(SeekFrom::Start(self.offset_events)).is_err() {
            return;
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            return;
        }
        // Only consume complete lines; a partial trailing line is re-read on
        // the next poll.
        let consumed = match buf.rfind('\n') {
            Some(i) => i + 1,
            None => return,
        };
        for line in buf[..consumed].lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let ts = v["ts"].as_u64().unwrap_or(0);
            match v["event"].as_str() {
                Some("UserPromptSubmit") => self.hook_status = Some((true, ts)),
                Some("Stop") => {
                    self.hook_status = Some((false, ts));
                    self.last_stop = Some(ts);
                }
                Some("Notification") => {
                    self.hook_status = Some((false, ts));
                    if let Some(nt) = v["notification_type"].as_str() {
                        self.last_notification = Some((nt.to_string(), ts));
                    }
                }
                Some("PostToolUse") => {
                    self.hook_status = Some((true, ts));
                    if let Some(t) = v["tool"].as_str() {
                        self.last_tool = Some((t.to_string(), ts));
                    }
                }
                _ => {}
            }
            // Append to the activity ring from the SAME parsed line. An
            // event-less line is skipped (same guard the match keys on); the
            // ring is bounded drop-oldest at `ACTIVITY_CAP` (T-03-01).
            if let Some(ev) = v["event"].as_str() {
                self.activity.push_back(HookEvent {
                    event: ev.to_string(),
                    tool: v["tool"].as_str().map(str::to_string),
                    notification_type: v["notification_type"].as_str().map(str::to_string),
                    ts,
                });
                if self.activity.len() > ACTIVITY_CAP {
                    self.activity.pop_front();
                }
            }
        }
        self.offset_events += consumed as u64;
        // Record both halves of the gate after a successful read.
        self.last_events_mtime = events_mtime;
        self.last_events_len = events_len;
    }

    /// Context usage bridge file written by statusline hooks (e.g. the GSD
    /// statusline): /tmp/claude-ctx-<sessionId>.json.
    fn read_context_file(&mut self) {
        let Some(sid) = &self.session_id else {
            return;
        };
        let Some(v) = read_json(&PathBuf::from(format!("/tmp/claude-ctx-{sid}.json"))) else {
            return;
        };
        if let Some(used) = v["used_pct"].as_u64() {
            self.context_used_pct = Some(used.min(100) as u8);
        } else if let Some(rem) = v["remaining_percentage"].as_u64() {
            self.context_used_pct = Some((100 - rem.min(100)) as u8);
        }
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn newest_jsonl(dir: &Path, since_unix_ms: u64) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "jsonl").unwrap_or(true) {
            continue;
        }
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if mtime + 10_000 < since_unix_ms {
            continue;
        }
        if best.as_ref().map(|(m, _)| mtime > *m).unwrap_or(true) {
            best = Some((mtime, path));
        }
    }
    best.map(|(_, p)| p)
}

/// Parse GSD state from `.planning/STATE.md` frontmatter + body.
pub fn parse_gsd(repo_root: &Path) -> Option<GsdState> {
    let text = fs::read_to_string(repo_root.join(".planning").join("STATE.md")).ok()?;
    let mut g = GsdState::default();
    let unquote = |s: &str| s.trim().trim_matches(['"', '\'']).to_string();
    let mut in_frontmatter = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t == "---" {
            if i == 0 {
                in_frontmatter = true;
                continue;
            }
            in_frontmatter = false;
            continue;
        }
        if in_frontmatter {
            if let Some(v) = t.strip_prefix("milestone:") {
                g.milestone = Some(unquote(v));
            } else if let Some(v) = t.strip_prefix("status:") {
                g.status = Some(unquote(v));
            } else if let Some(v) = t.strip_prefix("active_phase:") {
                g.active_phase = Some(unquote(v));
            } else if let Some(v) = t.strip_prefix("next_action:") {
                g.next_action = Some(unquote(v));
            } else if let Some(v) = t.strip_prefix("percent:") {
                g.percent = v.trim().parse().ok();
            }
        } else if t.starts_with("Phase:") || t.starts_with("Progress:") {
            let existing = g.phase_line.take();
            g.phase_line = Some(match existing {
                Some(e) => format!("{e} · {t}"),
                None => t.to_string(),
            });
        }
    }
    Some(g)
}

/// Current branch from .git/HEAD without spawning git. Handles worktrees,
/// where `.git` is a file pointing at the real gitdir.
fn read_git_branch(cwd: &Path) -> Option<String> {
    let mut dir = cwd.to_path_buf();
    let head = loop {
        let dotgit = dir.join(".git");
        if dotgit.is_dir() {
            break dotgit.join("HEAD");
        }
        if dotgit.is_file() {
            // worktree: "gitdir: /path/to/main/.git/worktrees/<name>"
            let text = fs::read_to_string(&dotgit).ok()?;
            let gitdir = text.strip_prefix("gitdir:")?.trim();
            break PathBuf::from(gitdir).join("HEAD");
        }
        if !dir.pop() {
            return None;
        }
    };
    let head = fs::read_to_string(head).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(|b| b.to_string())
        .or(Some("detached".into()))
}

/// "in 46m" / "in 3d 4h" until a unix-seconds timestamp.
pub fn human_until(unix_s: u64) -> String {
    let now_s = now_unix_ms() / 1000;
    let secs = unix_s.saturating_sub(now_s);
    if secs == 0 {
        return "now".into();
    }
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("in {d}d {h}h")
    } else if h > 0 {
        format!("in {h}h {m}m")
    } else {
        format!("in {}m", m.max(1))
    }
}

pub fn human_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Short display form of a model id: "claude-fable-5" → "fable-5".
pub fn short_model(model: &str) -> String {
    model.strip_prefix("claude-").unwrap_or(model).to_string()
}

/// Short display form of a permission mode.
pub fn short_mode(mode: &str) -> &'static str {
    match mode {
        "bypassPermissions" => "bypass",
        "acceptEdits" => "accept",
        "plan" => "plan",
        "default" => "ask",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a bridge file at the path `read_bridge_file` will look for, build
    /// a `ClaudeMeta` pointed at that session id, run the (private) reader, and
    /// return the populated meta. The fixture path is pid+counter-keyed so
    /// parallel tests never collide. Cleans up the temp file before returning.
    fn read_fixture(suffix: &str, json: &str) -> ClaudeMeta {
        let sid = format!("test-{}-{}", std::process::id(), suffix);
        let path = crate::bridge::bridge_path(&sid);
        fs::write(&path, json).unwrap();
        let mut meta = ClaudeMeta {
            session_id: Some(sid),
            ..Default::default()
        };
        meta.read_bridge_file();
        fs::remove_file(&path).ok();
        meta
    }

    #[test]
    fn reads_v2_bridge() {
        let meta = read_fixture(
            "v2",
            r#"{
                "schema": 2,
                "cost_usd": 1.25,
                "context_used_pct": 42.0,
                "updated_unix_ms": 12345,
                "five_hour": {"used_pct": 10.0, "resets_at": 111},
                "seven_day": {"used_pct": 20.0, "resets_at": 222},
                "model": "Claude Opus 4.8",
                "effort": "high",
                "thinking": true,
                "vim_mode": "NORMAL",
                "pr": {"number": 42, "url": "https://example.com/pr/42", "review_state": "approved"},
                "worktree": {"name": "wt", "path": "/tmp/wt", "branch": "feature/x"}
            }"#,
        );

        // new fields populated
        assert_eq!(meta.effort.as_deref(), Some("high"));
        assert_eq!(meta.thinking, Some(true));
        assert_eq!(meta.vim_mode.as_deref(), Some("NORMAL"));
        assert_eq!(meta.model.as_deref(), Some("Claude Opus 4.8"));

        let pr = meta.pr.as_ref().expect("pr present");
        assert_eq!(pr.number, Some(42));
        assert_eq!(pr.url.as_deref(), Some("https://example.com/pr/42"));
        assert_eq!(pr.review_state.as_deref(), Some("approved"));

        let wt = meta.worktree.as_ref().expect("worktree present");
        assert_eq!(wt.name.as_deref(), Some("wt"));
        assert_eq!(wt.path.as_deref(), Some("/tmp/wt"));
        assert_eq!(wt.branch.as_deref(), Some("feature/x"));

        // legacy fields still read
        assert_eq!(meta.session_cost_usd, Some(1.25));
        assert_eq!(meta.context_used_pct, Some(42));
        assert_eq!(meta.rate_updated_unix_ms, 12345);
        assert_eq!(meta.rate_5h.unwrap().used_pct, Some(10.0));
        assert_eq!(meta.rate_week.unwrap().resets_at_unix_s, Some(222));
    }

    #[test]
    fn reads_legacy_bridge() {
        // schema-absent file with ONLY the four legacy fields.
        let meta = read_fixture(
            "legacy",
            r#"{
                "cost_usd": 0.5,
                "context_used_pct": 12.0,
                "updated_unix_ms": 99,
                "five_hour": {"used_pct": 5.0, "resets_at": 1}
            }"#,
        );

        // legacy fields present
        assert_eq!(meta.session_cost_usd, Some(0.5));
        assert_eq!(meta.context_used_pct, Some(12));
        assert_eq!(meta.rate_5h.unwrap().used_pct, Some(5.0));

        // new fields all None (additive back-compat, new-reader-reads-old-file)
        assert!(meta.effort.is_none());
        assert!(meta.thinking.is_none());
        assert!(meta.vim_mode.is_none());
        assert!(meta.pr.is_none());
        assert!(meta.worktree.is_none());
    }

    #[test]
    fn does_not_branch_on_schema() {
        // schema:99 with new fields present → still read (no `if schema == 2`).
        let meta = read_fixture(
            "schema99",
            r#"{
                "schema": 99,
                "effort": "low",
                "thinking": false,
                "vim_mode": "INSERT",
                "pr": {"number": 1}
            }"#,
        );
        assert_eq!(meta.effort.as_deref(), Some("low"));
        assert_eq!(meta.thinking, Some(false));
        assert_eq!(meta.vim_mode.as_deref(), Some("INSERT"));
        assert_eq!(meta.pr.as_ref().unwrap().number, Some(1));
    }

    #[test]
    fn pr_absent_is_none() {
        let meta = read_fixture("nopr", r#"{"schema": 2, "effort": "medium"}"#);
        assert!(meta.pr.is_none());
        assert!(meta.worktree.is_none());
    }

    #[test]
    fn pr_present_review_state_absent() {
        // pr exists but review_state absent → pr Some, review_state None.
        let meta = read_fixture(
            "pr-partial",
            r#"{"schema": 2, "pr": {"number": 7, "url": "u"}}"#,
        );
        let pr = meta.pr.as_ref().expect("pr present");
        assert_eq!(pr.number, Some(7));
        assert_eq!(pr.url.as_deref(), Some("u"));
        assert!(pr.review_state.is_none());
    }

    #[test]
    fn present_then_absent_bridge_fields_clear() {
        // A bridge that emits pr/effort/thinking/vim_mode, then a later write
        // that drops them, must CLEAR the captured state (WR-01). The classic
        // case: a PR merges, the bridge writes `"pr": null`, and the overlay
        // must stop showing the now-closed PR.
        let sid = format!("test-{}-clear", std::process::id());
        let path = crate::bridge::bridge_path(&sid);
        let mut meta = ClaudeMeta {
            session_id: Some(sid),
            ..Default::default()
        };

        // First tick: everything present.
        fs::write(
            &path,
            r#"{
                "schema": 2,
                "effort": "high",
                "thinking": true,
                "vim_mode": "NORMAL",
                "pr": {"number": 42},
                "worktree": {"name": "wt"}
            }"#,
        )
        .unwrap();
        meta.read_bridge_file();
        assert_eq!(meta.effort.as_deref(), Some("high"));
        assert_eq!(meta.thinking, Some(true));
        assert_eq!(meta.vim_mode.as_deref(), Some("NORMAL"));
        assert_eq!(meta.pr.as_ref().unwrap().number, Some(42));
        assert_eq!(meta.worktree.as_ref().unwrap().name.as_deref(), Some("wt"));

        // Second tick: fields gone (pr explicitly null, others absent).
        fs::write(&path, r#"{"schema": 2, "pr": null}"#).unwrap();
        meta.read_bridge_file();
        fs::remove_file(&path).ok();

        assert!(meta.effort.is_none(), "stale effort lingered");
        assert!(meta.thinking.is_none(), "stale thinking lingered");
        assert!(meta.vim_mode.is_none(), "stale vim_mode lingered");
        assert!(meta.pr.is_none(), "stale pr lingered after null");
        assert!(meta.worktree.is_none(), "stale worktree lingered");
    }

    /// Append JSONL records to a transcript fixture and point `meta` at it,
    /// then drive the real incremental reader. Mirrors how `poll()` feeds
    /// `read_transcript_tail` (offset-tracked tail reads), so tests exercise
    /// the same seam production does. Returns the temp path for cleanup.
    fn feed_transcript(meta: &mut ClaudeMeta, suffix: &str, lines: &[&str]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "baude-test-transcript-{}-{}.jsonl",
            std::process::id(),
            suffix
        ));
        let mut body = String::new();
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        fs::write(&path, body).unwrap();
        meta.transcript = Some(path.clone());
        meta.read_transcript_tail();
        path
    }

    #[test]
    fn model_bridge_wins_then_survives() {
        // Drive the model through the REAL transcript path (read_transcript_tail),
        // the way poll() does, so this guards the load-bearing ordering claim —
        // not just the read_bridge_file accessor (WR-02).

        // Case A: transcript sets the model, then the bridge emits its own —
        // bridge wins (poll order: transcript, then bridge).
        let mut meta = ClaudeMeta::default();
        let tpath = feed_transcript(
            &mut meta,
            "model-wins",
            &[r#"{"type":"assistant","message":{"model":"from-transcript"}}"#],
        );
        assert_eq!(
            meta.model.as_deref(),
            Some("from-transcript"),
            "transcript model not read through the real seam"
        );
        let sid = format!("test-{}-model-wins", std::process::id());
        let bpath = crate::bridge::bridge_path(&sid);
        fs::write(&bpath, r#"{"schema": 2, "model": "from-bridge"}"#).unwrap();
        meta.session_id = Some(sid);
        meta.read_bridge_file();
        fs::remove_file(&bpath).ok();
        fs::remove_file(&tpath).ok();
        assert_eq!(meta.model.as_deref(), Some("from-bridge"));

        // Case B: transcript sets the model, the bridge omits it —
        // the transcript value survives (the model guard prevents clobber).
        let mut meta2 = ClaudeMeta::default();
        let tpath2 = feed_transcript(
            &mut meta2,
            "model-survives",
            &[r#"{"type":"assistant","message":{"model":"keep-me"}}"#],
        );
        assert_eq!(meta2.model.as_deref(), Some("keep-me"));
        let sid2 = format!("test-{}-model-survives", std::process::id());
        let bpath2 = crate::bridge::bridge_path(&sid2);
        fs::write(&bpath2, r#"{"schema": 2, "effort": "high"}"#).unwrap();
        meta2.session_id = Some(sid2);
        meta2.read_bridge_file();
        fs::remove_file(&bpath2).ok();
        fs::remove_file(&tpath2).ok();
        assert_eq!(meta2.model.as_deref(), Some("keep-me"));
    }

    /// Write event lines to the path `read_event_tail` will look for (keyed by
    /// `meta.session_id`), then drive the REAL incremental reader the way
    /// `poll()` does — mirroring the `model_bridge_wins_then_survives`
    /// real-seam rigor. Appends (does not truncate) so multi-tick offset tests
    /// accumulate. Returns the event-file path for cleanup.
    fn feed_events(meta: &mut ClaudeMeta, suffix: &str, lines: &[&str]) -> PathBuf {
        let sid = format!("test-{}-{}", std::process::id(), suffix);
        let path = PathBuf::from(crate::hook::event_path(&sid));
        let mut body = String::new();
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        // First feed (meta has no session yet) truncates any stale file from
        // a prior run; subsequent feeds append to extend the same file so the
        // offset semantics are exercised.
        if meta.session_id.is_none() {
            fs::write(&path, &body).unwrap();
        } else {
            let existing = fs::read_to_string(&path).unwrap_or_default();
            fs::write(&path, format!("{existing}{body}")).unwrap();
        }
        meta.session_id = Some(sid);
        meta.read_event_tail();
        path
    }

    #[test]
    fn event_tail_drives_state() {
        // UserPromptSubmit -> Busy; Stop -> Waiting; through the real seam.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "drives-state",
            &[r#"{"schema":1,"event":"UserPromptSubmit","ts":100}"#],
        );
        assert_eq!(meta.hook_status, Some((true, 100)));

        // Same session id (deterministic from suffix) — feed appends.
        feed_events(
            &mut meta,
            "drives-state",
            &[r#"{"schema":1,"event":"Stop","ts":200}"#],
        );
        assert_eq!(meta.hook_status, Some((false, 200)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn stop_event_captures_last_stop() {
        // Stop -> hook_status Waiting AND last_stop records the event ts,
        // symmetric to how Notification records last_notification.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "last-stop",
            &[r#"{"schema":1,"event":"Stop","ts":150}"#],
        );
        assert_eq!(meta.hook_status, Some((false, 150)));
        assert_eq!(meta.last_stop, Some(150));

        // A LATER Stop event updates last_stop (not just the first one).
        feed_events(
            &mut meta,
            "last-stop",
            &[r#"{"schema":1,"event":"Stop","ts":400}"#],
        );
        assert_eq!(meta.last_stop, Some(400));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn last_stop_resets_on_session_id_change() {
        // Mirrors event_tail_resets_offset_on_session_id_change (WR-03): a
        // rotated session_id must not carry a stale last_stop forward.
        let mut meta = ClaudeMeta::default();
        let path_a = feed_events(
            &mut meta,
            "last-stop-rot-a",
            &[r#"{"schema":1,"event":"Stop","ts":100}"#],
        );
        assert_eq!(meta.last_stop, Some(100));

        let path_b = feed_events(
            &mut meta,
            "last-stop-rot-b",
            &[r#"{"schema":1,"event":"UserPromptSubmit","ts":900}"#],
        );
        assert_eq!(meta.last_stop, None, "rotation must clear last_stop");

        fs::remove_file(&path_a).ok();
        fs::remove_file(&path_b).ok();
    }

    #[test]
    fn notification_permission_captured() {
        // Notification -> Waiting AND notification_type captured in meta
        // (last_notification), not exposed as a structured field.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "notif",
            &[
                r#"{"schema":1,"event":"Notification","ts":250,"notification_type":"permission_prompt"}"#,
            ],
        );
        assert_eq!(meta.hook_status, Some((false, 250)));
        assert_eq!(
            meta.last_notification,
            Some(("permission_prompt".to_string(), 250))
        );
        fs::remove_file(&path).ok();
    }

    #[test]
    fn post_tool_use_records_last_tool() {
        // PostToolUse -> stays Busy AND records the tool name + ts.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "posttool",
            &[r#"{"schema":1,"event":"PostToolUse","ts":300,"tool":"Bash"}"#],
        );
        assert_eq!(meta.hook_status, Some((true, 300)));
        assert_eq!(meta.last_tool, Some(("Bash".to_string(), 300)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn event_tail_offset_does_not_reprocess() {
        // A second read over an UNCHANGED file does not re-process lines: the
        // Stop at ts=200 must not be clobbered back to the earlier prompt.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "offset",
            &[
                r#"{"schema":1,"event":"UserPromptSubmit","ts":100}"#,
                r#"{"schema":1,"event":"Stop","ts":200}"#,
            ],
        );
        assert_eq!(meta.hook_status, Some((false, 200)));
        let off_after_first = meta.offset_events;
        // Re-read the same file (no new lines) — no change.
        meta.read_event_tail();
        assert_eq!(meta.hook_status, Some((false, 200)));
        assert_eq!(meta.offset_events, off_after_first);
        // The transcript offset is untouched by the event tail.
        assert_eq!(meta.offset, 0);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn event_tail_defers_partial_line() {
        // A trailing line without a newline is NOT consumed until completed.
        let mut meta = ClaudeMeta::default();
        let sid = format!("test-{}-partial", std::process::id());
        let path = PathBuf::from(crate::hook::event_path(&sid));
        meta.session_id = Some(sid);

        // One complete line + a partial (no newline) trailing line.
        fs::write(
            &path,
            "{\"schema\":1,\"event\":\"Stop\",\"ts\":10}\n{\"schema\":1,\"event\":\"UserPromptSubmit\",\"ts\":20",
        )
        .unwrap();
        meta.read_event_tail();
        // Only the complete Stop line consumed.
        assert_eq!(meta.hook_status, Some((false, 10)));

        // Complete the partial line.
        fs::write(
            &path,
            "{\"schema\":1,\"event\":\"Stop\",\"ts\":10}\n{\"schema\":1,\"event\":\"UserPromptSubmit\",\"ts\":20}\n",
        )
        .unwrap();
        meta.read_event_tail();
        assert_eq!(meta.hook_status, Some((true, 20)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn event_tail_skips_malformed_line() {
        // A malformed JSON line is skipped without panicking; the surrounding
        // valid lines still drive state.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "malformed",
            &[
                r#"{"schema":1,"event":"UserPromptSubmit","ts":100}"#,
                "{ this is not json",
                r#"{"schema":1,"event":"Stop","ts":300}"#,
            ],
        );
        assert_eq!(meta.hook_status, Some((false, 300)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn event_tail_resets_offset_on_session_id_change() {
        // WR-03: when the session_id rotates mid-session the event-file path
        // flips. A stale (here larger) offset must NOT short-circuit the new,
        // shorter file forever — read_event_tail resets the offset and clears
        // event-derived state when the path changes.

        // First session: a longer file that advances the offset well past the
        // length of the second session's file.
        let sid_a = format!("test-{}-reset-a", std::process::id());
        let path_a = PathBuf::from(crate::hook::event_path(&sid_a));
        let mut meta = ClaudeMeta {
            session_id: Some(sid_a),
            ..Default::default()
        };
        let mut body_a = String::new();
        for ts in 0..50 {
            body_a.push_str(&format!(
                r#"{{"schema":1,"event":"PostToolUse","ts":{ts},"tool":"Bash"}}"#
            ));
            body_a.push('\n');
        }
        fs::write(&path_a, &body_a).unwrap();
        meta.read_event_tail();
        assert!(meta.offset_events > 0, "first session advanced the offset");
        assert!(meta.last_tool.is_some());
        let off_a = meta.offset_events;

        // Rotate to a NEW (deterministically distinct, shorter) session id. Its
        // file is far smaller than off_a — without the reset, `len <= offset`
        // would skip it forever.
        let sid_b = format!("test-{}-reset-b", std::process::id());
        let path_b = PathBuf::from(crate::hook::event_path(&sid_b));
        fs::write(
            &path_b,
            "{\"schema\":1,\"event\":\"UserPromptSubmit\",\"ts\":900}\n",
        )
        .unwrap();
        assert!(
            fs::metadata(&path_b).unwrap().len() < off_a,
            "second file must be shorter than the stale offset to prove the bug"
        );
        meta.session_id = Some(sid_b);
        meta.read_event_tail();

        // The new session's event was processed (offset was reset), not dropped.
        assert_eq!(
            meta.hook_status,
            Some((true, 900)),
            "rotated session's events must be read after the offset reset"
        );

        fs::remove_file(&path_a).ok();
        fs::remove_file(&path_b).ok();
    }

    #[test]
    fn activity_ring_caps_drop_oldest() {
        // Feed N = ACTIVITY_CAP + 5 PostToolUse lines; the ring caps at
        // ACTIVITY_CAP, oldest dropped, newest at back.
        let n = ACTIVITY_CAP + 5;
        let mut lines = Vec::with_capacity(n);
        for i in 0..n {
            lines.push(format!(
                r#"{{"schema":1,"event":"PostToolUse","ts":{i},"tool":"Bash"}}"#
            ));
        }
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let mut meta = ClaudeMeta::default();
        let path = feed_events(&mut meta, "ring-cap", &refs);

        assert_eq!(meta.activity().len(), ACTIVITY_CAP);
        // The oldest 5 (ts 0..5) were dropped; front is ts == 5.
        assert_eq!(meta.activity().front().unwrap().ts, 5);
        // Newest is at the back.
        assert_eq!(meta.activity().back().unwrap().ts, (n - 1) as u64);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn activity_ring_records_each_event_in_order() {
        // A mix of event kinds lands one HookEvent per line in arrival order,
        // with tool/notification_type populated only on the relevant kinds.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "ring-mix",
            &[
                r#"{"schema":1,"event":"UserPromptSubmit","ts":10}"#,
                r#"{"schema":1,"event":"PostToolUse","ts":20,"tool":"Bash"}"#,
                r#"{"schema":1,"event":"Stop","ts":30}"#,
                r#"{"schema":1,"event":"Notification","ts":40,"notification_type":"permission"}"#,
            ],
        );

        let act = meta.activity();
        assert_eq!(act.len(), 4);

        assert_eq!(act[0].event, "UserPromptSubmit");
        assert_eq!(act[0].tool, None);
        assert_eq!(act[0].notification_type, None);

        assert_eq!(act[1].event, "PostToolUse");
        assert_eq!(act[1].tool.as_deref(), Some("Bash"));
        assert_eq!(act[1].notification_type, None);

        assert_eq!(act[2].event, "Stop");
        assert_eq!(act[2].tool, None);

        assert_eq!(act[3].event, "Notification");
        assert_eq!(act[3].tool, None);
        assert_eq!(act[3].notification_type.as_deref(), Some("permission"));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn activity_ring_clears_on_path_rotation() {
        // Feed session A, then rotate the event path to session B; the ring is
        // cleared at rotation and reflects only B's events (mirrors WR-03).
        let mut meta = ClaudeMeta::default();
        let path_a = feed_events(
            &mut meta,
            "ring-rot-a",
            &[
                r#"{"schema":1,"event":"PostToolUse","ts":100,"tool":"Bash"}"#,
                r#"{"schema":1,"event":"PostToolUse","ts":200,"tool":"Edit"}"#,
            ],
        );
        assert_eq!(meta.activity().len(), 2);

        // Rotate to a NEW, deterministically distinct session id.
        let path_b = feed_events(
            &mut meta,
            "ring-rot-b",
            &[r#"{"schema":1,"event":"UserPromptSubmit","ts":900}"#],
        );

        let act = meta.activity();
        assert_eq!(act.len(), 1, "ring must be cleared on rotation");
        assert_eq!(act[0].event, "UserPromptSubmit");
        assert_eq!(act[0].ts, 900);

        fs::remove_file(&path_a).ok();
        fs::remove_file(&path_b).ok();
    }

    #[test]
    fn activity_ring_skips_malformed_lines() {
        // A non-JSON line and a line missing `event` are both skipped — they do
        // not push into the ring and do not panic.
        let mut meta = ClaudeMeta::default();
        let path = feed_events(
            &mut meta,
            "ring-malformed",
            &[
                r#"{"schema":1,"event":"PostToolUse","ts":10,"tool":"Bash"}"#,
                r#"not json at all"#,
                r#"{"schema":1,"ts":20}"#, // missing `event`
                r#"{"schema":1,"event":"Stop","ts":30}"#,
            ],
        );

        let act = meta.activity();
        assert_eq!(act.len(), 2, "only the two well-formed events are retained");
        assert_eq!(act[0].event, "PostToolUse");
        assert_eq!(act[1].event, "Stop");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn hook_event_serializes_post_tool_use() {
        // PostToolUse: tool present, notification_type omitted (skip_serializing_if).
        let ev = HookEvent {
            event: "PostToolUse".to_string(),
            tool: Some("Bash".to_string()),
            notification_type: None,
            ts: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""event":"PostToolUse""#), "json: {json}");
        assert!(json.contains(r#""tool":"Bash""#), "json: {json}");
        assert!(json.contains(r#""ts":1700000000000"#), "json: {json}");
        assert!(
            !json.contains("notification_type"),
            "None notification_type must be omitted: {json}"
        );
    }

    #[test]
    fn hook_event_serializes_notification() {
        // Notification: notification_type present, tool omitted.
        let ev = HookEvent {
            event: "Notification".to_string(),
            tool: None,
            notification_type: Some("permission".to_string()),
            ts: 1_700_000_000_001,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""event":"Notification""#), "json: {json}");
        assert!(
            json.contains(r#""notification_type":"permission""#),
            "json: {json}"
        );
        assert!(
            !json.contains(r#""tool""#),
            "None tool must be omitted: {json}"
        );
    }

    #[test]
    fn hook_event_round_trips() {
        // Serialize then deserialize; all four fields preserved.
        let ev = HookEvent {
            event: "PostToolUse".to_string(),
            tool: Some("Edit".to_string()),
            notification_type: None,
            ts: 42,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let back: HookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.event, ev.event);
        assert_eq!(back.tool, ev.tool);
        assert_eq!(back.notification_type, ev.notification_type);
        assert_eq!(back.ts, ev.ts);
    }

    // ---- claude_config_dir isolation (#72, D-03) ----
    //
    // `claude_config_dir()` is the only resolver in this crate that reaches
    // `~/.claude`, Claude Code's own session and transcript store. These cases
    // pin it to the thread's fixture redirect. Every one of them asserts the
    // resolved path BEFORE touching the filesystem, so a regression fails on a
    // path comparison rather than by reading the developer's real directory.

    /// A unique on-disk fixture root. Keyed by pid and a counter so parallel
    /// cases never collide, and rooted in the temp dir because the redirect
    /// short-circuits before any containment check.
    fn isolated_claude_root(label: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "baude-meta-claude-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    #[test]
    fn claude_config_dir_follows_the_thread_redirect() {
        let root = PathBuf::from("/nonexistent/baude-meta-claude-redirect");
        let _redirect = crate::testing::TestRedirect::new(&root);
        assert_eq!(claude_config_dir(), root.join("claude"));
    }

    #[test]
    fn claude_config_dir_nested_scopes_restore_the_outer_root() {
        let outer = PathBuf::from("/nonexistent/baude-meta-claude-outer");
        let inner = PathBuf::from("/nonexistent/baude-meta-claude-inner");
        let _outer = crate::testing::TestRedirect::new(&outer);
        assert_eq!(claude_config_dir(), outer.join("claude"));
        {
            let _inner = crate::testing::TestRedirect::new(&inner);
            assert_eq!(claude_config_dir(), inner.join("claude"));
        }
        assert_eq!(
            claude_config_dir(),
            outer.join("claude"),
            "dropping the inner guard must restore the outer synthetic root, \
             never fall through to a developer-path lookup"
        );
    }

    /// Holds no [`crate::testing::TestRedirect`] at all: the guard must be armed
    /// from the first instruction of the binary rather than by some earlier
    /// fixture's side effect (D-09). The probe is thread-local, so this mutates
    /// no environment variable and needs no serialization.
    #[test]
    #[should_panic(expected = "claude config dir resolved to the real user path")]
    fn claude_config_dir_unguarded_resolution_panics() {
        let _no_root = crate::testing::NoFixtureRoot::new();
        let _escaped = claude_config_dir();
    }

    /// Dropping the LAST guard restores the escape panic — the redirect is a
    /// scope, not a one-way arming switch.
    #[test]
    #[should_panic(expected = "claude config dir resolved to the real user path")]
    fn claude_config_dir_escape_returns_after_the_last_guard_drops() {
        let root = PathBuf::from("/nonexistent/baude-meta-claude-last");
        {
            let _redirect = crate::testing::TestRedirect::new(&root);
            assert_eq!(claude_config_dir(), root.join("claude"));
        }
        let _no_root = crate::testing::NoFixtureRoot::new();
        let _escaped = claude_config_dir();
    }

    /// Both `poll` call sites — `poll_session_file` and `resolve_transcript` —
    /// resolve through `claude_config_dir()`, so one redirect moves the session
    /// file AND the transcript. Neither call site is reshaped and neither gains
    /// an `_at` variant: that is exactly why D-03 isolates by redirect rather
    /// than by parameter.
    #[test]
    fn claude_config_dir_poll_reads_redirected_session_data() {
        let root = isolated_claude_root("poll");
        let _redirect = crate::testing::TestRedirect::new(&root);
        let claude = root.join("claude");
        assert_eq!(
            claude_config_dir(),
            claude,
            "poll must resolve inside the fixture root BEFORE any filesystem read"
        );

        let cwd = root.join("work");
        fs::create_dir_all(&cwd).unwrap();
        let sid = format!("sid-{}", std::process::id());
        let pid = 424_242u32;
        let spawn = 1_700_000_000_000u64;

        fs::create_dir_all(claude.join("sessions")).unwrap();
        fs::write(
            claude.join("sessions").join(format!("{pid}.json")),
            serde_json::json!({
                "sessionId": sid,
                "cwd": cwd.to_string_lossy(),
                "startedAt": spawn,
                "status": "busy",
                "statusUpdatedAt": spawn + 5,
            })
            .to_string(),
        )
        .unwrap();

        let project_dir = claude.join("projects").join(encode_path(&cwd));
        fs::create_dir_all(&project_dir).unwrap();
        let transcript = project_dir.join(format!("{sid}.jsonl"));
        fs::write(
            &transcript,
            "{\"type\":\"assistant\",\"message\":{\"model\":\"fixture-model\",\
             \"usage\":{\"input_tokens\":3,\"output_tokens\":4}}}\n",
        )
        .unwrap();

        let mut meta = ClaudeMeta::default();
        meta.poll(&cwd, Some(pid), spawn, &cwd);

        assert_eq!(meta.session_id.as_deref(), Some(sid.as_str()));
        assert_eq!(meta.claude_status, Some((true, spawn + 5)));
        assert_eq!(meta.model.as_deref(), Some("fixture-model"));
        assert_eq!(meta.totals.input, 3);
        assert_eq!(meta.transcript_path(), Some(transcript.as_path()));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn mtime_gate_tracks_last_session_mtime() {
        // Metadata mtime tracking should prevent reads when mtime unchanged
        let mut meta = ClaudeMeta::default();
        assert_eq!(meta.last_session_mtime, None);
        assert_eq!(meta.last_events_mtime, None);
        assert_eq!(meta.last_session_len, None);
        assert_eq!(meta.last_events_len, None);

        // The hazard the size half exists for: two appends inside one
        // filesystem timestamp tick. Pin the mtime to a fixed value so the
        // test reproduces coarse-granularity behaviour on any filesystem,
        // then confirm a grown file is still read.
        let dir = std::env::temp_dir().join(format!(
            "baude-meta-gate-{}-{}",
            std::process::id(),
            "same-tick"
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("hook-events.jsonl");
        fs::write(&path, b"{\"schema\":1,\"event\":\"Stop\",\"ts\":1}\n").unwrap();
        let frozen = fs::metadata(&path).unwrap().modified().unwrap();
        let first_len = fs::metadata(&path).unwrap().len();
        meta.last_events_mtime = Some(frozen);
        meta.last_events_len = Some(first_len);

        // Append more, then force the mtime back to the recorded value: the
        // file grew but the clock did not move.
        use std::io::Write as _;
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"schema\":1,\"event\":\"Stop\",\"ts\":2}\n")
            .unwrap();
        drop(f);
        let grown_len = fs::metadata(&path).unwrap().len();
        assert!(grown_len > first_len, "the append must have grown the file");

        let mtime_unchanged = Some(frozen) == meta.last_events_mtime;
        let len_unchanged = Some(grown_len) == meta.last_events_len;
        assert!(
            mtime_unchanged && !len_unchanged,
            "the scenario under test is: mtime says unchanged, size says grown"
        );
        assert!(
            !(mtime_unchanged && len_unchanged),
            "an mtime-only gate would skip this read and lose the second Stop event"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // ---- BL-02: the claude session id must resolve for every real spawn ----
    //
    // Fixtures here use the shapes current Claude Code (v2.1.310) writes:
    // `sessions/<pid>.json` carries the PHYSICAL cwd and a per-process
    // `startedAt`, and the transcript sits under the encoding of that physical
    // cwd. A friendlier fixture that reuses the as-given cwd everywhere is
    // exactly what let these defects ship.

    /// A `sessions/<pid>.json` in the shape the real CLI writes.
    fn write_real_session_file(claude: &Path, pid: u32, sid: &str, cwd: &Path, started: u64) {
        fs::create_dir_all(claude.join("sessions")).unwrap();
        fs::write(
            claude.join("sessions").join(format!("{pid}.json")),
            serde_json::json!({
                "pid": pid,
                "sessionId": sid,
                "cwd": cwd.to_string_lossy(),
                "startedAt": started,
                "procStart": "Sat Oct 24 15:19:12 2026",
                "version": "2.1.310",
                "kind": "interactive",
                "entrypoint": "cli",
                "status": "idle",
                "updatedAt": started + 5,
                "statusUpdatedAt": started + 5,
            })
            .to_string(),
        )
        .unwrap();
    }

    /// A transcript with one assistant turn on `model`, under Claude's own
    /// encoding of `physical_cwd`, last written `age` ago.
    fn write_real_transcript(
        claude: &Path,
        physical_cwd: &Path,
        sid: &str,
        model: &str,
        age: std::time::Duration,
    ) -> PathBuf {
        let dir = claude.join("projects").join(encode_path(physical_cwd));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{sid}.jsonl"));
        fs::write(
            &path,
            format!(
                "{{\"type\":\"user\",\"cwd\":{cwd:?},\"sessionId\":\"{sid}\"}}\n\
                 {{\"type\":\"assistant\",\"message\":{{\"model\":\"{model}\"}}}}\n",
                cwd = physical_cwd.to_string_lossy()
            ),
        )
        .unwrap();
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
        path
    }

    /// Vectors computed by running the sanitizer lifted verbatim from the
    /// Claude Code 2.1.310 binary under node.
    #[test]
    fn encode_path_matches_claude_code_sanitizer() {
        let cases = [
            (
                "/Users/joese/.local/share/baude/worktrees/claude/repository-14/primary-14",
                "-Users-joese--local-share-baude-worktrees-claude-repository-14-primary-14",
            ),
            ("/tmp/caf\u{e9} repo/x_y", "-tmp-caf--repo-x-y"),
            // Outside the BMP: two UTF-16 units, so two dashes.
            ("/tmp/rocket-\u{1F680}/r", "-tmp-rocket----r"),
        ];
        for (cwd, want) in cases {
            assert_eq!(encode_path(Path::new(cwd)), want, "encoding of {cwd}");
        }
        let long = format!("/Users/dev/{}repo", "deeply_nested.dir/".repeat(12));
        assert_eq!(
            encode_path(Path::new(&long)),
            "-Users-dev-deeply-nested-dir-deeply-nested-dir-deeply-nested-dir-\
             deeply-nested-dir-deeply-nested-dir-deeply-nested-dir-deeply-nested-dir-\
             deeply-nested-dir-deeply-nested-dir-deeply-nested-dir-deeply-ne-cf4m9y",
            "past 200 chars Claude truncates and appends a base36 hash"
        );
    }

    /// Sub-cause 1: a pane opened through a symlinked path. Claude records
    /// and keys everything by the resolved path, so encoding the as-given cwd
    /// looks in a project dir that does not exist and the model never shows.
    #[test]
    fn symlinked_cwd_resolves_model_from_the_physical_project_dir() {
        let root = isolated_claude_root("symlink-cwd");
        let _redirect = crate::testing::TestRedirect::new(&root);
        let claude = root.join("claude");
        let real = root.join("real-repo");
        fs::create_dir_all(&real).unwrap();
        let link = root.join("linked-repo");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let physical = fs::canonicalize(&link).unwrap();

        let pid = std::process::id();
        let spawn = now_unix_ms();
        let sid = format!("symlink-{pid}");
        write_real_session_file(&claude, pid, &sid, &physical, spawn + 300);
        write_real_transcript(
            &claude,
            &physical,
            &sid,
            "claude-opus-4-8",
            std::time::Duration::ZERO,
        );

        let mut meta = ClaudeMeta::default();
        meta.poll(&link, Some(pid), spawn, &link);

        assert_eq!(meta.session_id.as_deref(), Some(sid.as_str()));
        assert_eq!(meta.model.as_deref(), Some("claude-opus-4-8"));
        fs::remove_dir_all(&root).ok();
    }

    /// Sub-cause 2, as suspected, does not reproduce: Claude stamps
    /// `startedAt` per PROCESS, and the exact-pid path applies no age filter,
    /// so a `--resume` of an old conversation whose session file is far older
    /// than the pane's spawn time still resolves. (On macOS this test also
    /// failed before the fix, but only because `temp_dir()` sits under the
    /// `/var` symlink, which is sub-cause 1; on Linux it guards sub-cause 2.)
    #[test]
    fn resume_with_session_file_older_than_spawn_still_resolves_model() {
        let root = isolated_claude_root("resume-old");
        let _redirect = crate::testing::TestRedirect::new(&root);
        let claude = root.join("claude");
        let cwd = root.join("repo");
        fs::create_dir_all(&cwd).unwrap();
        let physical = fs::canonicalize(&cwd).unwrap();

        let pid = std::process::id();
        let spawn = now_unix_ms();
        let sid = format!("resume-old-{pid}");
        write_real_session_file(&claude, pid, &sid, &physical, spawn - 60_000);
        write_real_transcript(
            &claude,
            &physical,
            &sid,
            "claude-sonnet-4-6",
            std::time::Duration::from_secs(3600),
        );

        let mut meta = ClaudeMeta::default();
        meta.poll(&cwd, Some(pid), spawn, &cwd);

        assert_eq!(meta.model.as_deref(), Some("claude-sonnet-4-6"));
        fs::remove_dir_all(&root).ok();
    }

    /// A session-leader shell that forks one child, the shape of the pane
    /// running `claude --continue || exec claude`. Returns the shell and the
    /// forked child's pid.
    fn spawn_pane_shell_with_forked_child() -> (std::process::Child, u32) {
        use std::io::BufRead;
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", "sleep 20 & echo $!; wait"])
            .stdout(std::process::Stdio::piped());
        // SAFETY: setsid is async-signal-safe; it makes the shell a session
        // leader exactly as portable-pty does for every pane.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut shell = cmd.spawn().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(shell.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let child = line.trim().parse().unwrap();
        (shell, child)
    }

    fn reap(mut shell: std::process::Child, child: u32) {
        // SAFETY: `child` is the sleep this test's own shell forked.
        unsafe {
            libc::kill(child as i32, libc::SIGKILL);
        }
        let _ = shell.kill();
        let _ = shell.wait();
    }

    /// Sub-cause found while reproducing: the continue spawn
    /// `claude --continue 2>/dev/null || exec claude` FORKS its first claude,
    /// so that claude writes `sessions/<its pid>.json` and the pane's own pid
    /// never gets a file. The continued conversation's transcript is older
    /// than the spawn, so the cwd fallback cannot find it either, and model,
    /// cost, context and hook status all stay empty for the life of the pane.
    /// A same-cwd claude in ANOTHER process session must not be adopted.
    #[test]
    fn continue_spawn_resolves_model_from_the_forked_claude_session_file() {
        let root = isolated_claude_root("continue-fork");
        let _redirect = crate::testing::TestRedirect::new(&root);
        let claude = root.join("claude");
        let cwd = root.join("repo");
        fs::create_dir_all(&cwd).unwrap();
        let physical = fs::canonicalize(&cwd).unwrap();

        let spawn = now_unix_ms();
        let (pane, forked) = spawn_pane_shell_with_forked_child();
        let (other_pane, other_forked) = spawn_pane_shell_with_forked_child();

        let sid = format!("continued-{}", std::process::id());
        write_real_session_file(&claude, forked, &sid, &physical, spawn + 200);
        write_real_transcript(
            &claude,
            &physical,
            &sid,
            "claude-opus-4-8",
            std::time::Duration::from_secs(3600),
        );
        let foreign = format!("foreign-{}", std::process::id());
        write_real_session_file(&claude, other_forked, &foreign, &physical, spawn + 900);
        write_real_transcript(
            &claude,
            &physical,
            &foreign,
            "claude-haiku-4-5",
            std::time::Duration::ZERO,
        );

        let mut meta = ClaudeMeta::default();
        meta.poll(&cwd, Some(pane.id()), spawn, &cwd);
        let first = (meta.session_id.clone(), meta.model.clone());
        // A second poll reads the cached member file, not a rescan.
        meta.poll(&cwd, Some(pane.id()), spawn, &cwd);
        let second = meta.model.clone();

        reap(pane, forked);
        reap(other_pane, other_forked);
        fs::remove_dir_all(&root).ok();

        assert_eq!(first.0.as_deref(), Some(sid.as_str()));
        assert_eq!(first.1.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(second.as_deref(), Some("claude-opus-4-8"));
    }

    // ---- BL-02: the child's CLAUDE_CONFIG_DIR differs from baude's ----

    /// Stand-in claude for the two-account tests: an ordinary (non-platform)
    /// binary, because macOS hides the environment of Apple platform binaries
    /// such as `/bin/sleep` while exposing that of user-installed ones like
    /// claude. Runs only when a test launches it by name.
    #[test]
    #[ignore = "stand-in process launched by the two-account tests"]
    fn pane_claude_stand_in() {
        std::thread::sleep(std::time::Duration::from_secs(20));
    }

    /// Shell text that runs [`pane_claude_stand_in`] from this test binary.
    fn stand_in_claude() -> String {
        format!(
            "'{}' --exact meta::tests::pane_claude_stand_in --ignored >/dev/null 2>&1",
            std::env::current_exe().unwrap().display()
        )
    }

    /// A process standing in for a pane, launched with `CLAUDE_CONFIG_DIR`
    /// set to `own` and running `script`. Returns the process and the pid the
    /// script echoes first (the forked child, or the shell itself).
    fn spawn_pane(own: &Path, script: &str) -> (std::process::Child, u32) {
        use std::io::BufRead;
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", script])
            .env("CLAUDE_CONFIG_DIR", own)
            .stdout(std::process::Stdio::piped());
        // SAFETY: setsid is async-signal-safe; portable-pty does the same.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut shell = cmd.spawn().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(shell.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let pid = line.trim().parse().unwrap();
        (shell, pid)
    }

    /// Two-account shape seen on the operator's machine: baude runs with one
    /// `CLAUDE_CONFIG_DIR` while the pane's claude runs with another (an
    /// account launcher, or a per-directory env the login shell applies in
    /// the session cwd). Claude writes `sessions/<pid>.json` and the transcript
    /// under ITS dir, so a lookup in baude's dir never resolves the session.
    /// Exec shape: the pane process itself carries the account's dir.
    #[test]
    fn model_resolves_when_the_pane_claude_uses_another_config_dir() {
        let root = isolated_claude_root("two-accounts-exec");
        let _redirect = crate::testing::TestRedirect::new(&root);
        let own = root.join("claude");
        let account = root.join("claude-account");
        let cwd = root.join("repo");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(own.join("sessions")).unwrap();
        let physical = fs::canonicalize(&cwd).unwrap();

        let spawn = now_unix_ms();
        let (pane, pid) = spawn_pane(&account, &format!("echo $$; exec {}", stand_in_claude()));
        let sid = format!("account-{}", std::process::id());
        write_real_session_file(&account, pid, &sid, &physical, spawn + 300);
        write_real_transcript(
            &account,
            &physical,
            &sid,
            "claude-opus-4-8",
            std::time::Duration::ZERO,
        );

        let mut meta = ClaudeMeta::default();
        meta.poll(&cwd, Some(pid), spawn, &cwd);
        let resolved = (meta.session_id.clone(), meta.model.clone());
        let mut pane = pane;
        let _ = pane.kill();
        let _ = pane.wait();

        assert_eq!(resolved, (Some(sid), Some("claude-opus-4-8".to_string())));
        fs::remove_dir_all(&root).ok();
    }

    /// Per-directory env shape: the pane shell starts with baude's dir, and
    /// only after its startup files run (mise in the session cwd) does the
    /// claude it forks get the account's dir. The pane's own exec-time env
    /// never names the account dir, so the lookup has to read the child's.
    #[test]
    fn model_resolves_when_only_the_forked_claude_uses_another_config_dir() {
        let root = isolated_claude_root("two-accounts-fork");
        let _redirect = crate::testing::TestRedirect::new(&root);
        let own = root.join("claude");
        let account = root.join("claude-account");
        let cwd = root.join("repo");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(own.join("sessions")).unwrap();
        let physical = fs::canonicalize(&cwd).unwrap();

        let spawn = now_unix_ms();
        let script = format!(
            "export CLAUDE_CONFIG_DIR='{}'; {} & echo $!; wait",
            account.display(),
            stand_in_claude()
        );
        let (pane, forked) = spawn_pane(&own, &script);
        let sid = format!("account-fork-{}", std::process::id());
        write_real_session_file(&account, forked, &sid, &physical, spawn + 300);
        write_real_transcript(
            &account,
            &physical,
            &sid,
            "claude-sonnet-4-6",
            std::time::Duration::ZERO,
        );

        let mut meta = ClaudeMeta::default();
        meta.poll(&cwd, Some(pane.id()), spawn, &cwd);
        let first = meta.model.clone();
        meta.poll(&cwd, Some(pane.id()), spawn, &cwd);
        let second = meta.model.clone();
        reap(pane, forked);

        assert_eq!(first.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(second.as_deref(), Some("claude-sonnet-4-6"));
        fs::remove_dir_all(&root).ok();
    }

    /// The macOS `KERN_PROCARGS2` layout: argc, exec path, NUL padding, argv,
    /// then env strings up to the first empty one.
    #[test]
    fn procargs2_env_skips_path_padding_and_argv() {
        let mut buf = 2i32.to_ne_bytes().to_vec();
        buf.extend_from_slice(
            b"/bin/sleep\0\0\0\0sleep\x0020\0A=1\0CLAUDE_CONFIG_DIR=/x/y\0\0junk=1\0",
        );
        let env = procargs2_env(&buf);
        assert_eq!(env, vec![&b"A=1"[..], &b"CLAUDE_CONFIG_DIR=/x/y"[..]]);
        assert_eq!(
            env_entry(&env, "CLAUDE_CONFIG_DIR"),
            Some(std::ffi::OsString::from("/x/y"))
        );
        assert!(procargs2_env(&buf[..2]).is_empty());
    }
}
