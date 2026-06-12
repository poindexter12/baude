//! bauded — headless baude session daemon. Owns Claude Code sessions in PTYs
//! (via baude-core) and exposes them over REST + SSE so thin clients (the
//! phone PWA, eventually the TUI) can triage and chat from anywhere on the
//! tailnet. See docs/remote-daemon-plan.md.

mod api;
mod manager;
mod transcript;
mod web;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;

use manager::{lock, Manager};

const DEFAULT_BIND: &str = "127.0.0.1:8642";
const META_POLL_MS: u64 = 1000;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
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

    // Metadata poll loop — same cadence as the TUI tick. Plain thread: the
    // work is blocking file IO under the manager lock.
    {
        let state = Arc::clone(&state);
        std::thread::spawn(move || loop {
            lock(&state).poll();
            std::thread::sleep(Duration::from_millis(META_POLL_MS));
        });
    }

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("bauded listening on http://{bind} ({restored} session(s) restored)");
    axum::serve(listener, api::router(Arc::clone(&state)))
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
