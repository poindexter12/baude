//! The opencode backend (sst/opencode). Where claude is managed by scraping
//! on-disk artifacts, opencode is server-first: every `opencode` TUI also runs
//! an HTTP server, and baude pins its port at spawn (`--port`, default 0 =
//! random) so [`Backend::poll_meta`] can read session state over loopback and
//! the daemon's permission bridge can subscribe to `/event`.
//!
//! Wire-schema assumptions verified against opencode **1.18.16**
//! (`.planning/spikes/001-*` + live probes 2026-08-14) — re-verify on version
//! bumps; the bundled `@opencode-ai/sdk` types are known to drift from the
//! live server (`permission.updated` vs the real `permission.asked`,
//! `permissionID/response` vs the real `requestID/reply`):
//!
//! - root TUI accepts `--port`/`--hostname`; `--continue` on a fresh
//!   directory starts cleanly (no fallback wrap needed, unlike claude).
//! - `GET /session?directory=<cwd>` filters the global session list to the
//!   project; objects carry `id`, `directory`, `title`, `cost`,
//!   `tokens{input,output,reasoning,cache{read,write}}`, `model{id,providerID}`,
//!   `time{created,updated}`.
//! - `GET /session/status` returns `{<sessionID>: {"type":"busy"|…}}` for
//!   ACTIVE sessions only — absence means idle.
//! - default permission config ALLOWS bash (verified: no `permission.asked`,
//!   command ran); `--auto` additionally auto-approves anything not
//!   explicitly denied (the `--dangerously-skip-permissions` analog).
//! - `OPENCODE_CONFIG_CONTENT` injects inline config that merges OVER the
//!   project config — prompt mode uses it to force ask-rules with no file
//!   seeding at all.

use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::{Backend, SpawnPlan};
use crate::meta::{now_unix_ms, ClaudeMeta, Usage};
use crate::permission::ResolvedCmd;

/// The inline config exported in `prompt` mode: gate the mutating/dangerous
/// tools behind ask so permissions surface (in the opencode TUI locally, and
/// on the daemon's `/event` bridge for remote approval). Single-quoted into
/// the spawn command — must never contain a single quote.
const PROMPT_CONFIG: &str = r#"{"permission":{"bash":"ask","edit":"ask","webfetch":"ask"}}"#;

/// The flag appended in `skip` mode (and the default): auto-approve anything
/// not explicitly denied. opencode's defaults already allow bash/edit, but a
/// user's global opencode config may carry ask-rules — `skip` means baude
/// sessions never block on a permission, so override them (parity with
/// claude's `--dangerously-skip-permissions`).
const AUTO_FLAG: &str = " --auto";

pub struct OpencodeBackend;

/// Pure: resolve the spawn command for an explicit `mode` (`None` = unset).
/// Mirrors [`crate::permission::resolve_claude_cmd`]'s contract, including
/// BL-04: `prompt` strips a conflicting `--auto` baked into the operator's
/// base command (and reports it via `stripped_skip`) so an operator's skip
/// default can't silently suppress prompt mode. `skip`/default/unrecognized
/// append [`AUTO_FLAG`] once (fail-safe default, T-04-01).
pub fn resolve_opencode_cmd(mode: Option<&str>, base_cmd: &str) -> ResolvedCmd {
    let has_auto = base_cmd.contains("--auto");
    match mode {
        Some("prompt") => {
            if has_auto {
                let cmd = base_cmd
                    .replace(" --auto", "")
                    .replace("--auto ", "")
                    .replace("--auto", "");
                ResolvedCmd {
                    cmd: cmd.trim().to_string(),
                    stripped_skip: true,
                }
            } else {
                ResolvedCmd {
                    cmd: base_cmd.to_string(),
                    stripped_skip: false,
                }
            }
        }
        _ => ResolvedCmd {
            cmd: if has_auto {
                base_cmd.to_string()
            } else {
                format!("{base_cmd}{AUTO_FLAG}")
            },
            stripped_skip: false,
        },
    }
}

/// Pure: compose the spawn command given an allocated `port`. Split from
/// [`Backend::spawn_plan`] so the exact strings are testable without binding
/// sockets or reading the env.
pub fn compose_spawn_cmd(resolved_cmd: &str, port: u16, resume: bool, prompt_mode: bool) -> String {
    let cont = if resume { " --continue" } else { "" };
    let inner = format!("exec {resolved_cmd} --port {port} --hostname 127.0.0.1{cont}");
    if prompt_mode {
        format!("export OPENCODE_CONFIG_CONTENT='{PROMPT_CONFIG}'; {inner}")
    } else {
        inner
    }
}

/// Allocate a free loopback port by binding :0 and dropping the listener.
/// The classic race (someone else grabs it before opencode binds) is
/// vanishingly rare on loopback and non-fatal: opencode fails to bind, the
/// session PTY shows the error, and a restart re-rolls.
fn alloc_port() -> Option<u16> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .ok()
}

impl Backend for OpencodeBackend {
    fn name(&self) -> &'static str {
        "opencode"
    }

    // opencode brands itself lowercase.
    fn display_name(&self) -> &'static str {
        "opencode"
    }

    fn default_cmd(&self) -> &'static str {
        "opencode"
    }

    fn resolve_cmd(&self, base_cmd: &str) -> ResolvedCmd {
        resolve_opencode_cmd(
            std::env::var("BAUDE_PERMISSION_MODE").ok().as_deref(),
            base_cmd,
        )
    }

    /// `--continue` resumes the last session in the directory and starts
    /// cleanly on a fresh one, so there is no `|| exec` fallback. `event_url`
    /// is claude hook plumbing — opencode has no hooks; status flows back
    /// through the pinned server port instead, so it is ignored.
    fn spawn_plan(&self, resolved_cmd: &str, _event_url: Option<&str>, resume: bool) -> SpawnPlan {
        let port = alloc_port();
        let cmd = compose_spawn_cmd(
            resolved_cmd,
            port.unwrap_or(0),
            resume,
            crate::permission::is_prompt_mode(),
        );
        SpawnPlan {
            cmd,
            // Port 0 would let opencode pick a random port baude can't
            // discover — report no server rather than a wrong one.
            server_port: port,
        }
    }

    /// Nothing to seed: permission policy rides the spawn command (`--auto` /
    /// `OPENCODE_CONFIG_CONTENT`), and there are no hooks to wire.
    fn prepare_cwd(&self, _cwd: &Path) {}

    fn poll_meta(
        &self,
        meta: &mut ClaudeMeta,
        cwd: &Path,
        _pid: Option<u32>,
        _spawn_unix_ms: u64,
        repo_root: &Path,
    ) {
        meta.poll_neutral(cwd, repo_root);
        let Some(port) = meta.backend_port else {
            return;
        };
        // Loopback with tight timeouts: this runs on the 1s poll tick, so a
        // dead/starting server must fail fast, not stall the UI loop.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_millis(100))
            .timeout(Duration::from_millis(300))
            .build();
        let base = format!("http://127.0.0.1:{port}");

        if let Ok(resp) = agent
            .get(&format!("{base}/session"))
            .query("directory", &cwd.to_string_lossy())
            .call()
        {
            // The server answered: the TUI is up and can take input. opencode
            // creates its session lazily on the FIRST prompt, so an empty list
            // here is normal — readiness must not wait for a session_id.
            meta.backend_ready = true;
            if let Ok(sessions) = resp.into_json::<Value>() {
                if let Some(best) = pick_session(&sessions) {
                    apply_session(meta, best);
                }
            }
        }

        if let Some(sid) = meta.session_id.clone() {
            if let Ok(resp) = agent.get(&format!("{base}/session/status")).call() {
                if let Ok(statuses) = resp.into_json::<Value>() {
                    apply_status(meta, is_busy(&statuses, &sid), now_unix_ms());
                }
            }
        }
    }

    /// opencode's own TUI prompts in-terminal, so prompt mode works fine
    /// without a daemon — the daemon only ADDS remote (phone) approval.
    fn prompt_mode_needs_daemon(&self) -> bool {
        false
    }
}

/// Pick this PTY's session from the directory-filtered list: the most
/// recently UPDATED one. Covers both spawn shapes — a fresh session is the
/// newest by construction, and `--continue` re-activates the previously
/// newest — without a created-after-spawn filter that would exclude resumed
/// sessions (their `time.created` predates the baude spawn).
fn pick_session(sessions: &Value) -> Option<&Value> {
    sessions
        .as_array()?
        .iter()
        .max_by_key(|s| s["time"]["updated"].as_u64().unwrap_or(0))
}

/// Fill meta from one `/session` object. Untyped `Value` accessors throughout
/// (the [`crate::bridge`] posture): absent/wrong-type keys yield `None`/0 and
/// never panic.
fn apply_session(meta: &mut ClaudeMeta, s: &Value) {
    if let Some(id) = s["id"].as_str() {
        meta.session_id = Some(id.to_string());
    }
    if let Some(t) = s["title"].as_str() {
        meta.title = Some(t.to_string());
    }
    if let Some(m) = s["model"]["id"].as_str() {
        meta.model = Some(m.to_string());
    }
    if let Some(c) = s["cost"].as_f64() {
        meta.session_cost_usd = Some(c);
    }
    let t = &s["tokens"];
    if t.is_object() {
        meta.totals = Usage {
            input: t["input"].as_u64().unwrap_or(0),
            output: t["output"].as_u64().unwrap_or(0),
            cache_read: t["cache"]["read"].as_u64().unwrap_or(0),
            cache_create: t["cache"]["write"].as_u64().unwrap_or(0),
        };
    }
}

/// Whether the `/session/status` map marks `sid` busy. Presence with any
/// non-`idle` type counts (observed types: `busy`; the event stream also
/// carries `tool`/`text`/`reasoning` sub-states) — absence means idle.
fn is_busy(statuses: &Value, sid: &str) -> bool {
    match statuses[sid]["type"].as_str() {
        None => false,
        Some("idle") => false,
        Some(_) => true,
    }
}

/// Record busy/idle into the session-file-tier status slot, stamping the
/// timestamp ONLY on a state transition so `Session::waiting_for_ms` measures
/// time since the flip, not time since the last poll.
fn apply_status(meta: &mut ClaudeMeta, busy: bool, now_unix: u64) {
    if meta.claude_status.map(|(b, _)| b) != Some(busy) {
        meta.claude_status = Some((busy, now_unix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SpawnMode;
    use serde_json::json;

    // ---- resolve_opencode_cmd ------------------------------------------

    #[test]
    fn skip_and_default_append_auto_once() {
        for mode in [None, Some("skip"), Some("bogus")] {
            let r = resolve_opencode_cmd(mode, "opencode");
            assert_eq!(r.cmd, "opencode --auto", "mode {mode:?}");
            assert!(!r.stripped_skip);
        }
        // No double-append when the operator already baked it in.
        let r = resolve_opencode_cmd(None, "opencode --auto");
        assert_eq!(r.cmd, "opencode --auto");
    }

    #[test]
    fn prompt_leaves_cmd_and_strips_conflicting_auto() {
        let r = resolve_opencode_cmd(Some("prompt"), "opencode");
        assert_eq!(r.cmd, "opencode");
        assert!(!r.stripped_skip);

        // BL-04 parity: a baked-in --auto must not suppress prompt mode.
        let r = resolve_opencode_cmd(Some("prompt"), "opencode --auto");
        assert_eq!(r.cmd, "opencode");
        assert!(r.stripped_skip, "conflicting --auto must be reported");
    }

    // ---- compose_spawn_cmd ---------------------------------------------

    #[test]
    fn spawn_cmd_pins_port_and_resume_flag() {
        assert_eq!(
            compose_spawn_cmd("opencode --auto", 14711, SpawnMode::Fresh, false),
            "exec opencode --auto --port 14711 --hostname 127.0.0.1"
        );
        assert_eq!(
            compose_spawn_cmd(
                "opencode --auto",
                14711,
                SpawnMode::ContinueLatest,
                false,
            ),
            "exec opencode --auto --port 14711 --hostname 127.0.0.1 --continue"
        );
    }

    #[test]
    fn targeted_session_is_opaque_environment_data() {
        let hostile = "--help ; $(touch /tmp/baude-nope) ' quoted value";
        let plan = OpencodeBackend.spawn_plan(
            "opencode --auto",
            None,
            SpawnMode::ResumeId(hostile.into()),
        );
        let port = plan.server_port.expect("loopback port allocated");

        assert_eq!(
            plan.cmd,
            format!(
                "exec opencode --auto --port {port} --hostname 127.0.0.1 --session \"$BAUDE_RESUME_ID\""
            )
        );
        assert_eq!(
            plan.env,
            vec![("BAUDE_RESUME_ID".into(), hostile.to_string())]
        );
        assert!(!plan.cmd.contains(hostile));
        assert!(!plan.cmd.contains("--continue"));
    }

    #[test]
    fn prompt_mode_exports_inline_ask_config() {
        let cmd = compose_spawn_cmd("opencode", 9000, SpawnMode::ContinueLatest, true);
        assert!(
            cmd.starts_with("export OPENCODE_CONFIG_CONTENT='{\"permission\""),
            "got: {cmd}"
        );
        // export covers the exec'd process; ask-rules gate the big three.
        for tool in ["bash", "edit", "webfetch"] {
            assert!(cmd.contains(&format!("\"{tool}\":\"ask\"")), "got: {cmd}");
        }
        assert!(cmd.ends_with("exec opencode --port 9000 --hostname 127.0.0.1 --continue"));
        // The single-quoted payload must not itself contain a single quote.
        assert!(!PROMPT_CONFIG.contains('\''));
    }

    // ---- session parsing (fixture captured from opencode 1.18.16) ------

    fn fixture() -> Value {
        json!([
            {
                "id": "ses_old", "directory": "/w", "title": "older",
                "cost": 0.01,
                "tokens": {"input": 5, "output": 5, "reasoning": 0, "cache": {"read": 0, "write": 0}},
                "model": {"id": "gpt-5.4", "providerID": "github-copilot"},
                "time": {"created": 100, "updated": 200}
            },
            {
                "id": "ses_new", "directory": "/w", "title": "env-perm-probe",
                "cost": 0.25,
                "tokens": {"input": 1200, "output": 340, "reasoning": 12, "cache": {"read": 900, "write": 80}},
                "model": {"id": "claude-opus-4.8", "providerID": "github-copilot"},
                "time": {"created": 150, "updated": 900}
            }
        ])
    }

    #[test]
    fn pick_session_prefers_most_recently_updated() {
        let sessions = fixture();
        let best = pick_session(&sessions).expect("non-empty list");
        assert_eq!(best["id"].as_str(), Some("ses_new"));
        // Empty / non-array inputs never panic.
        assert!(pick_session(&json!([])).is_none());
        assert!(pick_session(&json!({})).is_none());
    }

    #[test]
    fn apply_session_fills_meta() {
        let mut meta = ClaudeMeta::default();
        let sessions = fixture();
        apply_session(&mut meta, pick_session(&sessions).unwrap());
        assert_eq!(meta.session_id.as_deref(), Some("ses_new"));
        assert_eq!(meta.title.as_deref(), Some("env-perm-probe"));
        assert_eq!(meta.model.as_deref(), Some("claude-opus-4.8"));
        assert_eq!(meta.session_cost_usd, Some(0.25));
        assert_eq!(meta.totals.input, 1200);
        assert_eq!(meta.totals.output, 340);
        assert_eq!(meta.totals.cache_read, 900);
        assert_eq!(meta.totals.cache_create, 80);
        // A minimal object never panics and leaves fields untouched.
        apply_session(&mut meta, &json!({}));
        assert_eq!(meta.session_id.as_deref(), Some("ses_new"));
    }

    #[test]
    fn is_busy_reads_status_map() {
        // Shape captured live: {"<sid>": {"type": "busy"}}; absent = idle.
        let statuses = json!({"ses_a": {"type": "busy"}});
        assert!(is_busy(&statuses, "ses_a"));
        assert!(!is_busy(&statuses, "ses_b"));
        assert!(!is_busy(&json!({}), "ses_a"));
        assert!(!is_busy(&json!({"ses_a": {"type": "idle"}}), "ses_a"));
        // Unknown future sub-states count as busy (fail toward "working").
        assert!(is_busy(&json!({"ses_a": {"type": "tool"}}), "ses_a"));
    }

    #[test]
    fn apply_status_stamps_only_on_transition() {
        let mut meta = ClaudeMeta::default();
        apply_status(&mut meta, true, 1000);
        assert_eq!(meta.claude_status, Some((true, 1000)));
        // Same state later: timestamp must NOT advance (waiting_for_ms
        // measures since-transition, not since-last-poll).
        apply_status(&mut meta, true, 2000);
        assert_eq!(meta.claude_status, Some((true, 1000)));
        // Transition to idle: fresh stamp.
        apply_status(&mut meta, false, 3000);
        assert_eq!(meta.claude_status, Some((false, 3000)));
        apply_status(&mut meta, false, 4000);
        assert_eq!(meta.claude_status, Some((false, 3000)));
    }

    // ---- spawn_plan (env-free bits) ------------------------------------

    #[test]
    fn spawn_plan_allocates_a_real_port() {
        let plan = OpencodeBackend.spawn_plan("opencode --auto", None, SpawnMode::Fresh);
        let port = plan.server_port.expect("loopback port allocated");
        assert!(port > 0);
        assert!(plan.cmd.contains(&format!("--port {port}")), "{}", plan.cmd);
        assert!(plan.cmd.contains("--hostname 127.0.0.1"));
    }
}
