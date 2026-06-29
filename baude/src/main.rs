mod app;
mod keys;
mod remote;
mod ui;
mod usage;

use std::io::stdout;
use std::time::Duration;

use anyhow::Result;

const AUTO_DAEMON_URL: &str = "http://127.0.0.1:8642";

fn daemon_is_up() -> bool {
    ureq::get(&format!("{AUTO_DAEMON_URL}/sessions"))
        .timeout(Duration::from_millis(300))
        .call()
        .is_ok()
}

/// If `auto_daemon` is enabled (config or env) and no explicit daemon URL is
/// configured, ensure a local `bauded` is running and return its URL.
/// Returns `None` when auto-daemon is disabled or already handled by config.
fn ensure_daemon(config: &baude_core::persist::Config) -> Option<String> {
    // Explicit URL already configured — nothing to do.
    if std::env::var("BAUDE_DAEMON_URL").is_ok() || config.daemon_url.is_some() {
        return None;
    }
    let auto = config.auto_daemon
        || std::env::var("BAUDE_AUTO_DAEMON")
            .map(|v| matches!(v.as_str(), "1" | "true"))
            .unwrap_or(false);
    if !auto {
        return None;
    }
    if daemon_is_up() {
        return Some(AUTO_DAEMON_URL.to_string());
    }
    // Locate bauded: same directory as this binary, then PATH.
    let bauded = std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name("bauded"))
        .filter(|p| p.exists())
        .or_else(|| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|dir| dir.join("bauded"))
                    .find(|p| p.exists())
            })
        })?;
    // Spawn detached; drop the handle — bauded outlives the TUI.
    std::process::Command::new(&bauded)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    // Wait up to 2 s for bauded to bind.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(200));
        if daemon_is_up() {
            return Some(AUTO_DAEMON_URL.to_string());
        }
    }
    None
}
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

use app::App;

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    );
}

/// `<binary> hook` — Claude Code lifecycle-event hook, no TUI. Claude invokes it
/// headless per event, piping the hook JSON to stdin. Reads stdin, then defers
/// to `baude_core::hook::dispatch_hook`, which normalizes the payload and routes
/// it: POST to `$BAUDE_EVENT_URL` (daemon transport) or append to
/// `/tmp/baude-events-<sid>.jsonl` (TUI-local). On a POST failure (wrong/dead
/// port, transport error, OR timeout) it falls back to the file-append so the
/// event is never silently lost — the daemon tails the same file (WR-02).
///
/// The POST uses a bounded agent (WR-04): the hook runs synchronously in Claude
/// Code's critical path and the contract is "ALWAYS exit 0 so a hook failure
/// never blocks Claude". A loopback peer that accepts then stalls would hang the
/// POST; the connect/read timeouts cap that, then the file-append fallback runs.
///
/// NOTE: `bauded` carries a byte-identical `run_hook` because `seed_settings`
/// seeds `current_exe()` and the daemon binary spawns its own sessions — keep
/// the two in sync (the shared normalization lives in `dispatch_hook`).
fn run_hook() -> ! {
    use std::io::Read;
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let url = std::env::var("BAUDE_EVENT_URL").ok();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(2))
        .build();
    baude_core::hook::dispatch_hook(&input, url.as_deref(), |url, line| {
        agent.post(url).send_string(line).is_ok()
    });
    std::process::exit(0);
}

/// `<binary> permission-mcp` — the blocking stdio JSON-RPC MCP server Claude
/// invokes (via `--permission-prompt-tool mcp__baude__approve`) for each
/// unresolved tool-permission decision in `prompt` mode. The BLOCKING inverse of
/// `run_hook`: where the hook is fire-and-forget exit-0, this sits on Claude's
/// critical path and each `tools/call` blocks until a human `allow`/`deny`
/// arrives OR the deadline denies (deny-on-timeout, never auto-allow — V4).
///
/// All framing/protocol lives in `baude_core::permission::run_permission_mcp`;
/// this binary owns only the env read + the `ureq` daemon round-trip (the
/// `dispatch_hook` split). The resolver POSTs the pending request to
/// `…/sessions/{id}/permission` then long-polls GET until a decision appears or
/// `$BAUDE_PERMISSION_TIMEOUT_S` (default 120s) elapses. If `$BAUDE_EVENT_URL`
/// is absent (no daemon), it fails CLOSED to `deny` (never allow).
///
/// NOTE: `bauded` carries a byte-identical `run_permission_mcp` because the
/// daemon seeds `current_exe()` (= `bauded`) as the `.mcp.json` command — WITHOUT
/// the arm, `bauded permission-mcp` would fall through and boot a *second
/// daemon* (the Phase-2 `bauded hook` trap; Pitfall 2). Keep the two in sync.
fn run_permission_mcp() -> ! {
    use baude_core::permission::{
        decide_with_timeout, permission_timeout_s, permission_url_from_event_url,
    };
    use std::time::{Duration, Instant};

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
        // into the poll loop, which denies on the deadline. The body IS JSON, so
        // declare `application/json` — the daemon's `Json<PermissionBody>`
        // extractor 415s a `text/plain` body (ureq's `send_string` default), which
        // would silently drop the registration and deny every tool (PERM-BUG).
        let _ = agent
            .post(perm_url)
            .set("Content-Type", "application/json")
            .send_string(&req.to_string());

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

fn main() -> Result<()> {
    // `baude statusline [--wrap <cmd>]` — statusline bridge mode, no TUI.
    // Must be dispatched before anything touches the terminal: Claude Code
    // invokes it headless on every statusline refresh.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("statusline") {
        let wrap = args
            .iter()
            .position(|a| a == "--wrap")
            .and_then(|i| args.get(i + 1))
            .cloned();
        std::process::exit(baude_core::bridge::run(wrap));
    }

    // `baude hook` — Claude Code lifecycle-event hook, no TUI. Claude invokes
    // it headless per event, piping the hook JSON to stdin. We normalize it to
    // one event line and route it: POST to `$BAUDE_EVENT_URL` (daemon
    // transport) or append to `/tmp/baude-events-<sid>.jsonl` (TUI-local).
    // Best-effort throughout — ALWAYS exit 0 so a hook failure never blocks
    // Claude (a non-zero exit is a blocking signal to the CLI).
    if args.get(1).map(String::as_str) == Some("hook") {
        run_hook();
    }

    // `baude permission-mcp` — the blocking stdio JSON-RPC permission bridge
    // Claude invokes in `prompt` mode. MUST be dispatched before the TUI
    // touches the terminal (it speaks MCP on stdio, no UI). Byte-identical to
    // the `bauded` arm (Pitfall 2). Blocks on Claude's critical path with
    // deny-on-timeout — contrast `hook`'s always-exit-0.
    if args.get(1).map(String::as_str) == Some("permission-mcp") {
        run_permission_mcp();
    }

    // `baude --version` / `--help` — print and exit BEFORE the launch-dir logic
    // below (which would otherwise treat the flag as a repo path and boot the
    // TUI without a TTY). Mirrors the `bauded` arms.
    match args.get(1).map(String::as_str) {
        Some("--version" | "-V") => {
            println!("baude {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!(
                "baude {} — multiple Claude Code sessions in one terminal\n\n\
                 usage: baude [<repo-dir>]\n\n\
                 subcommands: statusline, hook, permission-mcp\n\
                 options:     --version/-V, --help/-h",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        _ => {}
    }

    let launch_dir = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let launch_dir = launch_dir.canonicalize().unwrap_or(launch_dir);

    // Auto-start local bauded when auto_daemon is configured. Must run before
    // App::new() reads the env, and before any threads start (set_var is not
    // thread-safe, but we're still single-threaded here).
    let config = baude_core::persist::load_config();
    if let Some(url) = ensure_daemon(&config) {
        std::env::set_var("BAUDE_DAEMON_URL", url);
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    enable_raw_mode()?;
    execute!(
        stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout()))?;

    let mut app = App::new(launch_dir);
    app.restore();

    let result = run(&mut terminal, &mut app);

    app.save();
    app.kill_all();
    restore_terminal();
    result
}

fn run(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        app.tick();

        let area = terminal.get_frame().area();
        app.sync_sizes(area);

        terminal.draw(|frame| ui::draw(frame, app))?;

        // Drain pending events, then sleep briefly (the draw loop doubles as
        // the refresh tick for streaming PTY output and status timers).
        if event::poll(Duration::from_millis(50))? {
            loop {
                app.handle_event(event::read()?);
                if !event::poll(Duration::from_millis(0))? {
                    break;
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
