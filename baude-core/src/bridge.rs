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

pub fn run(wrap: Option<String>) -> i32 {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return 1;
    }

    // Best-effort capture — never break the user's statusline over it.
    if let Ok(v) = serde_json::from_str::<Value>(&input) {
        if let Some(sid) = v["session_id"].as_str() {
            let bridge = json!({
                "session_id": sid,
                "updated_unix_ms": now_unix_ms(),
                "cost_usd": v["cost"]["total_cost_usd"].as_f64(),
                "context_used_pct": v["context_window"]["used_percentage"].as_f64(),
                "five_hour": window(&v, "five_hour", "fiveHour"),
                "seven_day": window(&v, "seven_day", "sevenDay"),
            });
            let _ = std::fs::write(bridge_path(sid), bridge.to_string());
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
