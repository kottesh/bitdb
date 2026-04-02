pub mod app;
mod event;
mod ui;

use std::io;
use std::path::Path;

use crossterm::ExecutableCommand;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::error::Result;

/// Launch the TUI for the database at `data_dir`.
///
/// Sets up the terminal, runs the event loop until the user quits, then
/// restores the terminal unconditionally (even on error) so the shell is
/// never left in raw mode.
pub fn run(data_dir: &Path) -> Result<()> {
    let mut app = app::App::new(data_dir)?;
    run_loop(&mut app).map_err(crate::error::BitdbError::Io)
}

fn run_loop(app: &mut app::App) -> io::Result<()> {
    // Enter alternate screen and raw mode.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    // Draw once before the first event so the screen is never blank.
    terminal.draw(|frame| ui::draw(frame, app))?;

    let result = loop {
        // Handle the next terminal event.
        if let Err(e) = event::handle_events(app) {
            break Err(e);
        }

        if app.should_quit {
            break Ok(());
        }

        // Redraw after every event (ratatui is immediate mode).
        if let Err(e) = terminal.draw(|frame| ui::draw(frame, app)) {
            break Err(e);
        }
    };

    // Always restore the terminal, regardless of whether the loop errored.
    let _ = disable_raw_mode();
    let _ = stdout.execute(LeaveAlternateScreen);

    result
}
