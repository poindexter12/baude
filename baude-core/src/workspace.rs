//! Workspaces: named, hard-separated session contexts, each BOUND to one
//! backend so claude and opencode session pools can never mix — not on
//! restore, not through a shared daemon.
//!
//! A workspace owns its persisted session state (`state-<name>.json` /
//! `daemon-state-<name>.json` under the baude config dir) and pins the
//! backend every session in it uses. Two implicit workspaces exist with zero
//! config — `claude` and `opencode`, each bound to the backend of the same
//! name — and custom ones are declared in config.json:
//!
//! ```json
//! {
//!   "workspace": "oss",
//!   "workspaces": {
//!     "oss":  { "backend": "opencode", "daemon_port": 8650 },
//!     "work": { "backend": "claude", "daemon_url": "http://bauded:8642" }
//!   }
//! }
//! ```
//!
//! Selection: `BAUDE_WORKSPACE` env, then config `workspace`, then the
//! backend name (`BAUDE_BACKEND` / config `backend` / `claude`) — so
//! `BAUDE_BACKEND=opencode` alone lands in the `opencode` workspace and gets
//! separated state automatically. A workspace's backend BINDING WINS over
//! `BAUDE_BACKEND` (that is the whole point: the env var can't cross-wire a
//! workspace onto the wrong backend; a conflict warns and is ignored).
//!
//! Back-compat: the `claude` workspace falls back to reading the legacy
//! un-suffixed `state.json` / `daemon-state.json` when its own file does not
//! exist yet, so pre-workspace session lists survive the upgrade (saves go to
//! the new name).

use std::sync::OnceLock;

use crate::backend::{self, Backend};
use crate::persist::Config;

pub struct Workspace {
    pub name: String,
    pub backend: &'static dyn Backend,
    /// Per-workspace remote daemon URL (config `workspaces.<n>.daemon_url`);
    /// falls back to the global `daemon_url` at the call site.
    pub daemon_url: Option<String>,
    /// Explicit auto-daemon port (config `workspaces.<n>.daemon_port`).
    pub daemon_port: Option<u16>,
}

/// The default workspace/backend name, and the only one whose state files
/// have a legacy un-suffixed form.
pub const DEFAULT: &str = "claude";

impl Workspace {
    /// Session-state filename for one of the two state kinds (`"state"` for
    /// the TUI, `"daemon-state"` for bauded).
    pub fn state_file(&self, base: &str) -> String {
        format!("{base}-{}.json", self.name)
    }

    /// The pre-workspace filename this workspace may fall back to READING
    /// (never writing): only the `claude` workspace has one.
    pub fn legacy_state_file(&self, base: &str) -> Option<String> {
        (self.name == DEFAULT).then(|| format!("{base}.json"))
    }

    /// Human-facing label: the platform's product name, prefixed by the
    /// workspace name when it adds information. An implicit workspace named
    /// after its backend reads as just the platform ("Claude Code" /
    /// "opencode"); a custom one reads as "oss · opencode" so both the pool
    /// and the platform it operates on are visible at a glance.
    pub fn display_label(&self) -> String {
        if self.name == self.backend.name() {
            self.backend.display_name().to_string()
        } else {
            format!("{} · {}", self.name, self.backend.display_name())
        }
    }

    /// The loopback port `auto_daemon` uses for this workspace. Implicit
    /// workspaces get stable defaults (claude keeps the historical 8642);
    /// custom workspaces must set `daemon_port` in config — `None` here means
    /// auto_daemon cannot safely pick one.
    pub fn auto_daemon_port(&self) -> Option<u16> {
        self.daemon_port.or(match self.name.as_str() {
            "claude" => Some(8642),
            "opencode" => Some(8643),
            _ => None,
        })
    }
}

/// Keep workspace names filesystem- and URL-safe: anything outside
/// `[A-Za-z0-9_-]` becomes `-`.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Pure resolution from explicit inputs — the testable core of [`active`].
/// `warn` receives human-readable conflict/fallback messages (the binaries
/// route it to stderr; tests capture it).
pub fn resolve(
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    config: &Config,
    mut warn: impl FnMut(String),
) -> Workspace {
    let name = sanitize(
        ws_env
            .or(config.workspace.as_deref())
            .or(backend_env)
            .or(config.backend.as_deref())
            .unwrap_or(DEFAULT),
    );
    let entry = config
        .workspaces
        .as_ref()
        .and_then(|m| m.get(&name))
        .cloned()
        .unwrap_or_default();

    // Backend: the workspace's explicit binding wins; an implicit workspace
    // named after a backend binds to it; otherwise fall through the plain
    // backend chain. Unknown names fall back to claude (with a warning).
    let bound = entry
        .backend
        .clone()
        .or_else(|| match name.as_str() {
            "claude" | "opencode" => Some(name.clone()),
            _ => None,
        })
        .or_else(|| backend_env.map(str::to_string))
        .or_else(|| config.backend.clone());
    let be = backend::backend_for(bound.as_deref());
    if let Some(b) = bound.as_deref() {
        if b != be.name() {
            warn(format!(
                "unknown backend {b:?} for workspace {name:?} — using {}",
                be.name()
            ));
        }
    }
    // The binding is authoritative: a conflicting BAUDE_BACKEND is ignored,
    // loudly — silently honoring it would re-open the cross-wiring hole
    // workspaces exist to close.
    if let (Some(bound), Some(env)) = (entry.backend.as_deref(), backend_env) {
        if bound != env {
            warn(format!(
                "workspace {name:?} is bound to backend {bound:?} — ignoring BAUDE_BACKEND={env:?}"
            ));
        }
    }

    Workspace {
        name,
        backend: be,
        daemon_url: entry.daemon_url,
        daemon_port: entry.daemon_port,
    }
}

/// The active workspace for this process: resolved once from
/// `BAUDE_WORKSPACE`/`BAUDE_BACKEND`/config and cached (the poll loop reads
/// it every tick via [`backend::active`]).
pub fn active() -> &'static Workspace {
    static ACTIVE: OnceLock<Workspace> = OnceLock::new();
    ACTIVE.get_or_init(|| {
        resolve(
            std::env::var("BAUDE_WORKSPACE").ok().as_deref(),
            std::env::var("BAUDE_BACKEND").ok().as_deref(),
            &crate::persist::load_config(),
            |msg| eprintln!("baude: {msg}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::WorkspaceConfig;
    use std::collections::HashMap;

    fn cfg(workspaces: &[(&str, WorkspaceConfig)]) -> Config {
        Config {
            workspaces: Some(
                workspaces
                    .iter()
                    .map(|(n, w)| (n.to_string(), w.clone()))
                    .collect::<HashMap<_, _>>(),
            ),
            ..Config::default()
        }
    }

    fn no_warn(msg: String) {
        panic!("unexpected warning: {msg}");
    }

    #[test]
    fn default_is_claude_workspace() {
        let ws = resolve(None, None, &Config::default(), no_warn);
        assert_eq!(ws.name, "claude");
        assert_eq!(ws.backend.name(), "claude");
        assert_eq!(ws.state_file("state"), "state-claude.json");
        assert_eq!(ws.legacy_state_file("state").as_deref(), Some("state.json"));
        assert_eq!(ws.auto_daemon_port(), Some(8642));
    }

    #[test]
    fn backend_env_lands_in_implicit_workspace() {
        // BAUDE_BACKEND=opencode alone → the `opencode` workspace: separated
        // state with zero new configuration.
        let ws = resolve(None, Some("opencode"), &Config::default(), no_warn);
        assert_eq!(ws.name, "opencode");
        assert_eq!(ws.backend.name(), "opencode");
        assert_eq!(ws.state_file("daemon-state"), "daemon-state-opencode.json");
        // Only claude has a legacy fallback.
        assert_eq!(ws.legacy_state_file("daemon-state"), None);
        assert_eq!(ws.auto_daemon_port(), Some(8643));
    }

    #[test]
    fn bound_workspace_wins_over_backend_env() {
        let cfg = cfg(&[(
            "oss",
            WorkspaceConfig {
                backend: Some("opencode".into()),
                daemon_port: Some(8650),
                ..Default::default()
            },
        )]);
        let mut warned = Vec::new();
        let ws = resolve(Some("oss"), Some("claude"), &cfg, |m| warned.push(m));
        assert_eq!(ws.name, "oss");
        assert_eq!(ws.backend.name(), "opencode", "binding must win");
        assert_eq!(ws.auto_daemon_port(), Some(8650));
        assert_eq!(warned.len(), 1, "conflict must warn: {warned:?}");
        assert!(warned[0].contains("ignoring BAUDE_BACKEND"));
    }

    #[test]
    fn custom_workspace_without_binding_follows_backend_chain() {
        let cfg = cfg(&[("scratch", WorkspaceConfig::default())]);
        let ws = resolve(Some("scratch"), Some("opencode"), &cfg, no_warn);
        assert_eq!(ws.name, "scratch");
        assert_eq!(ws.backend.name(), "opencode");
        // No implicit port for custom workspaces — auto_daemon must be
        // configured explicitly, never guessed.
        assert_eq!(ws.auto_daemon_port(), None);
    }

    #[test]
    fn unknown_names_sanitize_and_fail_safe() {
        let mut warned = Vec::new();
        let cfg = cfg(&[(
            "weird",
            WorkspaceConfig {
                backend: Some("codex".into()),
                ..Default::default()
            },
        )]);
        let ws = resolve(Some("weird"), None, &cfg, |m| warned.push(m));
        assert_eq!(ws.backend.name(), "claude", "unknown backend → claude");
        assert!(warned[0].contains("unknown backend"));
        // Path-hostile names can't escape the config dir.
        let ws = resolve(Some("../evil name"), None, &Config::default(), no_warn);
        assert_eq!(ws.name, "---evil-name");
        assert_eq!(ws.state_file("state"), "state----evil-name.json");
    }

    #[test]
    fn display_label_shows_platform_and_custom_name() {
        // Implicit workspaces read as just the platform product name —
        // "claude" the id renders as "Claude Code" the product.
        let ws = resolve(None, None, &Config::default(), no_warn);
        assert_eq!(ws.display_label(), "Claude Code");
        let ws = resolve(None, Some("opencode"), &Config::default(), no_warn);
        assert_eq!(ws.display_label(), "opencode");
        // Custom names show pool AND platform.
        let oss_cfg = cfg(&[(
            "oss",
            WorkspaceConfig {
                backend: Some("opencode".into()),
                ..Default::default()
            },
        )]);
        let ws = resolve(Some("oss"), None, &oss_cfg, no_warn);
        assert_eq!(ws.display_label(), "oss · opencode");
        let work_cfg = cfg(&[(
            "work",
            WorkspaceConfig {
                backend: Some("claude".into()),
                ..Default::default()
            },
        )]);
        let ws = resolve(Some("work"), None, &work_cfg, no_warn);
        assert_eq!(ws.display_label(), "work · Claude Code");
    }

    #[test]
    fn undeclared_workspace_name_still_resolves() {
        // Using a workspace name with no config entry is fine — it gets its
        // own state namespace and the default backend chain.
        let ws = resolve(Some("side"), None, &Config::default(), no_warn);
        assert_eq!(ws.name, "side");
        assert_eq!(ws.backend.name(), "claude");
        assert_eq!(ws.legacy_state_file("state"), None);
    }
}
