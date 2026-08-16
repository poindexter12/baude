//! The backend seam: what baude knows about the AI CLI it manages, behind one
//! trait. Two implementations exist — [`claude::ClaudeBackend`] (the default)
//! and [`opencode::OpencodeBackend`] — selected process-globally by
//! [`active`]: `BAUDE_BACKEND` env, then config.json `backend`, then claude.
//! Selection is global, not per-session: every session in one baude/bauded
//! process uses the same backend.
//!
//! What routes through the trait — the surfaces the spawn paths (TUI
//! `App::add_session`/`restart_session`, daemon `Manager::spawn`/`restart`)
//! and the poll loop consume:
//!
//! - [`Backend::default_cmd`] — the binary when no `*_cmd` config/env is set.
//! - [`Backend::resolve_cmd`] — permission-mode flag resolution (PERM-01):
//!   claude `--dangerously-skip-permissions` / `--permission-prompt-tool`,
//!   opencode `--auto`.
//! - [`Backend::spawn_plan`] — the PTY spawn command (resume wrap, claude's
//!   `$BAUDE_EVENT_URL` export (WR-01), opencode's pinned `--port` +
//!   prompt-mode `OPENCODE_CONFIG_CONTENT` export) plus the allocated
//!   server port, if the backend runs one.
//! - [`Backend::prepare_cwd`] — best-effort per-cwd wiring before spawn
//!   (claude: hook + prompt-mode MCP seeding; opencode: nothing — all its
//!   wiring rides the spawn command env/flags).
//! - [`Backend::poll_meta`] — filling [`crate::meta::ClaudeMeta`] from the
//!   backend's artifacts (claude: on-disk session/transcript/bridge files;
//!   opencode: the session server's HTTP API).
//! - [`Backend::prompt_mode_needs_daemon`] — whether `prompt` mode is
//!   unusable without a daemon (claude's MCP bridge fails closed; opencode's
//!   own TUI prompt still works locally).
//!
//! Deliberately NOT behind the trait (claude-direct; absorb when a second
//! backend actually needs a seam there, not speculatively): the
//! `hook`/`statusline`/`permission-mcp` subcommand arms in both binaries,
//! `bauded`'s transcript JSONL parsing, and the TUI usage pane's `ccusage`
//! invocation. The daemon-side opencode permission bridge lives in
//! `bauded/src/permission_bridge.rs` (it is daemon plumbing, not backend
//! policy).

pub mod claude;
pub mod opencode;

use std::path::Path;

use crate::meta::ClaudeMeta;
use crate::permission::ResolvedCmd;

/// What a backend hands the spawn site for one session.
pub struct SpawnPlan {
    /// The full shell command for [`crate::pty::Pty::spawn`].
    pub cmd: String,
    /// Local port of the per-session backend server baked into `cmd`
    /// (opencode `--port`); `None` for backends without one (claude).
    /// The spawn site stores it in [`ClaudeMeta::backend_port`].
    pub server_port: Option<u16>,
}

pub trait Backend: Send + Sync {
    /// Stable identifier for display and config selection.
    fn name(&self) -> &'static str;

    /// The command run per session when neither the `BAUDE_CLAUDE_CMD` env var
    /// nor config.json `claude_cmd` overrides it.
    fn default_cmd(&self) -> &'static str;

    /// Apply the `BAUDE_PERMISSION_MODE` permission-mode policy to the
    /// operator's base command (PERM-01). `skip` (the unattended default)
    /// vs opt-in `prompt`.
    fn resolve_cmd(&self, base_cmd: &str) -> ResolvedCmd;

    /// Build the spawn command (and allocate the backend server port, if any)
    /// for one session.
    ///
    /// `resume` selects the backend's resume form. `event_url` (daemon spawns
    /// only) is claude's hook transport — exported, not assignment-prefixed,
    /// so it survives the resume fallback (WR-01); backends without hooks
    /// ignore it. The TUI passes `None`.
    fn spawn_plan(&self, resolved_cmd: &str, event_url: Option<&str>, resume: bool) -> SpawnPlan;

    /// Best-effort per-cwd wiring so a spawned session reports back to baude.
    /// Idempotent and non-clobbering — re-run on every restore-driven
    /// re-spawn — and a failure must NEVER abort a spawn.
    fn prepare_cwd(&self, cwd: &Path);

    /// Refresh `meta` from the backend's on-disk/live artifacts for one
    /// session. Called from the poll loop via
    /// [`crate::session::Session::poll_meta`].
    fn poll_meta(
        &self,
        meta: &mut ClaudeMeta,
        cwd: &Path,
        pid: Option<u32>,
        spawn_unix_ms: u64,
        repo_root: &Path,
    );

    /// Whether `prompt` mode is unusable without a daemon. Claude's
    /// `permission-mcp` bridge fails CLOSED (denies every tool) with no
    /// daemon, so the TUI warns loudly; opencode still prompts in its own
    /// TUI, so a daemon only adds remote approval on top.
    fn prompt_mode_needs_daemon(&self) -> bool;
}

static CLAUDE: claude::ClaudeBackend = claude::ClaudeBackend;
static OPENCODE: opencode::OpencodeBackend = opencode::OpencodeBackend;

/// Pure name → backend resolution. Unknown names fall back to claude
/// (fail-safe: never a panic on a typo'd config; `active` warns once).
pub fn backend_for(name: Option<&str>) -> &'static dyn Backend {
    match name {
        Some("opencode") => &OPENCODE,
        _ => &CLAUDE,
    }
}

/// The active backend for this process: whatever backend the active
/// WORKSPACE is bound to ([`crate::workspace::active`] — `BAUDE_WORKSPACE` /
/// `BAUDE_BACKEND` / config, workspace binding wins). Cached — the poll loop
/// calls this every tick.
pub fn active() -> &'static dyn Backend {
    crate::workspace::active().backend
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_for_selects_and_fails_safe() {
        assert_eq!(backend_for(None).name(), "claude");
        assert_eq!(backend_for(Some("claude")).name(), "claude");
        assert_eq!(backend_for(Some("opencode")).name(), "opencode");
        // Unknown values must fall back to claude, never panic.
        assert_eq!(backend_for(Some("codex")).name(), "claude");
        assert_eq!(backend_for(Some("")).name(), "claude");
    }

    #[test]
    fn default_cmds() {
        assert_eq!(backend_for(None).default_cmd(), "claude");
        assert_eq!(backend_for(Some("opencode")).default_cmd(), "opencode");
    }
}
