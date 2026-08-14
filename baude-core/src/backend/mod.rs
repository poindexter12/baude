//! The backend seam: what baude knows about the AI CLI it manages, behind one
//! trait, so a second CLI (opencode — see `.planning/spikes/`) can slot in as
//! a sibling implementation without another refactor.
//!
//! Today there is exactly one implementation, [`claude::ClaudeBackend`], and
//! [`active`] always returns it; backend *selection* (config/env) arrives with
//! the second backend, not before.
//!
//! What routes through the trait now — the surfaces both spawn paths (TUI
//! `App::add_session`, daemon `Manager::spawn`) and the poll loop consume:
//!
//! - [`Backend::default_cmd`] — the binary when no `*_cmd` config/env is set.
//! - [`Backend::resolve_cmd`] — permission-mode flag resolution (PERM-01).
//! - [`Backend::shell_command`] — the PTY spawn command, including the
//!   resume-with-fallback wrap and the `$BAUDE_EVENT_URL` export (WR-01).
//! - [`Backend::prepare_cwd`] — best-effort per-cwd wiring before spawn
//!   (lifecycle-hook seeding, prompt-mode permission-MCP seeding).
//! - [`Backend::poll_meta`] — filling [`crate::meta::ClaudeMeta`] from the
//!   backend's own artifacts.
//!
//! Deliberately NOT behind the trait yet (still Claude-direct; absorb them
//! when the second backend actually needs a seam there, not speculatively):
//!
//! - the `hook` / `statusline` / `permission-mcp` subcommand arms in both
//!   binaries — transport endpoints *invoked by* the Claude CLI, meaningless
//!   to a server-API backend like opencode;
//! - `bauded`'s transcript JSONL parsing (`transcript.rs`) — opencode serves
//!   messages over HTTP, so the seam there is a different read model;
//! - the TUI usage pane's `ccusage` invocation (`baude/src/usage.rs`);
//! - `permission::is_prompt_mode` at the TUI warning site
//!   (`warn_prompt_mode_without_daemon`).

pub mod claude;

use std::path::Path;

use crate::meta::ClaudeMeta;
use crate::permission::ResolvedCmd;

pub trait Backend: Send + Sync {
    /// Stable identifier for display and (future) config selection.
    fn name(&self) -> &'static str;

    /// The command run per session when neither the `BAUDE_CLAUDE_CMD` env var
    /// nor config.json `claude_cmd` overrides it.
    fn default_cmd(&self) -> &'static str;

    /// Apply the `BAUDE_PERMISSION_MODE` permission-mode policy to the
    /// operator's base command (PERM-01). `skip` (the unattended default)
    /// vs opt-in `prompt`; see [`crate::permission::resolve_claude_cmd`].
    fn resolve_cmd(&self, base_cmd: &str) -> ResolvedCmd;

    /// The full shell command handed to [`crate::pty::Pty::spawn`].
    ///
    /// `resume` wraps the resolved command in the backend's
    /// resume-with-fresh-fallback form. `event_url` (daemon spawns only)
    /// is exported — not assignment-prefixed — so it survives the fallback
    /// after `||` and any sub-`exec` (WR-01); the TUI passes `None` and gets
    /// the bare command, which routes hook events to the `/tmp` append path.
    fn shell_command(&self, resolved_cmd: &str, event_url: Option<&str>, resume: bool) -> String;

    /// Best-effort per-cwd wiring so a spawned session reports back to baude:
    /// lifecycle-event hooks, plus the permission-MCP registration in `prompt`
    /// mode. Idempotent and non-clobbering — re-run on every restore-driven
    /// re-spawn — and a failure must NEVER abort a spawn (the session just
    /// falls back to the silence status path).
    fn prepare_cwd(&self, cwd: &Path);

    /// Refresh `meta` from the backend's on-disk/live artifacts for one
    /// session. Called from the poll loop via [`crate::session::Session::poll_meta`].
    fn poll_meta(
        &self,
        meta: &mut ClaudeMeta,
        cwd: &Path,
        pid: Option<u32>,
        spawn_unix_ms: u64,
        repo_root: &Path,
    );
}

/// The active backend. Always Claude Code today — selection plumbing lands
/// with the second backend.
pub fn active() -> &'static dyn Backend {
    &claude::ClaudeBackend
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_is_claude() {
        assert_eq!(active().name(), "claude");
        assert_eq!(active().default_cmd(), "claude");
    }
}
