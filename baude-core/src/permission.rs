//! Permission-mode spawn-flag selection — the pure, testable half of PERM-01.
//!
//! A per-deploy `BAUDE_PERMISSION_MODE = skip | prompt` env var (default
//! `skip`) selects exactly one permission flag for the spawned `claude`
//! command:
//!
//! - `skip` (the unattended default) appends `--dangerously-skip-permissions`,
//!   preserving today's behavior.
//! - `prompt` (strictly opt-in) appends `--permission-prompt-tool
//!   mcp__baude__approve` and — at the spawn sites, NOT here — seeds a
//!   `.mcp.json` registering the `permission-mcp` stdio server.
//!
//! SECURITY-CRITICAL (PERM-01 / T-04-01): `prompt` is reachable ONLY by the
//! exact literal `"prompt"`; an unset var and ANY unrecognized value fall back
//! to `skip` (fail-safe — never accidentally gate tool execution behind a
//! phone). A regression making `prompt` the default would block overnight runs
//! and is a high-severity finding.
//!
//! Like [`crate::hook`], this module is the PURE half: no HTTP, no process
//! spawning, no filesystem writes. The env read happens here (it is a pure
//! read of process state); the `current_exe()` resolution and the actual
//! `.mcp.json` write live in the binaries (the same core/binary split as
//! `hook::seed_settings` vs `hook::baude_hook_command`). Every value is built
//! via `serde_json::json!` so a build never panics.

use std::path::{Path, PathBuf};

/// The flag appended to a base `claude` command in `skip` mode (and the
/// default). Leading space lets the caller append directly to `base_cmd`.
const SKIP_FLAG: &str = " --dangerously-skip-permissions";

/// The flag appended in `prompt` mode. The MCP tool name follows the standard
/// `mcp__<server>__<tool>` form (research §A); the `baude` server exposing the
/// `approve` tool is registered via the seeded `.mcp.json`.
const PROMPT_FLAG: &str = " --permission-prompt-tool mcp__baude__approve";

/// Select the single permission flag to append to `base_cmd`.
///
/// No-double-add (locked decision / T-04-02): if `base_cmd` already carries any
/// permission flag (`--dangerously-skip-permissions`, `--permission-prompt-tool`,
/// or `--permission-mode`), return `""` so an operator-set flag is never
/// duplicated or overridden.
///
/// Otherwise read `BAUDE_PERMISSION_MODE`: the exact literal `"prompt"` returns
/// [`PROMPT_FLAG`]; every other case (unset, `"skip"`, and any unrecognized
/// value) returns [`SKIP_FLAG`]. The two real flags are NEVER returned
/// together — exactly one of `{skip, prompt, ""}`.
pub fn permission_flag(_base_cmd: &str) -> &'static str {
    // RED stub — implemented in the GREEN step.
    ""
}

/// `true` iff `prompt` mode is active (the exact literal `"prompt"`).
///
/// The spawn sites use this to decide whether to additionally seed `.mcp.json`.
/// Mirrors the fail-safe of [`permission_flag`]: only the exact `"prompt"`
/// literal is `prompt` mode; everything else is `skip`.
pub fn is_prompt_mode() -> bool {
    // RED stub — implemented in the GREEN step.
    false
}

/// Build the `.mcp.json` body registering the `baude` permission MCP server.
///
/// Returns `{"mcpServers":{"baude":{"command":<exe>,"args":["permission-mcp"]}}}`.
/// `exe` is the absolute `current_exe()` path the CALLER resolves (mirroring
/// [`crate::hook::baude_hook_command`]) — core stays pure and never calls
/// `current_exe()`. Building the Value never panics.
pub fn mcp_server_config(_exe: &str) -> serde_json::Value {
    // RED stub — implemented in the GREEN step.
    serde_json::json!({})
}

/// The seeded `.mcp.json` location for a session cwd. Both spawn sites agree on
/// this so the daemon (which re-spawns on `restore`) and the TUI write the same
/// file.
pub fn mcp_config_path(cwd: &Path) -> PathBuf {
    cwd.join(".mcp.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `BAUDE_PERMISSION_MODE` is process-global; serialize the env-mutating
    /// tests so parallel cases never race the same var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // ---- permission_flag: env-driven selection -------------------------

    #[test]
    fn permission_flag_env_selection() {
        let _guard = ENV_LOCK.lock().unwrap();

        // Unset -> default skip (security-critical default).
        std::env::remove_var("BAUDE_PERMISSION_MODE");
        assert_eq!(permission_flag("claude"), " --dangerously-skip-permissions");

        // Explicit "skip" -> skip.
        std::env::set_var("BAUDE_PERMISSION_MODE", "skip");
        assert_eq!(permission_flag("claude"), " --dangerously-skip-permissions");

        // "prompt" -> prompt flag (only on the exact literal).
        std::env::set_var("BAUDE_PERMISSION_MODE", "prompt");
        assert_eq!(
            permission_flag("claude"),
            " --permission-prompt-tool mcp__baude__approve"
        );

        // Unrecognized value -> fail-safe to skip (never reach prompt).
        std::env::set_var("BAUDE_PERMISSION_MODE", "bogus");
        assert_eq!(permission_flag("claude"), " --dangerously-skip-permissions");

        // Case-mismatch is unrecognized -> skip (exact literal only).
        std::env::set_var("BAUDE_PERMISSION_MODE", "Prompt");
        assert_eq!(permission_flag("claude"), " --dangerously-skip-permissions");

        std::env::remove_var("BAUDE_PERMISSION_MODE");
    }

    // ---- permission_flag: no-double-add (independent of env) ------------

    #[test]
    fn permission_flag_no_double_add() {
        let _guard = ENV_LOCK.lock().unwrap();

        // Even in prompt mode, an existing permission flag suppresses ours.
        std::env::set_var("BAUDE_PERMISSION_MODE", "prompt");
        assert_eq!(
            permission_flag("claude --dangerously-skip-permissions"),
            ""
        );
        assert_eq!(
            permission_flag("claude --permission-prompt-tool mcp__other__x"),
            ""
        );
        assert_eq!(permission_flag("claude --permission-mode acceptEdits"), "");

        // And in skip mode too.
        std::env::set_var("BAUDE_PERMISSION_MODE", "skip");
        assert_eq!(
            permission_flag("claude --dangerously-skip-permissions"),
            ""
        );

        std::env::remove_var("BAUDE_PERMISSION_MODE");
    }

    // ---- mutual exclusion ----------------------------------------------

    #[test]
    fn permission_flag_returns_exactly_one_known_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        for mode in ["skip", "prompt", "bogus", ""] {
            if mode.is_empty() {
                std::env::remove_var("BAUDE_PERMISSION_MODE");
            } else {
                std::env::set_var("BAUDE_PERMISSION_MODE", mode);
            }
            let flag = permission_flag("claude");
            assert!(
                flag == " --dangerously-skip-permissions"
                    || flag == " --permission-prompt-tool mcp__baude__approve"
                    || flag.is_empty(),
                "flag must be exactly one known value, got {flag:?} for mode {mode:?}"
            );
            // Never both flags at once.
            assert!(
                !(flag.contains("--dangerously-skip-permissions")
                    && flag.contains("--permission-prompt-tool")),
                "the two flags must never appear together"
            );
        }
        std::env::remove_var("BAUDE_PERMISSION_MODE");
    }

    // ---- is_prompt_mode -------------------------------------------------

    #[test]
    fn is_prompt_mode_only_on_exact_literal() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::remove_var("BAUDE_PERMISSION_MODE");
        assert!(!is_prompt_mode());

        std::env::set_var("BAUDE_PERMISSION_MODE", "skip");
        assert!(!is_prompt_mode());

        std::env::set_var("BAUDE_PERMISSION_MODE", "prompt");
        assert!(is_prompt_mode());

        std::env::set_var("BAUDE_PERMISSION_MODE", "Prompt");
        assert!(!is_prompt_mode());

        std::env::set_var("BAUDE_PERMISSION_MODE", "bogus");
        assert!(!is_prompt_mode());

        std::env::remove_var("BAUDE_PERMISSION_MODE");
    }

    // ---- mcp_server_config ---------------------------------------------

    #[test]
    fn mcp_server_config_shape() {
        let v = mcp_server_config("/abs/baude");
        assert_eq!(
            v["mcpServers"]["baude"]["command"].as_str(),
            Some("/abs/baude")
        );
        assert_eq!(
            v["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );
        assert_eq!(
            v["mcpServers"]["baude"]["args"]
                .as_array()
                .map(|a| a.len()),
            Some(1)
        );
    }

    #[test]
    fn mcp_server_config_never_panics_on_odd_exe() {
        // Empty / odd exe strings still build a valid Value (never panics).
        let v = mcp_server_config("");
        assert_eq!(v["mcpServers"]["baude"]["command"].as_str(), Some(""));
        let v = mcp_server_config("with spaces/baude binary");
        assert_eq!(
            v["mcpServers"]["baude"]["command"].as_str(),
            Some("with spaces/baude binary")
        );
    }

    // ---- mcp_config_path ------------------------------------------------

    #[test]
    fn mcp_config_path_joins_dot_mcp_json() {
        let p = mcp_config_path(Path::new("/tmp/session"));
        assert_eq!(p, PathBuf::from("/tmp/session/.mcp.json"));
    }
}
