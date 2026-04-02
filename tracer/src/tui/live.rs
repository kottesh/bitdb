use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::worker::{RunResult, SlotState, ThreadState};

/// Bar width in characters for file slot progress bars.
const BAR_WIDTH: usize = 20;

/// Colors used for slot states.
const COLOR_QUEUED: Color = Color::DarkGray;
const COLOR_PROCESSING: Color = Color::Yellow;
const COLOR_DONE: Color = Color::Green;
const COLOR_HEADER: Color = Color::Cyan;

/// A snapshot of one side (serial or parallel) that the renderer reads.
pub struct SideSnapshot {
    pub label: String,
    pub thread_states: Vec<ThreadState>,
    pub elapsed_us: u64,
    pub done: bool,
    pub result: Option<RunResult>,
}

/// All state the live screen needs.
pub struct LiveState {
    /// Serial side (always present - may be empty slots if parallel-only mode).
    pub serial: SideSnapshot,
    /// Parallel side.
    pub parallel: SideSnapshot,
    /// Total keys in the dataset (for the footer counter).
    pub total_keys: usize,
    /// When the live view started (for wall clock display).
    pub started_at: Instant,
    /// How many columns to show (1 = single mode, 2 = both).
    pub columns: usize,
    /// Scroll offset (lines) within each column.
    pub scroll: u16,
}

impl LiveState {
    pub fn keys_rebuilt(&self) -> usize {
        // Count Done slots across whichever side is running.
        let states = if self.columns == 2 || self.serial.elapsed_us > 0 {
            &self.serial.thread_states
        } else {
            &self.parallel.thread_states
        };
        states
            .iter()
            .flat_map(|ts| ts.slots.iter())
            .map(|slot| match slot.state {
                SlotState::Done { keys_found, .. } => keys_found,
                SlotState::Processing { keys_found, .. } => keys_found,
                _ => 0,
            })
            .sum()
    }
}

/// Render the live side-by-side view.
pub fn draw(frame: &mut Frame, state: &LiveState) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    // Outer block with title.
    let title = format!(
        " bitdb tracer   keys:{}   threads:{} ",
        format_num(state.total_keys),
        state.parallel.thread_states.len()
    );
    let outer = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(outer, area);

    // Split into content + footer.
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let footer_height = 4u16;
    let content_height = inner.height.saturating_sub(footer_height);

    let content_rect = Rect {
        height: content_height,
        ..inner
    };
    let footer_rect = Rect {
        y: inner.y + content_height,
        height: footer_height,
        ..inner
    };

    // Split content into columns.
    let column_rects: Vec<Rect> = if state.columns == 2 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(content_rect)
            .to_vec()
    } else {
        vec![content_rect]
    };

    draw_column(
        frame,
        column_rects[0],
        &state.serial,
        state.scroll,
        state.columns == 1,
    );
    if state.columns == 2 {
        draw_column(frame, column_rects[1], &state.parallel, state.scroll, false);
    }

    draw_footer(frame, footer_rect, state);
}

fn draw_column(frame: &mut Frame, area: Rect, side: &SideSnapshot, scroll: u16, full_width: bool) {
    let border_style = if side.done {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let status = if side.done { "done" } else { "running..." };
    let block = Block::default()
        .title(format!(" {} - {} ", side.label, status))
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    for thread in &side.thread_states {
        // Thread header.
        lines.push(Line::from(Span::styled(
            format!("Thread {}", thread.thread_id),
            Style::default()
                .fg(COLOR_HEADER)
                .add_modifier(Modifier::BOLD),
        )));

        // One line per file slot.
        for slot in &thread.slots {
            let bar = render_bar(slot.file_size_bytes, &slot.state);
            let suffix = slot_suffix(&slot.state, full_width);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  f:{:03} ", slot.file_id),
                    Style::default().fg(Color::DarkGray),
                ),
                bar,
                Span::styled(suffix, Style::default().fg(Color::DarkGray)),
            ]));
        }

        lines.push(Line::default());
    }

    let para = Paragraph::new(lines).scroll((scroll, 0));
    frame.render_widget(para, inner);
}

fn render_bar(file_size: u64, state: &SlotState) -> Span<'static> {
    let (fill, color) = match state {
        SlotState::Queued => (0, COLOR_QUEUED),
        SlotState::Processing { bytes_done, .. } => {
            let ratio = if file_size == 0 {
                0.5
            } else {
                (*bytes_done as f64 / file_size as f64).min(1.0)
            };
            ((ratio * BAR_WIDTH as f64) as usize, COLOR_PROCESSING)
        }
        SlotState::Done { .. } => (BAR_WIDTH, COLOR_DONE),
    };

    let filled = "\u{2588}".repeat(fill);
    let empty = "\u{2591}".repeat(BAR_WIDTH - fill);
    let bar = format!("[{filled}{empty}]");
    Span::styled(bar, Style::default().fg(color))
}

fn slot_suffix(state: &SlotState, show_detail: bool) -> String {
    if !show_detail {
        return String::new();
    }
    match state {
        SlotState::Queued => "  queued".to_string(),
        SlotState::Processing { keys_found, .. } => {
            format!("  {} keys...", format_num(*keys_found))
        }
        SlotState::Done {
            duration_us,
            keys_found,
            ..
        } => {
            format!("  {}us  {} keys", duration_us, format_num(*keys_found))
        }
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &LiveState) {
    let serial_ms = state.serial.elapsed_us / 1000;
    let parallel_ms = state.parallel.elapsed_us / 1000;

    let speedup = if parallel_ms > 0 && serial_ms > 0 {
        format!("{:.2}x faster", serial_ms as f64 / parallel_ms as f64)
    } else {
        "---".to_string()
    };

    let keys_rebuilt = state.keys_rebuilt();

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  serial    {:>6}ms  ", serial_ms),
                Style::default().fg(Color::White),
            ),
            wall_bar(
                state.serial.elapsed_us,
                state.serial.elapsed_us.max(1),
                Color::Blue,
            ),
            Span::styled(
                if state.serial.done {
                    "  done"
                } else {
                    "  running..."
                },
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  parallel  {:>6}ms  ", parallel_ms),
                Style::default().fg(Color::White),
            ),
            wall_bar(
                state.parallel.elapsed_us,
                state.serial.elapsed_us.max(1),
                Color::Green,
            ),
            Span::styled(
                if state.parallel.done {
                    "  done"
                } else {
                    "  running..."
                },
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  speedup   {:<12}  ", speedup),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "keys rebuilt: {} / {}",
                    format_num(keys_rebuilt),
                    format_num(state.total_keys)
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// A compact horizontal bar showing elapsed vs reference (serial) time.
fn wall_bar(elapsed_us: u64, reference_us: u64, color: Color) -> Span<'static> {
    const W: usize = 32;
    let ratio = if reference_us == 0 {
        0.0
    } else {
        (elapsed_us as f64 / reference_us as f64).min(1.0)
    };
    let fill = (ratio * W as f64) as usize;
    let filled = "\u{2588}".repeat(fill);
    let empty = "\u{2591}".repeat(W - fill);
    Span::styled(format!("[{filled}{empty}]"), Style::default().fg(color))
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
