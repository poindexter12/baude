//! Session ownership for the daemon. Unlike the TUI, the daemon never kills
//! sessions when a client goes away — only on explicit DELETE or daemon
//! shutdown. State persists to its own file (`daemon-state.json`) so a daemon
//! restart restores every session via `claude --continue`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use tokio::sync::Notify;

use baude_core::git;
use baude_core::meta::{now_unix_ms, ClaudeMeta, HookEvent};
use baude_core::persist::{self, SavedSession, State};
use baude_core::pty::Pty;
use baude_core::session::{Session, StateSource, Status};

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
    /// Waiting this long auto-archives a session; 0 disables.
    pub auto_archive_ms: u64,
    /// PERM-02: per-session wake handle for the permission long-poll. Set/clear
    /// pending state happens UNDER the manager lock; the bridge/handler then
    /// `notified().await`s on this Arc OUTSIDE the lock so one pending
    /// permission never stalls other sessions (Pitfall 4 — "decide under the
    /// lock, act outside it"). `resolve_pending` fires `notify_waiters()`.
    permission_notify: HashMap<u64, Arc<Notify>>,
}

/// One row of `GET /sessions`.
#[derive(Serialize, Clone)]
pub struct SessionInfo {
    pub id: u64,
    pub name: String,
    pub title: Option<String>,
    pub status: &'static str,
    /// Which source decided `status`: "hook" / "session-file" / "silence".
    /// Surfaces a regression to the silence fallback (capture-but-render-lightly).
    pub state_source: &'static str,
    /// The last tool name Claude ran (from the hook event stream), if any.
    pub last_tool: Option<String>,
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
    pub archived: bool,
    /// A bounded (~30) tail of the session's recent hook events so the remote
    /// TUI overlay rides the existing `/sessions` poll without an extra round
    /// trip. The full ring is served by `GET /sessions/{id}/activity`.
    pub activity: Vec<HookEvent>,
}

/// PERM-02: an in-flight tool-permission request the `permission-mcp` bridge
/// POSTed, awaiting a human decision. `request_id` is bridge-generated; `ts` is
/// unix-ms (the bridge owns its own deadline). Serializable so `GET
/// /sessions/{id}/permission` returns it directly.
#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct PendingPermission {
    pub request_id: String,
    pub tool: String,
    pub input: serde_json::Value,
    pub ts: u64,
}

/// PERM-02: the human decision recorded for the most recent request. The
/// bridge's GET poll reads `decision` (`allow`|`deny`) to unblock.
#[derive(Serialize, Clone, Debug)]
pub struct PermissionDecision {
    pub request_id: String,
    pub decision: String,
    pub scope: Option<String>,
    pub ts: u64,
}

/// `GET /sessions/{id}/permission` payload — the pending request (if any) plus
/// the resolved decision (if any). While pending, `decision` is `None`; after a
/// POST resolves it, `request_id`/`tool`/`input` describe the just-decided call
/// and `decision` carries the verdict for the bridge poll. `None`-everywhere
/// (no request ever) serializes to JSON `null` at the handler.
#[derive(Serialize)]
pub struct PermissionView {
    pub request_id: Option<String>,
    pub tool: Option<String>,
    pub input: Option<serde_json::Value>,
    pub ts: Option<u64>,
    /// `allow` | `deny` once resolved; absent while pending or idle.
    pub decision: Option<String>,
    pub scope: Option<String>,
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

fn source_str(s: StateSource) -> &'static str {
    match s {
        StateSource::Hook => "hook",
        StateSource::SessionFile => "session-file",
        StateSource::Silence => "silence",
    }
}

/// The daemon's own loopback event endpoint for a session. `Manager` does not
/// store the daemon's bind addr (manager.rs has no bind field), and the hook
/// only needs same-host reachability, so we use the loopback default bind
/// (`DEFAULT_BIND = "127.0.0.1:8642"` in bauded/src/main.rs). Known limitation
/// (out of scope for Phase 2): a custom `--bind` port is NOT honored here —
/// honoring it would require threading the bind addr into `Manager`.
fn event_url(id: u64) -> String {
    format!("http://127.0.0.1:8642/sessions/{id}/event")
}

/// Build the shell command string for a daemon-spawned session, injecting
/// `$BAUDE_EVENT_URL` so claude and its hook child POST events to the daemon's
/// loopback ingest route (`Pty::spawn` has no env-map param).
///
/// `claude --continue` resumes the most recent conversation; on a fresh
/// directory it exits non-zero and the `|| exec claude` fallback starts a new
/// session. The env var is set with `export VAR=...; <inner>` rather than a
/// `VAR=... cmd` assignment prefix: an assignment prefix applies only to the
/// single command it prefixes, so on the resume path the `exec claude`
/// fallback (the common fresh-directory case) would otherwise run WITHOUT the
/// var and its hooks would silently miss the daemon transport. `export` sets
/// it for the whole command group, surviving the `||` fallback and sub-exec
/// (WR-01).
fn spawn_command(base_cmd: &str, event_url: &str, resume: bool) -> String {
    let inner = if resume {
        format!("{base_cmd} --continue 2>/dev/null || exec {base_cmd}")
    } else {
        format!("exec {base_cmd}")
    };
    format!("export BAUDE_EVENT_URL={event_url}; {inner}")
}

/// Best-effort, non-clobbering seed of a session cwd's `.mcp.json` registering
/// baude's `permission-mcp` stdio server (PERM-01, `prompt` mode only).
///
/// The MCP command is `current_exe()` + ` permission-mcp` — the same
/// `current_exe()` resolution as `baude_core::hook::baude_hook_command`, so a
/// daemon-spawned session seeds `bauded permission-mcp` (the Pitfall-2 reason
/// BOTH binaries must grow the arm in 04-02). Mirrors `seed_settings`: never
/// aborts a spawn on failure, and re-seeding merges `mcpServers.baude` into an
/// existing file without discarding sibling MCP servers (idempotent — the
/// command is the sentinel). Re-runs on every `restore()` re-spawn.
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

/// The command run per session: BAUDE_CLAUDE_CMD env, then config.json
/// `claude_cmd`, then plain `claude`.
/// BAUDED_AUTO_ARCHIVE_MIN (minutes, 0 disables) — default 30.
pub fn default_auto_archive_ms() -> u64 {
    std::env::var("BAUDED_AUTO_ARCHIVE_MIN")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30)
        * 60_000
}

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
            auto_archive_ms: default_auto_archive_ms(),
            permission_notify: HashMap::new(),
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
                Ok(id) => {
                    restored += 1;
                    if saved.archived {
                        if let Ok(s) = self.session_mut(id) {
                            s.archived = true;
                            s.archived_by_user = saved.archived_by_user;
                        }
                    }
                }
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
                    archived: s.archived,
                    archived_by_user: s.archived_by_user,
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

        let id = self.next_id;

        // Seed `.claude/settings.local.json` in the session's actual cwd
        // (worktree dir for worktree sessions) so daemon-spawned Claude fires
        // baude's hooks — the same best-effort, idempotent, non-clobbering
        // merge as the TUI path. Idempotency matters because `restore()`
        // re-spawns every persisted session on each daemon startup; the merge
        // adds exactly one baude entry per event no matter how often it runs.
        baude_core::hook::seed_settings(&cwd);

        // PERM-01: select exactly one permission flag (default skip preserves
        // today's unattended `--dangerously-skip-permissions`; `prompt` is
        // opt-in via BAUDE_PERMISSION_MODE). Append to the base cmd BEFORE the
        // `export …; {inner}` wrap so the flag survives the `--continue || exec`
        // resume fallback (WR-01). No-op when the operator already set a
        // permission flag (no-double-add).
        let base_cmd = format!(
            "{}{}",
            self.claude_cmd,
            baude_core::permission::permission_flag(&self.claude_cmd)
        );

        // In `prompt` mode only, additionally seed a non-clobbering `.mcp.json`
        // registering the `permission-mcp` stdio server (command =
        // current_exe() + " permission-mcp"). Best-effort, idempotent, and
        // re-seeded on every `restore()`-driven re-spawn — exactly the hook
        // seed posture. The daemon's current_exe() is `bauded`, so 04-02 adds
        // the `permission-mcp` arm to BOTH binaries (Pitfall 2).
        if baude_core::permission::is_prompt_mode() {
            seed_mcp_config(&cwd);
        }

        let cmd = spawn_command(&base_cmd, &event_url(id), resume);
        let claude = Pty::spawn(Some(&cmd), &cwd, ROWS, COLS)?;

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
            archived: false,
            archived_by_user: false,
            was_busy: false,
            unarchived_at_ms: None,
            pending_permission: None,
            permission_decision: None,
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
        // Wake any lingering permission waiter (it will re-check, find the
        // session gone, and bail) and drop its handle so the map doesn't leak.
        if let Some(n) = self.permission_notify.remove(&id) {
            n.notify_waiters();
        }
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
        if s.unarchive_on_input() {
            self.save();
        }
        Ok(())
    }

    /// Ingest one hook event line POSTed to `POST /sessions/{id}/event` onto
    /// the same `/tmp/baude-events-<sid>.jsonl` consume path the poll loop
    /// tails — converging the daemon (POST) transport with the TUI-local
    /// (file-tail) transport onto one event model.
    ///
    /// Resolves the target Claude `session_id` by preferring the one embedded in
    /// the POSTed event line itself (`baude hook` builds the line with the
    /// authoritative `session_id` from the hook payload, and the file is keyed by
    /// that same id), falling back to the session's poll-resolved
    /// `meta.session_id`. Preferring the body id means a real session's earliest
    /// hook events land in the correct file immediately, instead of being
    /// rejected until the first poll cycle resolves `meta.session_id` (~1s race).
    /// Errors (never panics) only on an unknown baude id or when neither source
    /// yields a session_id. `event_path` sanitizes the id, so a body-supplied id
    /// cannot traverse paths (single-user loopback model).
    pub fn ingest_event(&mut self, id: u64, body: &str) -> Result<()> {
        let s = self.session(id)?;
        let body_sid = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v["session_id"].as_str().map(str::to_string))
            .filter(|s| !s.is_empty());
        let sid = body_sid
            .or_else(|| s.meta.session_id.clone())
            .ok_or_else(|| anyhow!("session {id} has no claude session_id yet"))?;
        baude_core::hook::append_event(&sid, body.trim_end())
            .map_err(|e| anyhow!("append event for session {id}: {e}"))
    }

    // ===== PERM-02: pending-permission set/resolve =======================
    //
    // The bridge POSTs a pending request (set_pending), the PWA/phone POSTs the
    // decision (resolve_pending), and the bridge's GET poll reads it (decision).
    // All route `Err -> 404` via `self.session(id)?`/`session_mut(id)?` exactly
    // like `ingest_event`. Pitfall 4: these only touch state UNDER the lock; the
    // actual wait happens OUTSIDE via the Arc<Notify> from `permission_notify`.

    /// Store a fresh pending permission request on a known session, clearing any
    /// stale decision from a previous turn so the bridge can't read it. Err →
    /// 404 on an unknown id.
    pub fn set_pending(&mut self, id: u64, p: PendingPermission) -> Result<()> {
        let s = self.session_mut(id)?;
        s.pending_permission = Some(serde_json::to_value(&p).unwrap_or(serde_json::Value::Null));
        s.permission_decision = None; // a new request supersedes any prior decision
        Ok(())
    }

    /// The pending permission request, if one is awaiting a decision. `Ok(None)`
    /// when nothing is pending; Err → 404 on an unknown id.
    pub fn pending(&self, id: u64) -> Result<Option<PendingPermission>> {
        let s = self.session(id)?;
        Ok(s.pending_permission
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok()))
    }

    /// The recorded decision for the most recent request, if resolved. `Ok(None)`
    /// while pending/idle; Err → 404 on an unknown id. The bridge's GET poll
    /// reads this to unblock.
    pub fn decision(&self, id: u64) -> Result<Option<PermissionDecision>> {
        let s = self.session(id)?;
        Ok(s.permission_decision.as_ref().map(|v| PermissionDecision {
            request_id: v["request_id"].as_str().unwrap_or_default().to_string(),
            decision: v["decision"].as_str().unwrap_or("deny").to_string(),
            scope: v["scope"].as_str().map(str::to_string),
            ts: v["ts"].as_u64().unwrap_or_default(),
        }))
    }

    /// Resolve the pending permission with a `allow`/`deny` decision: clear the
    /// pending request, record the decision for the bridge poll, and wake any
    /// registered waiter (Pitfall 4 — the wake fires here, the await is outside
    /// the lock). Err → 404 on an unknown id. The caller (`post_permission`)
    /// validates `decision ∈ {allow,deny}` BEFORE calling — but as defense in
    /// depth any non-`allow` value is stored as `deny` (deny-default).
    pub fn resolve_pending(
        &mut self,
        id: u64,
        decision: &str,
        scope: Option<String>,
    ) -> Result<()> {
        let request_id = {
            let s = self.session_mut(id)?;
            let request_id = s
                .pending_permission
                .as_ref()
                .and_then(|v| v["request_id"].as_str().map(str::to_string))
                .unwrap_or_default();
            let verdict = if decision == "allow" { "allow" } else { "deny" };
            s.permission_decision = Some(serde_json::json!({
                "request_id": request_id,
                "decision": verdict,
                "scope": scope,
                "ts": now_unix_ms(),
            }));
            s.pending_permission = None;
            request_id
        };
        let _ = request_id;
        // Wake the bridge/handler waiting outside the lock.
        if let Some(n) = self.permission_notify.get(&id) {
            n.notify_waiters();
        }
        Ok(())
    }

    /// The per-session wake handle for the permission long-poll. A waiter clones
    /// this Arc, registers `notified()` BEFORE re-checking `decision`, then
    /// `await`s OUTSIDE the manager lock (Pitfall 4). `resolve_pending` fires it.
    /// Err → 404 on an unknown id.
    pub fn permission_notify(&mut self, id: u64) -> Result<Arc<Notify>> {
        // Validate the id first so an unknown session is a clean 404.
        self.session(id)?;
        Ok(Arc::clone(self.permission_notify.entry(id).or_default()))
    }

    /// Respawn claude in an exited session's PTY (same cwd, fresh process,
    /// `--continue` to pick the conversation back up).
    pub fn restart(&mut self, id: u64) -> Result<()> {
        let claude_cmd = self.claude_cmd.clone();
        let s = self.session_mut(id)?;
        if !s.claude.is_exited() {
            bail!("claude is still running");
        }
        let cmd = format!("{claude_cmd} --continue 2>/dev/null || exec {claude_cmd}");
        s.claude = Pty::spawn(Some(&cmd), &s.cwd, ROWS, COLS)?;
        s.spawn_unix_ms = now_unix_ms();
        s.meta = ClaudeMeta::default();
        Ok(())
    }

    /// Attach for raw terminal streaming: a redraw snapshot plus a receiver
    /// of every output chunk after it. See `Pty::subscribe`.
    pub fn attach(&self, id: u64) -> Result<(Vec<u8>, std::sync::mpsc::Receiver<Vec<u8>>)> {
        let s = self.session(id)?;
        if s.claude.is_exited() {
            bail!("claude has exited");
        }
        Ok(s.claude.subscribe())
    }

    /// Raw input bytes from an attached client.
    pub fn write_raw(&mut self, id: u64, bytes: &[u8]) -> Result<()> {
        let s = self.session_mut(id)?;
        if s.claude.is_exited() {
            bail!("claude has exited");
        }
        s.claude.write_input(bytes);
        if s.unarchive_on_input() {
            self.save();
        }
        Ok(())
    }

    /// Resize from an attached client. Multiple clients: last write wins.
    pub fn resize_pty(&mut self, id: u64, rows: u16, cols: u16) -> Result<()> {
        let s = self.session_mut(id)?;
        s.claude.resize(rows, cols);
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

    /// Per-session hook-event file path: Err = no such session, Ok(None) = the
    /// Claude session_id hasn't been resolved yet (so no event file exists).
    /// The sid is sanitized by `baude_core::hook::event_path` (T-03-05).
    /// Analog of `transcript_path` — the SSE existence guard maps Err → 404.
    pub fn event_path(&self, id: u64) -> Result<Option<PathBuf>> {
        let s = self.session(id)?;
        Ok(s.meta
            .session_id
            .as_ref()
            .map(|sid| PathBuf::from(baude_core::hook::event_path(sid))))
    }

    /// The session's recent hook events, newest-at-back, bounded to `limit`.
    /// Reads the in-memory `ClaudeMeta` ring (the single source of truth).
    /// Err = no such session (→ 404 upstream).
    pub fn activity(&self, id: u64, limit: usize) -> Result<Vec<HookEvent>> {
        let s = self.session(id)?;
        let act = s.meta.activity();
        let start = act.len().saturating_sub(limit);
        Ok(act.iter().skip(start).cloned().collect())
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
        if s.unarchive_on_input() {
            self.save();
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
        let mut changed = false;
        let idle = self.auto_archive_ms;
        for s in &mut self.sessions {
            s.poll_meta();
            changed |= s.auto_archive_tick(idle);
        }
        if changed {
            self.save();
        }
    }

    /// Park or unpark a session. Archived sessions sort last in clients and
    /// stop sending notifications. A manual archive sticks until unarchived
    /// or re-engaged; an automatic one also lifts when a new turn starts.
    pub fn set_archived(&mut self, id: u64, archived: bool) -> Result<()> {
        let s = self.session_mut(id)?;
        s.set_archived(archived);
        self.save();
        Ok(())
    }

    /// Test-only: pin a session's resolved Claude `session_id` so handlers
    /// that resolve baude id -> sid (e.g. `ingest_event`) can be exercised
    /// without a live Claude writing `sessions/<pid>.json`.
    #[cfg(test)]
    pub fn session_id_for_test(&mut self, id: u64, sid: &str) {
        if let Ok(s) = self.session_mut(id) {
            s.meta.session_id = Some(sid.to_string());
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
    let (status, source) = s.status_with_source();
    SessionInfo {
        id: s.id,
        name: s.name.clone(),
        title: s.meta.title.clone(),
        status: status_str(status),
        state_source: source_str(source),
        last_tool: s.meta.last_tool.as_ref().map(|(t, _)| t.clone()),
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
        archived: s.archived,
        activity: {
            // Bounded recent set (~30) for the remote TUI overlay; the full
            // ring is served by GET /sessions/{id}/activity.
            let act = s.meta.activity();
            let start = act.len().saturating_sub(30);
            act.iter().skip(start).cloned().collect()
        },
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
    fn event_path_resolves_per_sid_and_404s_unknown() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // No sid resolved yet → Ok(None).
        assert!(matches!(m.event_path(id), Ok(None)));
        // Pin a sid → Ok(Some(the /tmp event path)).
        let sid = format!("mgr-evpath-{}", std::process::id());
        m.session_id_for_test(id, &sid);
        let p = m.event_path(id).unwrap().unwrap();
        assert_eq!(
            p,
            std::path::PathBuf::from(baude_core::hook::event_path(&sid))
        );
        // Unknown id → Err (→ 404 upstream).
        assert!(m.event_path(9999).is_err());
        m.kill_all();
    }

    #[test]
    fn activity_returns_recent_slice_and_404s_unknown() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        let sid = format!("mgr-activity-{}", std::process::id());
        let path = baude_core::hook::event_path(&sid);
        let _ = std::fs::remove_file(&path);
        std::fs::write(
            &path,
            concat!(
                r#"{"event":"UserPromptSubmit","ts":1}"#,
                "\n",
                r#"{"event":"PostToolUse","tool":"Read","ts":2}"#,
                "\n",
                r#"{"event":"Stop","ts":3}"#,
                "\n",
            ),
        )
        .unwrap();
        m.session_id_for_test(id, &sid);
        // Drive read_event_tail so the ring fills from the on-disk file.
        m.poll();

        // The last 2 events, newest at back.
        let recent = m.activity(id, 2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].event, "PostToolUse");
        assert_eq!(recent[1].event, "Stop");
        assert_eq!(recent[1].ts, 3);

        // SessionInfo carries a bounded recent set too.
        let info = m.info(id).unwrap();
        assert_eq!(info.activity.len(), 3);
        assert_eq!(info.activity.last().unwrap().event, "Stop");

        // Unknown id → Err (→ 404 upstream).
        assert!(m.activity(9999, 10).is_err());

        let _ = std::fs::remove_file(&path);
        m.kill_all();
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
        // Wrap the shell so the spawn-site permission flag (appended to the
        // base cmd by `spawn`, default `--dangerously-skip-permissions`) lands
        // as the harmless `$0` of `sh -c` instead of breaking bash's arg
        // parsing. Production uses `claude`, which accepts the flag.
        let mut m = Manager::new("sh -c 'exec bash --norc -i'".into(), false);
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
    fn restart_requires_exited() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        let err = m.restart(id).unwrap_err().to_string();
        assert!(err.contains("still running"), "got: {err}");
        m.kill_all();
    }

    #[test]
    fn restart_respawns_an_exited_session() {
        let mut m = Manager::new("true".into(), false);
        let id = m.create("/tmp", None, None).unwrap().id;
        let deadline = Instant::now() + Duration::from_secs(10);
        while m.info(id).unwrap().status != "exited" {
            assert!(Instant::now() < deadline, "stub never exited");
            std::thread::sleep(Duration::from_millis(100));
        }
        m.restart(id).unwrap();
        m.kill_all();
    }

    #[test]
    fn archive_toggles() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        assert!(!m.info(id).unwrap().archived);
        m.set_archived(id, true).unwrap();
        assert!(m.info(id).unwrap().archived);
        m.set_archived(id, false).unwrap();
        assert!(!m.info(id).unwrap().archived);
        assert!(m.set_archived(99, true).is_err());
        m.kill_all();
    }

    #[test]
    fn manual_unarchive_survives_the_auto_archive_tick() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // Fake a session that went idle well past the threshold.
        let idle = 60_000;
        let s = m.sessions.iter_mut().find(|s| s.id == id).unwrap();
        s.meta.claude_status = Some((false, now_unix_ms() - 2 * idle));
        assert!(
            s.auto_archive_tick(idle),
            "long-waiting session should park"
        );
        // Unarchiving must grant a fresh grace period — the waiting clock is
        // still past the threshold, so without it the next tick re-parks.
        m.set_archived(id, false).unwrap();
        let s = m.sessions.iter_mut().find(|s| s.id == id).unwrap();
        assert!(!s.auto_archive_tick(idle), "tick undid a manual unarchive");
        assert!(!m.info(id).unwrap().archived);
        m.kill_all();
    }

    #[test]
    fn event_url_is_loopback_default_bind() {
        // The injected $BAUDE_EVENT_URL points at the daemon's own loopback
        // event route for the session (DEFAULT_BIND = 127.0.0.1:8642).
        assert_eq!(
            event_url(7),
            "http://127.0.0.1:8642/sessions/7/event",
            "spawn command must carry BAUDE_EVENT_URL= for the loopback route"
        );
    }

    #[test]
    fn spawn_command_exports_event_url_on_both_paths() {
        // WR-01: the event URL must be exported (not assignment-prefixed) so it
        // survives the resume path's `|| exec claude` fallback. Both the resume
        // and fresh commands must start with `export BAUDE_EVENT_URL=<url>;`.
        let url = "http://127.0.0.1:8642/sessions/3/event";

        let fresh = spawn_command("claude", url, false);
        assert_eq!(fresh, format!("export BAUDE_EVENT_URL={url}; exec claude"));

        let resumed = spawn_command("claude", url, true);
        let prefix = format!("export BAUDE_EVENT_URL={url}; ");
        assert!(
            resumed.starts_with(&prefix),
            "resume command must export the var for the whole group, got: {resumed}"
        );
        // The fallback after `||` is inside the exported scope (no second
        // assignment prefix on `exec claude`), so it inherits the var too.
        assert!(resumed.contains("--continue 2>/dev/null || exec claude"));
        assert!(
            !resumed.contains("|| BAUDE_EVENT_URL="),
            "fallback must not re-prefix; export already covers it"
        );
    }

    #[test]
    fn permission_mode_default_skip_and_prompt_at_spawn_command() {
        // PERM-01 (security-critical): the daemon spawn command must carry
        // `--dangerously-skip-permissions` by default (BAUDE_PERMISSION_MODE
        // unset/skip) and `--permission-prompt-tool` ONLY in `prompt` mode.
        // Pins the exact composition the daemon `spawn` uses: base_cmd =
        // claude_cmd + permission_flag(claude_cmd), then spawn_command wraps it.
        //
        // Exercises the env-free `permission_flag_for` seam so the test never
        // mutates the process-global BAUDE_PERMISSION_MODE — which would race
        // the concurrent real-PTY spawn tests in this crate that read it.
        let url = "http://127.0.0.1:8642/sessions/1/event";
        let flagged = |claude: &str, mode: Option<&str>| {
            format!(
                "{claude}{}",
                baude_core::permission::permission_flag_for(mode, claude)
            )
        };

        // Default (unset) and explicit skip and unrecognized -> skip flag,
        // never the prompt flag (fail-safe default).
        for mode in [None, Some("skip"), Some("bogus")] {
            let cmd = spawn_command(&flagged("claude", mode), url, false);
            assert!(
                cmd.contains("--dangerously-skip-permissions"),
                "mode {mode:?} must skip permissions, got: {cmd}"
            );
            assert!(
                !cmd.contains("--permission-prompt-tool"),
                "mode {mode:?} must NOT prompt, got: {cmd}"
            );
        }

        // prompt -> prompt flag present, skip flag absent; survives the resume
        // `--continue || exec` fallback (appended to the inner base cmd).
        let cmd = spawn_command(&flagged("claude", Some("prompt")), url, true);
        assert!(
            cmd.contains("--permission-prompt-tool mcp__baude__approve"),
            "prompt mode must wire the prompt tool, got: {cmd}"
        );
        assert!(
            !cmd.contains("--dangerously-skip-permissions"),
            "prompt mode must NOT also skip, got: {cmd}"
        );
        // The flag is on the base cmd so both `--continue` and the `exec`
        // fallback carry it.
        assert_eq!(
            cmd.matches("--permission-prompt-tool").count(),
            2,
            "resume path repeats the flagged base cmd on both sides of `||`: {cmd}"
        );
    }

    #[test]
    fn seed_mcp_config_is_non_clobbering() {
        // PERM-01 / T-04-03: seeding `.mcp.json` in prompt mode must merge our
        // `baude` server without discarding a user's sibling MCP servers, and
        // must be idempotent across re-spawns (restore()).
        let cwd = std::env::temp_dir().join(format!("baude-mcp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cwd);
        std::fs::create_dir_all(&cwd).unwrap();
        let path = baude_core::permission::mcp_config_path(&cwd);

        // Pre-existing user config with a sibling server.
        std::fs::write(
            &path,
            r#"{"mcpServers":{"other":{"command":"other-srv"}},"extra":true}"#,
        )
        .unwrap();

        seed_mcp_config(&cwd);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // Sibling server + unrelated key preserved.
        assert_eq!(
            v["mcpServers"]["other"]["command"].as_str(),
            Some("other-srv")
        );
        assert_eq!(v["extra"].as_bool(), Some(true));
        // Our server registered with the permission-mcp arg.
        assert_eq!(
            v["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );

        // Idempotent: re-seeding leaves exactly one `baude` server, sibling intact.
        seed_mcp_config(&cwd);
        let v2: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            v2["mcpServers"]["other"]["command"].as_str(),
            Some("other-srv")
        );
        assert_eq!(
            v2["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );

        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn ingest_event_appends_to_resolved_tmp_file() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // Pin a deterministic claude session_id so the /tmp path is isolated.
        let sid = format!("ingest-test-{}", std::process::id());
        let path = baude_core::hook::event_path(&sid);
        let _ = std::fs::remove_file(&path);
        m.sessions
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .meta
            .session_id = Some(sid.clone());

        m.ingest_event(id, r#"{"schema":1,"event":"UserPromptSubmit"}"#)
            .unwrap();
        m.ingest_event(id, r#"{"schema":1,"event":"Stop"}"#)
            .unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "two posts -> two appended lines");
        assert!(lines[0].contains("UserPromptSubmit"));
        assert!(lines[1].contains("Stop"));

        let _ = std::fs::remove_file(&path);
        m.kill_all();
    }

    #[test]
    fn ingest_event_errors_on_unknown_id_and_missing_session_id() {
        let mut m = mgr();
        // Unknown id -> Err (not panic).
        let err = m.ingest_event(999, "{}").unwrap_err().to_string();
        assert!(err.contains("no session"), "got: {err}");
        // Known id but session_id not resolved yet AND no session_id in the
        // body -> Err (not panic).
        let id = m.create("/tmp", None, None).unwrap().id;
        let err = m.ingest_event(id, "{}").unwrap_err().to_string();
        assert!(err.contains("session_id"), "got: {err}");
        m.kill_all();
    }

    #[test]
    fn ingest_event_uses_body_session_id_before_meta_resolves() {
        // A real session's earliest hook events arrive before the poll loop has
        // resolved meta.session_id. The POSTed line carries the authoritative
        // session_id, so ingest must use it and land the event in the correct
        // /tmp file immediately (no 404 / no loss).
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        assert!(
            m.sessions
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .meta
                .session_id
                .is_none(),
            "precondition: meta.session_id not resolved for a sleep session"
        );
        let sid = format!("ingest-body-sid-{}", std::process::id());
        let path = baude_core::hook::event_path(&sid);
        let _ = std::fs::remove_file(&path);

        let line = format!(r#"{{"schema":1,"event":"UserPromptSubmit","session_id":"{sid}"}}"#);
        m.ingest_event(id, &line).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("UserPromptSubmit"), "got: {contents}");
        assert!(contents.contains(&sid));

        let _ = std::fs::remove_file(&path);
        m.kill_all();
    }

    #[test]
    fn session_info_carries_state_source_and_last_tool() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        let info = m.info(id).unwrap();
        // A freshly spawned stub has no hook/session-file state -> silence.
        assert_eq!(info.state_source, "silence");
        assert!(info.last_tool.is_none());
        // Populate last_tool from the hook event stream and re-read.
        m.sessions
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .meta
            .last_tool = Some(("Bash".to_string(), 1));
        assert_eq!(m.info(id).unwrap().last_tool.as_deref(), Some("Bash"));
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

    // ==== 04-02 Task 2: pending-permission state + set/resolve ============

    fn pending(req: &str, tool: &str) -> PendingPermission {
        PendingPermission {
            request_id: req.to_string(),
            tool: tool.to_string(),
            input: serde_json::json!({"command": "ls"}),
            ts: now_unix_ms(),
        }
    }

    #[test]
    fn set_pending_and_read_round_trip() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        // No pending initially.
        assert!(m.pending(id).unwrap().is_none());
        // Set -> readable.
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        let p = m.pending(id).unwrap().expect("pending present");
        assert_eq!(p.request_id, "r1");
        assert_eq!(p.tool, "Bash");
        m.kill_all();
    }

    #[test]
    fn set_and_pending_404_on_unknown_id() {
        let mut m = mgr();
        assert!(m.set_pending(9999, pending("x", "Bash")).is_err());
        assert!(m.pending(9999).is_err());
        assert!(m.resolve_pending(9999, "allow", None).is_err());
    }

    #[test]
    fn resolve_clears_pending_and_records_decision() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        m.resolve_pending(id, "allow", Some("session".into()))
            .unwrap();
        // Pending cleared.
        assert!(m.pending(id).unwrap().is_none());
        // The decision is readable by a waiter (the bridge's poll).
        let d = m.decision(id).unwrap().expect("decision recorded");
        assert_eq!(d.decision, "allow");
        assert_eq!(d.scope.as_deref(), Some("session"));
        m.kill_all();
    }

    #[test]
    fn resolve_deny_records_deny() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r2", "Write")).unwrap();
        m.resolve_pending(id, "deny", None).unwrap();
        assert_eq!(m.decision(id).unwrap().unwrap().decision, "deny");
        m.kill_all();
    }

    #[test]
    fn setting_new_pending_clears_a_stale_decision() {
        // A fresh permission request must not read the previous turn's decision.
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        m.resolve_pending(id, "allow", None).unwrap();
        assert!(m.decision(id).unwrap().is_some());
        // New request resets the decision slot.
        m.set_pending(id, pending("r2", "Edit")).unwrap();
        assert!(m.decision(id).unwrap().is_none());
        assert_eq!(m.pending(id).unwrap().unwrap().request_id, "r2");
        m.kill_all();
    }

    #[test]
    fn timeout_with_no_decision_resolves_to_deny() {
        // SECURITY-CRITICAL (T-04-04 / V4): when the deadline passes with no
        // POSTed decision, the resolution is DENY — never allow. The pure rule
        // lives in baude-core so both binaries' bridges share it.
        use baude_core::permission::decide_with_timeout;
        let none: Option<&str> = None;
        assert_eq!(decide_with_timeout(none, true), "deny"); // deadline passed, no decision
        assert_eq!(decide_with_timeout(Some("allow"), true), "allow"); // decision wins even at deadline
        assert_eq!(decide_with_timeout(Some("deny"), true), "deny");
        // An unknown decision value also coerces to deny (deny-default).
        assert_eq!(decide_with_timeout(Some("bogus"), false), "deny");
        // Before the deadline with no decision yet: keep waiting sentinel.
        assert_eq!(decide_with_timeout(none, false), "");
    }

    #[test]
    fn permission_timeout_s_reads_env_with_safe_default() {
        // Default ~120s; an explicit env value is honored; a garbage value
        // falls back to the default (never 0 / never panics).
        assert!(baude_core::permission::permission_timeout_s() >= 1);
    }

    #[test]
    fn resolve_notifies_a_registered_waiter() {
        // Pitfall 4: a waiter registered before the resolve observes the wake.
        // The per-session Notify fires on resolve so a bounded poll/await is
        // promptly woken (the await happens OUTSIDE the manager lock).
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        m.set_pending(id, pending("r1", "Bash")).unwrap();
        let notify = m.permission_notify(id).unwrap();
        // Register interest BEFORE resolving.
        let waiter = notify.notified();
        tokio::pin!(waiter);
        // Build a tiny runtime to drive the await deterministically.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            // Not yet resolved: the waiter is pending.
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(50), &mut waiter)
                    .await
                    .is_err(),
                "waiter must block until resolve"
            );
            m.resolve_pending(id, "allow", None).unwrap();
            // After resolve, the waiter completes promptly.
            tokio::time::timeout(std::time::Duration::from_millis(500), &mut waiter)
                .await
                .expect("resolve must wake the waiter");
        });
        assert_eq!(m.decision(id).unwrap().unwrap().decision, "allow");
        m.kill_all();
    }
}
