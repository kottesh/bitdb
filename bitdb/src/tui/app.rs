use std::path::{Path, PathBuf};

use crate::config::Options;
use crate::engine::Engine;

/// Classifies a line in the output history so the renderer can style it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineKind {
    /// A `> cmd` echo of the submitted command.
    Prompt,
    /// A normal result or informational line.
    Output,
    /// An error or warning line.
    Error,
}

/// One line stored in the output history.
#[derive(Clone, Debug)]
pub struct OutputLine {
    pub text: String,
    pub kind: LineKind,
}

impl OutputLine {
    fn prompt(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: LineKind::Prompt,
        }
    }

    fn output(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: LineKind::Output,
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: LineKind::Error,
        }
    }
}

/// All mutable state for the TUI session.
///
/// The renderer and event handler borrow `App` but never own it.
/// Every method that changes state is synchronous and infallible from
/// the caller's perspective - engine errors are captured as error lines
/// in the output history rather than propagated.
pub struct App {
    /// The open database engine.
    engine: Engine,
    /// Path to the data directory; kept for display in the stats bar.
    data_dir: PathBuf,
    /// Current contents of the input line.
    input: String,
    /// Submitted command history (oldest first).
    history: Vec<String>,
    /// Index into `history` during up/down navigation.
    /// `None` means the cursor is past the newest entry (live input).
    history_cursor: Option<usize>,
    /// All output lines rendered in the main scroll area.
    output: Vec<OutputLine>,
    /// Whether the app should exit on the next event loop tick.
    pub should_quit: bool,
}

impl App {
    /// Open the engine at `data_dir` and return a fresh `App`.
    pub fn new(data_dir: &Path) -> crate::error::Result<Self> {
        let engine = Engine::open(data_dir, Options::default())?;
        Ok(Self {
            engine,
            data_dir: data_dir.to_path_buf(),
            input: String::new(),
            history: Vec::new(),
            history_cursor: None,
            output: Vec::new(),
            should_quit: false,
        })
    }

    // ---- read accessors used by the renderer and tests ----------------------

    /// Current text in the input line.
    pub fn current_input(&self) -> &str {
        &self.input
    }

    /// All lines in the output history.
    pub fn output_lines(&self) -> &[OutputLine] {
        &self.output
    }

    /// Formatted stats bar string: live_keys / tombstones / data_dir.
    pub fn stats_bar(&self) -> String {
        let s = self.engine.stats();
        format!(
            "  live_keys: {}   tombstones: {}   data_dir: {}  ",
            s.live_keys,
            s.tombstones,
            self.data_dir.display()
        )
    }

    // ---- input editing ------------------------------------------------------

    /// Replace the entire input buffer (used by tests and history navigation).
    pub fn set_input(&mut self, text: &str) {
        self.input = text.to_string();
        self.history_cursor = None;
    }

    /// Append a single character to the input buffer.
    pub fn push_char(&mut self, ch: char) {
        self.input.push(ch);
        self.history_cursor = None;
    }

    /// Remove the last character from the input buffer.
    pub fn backspace(&mut self) {
        self.input.pop();
        self.history_cursor = None;
    }

    // ---- history navigation -------------------------------------------------

    /// Move backward (older) through command history.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        self.history_cursor = Some(match self.history_cursor {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        });
        self.input = self.history[self.history_cursor.unwrap()].clone();
    }

    /// Move forward (newer) through command history.
    /// Reaching past the newest entry restores an empty input line.
    pub fn history_next(&mut self) {
        match self.history_cursor {
            None => {}
            Some(i) if i + 1 >= self.history.len() => {
                self.history_cursor = None;
                self.input.clear();
            }
            Some(i) => {
                self.history_cursor = Some(i + 1);
                self.input = self.history[i + 1].clone();
            }
        }
    }

    // ---- command submission -------------------------------------------------

    /// Submit the current input line, dispatch the command, append output.
    ///
    /// Does nothing if the input is empty (no blank lines added).
    pub fn submit(&mut self) {
        let raw = self.input.trim().to_string();
        if raw.is_empty() {
            return;
        }

        // Echo the command as a prompt line.
        self.output.push(OutputLine::prompt(format!("> {raw}")));

        // Record in history and reset cursor.
        self.history.push(raw.clone());
        self.history_cursor = None;
        self.input.clear();

        // Dispatch.
        let parts: Vec<&str> = raw.splitn(3, ' ').collect();
        match parts[0] {
            "help" => self.cmd_help(),
            "put" => self.cmd_put(&parts),
            "get" => self.cmd_get(&parts),
            "delete" => self.cmd_delete(&parts),
            "stats" => self.cmd_stats(),
            "merge" => self.cmd_merge(),
            "clear" => self.cmd_clear(),
            "quit" | "exit" => self.should_quit = true,
            other => {
                self.output.push(OutputLine::error(format!(
                    "unknown command: {other}  (type 'help' for a list of commands)"
                )));
            }
        }
    }

    // ---- individual command handlers ----------------------------------------

    fn cmd_help(&mut self) {
        let lines = [
            "commands:",
            "  put <key> <value>   insert or overwrite a key",
            "  get <key>           retrieve a value (NOT_FOUND if absent)",
            "  delete <key>        delete a key (tombstone)",
            "  stats               show live_keys and tombstones",
            "  merge               run compaction (removes dead records)",
            "  clear               clear this output history",
            "  quit / exit         exit the TUI",
        ];
        for line in lines {
            self.output.push(OutputLine::output(line));
        }
    }

    fn cmd_put(&mut self, parts: &[&str]) {
        if parts.len() < 3 {
            self.output
                .push(OutputLine::error("usage: put <key> <value>"));
            return;
        }
        match self.engine.put(parts[1].as_bytes(), parts[2].as_bytes()) {
            Ok(()) => self.output.push(OutputLine::output("OK")),
            Err(e) => self.output.push(OutputLine::error(format!("error: {e}"))),
        }
    }

    fn cmd_get(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.output.push(OutputLine::error("usage: get <key>"));
            return;
        }
        match self.engine.get(parts[1].as_bytes()) {
            Ok(Some(val)) => self.output.push(OutputLine::output(
                String::from_utf8_lossy(&val).into_owned(),
            )),
            Ok(None) => self.output.push(OutputLine::output("NOT_FOUND")),
            Err(e) => self.output.push(OutputLine::error(format!("error: {e}"))),
        }
    }

    fn cmd_delete(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.output.push(OutputLine::error("usage: delete <key>"));
            return;
        }
        match self.engine.delete(parts[1].as_bytes()) {
            Ok(()) => self.output.push(OutputLine::output("OK")),
            Err(e) => self.output.push(OutputLine::error(format!("error: {e}"))),
        }
    }

    fn cmd_stats(&mut self) {
        let s = self.engine.stats();
        self.output.push(OutputLine::output(format!(
            "live_keys={}  tombstones={}",
            s.live_keys, s.tombstones
        )));
    }

    fn cmd_merge(&mut self) {
        match self.engine.merge() {
            Ok(()) => self.output.push(OutputLine::output("OK")),
            Err(e) => self.output.push(OutputLine::error(format!("error: {e}"))),
        }
    }

    fn cmd_clear(&mut self) {
        self.output.clear();
    }
}
