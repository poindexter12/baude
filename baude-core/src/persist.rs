use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct State {
    pub sessions: Vec<SavedSession>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedSession {
    pub name: String,
    pub cwd: PathBuf,
    pub repo_root: PathBuf,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub shell_open: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub archived_by_user: bool,
}

fn config_base() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baude")
}

fn state_path(file: &str) -> PathBuf {
    config_base().join(file)
}

/// User configuration, ~/.config/baude/config.json. All fields optional.
#[derive(Deserialize, Default)]
pub struct Config {
    /// Command run for each session; BAUDE_CLAUDE_CMD overrides this.
    /// Example: "claude --dangerously-skip-permissions"
    pub claude_cmd: Option<String>,
    /// Prefill for the new-session path prompt, e.g. "~/Code/github.com".
    /// Defaults to the directory baude was launched from.
    pub new_session_dir: Option<String>,
    /// Base directory for the `c` clone prompt's default destination,
    /// laid out as `<base>/<host>/<owner>/<repo>`. Defaults to "~/Code".
    pub clone_base_dir: Option<String>,
    /// Command used by the sidebar `e` key to open a session's folder.
    /// The session cwd is appended as an argument. BAUDE_EDITOR_CMD overrides
    /// this. Defaults to "code".
    pub editor_cmd: Option<String>,
    /// Base URL of a remote bauded daemon whose sessions appear in the
    /// sidebar, e.g. "http://bauded:8642". BAUDE_DAEMON_URL overrides.
    pub daemon_url: Option<String>,
    /// Minutes of idle waiting before a session auto-archives; 0 disables
    /// auto-archiving. BAUDED_AUTO_ARCHIVE_MIN overrides. Defaults to 30.
    pub auto_archive_minutes: Option<u64>,
    /// When true, baude auto-starts a local bauded on startup if one is not
    /// already running, and routes new-session creation through it so sessions
    /// survive TUI restarts. BAUDE_AUTO_DAEMON=1 overrides.
    #[serde(default)]
    pub auto_daemon: bool,
    /// Which AI-CLI backend to manage: "claude" (default) or "opencode".
    /// Global — every session in this baude/bauded process uses the same
    /// backend. BAUDE_BACKEND overrides. Unknown values fall back to claude.
    pub backend: Option<String>,
}

impl Config {
    /// Resolved auto-archive idle window in ms: BAUDED_AUTO_ARCHIVE_MIN env
    /// (minutes, 0 disables), then `auto_archive_minutes`, then 30 minutes.
    pub fn auto_archive_ms(&self) -> u64 {
        std::env::var("BAUDED_AUTO_ARCHIVE_MIN")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .or(self.auto_archive_minutes)
            .map(|min| min * 60_000)
            .unwrap_or(crate::session::AUTO_ARCHIVE_IDLE_MS)
    }
}

pub fn load_config() -> Config {
    std::fs::read_to_string(config_base().join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn load() -> State {
    load_named("state.json")
}

pub fn save(state: &State) -> Result<()> {
    save_named("state.json", state)
}

/// Load session state from a specific file under the config dir. The TUI and
/// the daemon keep separate files so they never clobber each other's sessions.
pub fn load_named(file: &str) -> State {
    std::fs::read_to_string(state_path(file))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_named(file: &str, state: &State) -> Result<()> {
    let path = state_path(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_archive_ms_resolves_env_then_config_then_default() {
        std::env::remove_var("BAUDED_AUTO_ARCHIVE_MIN");
        let mut c = Config::default();
        assert_eq!(c.auto_archive_ms(), crate::session::AUTO_ARCHIVE_IDLE_MS);
        c.auto_archive_minutes = Some(5);
        assert_eq!(c.auto_archive_ms(), 5 * 60_000);
        c.auto_archive_minutes = Some(0);
        assert_eq!(c.auto_archive_ms(), 0, "0 disables auto-archiving");
        std::env::set_var("BAUDED_AUTO_ARCHIVE_MIN", "1");
        assert_eq!(c.auto_archive_ms(), 60_000, "env overrides config");
        std::env::remove_var("BAUDED_AUTO_ARCHIVE_MIN");
    }
}
