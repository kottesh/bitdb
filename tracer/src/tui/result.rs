use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::worker::RunResult;

/// Frozen final result screen.
pub struct ResultState {
    pub serial: Option<RunResult>,
    pub parallel: Option<RunResult>,
    pub total_keys: usize,
    pub total_files: usize,
    pub total_bytes: u64,
    pub threads: usize,
}

pub fn draw(frame: &mut Frame, state: &ResultState) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" bitdb tracer - COMPLETE ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacer
            Constraint::Length(6), // rebuild results
            Constraint::Length(1), // spacer
            Constraint::Length(6), // dataset info
            Constraint::Min(1),    // spacer
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // Rebuild results.
    let mut result_lines = vec![Line::from(Span::styled(
        "startup rebuild",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    ))];

    if let Some(s) = &state.serial {
        result_lines.push(result_row("serial", s.wall_time_us, s.keys_per_sec, None));
    }
    if let Some(p) = &state.parallel {
        let speedup = state.serial.as_ref().map(|s| {
            if p.wall_time_us > 0 {
                s.wall_time_us as f64 / p.wall_time_us as f64
            } else {
                0.0
            }
        });
        result_lines.push(result_row(
            "parallel",
            p.wall_time_us,
            p.keys_per_sec,
            speedup,
        ));
    }
    frame.render_widget(Paragraph::new(result_lines), chunks[1]);

    // Dataset info.
    let info_lines = vec![
        Line::from(Span::styled(
            "dataset",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        info_row("keys", &format_num(state.total_keys)),
        info_row("files", &state.total_files.to_string()),
        info_row("total size", &format_bytes(state.total_bytes)),
        info_row("threads", &state.threads.to_string()),
    ];
    frame.render_widget(Paragraph::new(info_lines), chunks[3]);

    // Hint bar.
    let hint = Paragraph::new("r: back to setup    q: quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[5]);
}

fn result_row(label: &str, wall_us: u64, keys_per_sec: f64, speedup: Option<f64>) -> Line<'static> {
    let ms = wall_us / 1000;
    let kps = format_num(keys_per_sec as usize);
    let speedup_str = match speedup {
        Some(s) if s > 0.0 => format!("    {:.2}x faster", s),
        _ => String::new(),
    };
    Line::from(vec![
        Span::styled(format!("  {:<12}", label), Style::default().fg(Color::Gray)),
        Span::styled(format!("{:>8}ms", ms), Style::default().fg(Color::White)),
        Span::styled(
            format!("   {:>14} keys/sec", kps),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            speedup_str,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn info_row(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {:<14}", label),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
    ])
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

fn format_bytes(b: u64) -> String {
    if b >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", b as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} KB", b as f64 / 1024.0)
    }
}
