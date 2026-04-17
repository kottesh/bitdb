pub mod generate;
pub mod live;
pub mod result;
pub mod setup;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::dataset::{GenerateProgress, generate};
use crate::worker::{RunResult, run_scan};

use std::collections::HashSet;
use self::live::{FocusedColumn, LiveState, SideSnapshot};
use crate::worker::LiveProgress;
use self::result::ResultState;
use self::setup::{RunMode, SetupState};

/// Top-level screen discriminant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Screen {
    Setup,
    Generating,
    Live,
    Result,
}

/// Run the tracer TUI.  Sets up the terminal, runs the event loop, and
/// unconditionally restores the terminal before returning.
pub fn run(data_dir: PathBuf) -> io::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, data_dir);

    // Always restore terminal, even on error.
    let _ = disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    data_dir: PathBuf,
) -> io::Result<()> {
    let mut screen = Screen::Setup;
    let mut setup = SetupState::new(data_dir.clone());
    let generate_progress: Arc<Mutex<GenerateProgress>> =
        Arc::new(Mutex::new(GenerateProgress::default()));
    let mut live_state: Option<LiveState> = None;
    let mut result_state: Option<ResultState> = None;
    let mut pending_serial: Arc<Mutex<Option<RunResult>>> = Arc::new(Mutex::new(None));
    let mut pending_parallel: Arc<Mutex<Option<RunResult>>> = Arc::new(Mutex::new(None));

    loop {
        // ---- render --------------------------------------------------------
        match screen {
            Screen::Setup => {
                terminal.draw(|f| setup::draw(f, &setup))?;
            }
            Screen::Generating => {
                terminal.draw(|f| generate::draw(f, &generate_progress))?;

                let done = {
                    let p = generate_progress.lock().unwrap();
                    p.total_keys > 0 && p.keys_written >= p.total_keys
                };
                if done {
                    screen = transition_to_live(
                        &setup,
                        &data_dir,
                        &mut live_state,
                        &pending_serial,
                        &pending_parallel,
                    );
                }
            }
            Screen::Live => {
                tick_live(
                    live_state.as_mut().unwrap(),
                    &pending_serial,
                    &pending_parallel,
                    &setup,
                    &data_dir,
                );

                terminal.draw(|f| live::draw(f, live_state.as_ref().unwrap()))?;

                if both_done(live_state.as_ref().unwrap(), &setup) {
                    live_state.as_mut().unwrap().finished = true;
                }
            }
            Screen::Result => {
                terminal.draw(|f| result::draw(f, result_state.as_ref().unwrap()))?;
            }
        }

        // ---- events (16ms non-blocking poll = ~60fps) ----------------------
        if !event::poll(std::time::Duration::from_millis(16))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };

        // Ctrl-C quits from any screen.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(());
        }

        match screen {
            Screen::Setup => match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('g') | KeyCode::Char('G') => {
                    spawn_generate(&data_dir, &setup, &generate_progress);
                    screen = Screen::Generating;
                }
                KeyCode::Enter => {
                    if setup.needs_generation() {
                        spawn_generate(&data_dir, &setup, &generate_progress);
                        screen = Screen::Generating;
                    } else {
                        screen = transition_to_live(
                            &setup,
                            &data_dir,
                            &mut live_state,
                            &pending_serial,
                            &pending_parallel,
                        );
                    }
                }
                KeyCode::Up => setup.focus_prev(),
                KeyCode::Down => setup.focus_next(),
                KeyCode::Left | KeyCode::Char('-') => setup.decrement(),
                KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => setup.increment(),
                _ => {}
            },

            Screen::Generating => {
                if key.code == KeyCode::Char('q') {
                    return Ok(());
                }
            }

            Screen::Live => {
                if let Some(ls) = live_state.as_mut() {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Left => {
                            if ls.columns == 2 {
                                ls.focused = FocusedColumn::Serial;
                            }
                        }
                        KeyCode::Right => {
                            if ls.columns == 2 {
                                ls.focused = FocusedColumn::Parallel;
                            }
                        }
                        KeyCode::Up => match ls.focused {
                            FocusedColumn::Serial => {
                                ls.serial_scroll = ls.serial_scroll.saturating_sub(1);
                            }
                            FocusedColumn::Parallel => {
                                ls.parallel_cursor = ls.parallel_cursor.saturating_sub(1);
                                // Keep scroll in sync so cursor stays visible.
                                ls.parallel_scroll = ls.parallel_scroll
                                    .min(ls.parallel_cursor as u16);
                            }
                        },
                        KeyCode::Down => match ls.focused {
                            FocusedColumn::Serial => {
                                ls.serial_scroll = ls.serial_scroll.saturating_add(1);
                            }
                            FocusedColumn::Parallel => {
                                let max = ls.parallel_thread_count().saturating_sub(1);
                                ls.parallel_cursor = (ls.parallel_cursor + 1).min(max);
                            }
                        },
                        KeyCode::Enter => {
                            if ls.focused == FocusedColumn::Parallel && ls.columns == 2 {
                                // Toggle the thread under the cursor.
                                ls.toggle_parallel_cursor();
                            } else if ls.finished {
                                result_state = Some(build_result(&setup, ls));
                                screen = Screen::Result;
                            }
                        }
                        KeyCode::Char(' ') if ls.finished => {
                            result_state = Some(build_result(&setup, ls));
                            screen = Screen::Result;
                        }
                        _ => {}
                    }
                }
            }

            Screen::Result => match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    // Reset run state; keep setup params and existing dataset.
                    pending_serial = Arc::new(Mutex::new(None));
                    pending_parallel = Arc::new(Mutex::new(None));
                    live_state = None;
                    result_state = None;
                    screen = Screen::Setup;
                }
                _ => {}
            },
        }
    }
}

// ---- helpers ----------------------------------------------------------------

/// Reset pending slots and transition to the Live screen, kicking off the
/// first run (serial in `Both`/`Serial` mode; parallel in `Parallel` mode).
fn transition_to_live(
    setup: &SetupState,
    data_dir: &Path,
    live_state: &mut Option<LiveState>,
    pending_serial: &Arc<Mutex<Option<RunResult>>>,
    pending_parallel: &Arc<Mutex<Option<RunResult>>>,
) -> Screen {
    // Clear any stale results from a previous run.
    *pending_serial.lock().unwrap() = None;
    *pending_parallel.lock().unwrap() = None;

    *live_state = Some(build_live_state(setup));

    let ls = live_state.as_mut().unwrap();

    match setup.mode {
        RunMode::Parallel => {
            // Skip serial; go straight to parallel.
            ls.parallel.started_at = Some(Instant::now());
            let lp: LiveProgress = Arc::new(Mutex::new(Vec::new()));
            ls.parallel.live_progress = Some(lp.clone());
            let dir = data_dir.to_path_buf();
            let threads = setup.threads;
            let pp = pending_parallel.clone();
            std::thread::spawn(move || {
                if let Ok(r) = run_scan(&dir, threads, Some(&lp)) {
                    *pp.lock().unwrap() = Some(r);
                }
            });
        }
        RunMode::Serial | RunMode::Both => {
            // Serial runs first; parallel will be launched by tick_live when
            // serial completes (in Both mode only).
            ls.serial.started_at = Some(Instant::now());
            let lp: LiveProgress = Arc::new(Mutex::new(Vec::new()));
            ls.serial.live_progress = Some(lp.clone());
            let dir = data_dir.to_path_buf();
            let ps = pending_serial.clone();
            std::thread::spawn(move || {
                if let Ok(r) = run_scan(&dir, 1, Some(&lp)) {
                    *ps.lock().unwrap() = Some(r);
                }
            });
        }
    }

    Screen::Live
}

/// Spawn a dataset generation thread and reset progress.
fn spawn_generate(data_dir: &Path, setup: &SetupState, progress: &Arc<Mutex<GenerateProgress>>) {
    *progress.lock().unwrap() = GenerateProgress::default();
    let dir = data_dir.to_path_buf();
    let params = setup.params();
    let prog = progress.clone();
    std::thread::spawn(move || {
        let _ = generate(&dir, &params, prog);
    });
}

/// Build a fresh `LiveState` from the current setup.
fn build_live_state(setup: &SetupState) -> LiveState {
    use crate::worker::assign_files;
    let serial_side = SideSnapshot {
        label: "SERIAL (1 thread)".to_string(),
        thread_states: assign_files(&[], 1),
        elapsed_us: 0,
        done: setup.mode == RunMode::Parallel, // mark done so it's grayed out
        result: None,
        started_at: None,
        live_progress: None,
    };
    let parallel_side = SideSnapshot {
        label: format!("PARALLEL ({} threads)", setup.threads),
        thread_states: assign_files(&[], setup.threads),
        elapsed_us: 0,
        done: setup.mode == RunMode::Serial, // mark done so it's grayed out
        result: None,
        started_at: None,
        live_progress: None,
    };
    let columns = if setup.mode == RunMode::Both { 2 } else { 1 };
    LiveState {
        serial: serial_side,
        parallel: parallel_side,
        total_keys: setup.keys,
        columns,
        serial_scroll: 0,
        parallel_scroll: 0,
        focused: FocusedColumn::Serial,
        parallel_cursor: 0,
        collapsed_threads: HashSet::new(),
        finished: false,
    }
}

/// Poll pending results and update the live state each tick.
fn tick_live(
    live: &mut LiveState,
    pending_serial: &Arc<Mutex<Option<RunResult>>>,
    pending_parallel: &Arc<Mutex<Option<RunResult>>>,
    setup: &SetupState,
    data_dir: &Path,
) {
    // --- serial side --------------------------------------------------------
    if !live.serial.done {
        if let Some(r) = pending_serial.lock().unwrap().take() {
            // Use the measured wall time from run_scan for the final value.
            live.serial.elapsed_us = r.wall_time_us;
            live.serial.thread_states = r.thread_states.clone();
            live.serial.done = true;
            live.serial.live_progress = None; // scan complete, drop the arc
            live.serial.result = Some(r);

            // In Both mode, kick off the parallel run immediately after serial.
            if setup.mode == RunMode::Both {
                let dir = data_dir.to_path_buf();
                let threads = setup.threads;
                let pp = pending_parallel.clone();
                live.parallel.started_at = Some(Instant::now());
                let lp: LiveProgress = Arc::new(Mutex::new(Vec::new()));
                live.parallel.live_progress = Some(lp.clone());
                std::thread::spawn(move || {
                    if let Ok(r) = run_scan(&dir, threads, Some(&lp)) {
                        *pp.lock().unwrap() = Some(r);
                    }
                });
            }
        } else {
            // Still running — pull the latest thread states from the shared arc.
            if let Some(lp) = &live.serial.live_progress {
                let snapshot = lp.lock().unwrap().clone();
                if !snapshot.is_empty() {
                    live.serial.thread_states = snapshot;
                }
            }
            if let Some(t0) = live.serial.started_at {
                live.serial.elapsed_us = t0.elapsed().as_micros() as u64;
            }
        }
    }

    // --- parallel side ------------------------------------------------------
    if !live.parallel.done {
        if let Some(r) = pending_parallel.lock().unwrap().take() {
            // Use the measured wall time from run_scan for the final value.
            live.parallel.elapsed_us = r.wall_time_us;
            live.parallel.thread_states = r.thread_states.clone();
            live.parallel.done = true;
            live.parallel.live_progress = None; // scan complete, drop the arc
            live.parallel.result = Some(r);
        } else if setup.mode != RunMode::Serial {
            // Still running — pull the latest thread states from the shared arc.
            if let Some(lp) = &live.parallel.live_progress {
                let snapshot = lp.lock().unwrap().clone();
                if !snapshot.is_empty() {
                    live.parallel.thread_states = snapshot;
                }
            }
            if let Some(t0) = live.parallel.started_at {
                live.parallel.elapsed_us = t0.elapsed().as_micros() as u64;
            }
        }
    }
}

fn both_done(live: &LiveState, setup: &SetupState) -> bool {
    match setup.mode {
        RunMode::Serial => live.serial.done,
        RunMode::Parallel => live.parallel.done,
        RunMode::Both => live.serial.done && live.parallel.done,
    }
}

fn build_result(setup: &SetupState, live: &LiveState) -> ResultState {
    // Estimate total bytes from record overhead: key(12) + value + header(~14).
    let params = setup.params();
    let record_overhead: u64 = 14 + 12; // header + "key:XXXXXXXX" key
    let total_bytes = params.keys as u64 * (record_overhead + params.value_size as u64);

    let total_files = live
        .serial
        .thread_states
        .iter()
        .flat_map(|ts| ts.slots.iter())
        .count()
        .max(
            live.parallel
                .thread_states
                .iter()
                .flat_map(|ts| ts.slots.iter())
                .count(),
        );

    ResultState {
        serial: live.serial.result.clone(),
        parallel: live.parallel.result.clone(),
        total_keys: setup.keys,
        total_files,
        total_bytes,
        threads: setup.threads,
    }
}
