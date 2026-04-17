use std::collections::HashSet;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::worker::{LiveProgress, RunResult, SlotState, ThreadState};

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
    /// Set to `Some(Instant::now())` when the background thread is launched.
    /// Used to compute an accurate live elapsed clock for the parallel side.
    pub started_at: Option<std::time::Instant>,
    /// Live in-flight progress written by the worker threads.
    /// `None` before the scan starts or after it completes.
    pub live_progress: Option<LiveProgress>,
}

/// Which column currently receives ↑/↓ scroll input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusedColumn {
    Serial,
    Parallel,
}

/// All state the live screen needs.
pub struct LiveState {
    /// Serial side (always present - may be empty slots if parallel-only mode).
    pub serial: SideSnapshot,
    /// Parallel side.
    pub parallel: SideSnapshot,
    /// Total keys in the dataset (for the footer counter).
    pub total_keys: usize,
    /// How many columns to show (1 = single mode, 2 = both).
    pub columns: usize,
    /// Independent scroll offsets for each column.
    pub serial_scroll: u16,
    pub parallel_scroll: u16,
    /// Which column ↑/↓ scrolls (only relevant in Both mode).
    pub focused: FocusedColumn,
    /// Cursor row index within the parallel column (points at a thread header).
    pub parallel_cursor: usize,
    /// Thread ids whose file slots are collapsed in the parallel column.
    pub collapsed_threads: HashSet<usize>,
    /// Set to true once all required scans are done.  The screen stays open
    /// so the user can scroll; Enter/Space advances to the Result screen.
    pub finished: bool,
}

impl LiveState {
    /// Number of thread headers in the parallel column (= thread count).
    pub fn parallel_thread_count(&self) -> usize {
        self.parallel.thread_states.len()
    }

    /// Toggle collapse for the thread at `parallel_cursor`.
    pub fn toggle_parallel_cursor(&mut self) {
        let n = self.parallel_thread_count();
        if n == 0 {
            return;
        }
        let cursor = self.parallel_cursor.min(n - 1);
        let tid = self.parallel.thread_states[cursor].thread_id;
        if self.collapsed_threads.contains(&tid) {
            self.collapsed_threads.remove(&tid);
        } else {
            self.collapsed_threads.insert(tid);
        }
    }

    pub fn keys_rebuilt(&self) -> usize {
        // Sum keys across both sides; whichever is actively running contributes.
        let count_side = |side: &SideSnapshot| -> usize {
            side.thread_states
                .iter()
                .flat_map(|ts| ts.slots.iter())
                .map(|slot| match slot.state {
                    SlotState::Done { keys_found, .. } => keys_found,
                    SlotState::Processing { keys_found, .. } => keys_found,
                    _ => 0,
                })
                .sum()
        };
        // In `both` mode the serial run is shown on left; use whichever is
        // currently in progress, falling back to the larger count.
        count_side(&self.serial).max(count_side(&self.parallel))
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
    let footer_height = 5u16; // 3 timing lines + separator + hint line
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

    let serial_focused = state.columns == 1 || state.focused == FocusedColumn::Serial;
    let parallel_focused = state.focused == FocusedColumn::Parallel;
    draw_column(
        frame,
        column_rects[0],
        &state.serial,
        state.serial_scroll,
        serial_focused,
        None, // serial column has no toggle cursor
        &HashSet::new(),
    );
    if state.columns == 2 {
        draw_column(
            frame,
            column_rects[1],
            &state.parallel,
            state.parallel_scroll,
            parallel_focused,
            Some(state.parallel_cursor),
            &state.collapsed_threads,
        );
    }

    draw_footer(frame, footer_rect, state);
}

fn draw_column(
    frame: &mut Frame,
    area: Rect,
    side: &SideSnapshot,
    scroll: u16,
    focused: bool,
    // Some(cursor) means this column has a navigable cursor for toggling.
    cursor: Option<usize>,
    collapsed: &HashSet<usize>,
) {
    let border_style = if side.done {
        Style::default().fg(Color::Green)
    } else if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let status = if side.done {
        "done"
    } else if side.started_at.is_some() {
        "running..."
    } else {
        "waiting"
    };
    let block = Block::default()
        .title(format!(" {} - {} ", side.label, status))
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    frame.render_widget(block, area);

    // Placeholder when no slots are assigned yet.
    if side.thread_states.iter().all(|ts| ts.slots.is_empty()) && !side.done {
        let msg = if side.started_at.is_none() {
            "  (waiting for previous run to finish...)"
        } else {
            "  (loading file list...)"
        };
        frame.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for (thread_idx, thread) in side.thread_states.iter().enumerate() {
        let is_collapsed = collapsed.contains(&thread.thread_id);
        let is_cursor = cursor.map(|c| c == thread_idx).unwrap_or(false);

        // Thread header: ▶/▼ indicator + optional cursor highlight.
        let toggle_icon = if is_collapsed { "▶" } else { "▼" };
        let file_count = thread.slots.len();
        let done_count = thread.slots.iter()
            .filter(|s| matches!(s.state, SlotState::Done { .. }))
            .count();
        let header_text = if is_collapsed {
            format!("{} Thread {}  ({}/{} files)", toggle_icon, thread.thread_id, done_count, file_count)
        } else {
            format!("{} Thread {}", toggle_icon, thread.thread_id)
        };
        let header_style = if is_cursor && focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(COLOR_HEADER)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(Span::styled(header_text, header_style)));

        // File slots — skipped when collapsed.
        if !is_collapsed {
            for slot in &thread.slots {
                let bar = render_bar(slot.file_size_bytes, &slot.state);
                let suffix = slot_suffix(&slot.state);
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

fn slot_suffix(state: &SlotState) -> String {
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

    // Hint line changes based on which column is focused and whether done.
    let parallel_focused = state.focused == FocusedColumn::Parallel && state.columns == 2;
    let hint_line = if state.finished && !parallel_focused {
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": results    ", Style::default().fg(Color::DarkGray)),
            Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": scroll    ", Style::default().fg(Color::DarkGray)),
            Span::styled("←/→", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": switch column    ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": quit", Style::default().fg(Color::DarkGray)),
        ])
    } else if parallel_focused {
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": move cursor    ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": toggle thread    ", Style::default().fg(Color::DarkGray)),
            Span::styled("←", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": serial column    ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": quit", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": scroll    ", Style::default().fg(Color::DarkGray)),
            Span::styled("←/→", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": switch column    ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(": quit", Style::default().fg(Color::DarkGray)),
        ])
    };

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
        hint_line,
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
