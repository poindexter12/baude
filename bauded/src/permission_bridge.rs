//! opencode prompt-mode permission bridge (daemon side).
//!
//! The flow spike 001a validated (`.planning/spikes/001-a-permission-reply-server-api/`,
//! opencode 1.18.16): a pending tool permission is announced as a
//! `permission.asked` event on the session server's SSE `/event` stream, the
//! agent stays blocked until someone replies, and
//! `POST /session/{sessionID}/permissions/{permissionID}` with
//! `{"response":"once"|"reject"}` resolves it.
//!
//! One watcher thread per (session, port) subscribes to that stream and maps
//! each ask onto the daemon's EXISTING permission plumbing — `set_pending`
//! renders the PWA approve/deny card and routes the distinct
//! `notified_permission` push (via the pending-aware `waiting_reason`), the
//! human's POST records the decision, and this bridge relays it back to
//! opencode. Deny-on-timeout, never auto-allow (V4): no decision within
//! `BAUDE_PERMISSION_TIMEOUT_S` (default 120s) replies `reject`.
//!
//! Claude sessions never get a watcher — their prompt mode rides the
//! `permission-mcp` stdio bridge instead. The watcher exits when the session
//! disappears or its port changes (a restart re-rolls the port, spawning a
//! fresh watcher; the old server died with the old PTY, ending the stream).

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::manager::{lock, PendingPermission, Shared};
use baude_core::meta::now_unix_ms;

/// Watchers already running, keyed (session id, server port). A restart
/// re-rolls the port, so the stale watcher (draining a dead stream) and the
/// fresh one never collide on a key.
fn watchers() -> &'static Mutex<HashSet<(u64, u16)>> {
    static W: OnceLock<Mutex<HashSet<(u64, u16)>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Start a permission watcher for `id` if (and only if) the active backend is
/// opencode, prompt mode is on, the session has a server port, and no watcher
/// for that (id, port) is already running. Call after every spawn path —
/// create, restore, restart. Cheap no-op otherwise.
pub fn watch_if_needed(shared: &Shared, id: u64) {
    if baude_core::backend::active().name() != "opencode" {
        return;
    }
    if !baude_core::permission::is_prompt_mode() {
        return;
    }
    let Ok(Some(port)) = lock(shared).backend_port(id) else {
        return;
    };
    {
        let mut w = watchers().lock().unwrap_or_else(|e| e.into_inner());
        if !w.insert((id, port)) {
            return; // already watching this exact server
        }
    }
    let shared = Shared::clone(shared);
    std::thread::spawn(move || {
        watcher_loop(&shared, id, port);
        watchers()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(id, port));
    });
}

/// Subscribe to `/event` and relay permission asks until the session goes
/// away or is restarted onto a different port. The SSE stream heartbeats
/// every ~10s, so a dead server surfaces as a read error/EOF promptly; the
/// outer loop then re-checks liveness and reconnects (e.g. a server still
/// booting right after spawn).
fn watcher_loop(shared: &Shared, id: u64, port: u16) {
    // No overall timeout: the GET is a long-lived SSE stream. Connect timeout
    // keeps the boot-retry loop snappy.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(1000))
        .build();
    loop {
        match lock(shared).backend_port(id) {
            Ok(Some(p)) if p == port => {}
            _ => return, // session gone or restarted onto a new port
        }
        if let Ok(resp) = agent.get(&format!("http://127.0.0.1:{port}/event")).call() {
            let reader = BufReader::new(resp.into_reader());
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let Ok(v) = serde_json::from_str::<Value>(payload.trim()) else {
                    continue;
                };
                if v["type"].as_str() == Some("permission.asked") {
                    handle_ask(shared, id, port, &agent, &v["properties"]);
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Map one `permission.asked` onto the daemon's pending/decision plumbing and
/// relay the outcome. Blocks this watcher thread until resolved — a second
/// concurrent ask queues in the stream buffer and is handled next (and an
/// opencode `reject` auto-rejects the session's other pending asks anyway).
fn handle_ask(shared: &Shared, id: u64, port: u16, agent: &ureq::Agent, props: &Value) {
    let (Some(perm_id), Some(sid)) = (props["id"].as_str(), props["sessionID"].as_str()) else {
        return;
    };
    let pending = PendingPermission {
        request_id: perm_id.to_string(),
        // e.g. "bash"; metadata carries the human detail ({"command": …}).
        tool: props["permission"].as_str().unwrap_or("tool").to_string(),
        input: props["metadata"].clone(),
        ts: now_unix_ms(),
    };
    if lock(shared).set_pending(id, pending).is_err() {
        return; // session vanished
    }

    let deadline =
        Instant::now() + Duration::from_secs(baude_core::permission::permission_timeout_s());
    let reply = loop {
        std::thread::sleep(Duration::from_millis(250));
        match lock(shared).decision(id) {
            // Only OUR request's decision counts — a stale decision from an
            // earlier ask must not resolve this one.
            Ok(Some(d)) if d.request_id == perm_id => {
                break decision_to_reply(&d.decision);
            }
            Err(_) => break "reject", // session gone — unblock opencode safely
            _ => {}
        }
        if Instant::now() >= deadline {
            // Deny-on-timeout (V4): clear the card and reject the tool.
            let _ = lock(shared).resolve_pending(id, "deny");
            break "reject";
        }
    };

    let url = format!("http://127.0.0.1:{port}/session/{sid}/permissions/{perm_id}");
    let _ = agent
        .post(&url)
        .timeout(Duration::from_secs(5))
        .set("Content-Type", "application/json")
        .send_string(&serde_json::json!({ "response": reply }).to_string());
}

/// Human decision → opencode reply. `allow` maps to `once` (never `always`:
/// scope is not enforced in baude's permission model, WR-03); anything else
/// is `reject` (deny-default).
fn decision_to_reply(decision: &str) -> &'static str {
    if decision == "allow" {
        "once"
    } else {
        "reject"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_maps_deny_default() {
        assert_eq!(decision_to_reply("allow"), "once");
        assert_eq!(decision_to_reply("deny"), "reject");
        // Defense in depth: anything unexpected rejects, never allows.
        assert_eq!(decision_to_reply("ALLOW"), "reject");
        assert_eq!(decision_to_reply(""), "reject");
    }

    #[test]
    fn pending_maps_from_permission_asked_props() {
        // Shape captured live from opencode 1.18.16 (spike 001a).
        let props: Value = serde_json::from_str(
            r#"{
                "id": "per_001",
                "sessionID": "ses_001",
                "permission": "bash",
                "patterns": ["echo hi > proof.txt"],
                "metadata": {"command": "echo hi > proof.txt"},
                "always": ["echo *"],
                "tool": {"messageID": "msg_1", "callID": "call_1"}
            }"#,
        )
        .unwrap();
        // The mapping handle_ask performs, minus the manager side effects.
        let pending = PendingPermission {
            request_id: props["id"].as_str().unwrap().to_string(),
            tool: props["permission"].as_str().unwrap_or("tool").to_string(),
            input: props["metadata"].clone(),
            ts: 1,
        };
        assert_eq!(pending.request_id, "per_001");
        assert_eq!(pending.tool, "bash");
        assert_eq!(
            pending.input["command"].as_str(),
            Some("echo hi > proof.txt")
        );
    }
}
