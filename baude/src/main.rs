mod app;
mod keys;
mod remote;
mod ui;
mod usage;

use std::io::stdout;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

use app::App;

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), DisableBracketedPaste, LeaveAlternateScreen);
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
        use std::io::Read;
        let mut input = String::new();
        let _ = std::io::stdin().read_to_string(&mut input);
        let v = serde_json::from_str::<serde_json::Value>(&input)
            .unwrap_or_else(|_| serde_json::json!({}));
        let line = baude_core::hook::build_event(&v).to_string();
        let sid = v["session_id"].as_str().unwrap_or_default();
        // Route the event: POST to the daemon transport when $BAUDE_EVENT_URL
        // is set, else append to the TUI-local /tmp file. On a POST failure
        // (wrong/dead port, transport error) `route_event` falls back to the
        // file-append so the event is never silently lost — the daemon tails
        // the same /tmp file, so it converges either way (WR-02).
        let url = std::env::var("BAUDE_EVENT_URL").ok();
        baude_core::hook::route_event(url.as_deref(), sid, &line, |url, line| {
            ureq::post(url).send_string(line).is_ok()
        });
        std::process::exit(0);
    }

    let launch_dir = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let launch_dir = launch_dir.canonicalize().unwrap_or(launch_dir);

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
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
        let rects = ui::layout(area);
        app.sync_sizes(rects.content);

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
