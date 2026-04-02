use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::app::{App, LineKind};

/// Cement grey used for the stats bar background.
const CEMENT: Color = Color::Rgb(120, 120, 110);
/// Foreground on the stats bar.
const CEMENT_FG: Color = Color::Rgb(240, 240, 235);

/// Colour used for prompt echo lines (`> cmd`).
const PROMPT_FG: Color = Color::Rgb(100, 180, 255);
/// Colour used for error lines.
const ERROR_FG: Color = Color::Rgb(220, 80, 80);
/// Colour used for normal output lines.
const OUTPUT_FG: Color = Color::White;

/// Render the full TUI frame.
///
/// Layout (top to bottom):
///   - 1-line stats bar with cement grey background.
///   - Remaining height: scrollable output + live input prompt.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // stats bar
            Constraint::Min(1),    // output + input area
        ])
        .split(area);

    draw_stats_bar(frame, app, chunks[0]);
    draw_output_area(frame, app, chunks[1]);
}

/// Render the stats bar: cement background, inline key/value pairs.
fn draw_stats_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let bar_style = Style::default()
        .bg(CEMENT)
        .fg(CEMENT_FG)
        .add_modifier(Modifier::BOLD);

    let text = app.stats_bar();
    let paragraph = Paragraph::new(text).style(bar_style);
    frame.render_widget(paragraph, area);
}

/// Render the scrollable output area.
///
/// All historical output lines are rendered first, followed by the live
/// input prompt (`> cursor`).  The paragraph scrolls so the bottom of the
/// content is always visible.
fn draw_output_area(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    // Build the list of ratatui `Line` values from history + live input.
    let mut lines: Vec<Line> = app
        .output_lines()
        .iter()
        .map(|l| {
            let style = match l.kind {
                LineKind::Prompt => Style::default().fg(PROMPT_FG),
                LineKind::Output => Style::default().fg(OUTPUT_FG),
                LineKind::Error => Style::default().fg(ERROR_FG),
            };
            Line::from(Span::styled(l.text.clone(), style))
        })
        .collect();

    // Append the live input prompt line with a blinking cursor marker.
    let input_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(PROMPT_FG)),
        Span::styled(
            app.current_input().to_string(),
            Style::default().fg(Color::White),
        ),
        // Blinking block cursor rendered as a highlighted space.
        Span::styled(
            " ",
            Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);
    lines.push(input_line);

    // Compute how far to scroll so the last line is always visible.
    let inner_height = area.height.saturating_sub(2) as usize; // subtract border rows
    let total_lines = lines.len();
    let scroll_offset = total_lines.saturating_sub(inner_height) as u16;

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(paragraph, area);
}
