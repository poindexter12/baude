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

use serde_json::{json, Value};

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

    // ==== 04-02 Task 1: JSON-RPC framing + MCP transforms ================
    // The §F-CONTRACT-isolated wire functions. Every bullet of the task
    // <behavior> is pinned here, including the *_never_panics posture mirrored
    // from hook.rs:222-296. The wire shape is the ASSUMED RESEARCH §C/§D
    // contract; the §F UAT confirms/corrects it cheaply via these functions.

    // ---- parse_frame ----------------------------------------------------

    #[test]
    fn parse_frame_line_delimited() {
        let buf = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n";
        let (body, consumed) = parse_frame(buf).expect("line frame parses");
        assert_eq!(body["method"].as_str(), Some("initialize"));
        assert_eq!(body["id"].as_u64(), Some(1));
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn parse_frame_content_length() {
        let body = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let (parsed, consumed) = parse_frame(frame.as_bytes()).expect("LSP frame parses");
        assert_eq!(parsed["method"].as_str(), Some("tools/list"));
        assert_eq!(consumed, frame.len());
    }

    #[test]
    fn parse_frame_content_length_lf_only_separator() {
        // Some peers use bare \n line endings; tolerate both.
        let body = r#"{"id":3,"method":"x"}"#;
        let frame = format!("Content-Length: {}\n\n{}", body.len(), body);
        let (parsed, consumed) = parse_frame(frame.as_bytes()).expect("LF-only LSP frame parses");
        assert_eq!(parsed["method"].as_str(), Some("x"));
        assert_eq!(consumed, frame.len());
    }

    #[test]
    fn parse_frame_consumes_only_one_frame() {
        // Two line-delimited frames back to back: parse_frame returns the first
        // and reports exactly how many bytes it consumed so the caller can
        // advance and parse the rest.
        let first = "{\"id\":1}\n";
        let buf = format!("{first}{{\"id\":2}}\n");
        let (body, consumed) = parse_frame(buf.as_bytes()).expect("first frame parses");
        assert_eq!(body["id"].as_u64(), Some(1));
        assert_eq!(consumed, first.len());
        let (body2, _) = parse_frame(&buf.as_bytes()[consumed..]).expect("second frame parses");
        assert_eq!(body2["id"].as_u64(), Some(2));
    }

    #[test]
    fn parse_frame_partial_yields_none() {
        // Incomplete line (no newline yet) -> None (accumulate more bytes).
        assert!(parse_frame(b"{\"id\":1").is_none());
        // Content-Length header but body not fully arrived -> None.
        let frame = b"Content-Length: 50\r\n\r\n{\"id\":1}";
        assert!(parse_frame(frame).is_none());
        // Header line started but not terminated -> None.
        assert!(parse_frame(b"Content-Length: 10").is_none());
    }

    #[test]
    fn parse_frame_never_panics_on_garbage() {
        assert!(parse_frame(b"").is_none());
        assert!(parse_frame(b"not json\n").is_none());
        assert!(parse_frame(b"Content-Length: abc\r\n\r\n{}").is_none());
        // Negative/overflowing length never panics.
        assert!(parse_frame(b"Content-Length: -1\r\n\r\n{}").is_none());
        assert!(parse_frame(b"\n").is_none());
    }

    // ---- parse_tool_call ------------------------------------------------

    #[test]
    fn parse_tool_call_reads_tool_name_and_input() {
        let params = json!({
            "tool_name": "Bash",
            "input": {"command": "rm -rf build/"},
            "tool_use_id": "toolu_01"
        });
        let (tool, input) = parse_tool_call(&params);
        assert_eq!(tool, "Bash");
        assert_eq!(input["command"].as_str(), Some("rm -rf build/"));
    }

    #[test]
    fn parse_tool_call_parameters_fallback() {
        // §C: when `input` is absent, fall back to `parameters` then `tool_input`.
        let p1 = json!({"tool_name": "Edit", "parameters": {"path": "/x"}});
        let (t, i) = parse_tool_call(&p1);
        assert_eq!(t, "Edit");
        assert_eq!(i["path"].as_str(), Some("/x"));

        let p2 = json!({"tool_name": "Write", "tool_input": {"path": "/y"}});
        let (t, i) = parse_tool_call(&p2);
        assert_eq!(t, "Write");
        assert_eq!(i["path"].as_str(), Some("/y"));
    }

    #[test]
    fn parse_tool_call_tolerates_missing_tool_use_id() {
        // tool_use_id is optional (§C) — absence must not break parsing.
        let params = json!({"tool_name": "Read", "input": {}});
        let (tool, _input) = parse_tool_call(&params);
        assert_eq!(tool, "Read");
    }

    #[test]
    fn parse_tool_call_empty_never_panics() {
        let (tool, input) = parse_tool_call(&json!({}));
        assert_eq!(tool, "");
        assert!(input.is_null());
        let (tool, input) = parse_tool_call(&Value::Null);
        assert_eq!(tool, "");
        assert!(input.is_null());
        // Odd-typed fields never panic.
        let (tool, input) = parse_tool_call(&json!({"tool_name": 5, "input": "str"}));
        assert_eq!(tool, "");
        assert_eq!(input.as_str(), Some("str"));
    }

    // ---- build_approve_result -------------------------------------------

    fn inner_body(env: &Value) -> Value {
        let text = env["content"][0]["text"]
            .as_str()
            .expect("content[0].text is a string");
        assert_eq!(env["content"][0]["type"].as_str(), Some("text"));
        serde_json::from_str(text).expect("inner text is JSON")
    }

    #[test]
    fn build_approve_result_allow_echoes_input() {
        let input = json!({"command": "ls"});
        let env = build_approve_result("allow", Some(&input), None);
        let body = inner_body(&env);
        assert_eq!(body["behavior"].as_str(), Some("allow"));
        assert_eq!(body["updatedInput"]["command"].as_str(), Some("ls"));
    }

    #[test]
    fn build_approve_result_allow_without_input_uses_empty_object() {
        let env = build_approve_result("allow", None, None);
        let body = inner_body(&env);
        assert_eq!(body["behavior"].as_str(), Some("allow"));
        assert!(body["updatedInput"].is_object());
    }

    #[test]
    fn build_approve_result_deny() {
        let env = build_approve_result("deny", None, None);
        let body = inner_body(&env);
        assert_eq!(body["behavior"].as_str(), Some("deny"));
        assert_eq!(body["message"].as_str(), Some("denied"));
    }

    #[test]
    fn build_approve_result_deny_custom_message() {
        let env = build_approve_result("deny", None, Some("denied from phone"));
        let body = inner_body(&env);
        assert_eq!(body["behavior"].as_str(), Some("deny"));
        assert_eq!(body["message"].as_str(), Some("denied from phone"));
    }

    #[test]
    fn build_approve_result_unknown_behavior_coerces_to_deny() {
        // SECURITY: any non-"allow" behavior is coerced to deny — never emit
        // allow for an unrecognized value (deny-default, T-04-04/V4).
        for bogus in ["", "ALLOW", "yes", "approve", "true", "Allow "] {
            let env = build_approve_result(bogus, Some(&json!({"x":1})), None);
            let body = inner_body(&env);
            assert_eq!(
                body["behavior"].as_str(),
                Some("deny"),
                "behavior {bogus:?} must coerce to deny, never allow"
            );
            // The echoed input must NOT leak as updatedInput on a deny coercion.
            assert!(body["updatedInput"].is_null());
        }
    }

    // ---- rpc_response / rpc_error ---------------------------------------

    #[test]
    fn rpc_response_well_formed() {
        let r = rpc_response(json!(7), json!({"ok": true}));
        assert_eq!(r["jsonrpc"].as_str(), Some("2.0"));
        assert_eq!(r["id"].as_u64(), Some(7));
        assert_eq!(r["result"]["ok"].as_bool(), Some(true));
        assert!(r.get("error").is_none());
    }

    #[test]
    fn rpc_error_well_formed() {
        let r = rpc_error(json!(8), -32601, "method not found");
        assert_eq!(r["jsonrpc"].as_str(), Some("2.0"));
        assert_eq!(r["id"].as_u64(), Some(8));
        assert_eq!(r["error"]["code"].as_i64(), Some(-32601));
        assert_eq!(r["error"]["message"].as_str(), Some("method not found"));
        assert!(r.get("result").is_none());
    }

    #[test]
    fn rpc_response_string_id_preserved() {
        // JSON-RPC ids may be strings; echo whatever the request used.
        let r = rpc_response(json!("abc"), json!(null));
        assert_eq!(r["id"].as_str(), Some("abc"));
    }
}
