//! The Claude Code backend — the original (and currently only) implementation
//! of [`Backend`]. A thin composition layer: the actual Claude-specific
//! machinery stays in the modules that already own it ([`crate::hook`],
//! [`crate::bridge`], [`crate::meta`], [`crate::permission`]); this impl is
//! the single choke point the binaries call so those modules stop being
//! spawn-path API.

use std::path::Path;

use super::{Backend, SpawnPlan};
use crate::meta::ClaudeMeta;
use crate::permission::ResolvedCmd;

pub struct ClaudeBackend;

impl Backend for ClaudeBackend {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn default_cmd(&self) -> &'static str {
        "claude"
    }

    fn resolve_cmd(&self, base_cmd: &str) -> ResolvedCmd {
        crate::permission::resolve_claude_cmd_env(base_cmd)
    }

    /// `claude --continue` resumes the most recent conversation; on a fresh
    /// directory it exits non-zero and the `|| exec claude` fallback starts a
    /// new session. The env var is set with `export VAR=...; <inner>` rather
    /// than a `VAR=... cmd` assignment prefix: an assignment prefix applies
    /// only to the single command it prefixes, so on the resume path the
    /// `exec claude` fallback (the common fresh-directory case) would
    /// otherwise run WITHOUT the var and its hooks would silently miss the
    /// daemon transport. `export` sets it for the whole command group,
    /// surviving the `||` fallback and sub-exec (WR-01).
    fn spawn_plan(&self, resolved_cmd: &str, event_url: Option<&str>, resume: bool) -> SpawnPlan {
        let inner = if resume {
            format!("{resolved_cmd} --continue 2>/dev/null || exec {resolved_cmd}")
        } else {
            format!("exec {resolved_cmd}")
        };
        let cmd = match event_url {
            Some(url) => format!("export BAUDE_EVENT_URL={url}; {inner}"),
            None => inner,
        };
        SpawnPlan {
            cmd,
            server_port: None,
        }
    }

    /// Seed `.claude/settings.local.json` so the spawned Claude fires baude's
    /// lifecycle hooks, and — in `prompt` mode only — a non-clobbering
    /// `.mcp.json` registering the `permission-mcp` stdio server. Both seeds
    /// resolve `current_exe()`, so a daemon-spawned session wires `bauded`
    /// and a TUI one wires `baude` (the Pitfall-2 reason both binaries carry
    /// the `hook`/`permission-mcp` arms).
    fn prepare_cwd(&self, cwd: &Path) {
        crate::hook::seed_settings(cwd);
        if crate::permission::is_prompt_mode() {
            seed_mcp_config(cwd);
        }
    }

    fn poll_meta(
        &self,
        meta: &mut ClaudeMeta,
        cwd: &Path,
        pid: Option<u32>,
        spawn_unix_ms: u64,
        repo_root: &Path,
    ) {
        meta.poll(cwd, pid, spawn_unix_ms, repo_root);
    }

    /// The `permission-mcp` bridge fails CLOSED (denies every tool) when no
    /// daemon injects `$BAUDE_EVENT_URL` — a bare-TUI prompt-mode session
    /// would silently deny everything, so the TUI must warn (WR-01).
    fn prompt_mode_needs_daemon(&self) -> bool {
        true
    }
}

/// Best-effort, non-clobbering seed of a session cwd's `.mcp.json` registering
/// baude's `permission-mcp` stdio server (PERM-01, `prompt` mode only).
///
/// The MCP command is `current_exe()` + ` permission-mcp` (same resolution as
/// [`crate::hook::baude_hook_command`]). Mirrors `seed_settings`: never aborts
/// a spawn on failure, and re-seeding merges `mcpServers.baude` into an
/// existing file via the pure `merge_mcp_config` without discarding a user's
/// sibling MCP servers (idempotent — the command is the sentinel). Previously
/// duplicated byte-identically in `baude/src/app.rs` and `bauded/src/manager.rs`;
/// this is now the single copy.
fn seed_mcp_config(cwd: &Path) {
    let exe = match std::env::current_exe() {
        Ok(p) => p.display().to_string(),
        Err(_) => return, // can't resolve the bridge command — best-effort skip.
    };
    let path = crate::permission::mcp_config_path(cwd);
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let merged = crate::permission::merge_mcp_config(&existing, &exe);
    let _ = std::fs::write(&path, merged.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- spawn_plan -----------------------------------------------------
    //
    // These pin the exact spawn strings both binaries relied on before the
    // seam (moved from bauded/src/manager.rs's spawn_command tests): the
    // daemon path (event_url = Some) and the TUI path (event_url = None).

    #[test]
    fn spawn_plan_exports_event_url_on_both_paths() {
        // WR-01: the event URL must be exported (not assignment-prefixed) so it
        // survives the resume path's `|| exec claude` fallback. Both the resume
        // and fresh commands must start with `export BAUDE_EVENT_URL=<url>;`.
        let url = "http://127.0.0.1:8642/sessions/3/event";

        let fresh = ClaudeBackend.spawn_plan("claude", Some(url), false).cmd;
        assert_eq!(fresh, format!("export BAUDE_EVENT_URL={url}; exec claude"));

        let resumed = ClaudeBackend.spawn_plan("claude", Some(url), true).cmd;
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
    fn spawn_plan_tui_path_has_no_export() {
        // TUI sessions get NO $BAUDE_EVENT_URL (only the daemon injects it),
        // which routes hook events to the /tmp append path. Exact strings the
        // TUI spawn used before the seam.
        assert_eq!(
            ClaudeBackend.spawn_plan("claude", None, false).cmd,
            "exec claude"
        );
        assert_eq!(
            ClaudeBackend.spawn_plan("claude", None, true).cmd,
            "claude --continue 2>/dev/null || exec claude"
        );
    }

    #[test]
    fn permission_mode_default_skip_and_prompt_at_spawn_plan() {
        // PERM-01 (security-critical): the spawn command must carry
        // `--dangerously-skip-permissions` by default (BAUDE_PERMISSION_MODE
        // unset/skip) and `--permission-prompt-tool` ONLY in `prompt` mode.
        // Pins the exact composition both spawn paths use: base_cmd =
        // claude_cmd + permission_flag(claude_cmd), then spawn_plan wraps it.
        //
        // Exercises the env-free `resolve_claude_cmd` seam so the test never
        // mutates the process-global BAUDE_PERMISSION_MODE — which would race
        // concurrently-running tests that read it.
        let url = "http://127.0.0.1:8642/sessions/1/event";
        let flagged = |claude: &str, mode: Option<&str>| {
            crate::permission::resolve_claude_cmd(mode, claude).cmd
        };

        // Default (unset) and explicit skip and unrecognized -> skip flag,
        // never the prompt flag (fail-safe default).
        for mode in [None, Some("skip"), Some("bogus")] {
            let cmd = ClaudeBackend
                .spawn_plan(&flagged("claude", mode), Some(url), false)
                .cmd;
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
        let cmd = ClaudeBackend
            .spawn_plan(&flagged("claude", Some("prompt")), Some(url), true)
            .cmd;
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

        // BL-04: a claude_cmd that already bakes in --dangerously-skip-permissions
        // must NOT suppress prompt mode — the skip is stripped and the prompt
        // flag wins (explicit opt-in), with no skip flag left in the spawn cmd.
        let cmd = ClaudeBackend
            .spawn_plan(
                &flagged("claude --dangerously-skip-permissions", Some("prompt")),
                Some(url),
                false,
            )
            .cmd;
        assert!(
            cmd.contains("--permission-prompt-tool mcp__baude__approve"),
            "BL-04: prompt must win over a baked-in skip flag, got: {cmd}"
        );
        assert!(
            !cmd.contains("--dangerously-skip-permissions"),
            "BL-04: the conflicting skip flag must be stripped, got: {cmd}"
        );
    }

    // ---- seed_mcp_config ------------------------------------------------

    #[test]
    fn seed_mcp_config_is_non_clobbering() {
        // PERM-01 / T-04-03: seeding `.mcp.json` in prompt mode must merge our
        // `baude` server without discarding a user's sibling MCP servers, and
        // must be idempotent across re-spawns (restore()). Moved from
        // bauded/src/manager.rs when the duplicated seeder collapsed here.
        let cwd = std::env::temp_dir().join(format!("baude-core-mcp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cwd);
        std::fs::create_dir_all(&cwd).unwrap();
        let path = crate::permission::mcp_config_path(&cwd);

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
}
