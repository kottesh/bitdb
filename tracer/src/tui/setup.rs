use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::dataset::{DatasetParams, params_match};

/// Selectable fields on the setup screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    Keys,
    ValueSize,
    FileSize,
    Threads,
    Mode,
}

impl Field {
    const ALL: [Field; 5] = [
        Field::Keys,
        Field::ValueSize,
        Field::FileSize,
        Field::Threads,
        Field::Mode,
    ];

    fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|f| f == &self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|f| f == &self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Which runs to perform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunMode {
    Serial,
    Parallel,
    Both,
}

impl RunMode {
    fn next(self) -> Self {
        match self {
            Self::Serial => Self::Parallel,
            Self::Parallel => Self::Both,
            Self::Both => Self::Serial,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Serial => Self::Both,
            Self::Parallel => Self::Serial,
            Self::Both => Self::Parallel,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::Parallel => "parallel",
            Self::Both => "both",
        }
    }
}

const VALUE_SIZES: [usize; 4] = [8, 64, 256, 1024];
const FILE_SIZES: [u64; 5] = [
    128 * 1024,
    256 * 1024,
    512 * 1024,
    1024 * 1024,
    4 * 1024 * 1024,
];

/// All mutable state for the setup screen.
pub struct SetupState {
    pub data_dir: std::path::PathBuf,
    pub keys: usize,
    pub value_size_idx: usize,
    pub file_size_idx: usize,
    pub threads: usize,
    pub mode: RunMode,
    pub focused: Field,
}

impl SetupState {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        Self {
            data_dir,
            keys: 1_000_000,
            value_size_idx: 1, // 64 bytes
            file_size_idx: 2,  // 512 KB
            threads: 4,
            mode: RunMode::Both,
            focused: Field::Keys,
        }
    }

    pub fn params(&self) -> DatasetParams {
        DatasetParams {
            keys: self.keys,
            value_size: VALUE_SIZES[self.value_size_idx],
            file_size_bytes: FILE_SIZES[self.file_size_idx],
        }
    }

    pub fn needs_generation(&self) -> bool {
        !params_match(&self.data_dir, &self.params())
    }

    pub fn file_size_label(&self) -> String {
        let b = FILE_SIZES[self.file_size_idx];
        if b >= 1024 * 1024 {
            format!("{}MB", b / (1024 * 1024))
        } else {
            format!("{}KB", b / 1024)
        }
    }

    pub fn focus_next(&mut self) {
        self.focused = self.focused.next();
    }

    pub fn focus_prev(&mut self) {
        self.focused = self.focused.prev();
    }

    pub fn increment(&mut self) {
        match self.focused {
            Field::Keys => self.keys = (self.keys + 100_000).min(5_000_000),
            Field::ValueSize => self.value_size_idx = (self.value_size_idx + 1) % VALUE_SIZES.len(),
            Field::FileSize => self.file_size_idx = (self.file_size_idx + 1) % FILE_SIZES.len(),
            Field::Threads => self.threads = (self.threads + 1).min(16),
            Field::Mode => self.mode = self.mode.next(),
        }
    }

    pub fn decrement(&mut self) {
        match self.focused {
            Field::Keys => self.keys = self.keys.saturating_sub(100_000).max(100_000),
            Field::ValueSize => {
                self.value_size_idx =
                    (self.value_size_idx + VALUE_SIZES.len() - 1) % VALUE_SIZES.len()
            }
            Field::FileSize => {
                self.file_size_idx = (self.file_size_idx + FILE_SIZES.len() - 1) % FILE_SIZES.len()
            }
            Field::Threads => self.threads = self.threads.saturating_sub(1).max(1),
            Field::Mode => self.mode = self.mode.prev(),
        }
    }
}

/// Render the setup screen.
pub fn draw(frame: &mut Frame, state: &SetupState) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" bitdb tracer - setup ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacer
            Constraint::Length(7), // dataset section
            Constraint::Length(1), // spacer
            Constraint::Length(3), // threads
            Constraint::Length(1), // spacer
            Constraint::Length(3), // mode
            Constraint::Length(1), // spacer
            Constraint::Length(2), // status
            Constraint::Min(1),    // spacer
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // Dataset section.
    let ds_lines = vec![
        section_header("dataset"),
        field_line(
            "keys",
            &format_keys(state.keys),
            state.focused == Field::Keys,
        ),
        field_line(
            "value size",
            &format!("{}b", VALUE_SIZES[state.value_size_idx]),
            state.focused == Field::ValueSize,
        ),
        field_line(
            "file size",
            &state.file_size_label(),
            state.focused == Field::FileSize,
        ),
        Line::default(),
    ];
    frame.render_widget(Paragraph::new(ds_lines), chunks[1]);

    // Threads section.
    let th_lines = vec![
        section_header("threads"),
        field_line(
            "parallel",
            &state.threads.to_string(),
            state.focused == Field::Threads,
        ),
    ];
    frame.render_widget(Paragraph::new(th_lines), chunks[3]);

    // Mode section.
    let mode_lines = vec![
        section_header("mode"),
        field_line("run mode", state.mode.label(), state.focused == Field::Mode),
    ];
    frame.render_widget(Paragraph::new(mode_lines), chunks[5]);

    // Dataset status line.
    let (status_text, status_color) = if state.needs_generation() {
        ("needs generation".to_string(), Color::Yellow)
    } else {
        let p = state.params();
        let approx_files = (p.keys as u64 * (p.value_size as u64 + 26)) / p.file_size_bytes + 1;
        (
            format!(
                "ready  ({} keys, {} files, {} file size)",
                format_keys(p.keys),
                approx_files,
                state.file_size_label()
            ),
            Color::Green,
        )
    };
    let status_line = Line::from(vec![
        Span::styled("dataset status  ", Style::default().fg(Color::DarkGray)),
        Span::styled(status_text, Style::default().fg(status_color)),
    ]);
    frame.render_widget(Paragraph::new(vec![status_line]), chunks[7]);

    // Hint bar.
    let hint = Paragraph::new(
        "Up/Down: select    Left/Right or -/+: change    Enter: run    g: regenerate    q: quit",
    )
    .style(Style::default().fg(Color::DarkGray))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[9]);
}

fn section_header(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_string(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    ))
}

fn field_line(name: &str, value: &str, focused: bool) -> Line<'static> {
    let prefix = if focused { "> " } else { "  " };
    let name_style = if focused {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let value_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    Line::from(vec![
        Span::styled(prefix.to_string(), name_style),
        Span::styled(format!("{:<14}", name), name_style),
        Span::styled(value.to_string(), value_style),
    ])
}

fn format_keys(n: usize) -> String {
    // Insert thousands separators.
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
