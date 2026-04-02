use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use super::app::App;

/// Poll for one terminal event and apply it to `app`.
///
/// Returns `Ok(())` on every handled or ignored event.
/// The caller should check `app.should_quit` after each call.
pub fn handle_events(app: &mut App) -> std::io::Result<()> {
    // Block until an event is available (100 ms timeout so the loop stays
    // responsive without burning CPU).
    if !event::poll(std::time::Duration::from_millis(100))? {
        return Ok(());
    }

    if let Event::Key(key) = event::read()? {
        handle_key(app, key);
    }

    Ok(())
}

/// Dispatch a single key event onto the app state.
fn handle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Submit the current input line.
        KeyCode::Enter => app.submit(),

        // Backspace removes the last character.
        KeyCode::Backspace => app.backspace(),

        // History navigation.
        KeyCode::Up => app.history_prev(),
        KeyCode::Down => app.history_next(),

        // Ctrl-C and Ctrl-D are conventional quit bindings.
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }

        // Any other printable character appends to the input buffer.
        KeyCode::Char(ch) => app.push_char(ch),

        // Ignore all other keys (function keys, escape, etc.).
        _ => {}
    }
}
