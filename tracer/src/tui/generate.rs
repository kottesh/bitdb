use std::sync::{Arc, Mutex};

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, Paragraph};

use crate::dataset::GenerateProgress;

/// Render the dataset generation progress screen.
pub fn draw(frame: &mut Frame, progress: &Arc<Mutex<GenerateProgress>>) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" bitdb tracer - generating dataset ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });

    let p = progress.lock().unwrap().clone();

    let ratio = if p.total_keys == 0 {
        0.0
    } else {
        p.keys_written as f64 / p.total_keys as f64
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // spacer + label
            Constraint::Length(1), // progress bar
            Constraint::Length(1), // spacer
            Constraint::Length(3), // stats
            Constraint::Min(1),
        ])
        .split(inner);

    // Label.
    let label = Paragraph::new("writing keys to disk...")
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);
    frame.render_widget(label, chunks[0]);

    // Progress bar.
    let pct = (ratio * 100.0) as u16;
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
        .percent(pct)
        .label(format!(
            "{} / {}",
            format_num(p.keys_written),
            format_num(p.total_keys)
        ));
    frame.render_widget(gauge, chunks[1]);

    // Stats.
    let elapsed_secs = p.elapsed_ms as f64 / 1000.0;
    let remaining = if ratio > 0.0 {
        let total_est = elapsed_secs / ratio;
        let rem = total_est - elapsed_secs;
        format!("~{:.0}s remaining", rem)
    } else {
        "calculating...".to_string()
    };

    let stats = vec![
        Line::from(vec![
            Span::styled("elapsed    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.1}s", elapsed_secs),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("estimated  ", Style::default().fg(Color::DarkGray)),
            Span::styled(remaining, Style::default().fg(Color::Yellow)),
        ]),
    ];
    frame.render_widget(Paragraph::new(stats), chunks[3]);
}

fn format_num(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}
