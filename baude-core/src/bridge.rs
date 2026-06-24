//! `baude statusline` — a statusline bridge for Claude Code.
//!
//! Claude Code pipes a JSON payload to the configured statusLine command on
//! every refresh. That payload is the only local source of the account
//! rate-limit percentages (`rate_limits.five_hour` / `seven_day`) and the
//! live session cost (`cost.total_cost_usd`). This subcommand captures those
//! fields to `/tmp/baude-usage-<sessionId>.json` for the baude TUI to read,
//! then (optionally) feeds the same payload to the wrapped "real" statusline
//! command so its display is unchanged.
//!
//! settings.json:
//! ```json
//! "statusLine": {
//!   "type": "command",
//!   "command": "baude statusline --wrap '<original statusline command>'"
//! }
//! ```

use std::io::{Read, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::meta::now_unix_ms;

pub fn bridge_path(session_id: &str) -> String {
    format!("/tmp/baude-usage-{session_id}.json")
}

/// Pull a rate-limit window out of the payload, tolerating both snake_case
/// and camelCase key styles across Claude Code versions.
fn window(v: &Value, snake: &str, camel: &str) -> Value {
    let w = if v["rate_limits"][snake].is_object() {
        &v["rate_limits"][snake]
    } else {
        &v["rate_limits"][camel]
    };
    if !w.is_object() {
        return Value::Null;
    }
    let pct = w["used_percentage"]
        .as_f64()
        .or_else(|| w["utilization"].as_f64());
    json!({
        "used_pct": pct,
        "resets_at": w["resets_at"].as_u64().or_else(|| w["resetsAt"].as_u64()),
    })
}

/// Build the bridge JSON from a parsed statusLine payload.
///
/// Reads every field via `serde_json::Value` accessors (never typed
/// `Deserialize`) so unknown keys are ignored and absent/wrong-type keys yield
/// `null` — that untyped tolerance is what keeps the on-disk format
/// back-compatible (STL-02). `schema` is informational only; readers must NOT
/// branch on it.
///
/// Field names verified against the Claude Code statusLine schema for CLI
/// v2.1.177 (snake_case throughout; nested objects). camelCase `.or_else`
/// fallbacks are defensive insurance against version drift.
fn build_bridge(v: &Value) -> Value {
    let pr = {
        let p = &v["pr"];
        if p.is_object() {
            json!({
                "number": p["number"].as_u64(),
                "url": p["url"].as_str(),
                "review_state": p["review_state"]
                    .as_str()
                    .or_else(|| p["reviewState"].as_str()),
            })
        } else {
            Value::Null
        }
    };
    let worktree = {
        let w = &v["worktree"];
        if w.is_object() {
            json!({
                "name": w["name"].as_str(),
                "path": w["path"].as_str(),
                "branch": w["branch"].as_str(),
            })
        } else {
            Value::Null
        }
    };
    json!({
        "schema": 2,
        "session_id": v["session_id"].as_str(),
        "updated_unix_ms": now_unix_ms(),
        "cost_usd": v["cost"]["total_cost_usd"].as_f64(),
        "context_used_pct": v["context_window"]["used_percentage"].as_f64(),
        "five_hour": window(v, "five_hour", "fiveHour"),
        "seven_day": window(v, "seven_day", "sevenDay"),
        // STL-01 — nested-object sources, Claude Code v2.1.177 schema:
        "model": v["model"]["display_name"]
            .as_str()
            .or_else(|| v["model"]["id"].as_str()),
        "effort": v["effort"]["level"].as_str(),
        "thinking": v["thinking"]["enabled"].as_bool(),
        "pr": pr,
        "worktree": worktree,
        // Captured but never rendered (locked scope: capture-but-don't-render).
        "vim_mode": v["vim"]["mode"].as_str(),
    })
}

pub fn run(wrap: Option<String>) -> i32 {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return 1;
    }

    // Best-effort capture — never break the user's statusline over it.
    if let Ok(v) = serde_json::from_str::<Value>(&input) {
        if let Some(sid) = v["session_id"].as_str() {
            let _ = std::fs::write(bridge_path(sid), build_bridge(&v).to_string());
        }
    }

    // Delegate to the wrapped statusline with the same payload; its stdout
    // (the rendered line) is inherited and goes straight back to Claude.
    let Some(cmd) = wrap else { return 0 };
    match Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(input.as_bytes());
            }
            child.wait().ok().and_then(|s| s.code()).unwrap_or(1)
        }
        Err(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::build_bridge;
    use serde_json::{json, Value};

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("fixture is valid JSON")
    }

    #[test]
    fn schema_is_2() {
        let v = parse(r#"{"session_id":"abc"}"#);
        let out = build_bridge(&v);
        assert_eq!(out["schema"].as_u64(), Some(2));
    }

    #[test]
    fn full_payload_captured() {
        let v = parse(
            r#"{
                "session_id": "sid-1",
                "cost": {"total_cost_usd": 1.25},
                "context_window": {"used_percentage": 42.0},
                "rate_limits": {
                    "five_hour": {"used_percentage": 10.0, "resets_at": 111},
                    "seven_day": {"used_percentage": 20.0, "resets_at": 222}
                },
                "model": {"display_name": "Claude Opus 4.8", "id": "claude-opus-4-8"},
                "effort": {"level": "high"},
                "thinking": {"enabled": true},
                "pr": {"number": 42, "url": "https://example.com/pr/42", "review_state": "approved"},
                "worktree": {"name": "wt", "path": "/tmp/wt", "branch": "feature/x"},
                "vim": {"mode": "NORMAL"}
            }"#,
        );
        let out = build_bridge(&v);

        assert_eq!(out["model"].as_str(), Some("Claude Opus 4.8"));
        assert_eq!(out["effort"].as_str(), Some("high"));
        assert_eq!(out["thinking"].as_bool(), Some(true));
        assert_eq!(out["vim_mode"].as_str(), Some("NORMAL"));

        assert!(out["pr"].is_object());
        assert_eq!(out["pr"]["number"].as_u64(), Some(42));
        assert_eq!(out["pr"]["url"].as_str(), Some("https://example.com/pr/42"));
        assert_eq!(out["pr"]["review_state"].as_str(), Some("approved"));

        assert!(out["worktree"].is_object());
        assert_eq!(out["worktree"]["name"].as_str(), Some("wt"));
        assert_eq!(out["worktree"]["path"].as_str(), Some("/tmp/wt"));
        assert_eq!(out["worktree"]["branch"].as_str(), Some("feature/x"));

        // legacy fields still present
        assert_eq!(out["cost_usd"].as_f64(), Some(1.25));
        assert_eq!(out["context_used_pct"].as_f64(), Some(42.0));
        assert_eq!(out["five_hour"]["used_pct"].as_f64(), Some(10.0));
        assert_eq!(out["seven_day"]["resets_at"].as_u64(), Some(222));
    }

    #[test]
    fn model_falls_back_to_id() {
        let v = parse(r#"{"session_id":"s","model":{"id":"claude-opus-4-8"}}"#);
        let out = build_bridge(&v);
        assert_eq!(out["model"].as_str(), Some("claude-opus-4-8"));
    }

    #[test]
    fn minimal_payload_ok() {
        let v = parse(r#"{"session_id":"only"}"#);
        let out = build_bridge(&v);
        assert!(out.is_object());
        assert!(out["model"].is_null());
        assert!(out["effort"].is_null());
        assert!(out["thinking"].is_null());
        assert!(out["vim_mode"].is_null());
        assert_eq!(out["pr"], Value::Null);
        assert_eq!(out["worktree"], Value::Null);
    }

    #[test]
    fn snake_camel_tolerated() {
        // camelCase rate window (regression guard for window())
        let v = parse(
            r#"{
                "session_id": "s",
                "rate_limits": {"fiveHour": {"utilization": 33.0, "resetsAt": 999}},
                "pr": {"number": 7, "reviewState": "changes_requested"}
            }"#,
        );
        let out = build_bridge(&v);
        assert_eq!(out["five_hour"]["used_pct"].as_f64(), Some(33.0));
        assert_eq!(out["five_hour"]["resets_at"].as_u64(), Some(999));
        // pr.reviewState (camel) read by defensive fallback
        assert_eq!(
            out["pr"]["review_state"].as_str(),
            Some("changes_requested")
        );
    }

    #[test]
    fn nested_read_not_scalar() {
        // effort is the object {"level":"high"} — proves we index to effort.level
        let v = parse(r#"{"session_id":"s","effort":{"level":"high"}}"#);
        let out = build_bridge(&v);
        assert_eq!(out["effort"].as_str(), Some("high"));
    }

    #[test]
    fn never_panics_on_empty_object() {
        let _ = build_bridge(&json!({}));
    }
}
