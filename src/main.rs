mod app;
mod git;
mod keys;
mod meta;
mod persist;
mod pty;
mod session;
mod ui;

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
