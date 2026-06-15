//! `baude hook` — a Claude Code lifecycle-event bridge.
//!
//! Claude Code invokes a configured command hook on each lifecycle event
//! (`UserPromptSubmit`, `Stop`, `Notification`, `PostToolUse`), piping a JSON
//! payload to the command's stdin. This module provides the pure, testable
//! transforms behind the `baude hook` subcommand:
//!
//! - [`build_event`] normalizes a hook payload into one schema-versioned event
//!   line (`{schema:1, ts, session_id, event, tool, notification_type}`).
//! - [`merge_hook_settings`] idempotently merges baude's four hook entries into
//!   an existing `.claude/settings.local.json` value, never clobbering a user's
//!   `statusLine` or their own hooks.
//! - [`event_path`] / [`append_event`] are the TUI-local file-tail transport
//!   ([`/tmp/baude-events-<sid>.jsonl`]).
//!
//! Like [`crate::bridge`], every field is read via untyped `serde_json::Value`
//! accessors (never typed `Deserialize`) so unknown/absent keys are tolerated
//! and a minimal or odd payload never panics. `schema` is informational only;
//! readers must NOT branch on it.
//!
//! Hook stdin field names verified against the Claude Code hooks schema for
//! CLI v2.1.177 (the same version pinned in `bridge.rs`): snake_case
//! `session_id`, `hook_event_name`, `tool_name`, `notification_type`.
//! Re-verify `claude --version` at execution time and update this comment if
//! it advances past 2.1.177.

use std::io::Write;

use serde_json::{json, Value};

use crate::meta::now_unix_ms;

/// The four Claude Code lifecycle events baude wires its hook into.
const EVENTS: &[&str] = &["UserPromptSubmit", "Stop", "Notification", "PostToolUse"];

/// Per-session event-transport file for the TUI-local path.
///
/// Mirrors [`crate::bridge::bridge_path`]. Defense-in-depth (T-02-01): a
/// `session_id` containing `/` or `..` could otherwise traverse outside
/// `/tmp`; we replace those substrings so the path is always a single file
/// directly under `/tmp`. `session_id` is a trusted local Claude value, but
/// this mirrors the V5 input-validation posture for filesystem path
/// construction from attacker-influenceable strings.
pub fn event_path(sid: &str) -> String {
    let safe = sid.replace("..", "_").replace('/', "_");
    format!("/tmp/baude-events-{safe}.jsonl")
}

/// Build the normalized event line from a parsed hook stdin payload.
///
/// Pure `Value -> Value` transform mirroring [`crate::bridge::build_bridge`].
/// Every field is read via `Value` accessors so an absent/wrong-type key
/// yields `null` and an empty object never panics. `schema` is informational
/// only — never branch on it.
pub fn build_event(v: &Value) -> Value {
    json!({
        "schema": 1,
        "ts": now_unix_ms(),
        "session_id": v["session_id"].as_str(),
        "event": v["hook_event_name"].as_str(),
        "tool": v["tool_name"].as_str(),
        // Carried so Phase 4 (PERM) can distinguish permission vs idle prompts.
        "notification_type": v["notification_type"].as_str(),
    })
}

/// The hook command string baude seeds into `settings.local.json`.
///
/// Resolves to the absolute path of the running binary plus ` hook`
/// (research A2: `baude` may not be on the managed session's PATH, so the
/// bare `baude hook` string could silently never fire). Falls back to the
/// bare `"baude hook"` string if `current_exe()` fails. This string IS the
/// idempotency sentinel for [`merge_hook_settings`].
pub fn baude_hook_command() -> String {
    match std::env::current_exe() {
        Ok(p) => format!("{} hook", p.display()),
        Err(_) => "baude hook".to_string(),
    }
}

/// Idempotently merge baude's hook entries into an existing settings value.
///
/// Reads whatever JSON is in `settings.local.json` and returns the merged
/// JSON. `command` is both the inserted command and the idempotency sentinel:
/// an entry is "baude's" iff one of its inner hooks has `command == command`.
/// Re-running this on its own output is a no-op (HOOK-01 idempotency).
///
/// Never clobbers sibling keys (`statusLine`, `permissions`, `env`, …) or a
/// user's own hook groups — only `.entry().or_insert()` into `hooks.<event>`
/// arrays, appending baude's group when absent. Never panics on a minimal /
/// non-object / odd file (T-02-04).
pub fn merge_hook_settings(existing: &Value, command: &str) -> Value {
    let mut root = existing.clone();
    if !root.is_object() {
        root = json!({});
    }
    let obj = root.as_object_mut().expect("root coerced to object");
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks_obj = hooks.as_object_mut().expect("hooks coerced to object");
    for ev in EVENTS {
        let arr = hooks_obj.entry(*ev).or_insert_with(|| json!([]));
        // User put a non-array under this event — leave it, skip (no clobber).
        let Some(groups) = arr.as_array_mut() else {
            continue;
        };
        let already = groups.iter().any(|g| {
            g["hooks"]
                .as_array()
                .is_some_and(|inner| inner.iter().any(|h| h["command"].as_str() == Some(command)))
        });
        if !already {
            groups.push(json!({
                "hooks": [ { "type": "command", "command": command } ]
            }));
        }
    }
    root
}

/// Append one event line to the per-session `/tmp` file (O_APPEND).
///
/// Mirrors the best-effort posture of [`crate::bridge::run`]; concurrent hook
/// processes are safe because `append(true)` (O_APPEND) gives atomic small
/// writes with no coordination.
pub fn append_event(sid: &str, line: &str) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(event_path(sid))?;
    writeln!(f, "{line}")
}

/// Best-effort, idempotent, non-clobbering seed of a session cwd's
/// `.claude/settings.local.json` so a managed Claude session fires baude's
/// hooks. Single source of truth for both the TUI (`baude`) and the daemon
/// (`bauded`) spawn paths (WR-06).
///
/// Every step is best-effort — a failure here must NEVER abort a spawn: the
/// session simply falls back to the silence path (no regression). The seeded
/// command is the `current_exe()` absolute path + ` hook` (so it resolves
/// regardless of the session PATH), and [`merge_hook_settings`] is idempotent
/// and non-clobbering so re-spawn/restart never duplicates entries and a
/// user's `statusLine`/own hooks survive.
pub fn seed_settings(cwd: &std::path::Path) {
    let dir = cwd.join(".claude");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("settings.local.json");
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}));
    let command = baude_hook_command();
    let merged = merge_hook_settings(&existing, &command);
    let _ = std::fs::write(&path, merged.to_string());
}

/// Route one normalized event line to its transport, never losing the event.
///
/// When `url` is `Some`, attempt the daemon POST via `post` (which returns
/// `true` on success). On a POST failure — connection refused on a wrong/dead
/// port (e.g. a custom daemon `--bind` the injected URL doesn't know about) or
/// any transport error — fall back to the TUI-local `/tmp` file-append so the
/// event is never silently dropped; the daemon tails that same file, so the
/// event converges either way (WR-02). When `url` is `None`, append directly.
///
/// `post` is injected so the routing/fallback decision is unit-testable without
/// a live network peer. The transport call itself stays in the binary.
pub fn route_event<F>(url: Option<&str>, sid: &str, line: &str, post: F)
where
    F: FnOnce(&str, &str) -> bool,
{
    match url {
        Some(url) => {
            let posted = post(url, line);
            if !posted && !sid.is_empty() {
                let _ = append_event(sid, line);
            }
        }
        None => {
            if !sid.is_empty() {
                let _ = append_event(sid, line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("fixture is valid JSON")
    }

    // ---- build_event ----------------------------------------------------

    #[test]
    fn build_event_user_prompt_submit() {
        let v = parse(r#"{"hook_event_name":"UserPromptSubmit","session_id":"abc"}"#);
        let out = build_event(&v);
        assert_eq!(out["schema"].as_u64(), Some(1));
        assert_eq!(out["session_id"].as_str(), Some("abc"));
        assert_eq!(out["event"].as_str(), Some("UserPromptSubmit"));
        assert!(out["tool"].is_null());
        assert!(out["notification_type"].is_null());
        assert!(out["ts"].as_u64().is_some());
    }

    #[test]
    fn build_event_post_tool_use_carries_tool() {
        let v = parse(r#"{"hook_event_name":"PostToolUse","session_id":"s","tool_name":"Bash"}"#);
        let out = build_event(&v);
        assert_eq!(out["event"].as_str(), Some("PostToolUse"));
        assert_eq!(out["tool"].as_str(), Some("Bash"));
    }

    #[test]
    fn build_event_notification_carries_type() {
        let v =
            parse(r#"{"hook_event_name":"Notification","notification_type":"permission_prompt"}"#);
        let out = build_event(&v);
        assert_eq!(out["event"].as_str(), Some("Notification"));
        assert_eq!(out["notification_type"].as_str(), Some("permission_prompt"));
    }

    #[test]
    fn build_event_empty_never_panics() {
        let out = build_event(&json!({}));
        assert_eq!(out["schema"].as_u64(), Some(1));
        assert!(out["event"].is_null());
        assert!(out["session_id"].is_null());
        assert!(out["ts"].as_u64().is_some());
    }

    // ---- merge_hook_settings -------------------------------------------

    const CMD: &str = "/abs/path/baude hook";

    fn baude_entry_count(v: &Value, ev: &str) -> usize {
        v["hooks"][ev]
            .as_array()
            .map(|groups| {
                groups
                    .iter()
                    .filter(|g| {
                        g["hooks"].as_array().is_some_and(|inner| {
                            inner.iter().any(|h| h["command"].as_str() == Some(CMD))
                        })
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn merge_preserves_user_statusline_and_user_hook() {
        let existing = parse(
            r#"{
                "statusLine": {"type":"command","command":"my-statusline --foo"},
                "permissions": {"allow":["Bash"]},
                "hooks": {
                    "PostToolUse": [
                        {"hooks":[{"type":"command","command":"user-own-hook"}]}
                    ]
                }
            }"#,
        );
        let out = merge_hook_settings(&existing, CMD);

        // statusLine survives byte-intact.
        assert_eq!(
            out["statusLine"], existing["statusLine"],
            "user statusLine must be preserved unchanged"
        );
        // unrelated sibling key untouched.
        assert_eq!(out["permissions"], existing["permissions"]);

        // user's own PostToolUse hook still present.
        let groups = out["hooks"]["PostToolUse"].as_array().unwrap();
        assert!(groups.iter().any(|g| {
            g["hooks"].as_array().is_some_and(|inner| {
                inner
                    .iter()
                    .any(|h| h["command"].as_str() == Some("user-own-hook"))
            })
        }));

        // baude's entry added for all four events.
        for ev in EVENTS {
            assert_eq!(
                baude_entry_count(&out, ev),
                1,
                "missing baude entry for {ev}"
            );
        }
    }

    #[test]
    fn merge_idempotent_applied_twice() {
        let once = merge_hook_settings(&json!({}), CMD);
        let twice = merge_hook_settings(&once, CMD);
        for ev in EVENTS {
            assert_eq!(
                baude_entry_count(&twice, ev),
                1,
                "exactly one baude entry per event after double merge ({ev})"
            );
        }
    }

    #[test]
    fn merge_minimal_and_odd_inputs_never_panic() {
        // Empty object.
        let out = merge_hook_settings(&json!({}), CMD);
        for ev in EVENTS {
            assert_eq!(baude_entry_count(&out, ev), 1);
        }
        // Non-object root.
        let out = merge_hook_settings(&json!(42), CMD);
        assert!(out.is_object());
        for ev in EVENTS {
            assert_eq!(baude_entry_count(&out, ev), 1);
        }
        // hooks is a non-object scalar.
        let out = merge_hook_settings(&parse(r#"{"hooks": 5}"#), CMD);
        for ev in EVENTS {
            assert_eq!(baude_entry_count(&out, ev), 1);
        }
        // one event is a non-array (user oddity) — left alone, not clobbered, no panic.
        let out = merge_hook_settings(&parse(r#"{"hooks":{"Stop":"weird"}}"#), CMD);
        assert_eq!(out["hooks"]["Stop"].as_str(), Some("weird"));
        // the other three events still get baude's entry.
        assert_eq!(baude_entry_count(&out, "UserPromptSubmit"), 1);
    }

    // ---- event_path / append_event -------------------------------------

    #[test]
    fn event_path_format() {
        assert_eq!(event_path("abc"), "/tmp/baude-events-abc.jsonl");
    }

    #[test]
    fn event_path_sanitizes_traversal() {
        let p = event_path("../../etc/passwd");
        assert!(p.starts_with("/tmp/baude-events-"));
        assert!(!p.contains(".."));
        assert!(!p.contains("/etc/"));
    }

    #[test]
    fn append_event_appends_two_lines() {
        let sid = format!("test-append-{}", std::process::id());
        let path = event_path(&sid);
        let _ = std::fs::remove_file(&path);

        append_event(&sid, r#"{"event":"UserPromptSubmit"}"#).unwrap();
        append_event(&sid, r#"{"event":"Stop"}"#).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "O_APPEND should produce two lines");
        assert!(lines[0].contains("UserPromptSubmit"));
        assert!(lines[1].contains("Stop"));

        let _ = std::fs::remove_file(&path);
    }

    // ---- route_event ---------------------------------------------------

    #[test]
    fn route_event_failed_post_falls_back_to_file() {
        // WR-02: a POST that fails (here: a wrong/dead port simulated by the
        // closure returning false) must NOT drop the event — it appends to the
        // per-session /tmp file so the daemon's file-tail still catches it.
        let sid = format!("test-route-fail-{}", std::process::id());
        let path = event_path(&sid);
        let _ = std::fs::remove_file(&path);

        route_event(
            Some("http://127.0.0.1:1/bad"),
            &sid,
            r#"{"event":"Stop"}"#,
            |_url, _line| false, // simulate connection refused / dead port
        );

        let contents = std::fs::read_to_string(&path).expect("fallback wrote the event file");
        assert!(
            contents.contains("Stop"),
            "failed POST must append to the file, not drop the event"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn route_event_successful_post_does_not_write_file() {
        // A successful POST consumes the event via the daemon transport; no
        // file is written (the daemon ingests and appends on its side).
        let sid = format!("test-route-ok-{}", std::process::id());
        let path = event_path(&sid);
        let _ = std::fs::remove_file(&path);

        route_event(
            Some("http://127.0.0.1:8642/ok"),
            &sid,
            r#"{"event":"Stop"}"#,
            |_url, _line| true, // POST succeeded
        );

        assert!(
            !std::path::Path::new(&path).exists(),
            "a successful POST must not also write the local file"
        );
    }

    #[test]
    fn route_event_no_url_appends_to_file() {
        // The TUI-local path (no $BAUDE_EVENT_URL) appends directly.
        let sid = format!("test-route-nourl-{}", std::process::id());
        let path = event_path(&sid);
        let _ = std::fs::remove_file(&path);

        let mut post_called = false;
        route_event(None, &sid, r#"{"event":"Stop"}"#, |_url, _line| {
            post_called = true;
            true
        });
        assert!(!post_called, "no URL means no POST attempt");

        let contents = std::fs::read_to_string(&path).expect("local append wrote the file");
        assert!(contents.contains("Stop"));
        let _ = std::fs::remove_file(&path);
    }

    // ---- seed_settings -------------------------------------------------

    #[test]
    fn seed_settings_writes_idempotent_merge() {
        // WR-06: the shared seed wrapper creates .claude/settings.local.json,
        // merges baude's hook entries, and is idempotent across re-runs (the
        // daemon re-seeds every persisted session on each restart).
        let cwd = std::env::temp_dir().join(format!("baude-seed-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cwd);
        std::fs::create_dir_all(&cwd).unwrap();

        seed_settings(&cwd);
        let path = cwd.join(".claude").join("settings.local.json");
        let first = std::fs::read_to_string(&path).expect("seed wrote settings file");
        let v: Value = serde_json::from_str(&first).expect("seed wrote valid JSON");
        // All four lifecycle events seeded.
        for ev in EVENTS {
            assert!(
                v["hooks"][ev].is_array(),
                "missing seeded hook array for {ev}"
            );
        }

        // Re-seeding is a no-op on the merged content (idempotent).
        seed_settings(&cwd);
        let second = std::fs::read_to_string(&path).unwrap();
        assert_eq!(first, second, "re-seed must be idempotent");

        let _ = std::fs::remove_dir_all(&cwd);
    }
}
