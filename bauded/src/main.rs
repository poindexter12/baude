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
