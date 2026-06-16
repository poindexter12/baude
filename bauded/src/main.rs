//! bauded — headless baude session daemon. Owns Claude Code sessions in PTYs
//! (via baude-core) and exposes them over REST + SSE so thin clients (the
//! phone PWA, eventually the TUI) can triage and chat from anywhere on the
//! tailnet. See docs/remote-daemon-plan.md.

mod api;
mod manager;
mod notify;
mod push;
mod transcript;
mod web;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;

use manager::{lock, Manager};

const DEFAULT_BIND: &str = "127.0.0.1:8642";
const META_POLL_MS: u64 = 1000;

/// `bauded hook` — headless Claude Code lifecycle-event hook. Byte-identical to
/// the `baude` (TUI) binary's `run_hook`: read the hook JSON from stdin, then
/// defer to `baude_core::hook::dispatch_hook`, which normalizes it and routes
/// it (POST to `$BAUDE_EVENT_URL`, else append to `/tmp/baude-events-<sid>.jsonl`,
/// with the file-append fallback on POST failure — WR-02). The bounded ureq
/// agent (WR-04) keeps a stalled loopback peer from blocking Claude. ALWAYS
/// exits 0 so a hook failure never blocks the CLI. The shared normalization
/// lives in `dispatch_hook` — keep this in sync with `baude`'s `run_hook`.
fn run_hook() -> ! {
    use std::io::Read;
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let url = std::env::var("BAUDE_EVENT_URL").ok();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(500))
        .timeout(Duration::from_secs(2))
        .build();
    baude_core::hook::dispatch_hook(&input, url.as_deref(), |url, line| {
        agent.post(url).send_string(line).is_ok()
    });
    std::process::exit(0);
}

/// `bauded permission-mcp` — the blocking stdio JSON-RPC permission bridge.
/// Byte-identical to the `baude` (TUI) binary's `run_permission_mcp`: the daemon
/// seeds `current_exe()` (= `bauded`) as the `.mcp.json` command, so the daemon
/// binary MUST handle `permission-mcp`. Without this arm, `bauded permission-mcp`
/// falls through and boots a *second daemon* instead of speaking MCP — exactly
/// the Phase-2 `bauded hook` trap (Pitfall 2).
///
/// The BLOCKING inverse of `run_hook`: it sits on Claude's critical path and
/// each `tools/call` blocks until a human `allow`/`deny` arrives OR the deadline
/// denies (deny-on-timeout, never auto-allow — V4). Framing/protocol live in
/// `baude_core::permission::run_permission_mcp`; this binary owns only the env
/// read + `ureq` daemon round-trip. Absent `$BAUDE_EVENT_URL` (no daemon) fails
/// CLOSED to `deny`. Keep in sync with `baude`'s `run_permission_mcp`.
fn run_permission_mcp() -> ! {
    use std::time::Instant;

    use baude_core::permission::{
        decide_with_timeout, permission_timeout_s, permission_url_from_event_url,
    };

    let timeout_s = permission_timeout_s();
    let perm_url = std::env::var("BAUDE_EVENT_URL")
        .ok()
        .and_then(|u| permission_url_from_event_url(&u));
    // WR-02: the client read timeout MUST be strictly greater than the server
    // long-poll window (`wait=5` below). When they are equal the GET frequently
    // times out at the exact boundary the daemon is still holding the poll open,
    // converting every long-poll into a spurious timeout-then-retry. 8s > 5s
    // leaves headroom while staying well under the deny-on-deadline window
    // (BAUDE_PERMISSION_TIMEOUT_S, default 120s, unchanged).
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(500))
        .timeout(Duration::from_secs(8))
        .build();
    let mut req_counter: u64 = 0;

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    baude_core::permission::run_permission_mcp(stdin.lock(), stdout.lock(), |tool, input| {
        // Fail closed: no daemon URL -> deny (never allow) — the no-daemon path.
        let Some(perm_url) = perm_url.as_deref() else {
            return "deny".to_string();
        };
        req_counter += 1;
        let request_id = format!("{}-{}", std::process::id(), req_counter);
        let req = serde_json::json!({ "request_id": request_id, "tool": tool, "input": input });
        // Register the pending request (best-effort POST). A failure still falls
        // into the poll loop, which denies on the deadline.
        let _ = agent.post(perm_url).send_string(&req.to_string());

        // Long-poll GET until a decision for THIS request appears or the
        // deadline passes -> deny (deny-on-timeout, security-critical, V4).
        let deadline = Instant::now() + Duration::from_secs(timeout_s);
        loop {
            let mut decision: Option<String> = None;
            if let Ok(resp) = agent.get(perm_url).query("wait", "5").call() {
                if let Ok(v) = resp.into_json::<serde_json::Value>() {
                    if v["request_id"].as_str() == Some(request_id.as_str()) {
                        decision = v["decision"].as_str().map(str::to_string);
                    }
                }
            }
            let passed = Instant::now() >= deadline;
            match decide_with_timeout(decision.as_deref(), passed) {
                "" => std::thread::sleep(Duration::from_millis(500)), // keep polling
                verdict => break verdict.to_string(),
            }
        }
    });
    std::process::exit(0);
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--version" | "-V") => {
            println!("bauded {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!(
                "bauded {} — headless baude session daemon\n\n\
                 usage: bauded [--bind <addr>]\n\n\
                 options:\n  \
                 --bind <addr>   listen address (default {DEFAULT_BIND}; env BAUDED_BIND)\n\n\
                 env:\n  \
                 BAUDE_CLAUDE_CMD   command run per session (default \"claude\")\n  \
                 BAUDED_BIND        listen address\n\n\
                 Serves the phone PWA at / and the REST+SSE API under /sessions.",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        // `bauded hook` — Claude Code lifecycle-event hook. The daemon seeds its
        // own `current_exe()` (= `bauded`) as the hook command in each spawned
        // session's settings.local.json, so the daemon binary MUST handle the
        // `hook` subcommand. Without this arm, `bauded hook` falls through and
        // boots a *second daemon* instead of emitting an event, silently
        // breaking hook-driven status for every daemon-managed session.
        Some("hook") => run_hook(),
        // `bauded permission-mcp` — the blocking stdio JSON-RPC permission
        // bridge. The daemon seeds its own `current_exe()` (= `bauded`) as the
        // `.mcp.json` command, so the daemon binary MUST handle `permission-mcp`.
        // Without this arm it falls through and boots a *second daemon* instead
        // of speaking MCP (Pitfall 2, the same trap as `bauded hook`).
        Some("permission-mcp") => run_permission_mcp(),
        _ => {}
    }
    let bind = args
        .iter()
        .position(|a| a == "--bind")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| std::env::var("BAUDED_BIND").ok())
        .unwrap_or_else(|| DEFAULT_BIND.to_string());

    let mut manager = Manager::new(manager::default_claude_cmd(), true);
    let restored = manager.restore();
    let state = Arc::new(Mutex::new(manager));
    let push_state: push::SharedPush = Arc::new(Mutex::new(push::PushState::load(true)?));

    // Metadata poll loop — same cadence as the TUI tick. Plain thread: the
    // work is blocking file IO under the manager lock. The notifier rides
    // along: decide under the lock, send (network) outside it.
    {
        let state = Arc::clone(&state);
        let push_state = Arc::clone(&push_state);
        std::thread::spawn(move || {
            let mut notifier = notify::Notifier::default();
            loop {
                let infos = {
                    let mut m = lock(&state);
                    m.poll();
                    m.list()
                };
                let pending = notifier.tick(&infos);
                if !pending.is_empty() {
                    // Snapshot under the lock; the network sends run outside.
                    let (subs, vapid) = {
                        let p = push::lock(&push_state);
                        (p.subs(), p.vapid.clone())
                    };
                    let mut dead = Vec::new();
                    for n in &pending {
                        let payload = n.to_json();
                        for sub in &subs {
                            match push::send(&vapid, sub, &payload) {
                                Ok(true) => {}
                                Ok(false) => dead.push(sub.endpoint.clone()),
                                Err(e) => eprintln!("push: {e}"),
                            }
                        }
                    }
                    push::lock(&push_state).remove_dead(&dead);
                }
                std::thread::sleep(Duration::from_millis(META_POLL_MS));
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("bauded listening on http://{bind} ({restored} session(s) restored)");
    let app = api::router(Arc::clone(&state)).merge(api::push_router(push_state));
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // The PTY children die with the daemon either way; save first so the next
    // start brings every conversation back via `claude --continue`.
    let mut m = lock(&state);
    m.save();
    m.kill_all();
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
