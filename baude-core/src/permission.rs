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
pub fn permission_flag(base_cmd: &str) -> &'static str {
    permission_flag_for(
        std::env::var("BAUDE_PERMISSION_MODE").ok().as_deref(),
        base_cmd,
    )
}

/// Pure flag selection given an explicit mode — the env-free core of
/// [`permission_flag`]. `mode` is the raw `BAUDE_PERMISSION_MODE` value (`None`
/// when unset). Split out so callers (and tests) can exercise every branch
/// WITHOUT mutating the process-global env var, which would race concurrent
/// session spawns that read it (the same env-read/pure split as `hook`).
///
/// `Some("prompt")` returns [`PROMPT_FLAG`]; every other case (`None`,
/// `Some("skip")`, and any unrecognized value) returns [`SKIP_FLAG`] — fail-safe
/// (T-04-01). No-double-add (T-04-02) returns `""` when `base_cmd` already
/// carries a permission flag. Exactly one of `{skip, prompt, ""}`.
pub fn permission_flag_for(mode: Option<&str>, base_cmd: &str) -> &'static str {
    let already = base_cmd.contains("--dangerously-skip-permissions")
        || base_cmd.contains("--permission-prompt-tool")
        || base_cmd.contains("--permission-mode");
    if already {
        return "";
    }
    match mode {
        Some("prompt") => PROMPT_FLAG,
        // Default skip preserves today's unattended behavior. Unset, "skip",
        // and any unrecognized value all fall here (fail-safe — never prompt).
        _ => SKIP_FLAG,
    }
}

/// `true` iff `prompt` mode is active (the exact literal `"prompt"`).
///
/// The spawn sites use this to decide whether to additionally seed `.mcp.json`.
/// Mirrors the fail-safe of [`permission_flag`]: only the exact `"prompt"`
/// literal is `prompt` mode; everything else is `skip`.
pub fn is_prompt_mode() -> bool {
    std::env::var("BAUDE_PERMISSION_MODE").as_deref() == Ok("prompt")
}

/// Build the `.mcp.json` body registering the `baude` permission MCP server.
///
/// Returns `{"mcpServers":{"baude":{"command":<exe>,"args":["permission-mcp"]}}}`.
/// `exe` is the absolute `current_exe()` path the CALLER resolves (mirroring
/// [`crate::hook::baude_hook_command`]) — core stays pure and never calls
/// `current_exe()`. Building the Value never panics.
pub fn mcp_server_config(exe: &str) -> serde_json::Value {
    serde_json::json!({
        "mcpServers": {
            "baude": {
                "command": exe,
                "args": ["permission-mcp"],
            }
        }
    })
}

/// Idempotently merge baude's `permission-mcp` server registration into an
/// existing `.mcp.json` value (PERM-01 / T-04-03).
///
/// Pure `Value -> Value` transform mirroring [`crate::hook::merge_hook_settings`]:
/// the binaries own the read/write (the env-read/`current_exe()`/filesystem
/// half), this owns the non-clobbering merge. Only the `mcpServers.baude` key is
/// set (to `exe` + `["permission-mcp"]`); a user's sibling servers and any other
/// top-level keys survive byte-intact. Re-running on its own output is a no-op
/// (idempotent). Never panics on a minimal / non-object / odd file.
pub fn merge_mcp_config(existing: &serde_json::Value, exe: &str) -> serde_json::Value {
    let mut root = existing.clone();
    if !root.is_object() {
        root = serde_json::json!({});
    }
    let obj = root.as_object_mut().expect("root coerced to object");
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    if !servers.is_object() {
        *servers = serde_json::json!({});
    }
    let servers_obj = servers
        .as_object_mut()
        .expect("mcpServers coerced to object");
    // Overwrite only our own `baude` entry; sibling servers are untouched.
    servers_obj.insert(
        "baude".to_string(),
        mcp_server_config(exe)["mcpServers"]["baude"].clone(),
    );
    root
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

    // ---- permission_flag_for: pure mode selection (no env mutation) -----
    // Branch coverage uses the env-free seam so it never races concurrent
    // session spawns (in any crate) that read the process-global env var.

    #[test]
    fn permission_flag_for_mode_selection() {
        // None (unset) -> default skip (security-critical default, T-04-01).
        assert_eq!(
            permission_flag_for(None, "claude"),
            " --dangerously-skip-permissions"
        );
        // Explicit "skip" -> skip.
        assert_eq!(
            permission_flag_for(Some("skip"), "claude"),
            " --dangerously-skip-permissions"
        );
        // "prompt" -> prompt flag (only on the exact literal).
        assert_eq!(
            permission_flag_for(Some("prompt"), "claude"),
            " --permission-prompt-tool mcp__baude__approve"
        );
        // Unrecognized value -> fail-safe to skip (never reach prompt).
        assert_eq!(
            permission_flag_for(Some("bogus"), "claude"),
            " --dangerously-skip-permissions"
        );
        // Case-mismatch is unrecognized -> skip (exact literal only).
        assert_eq!(
            permission_flag_for(Some("Prompt"), "claude"),
            " --dangerously-skip-permissions"
        );
    }

    #[test]
    fn permission_flag_for_no_double_add() {
        // Even in prompt mode, an existing permission flag suppresses ours
        // (T-04-02 — never duplicate/override an operator-set flag).
        assert_eq!(
            permission_flag_for(Some("prompt"), "claude --dangerously-skip-permissions"),
            ""
        );
        assert_eq!(
            permission_flag_for(
                Some("prompt"),
                "claude --permission-prompt-tool mcp__other__x"
            ),
            ""
        );
        assert_eq!(
            permission_flag_for(Some("prompt"), "claude --permission-mode acceptEdits"),
            ""
        );
        // And in skip mode too.
        assert_eq!(
            permission_flag_for(Some("skip"), "claude --dangerously-skip-permissions"),
            ""
        );
    }

    #[test]
    fn permission_flag_for_returns_exactly_one_known_value() {
        for mode in [None, Some("skip"), Some("prompt"), Some("bogus"), Some("")] {
            let flag = permission_flag_for(mode, "claude");
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
    }

    // ---- permission_flag: the env-reading wrapper delegates correctly ---
    // One guarded smoke test that the env wrapper reads BAUDE_PERMISSION_MODE
    // and routes to permission_flag_for. Kept minimal and mutex-guarded;
    // restores the var immediately to shrink the race window with spawns.

    #[test]
    fn permission_flag_reads_env_and_delegates() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("BAUDE_PERMISSION_MODE");
        assert_eq!(permission_flag("claude"), " --dangerously-skip-permissions");
        std::env::remove_var("BAUDE_PERMISSION_MODE");
    }

    // ---- is_prompt_mode -------------------------------------------------

    #[test]
    fn is_prompt_mode_only_on_exact_literal() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::remove_var("BAUDE_PERMISSION_MODE");
        assert!(!is_prompt_mode());

        std::env::set_var("BAUDE_PERMISSION_MODE", "prompt");
        assert!(is_prompt_mode());

        std::env::set_var("BAUDE_PERMISSION_MODE", "Prompt");
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
            v["mcpServers"]["baude"]["args"].as_array().map(|a| a.len()),
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

    // ---- merge_mcp_config ----------------------------------------------

    #[test]
    fn merge_mcp_config_preserves_siblings_and_is_idempotent() {
        let existing: serde_json::Value = serde_json::from_str(
            r#"{"mcpServers":{"other":{"command":"other-srv"}},"extra":true}"#,
        )
        .unwrap();

        let once = merge_mcp_config(&existing, "/abs/baude");
        // Sibling server + unrelated top-level key survive.
        assert_eq!(
            once["mcpServers"]["other"]["command"].as_str(),
            Some("other-srv")
        );
        assert_eq!(once["extra"].as_bool(), Some(true));
        // Our server registered with the permission-mcp arg.
        assert_eq!(
            once["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );
        assert_eq!(
            once["mcpServers"]["baude"]["command"].as_str(),
            Some("/abs/baude")
        );

        // Idempotent: re-merging its own output is a no-op.
        let twice = merge_mcp_config(&once, "/abs/baude");
        assert_eq!(once, twice);
    }

    #[test]
    fn merge_mcp_config_never_panics_on_odd_inputs() {
        // Empty object.
        let v = merge_mcp_config(&serde_json::json!({}), "/x");
        assert_eq!(
            v["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );
        // Non-object root.
        let v = merge_mcp_config(&serde_json::json!(42), "/x");
        assert!(v.is_object());
        assert_eq!(v["mcpServers"]["baude"]["command"].as_str(), Some("/x"));
        // mcpServers is a non-object scalar — coerced, no panic.
        let v = merge_mcp_config(&serde_json::json!({"mcpServers": 5}), "/x");
        assert_eq!(v["mcpServers"]["baude"]["command"].as_str(), Some("/x"));
    }

    // ---- mcp_config_path ------------------------------------------------

    #[test]
    fn mcp_config_path_joins_dot_mcp_json() {
        let p = mcp_config_path(Path::new("/tmp/session"));
        assert_eq!(p, PathBuf::from("/tmp/session/.mcp.json"));
    }
}
