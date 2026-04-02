pub mod generate;
pub mod live;
pub mod result;
pub mod setup;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::dataset::{GenerateProgress, generate};
use crate::worker::{RunResult, run_scan};

use self::live::{LiveState, SideSnapshot};
use self::result::ResultState;
use self::setup::SetupState;

/// Top-level screen discriminant.
enum Screen {
    Setup,
    Generating,
    Live,
    Result,
}

/// Run the tracer TUI.  Sets up the terminal, runs the event loop, and always
/// restores the terminal before returning.
pub fn run(data_dir: PathBuf) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, data_dir);

    let _ = disable_raw_mode();
    let _ = stdout.execute(LeaveAlternateScreen);
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

    // Background threads post results here when done.
    let pending_serial: Arc<Mutex<Option<RunResult>>> = Arc::new(Mutex::new(None));
    let pending_parallel: Arc<Mutex<Option<RunResult>>> = Arc::new(Mutex::new(None));

    loop {
        // Render current screen.
        match &screen {
            Screen::Setup => {
                terminal.draw(|f| setup::draw(f, &setup))?;
            }
            Screen::Generating => {
                terminal.draw(|f| generate::draw(f, &generate_progress))?;

                // Check if generation finished.
                let done = {
                    let p = generate_progress.lock().unwrap();
                    p.total_keys > 0 && p.keys_written >= p.total_keys
                };
                if done {
                    screen = Screen::Live;
                    live_state = Some(build_live_state(&setup));
                    // Kick off serial run in background.
                    start_serial_run(
                        data_dir.clone(),
                        1,
                        pending_serial.clone(),
                        live_state.as_mut().unwrap(),
                    );
                }
            }
            Screen::Live => {
                // Poll background run results.
                tick_live(
                    live_state.as_mut().unwrap(),
                    &pending_serial,
                    &pending_parallel,
                    &setup,
                    &data_dir,
                );

                terminal.draw(|f| live::draw(f, live_state.as_ref().unwrap()))?;

                // Both sides done -> transition to result.
                if both_done(live_state.as_ref().unwrap(), &setup) {
                    result_state = Some(build_result(&setup, live_state.as_ref().unwrap()));
                    screen = Screen::Result;
                }
            }
            Screen::Result => {
                terminal.draw(|f| result::draw(f, result_state.as_ref().unwrap()))?;
            }
        }

        // Handle key events (non-blocking 16ms poll).
        if !event::poll(std::time::Duration::from_millis(16))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            match screen {
                Screen::Setup => {
                    if handle_setup_key(&mut setup, key) == SetupAction::Quit {
                        return Ok(());
                    } else if handle_setup_key(&mut setup, key) == SetupAction::Run {
                        // Handled below - re-evaluate after match.
                    }
                    match handle_setup_key(&mut setup, key) {
                        SetupAction::Quit => return Ok(()),
                        SetupAction::Run => {
                            if setup.needs_generation() {
                                // Reset progress and launch generation thread.
                                *generate_progress.lock().unwrap() = GenerateProgress::default();
                                let prog = generate_progress.clone();
                                let params = setup.params();
                                let dir = data_dir.clone();
                                std::thread::spawn(move || {
                                    let _ = generate(&dir, &params, prog);
                                });
                                screen = Screen::Generating;
                            } else {
                                screen = Screen::Live;
                                live_state = Some(build_live_state(&setup));
                                start_serial_run(
                                    data_dir.clone(),
                                    1,
                                    pending_serial.clone(),
                                    live_state.as_mut().unwrap(),
                                );
                            }
                        }
                        SetupAction::Regenerate => {
                            *generate_progress.lock().unwrap() = GenerateProgress::default();
                            let prog = generate_progress.clone();
                            let params = setup.params();
                            let dir = data_dir.clone();
                            std::thread::spawn(move || {
                                let _ = generate(&dir, &params, prog);
                            });
                            screen = Screen::Generating;
                        }
                        SetupAction::None => {}
                    }
                }
                Screen::Generating => {
                    if is_quit(key) {
                        return Ok(());
                    }
                }
                Screen::Live => {
                    if is_quit(key) {
                        return Ok(());
                    }
                    if let Some(ls) = live_state.as_mut() {
                        match key.code {
                            KeyCode::Up => ls.scroll = ls.scroll.saturating_sub(1),
                            KeyCode::Down => ls.scroll += 1,
                            _ => {}
                        }
                    }
                }
                Screen::Result => match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('r') => {
                        // Reset everything and go back to setup.
                        *pending_serial.lock().unwrap() = None;
                        *pending_parallel.lock().unwrap() = None;
                        live_state = None;
                        result_state = None;
                        screen = Screen::Setup;
                    }
                    _ => {}
                },
            }
        }
    }
}

// ---- helpers ----------------------------------------------------------------

#[derive(Eq, PartialEq)]
enum SetupAction {
    None,
    Quit,
    Run,
    Regenerate,
}

fn handle_setup_key(state: &mut SetupState, key: KeyEvent) -> SetupAction {
    match key.code {
        KeyCode::Char('q') => SetupAction::Quit,
        KeyCode::Char('g') => SetupAction::Regenerate,
        KeyCode::Enter => SetupAction::Run,
        KeyCode::Up => {
            state.focus_prev();
            SetupAction::None
        }
        KeyCode::Down => {
            state.focus_next();
            SetupAction::None
        }
        KeyCode::Left | KeyCode::Char('-') => {
            state.decrement();
            SetupAction::None
        }
        KeyCode::Right | KeyCode::Char('+') => {
            state.increment();
            SetupAction::None
        }
        _ => SetupAction::None,
    }
}

fn is_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q'))
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

/// Build a fresh `LiveState` from the current setup.
fn build_live_state(setup: &SetupState) -> LiveState {
    use crate::worker::assign_files;
    // Empty assignments; will be filled by run_scan results.
    let empty_serial = SideSnapshot {
        label: "SERIAL".to_string(),
        thread_states: assign_files(&[], 1),
        elapsed_us: 0,
        done: false,
        result: None,
    };
    let empty_parallel = SideSnapshot {
        label: format!("PARALLEL ({} threads)", setup.threads),
        thread_states: assign_files(&[], setup.threads),
        elapsed_us: 0,
        done: false,
        result: None,
    };
    let columns = match setup.mode {
        setup::RunMode::Both => 2,
        _ => 1,
    };
    LiveState {
        serial: empty_serial,
        parallel: empty_parallel,
        total_keys: setup.keys,
        started_at: Instant::now(),
        columns,
        scroll: 0,
    }
}

/// Launch serial run in a background thread, posting result to `pending`.
fn start_serial_run(
    data_dir: PathBuf,
    threads: usize,
    pending: Arc<Mutex<Option<RunResult>>>,
    _live: &mut LiveState,
) {
    std::thread::spawn(move || {
        if let Ok(result) = run_scan(&data_dir, threads) {
            *pending.lock().unwrap() = Some(result);
        }
    });
}

/// Poll pending results and update live state accordingly.
fn tick_live(
    live: &mut LiveState,
    pending_serial: &Arc<Mutex<Option<RunResult>>>,
    pending_parallel: &Arc<Mutex<Option<RunResult>>>,
    setup: &SetupState,
    data_dir: &Path,
) {
    // Absorb completed serial result.
    if !live.serial.done {
        if let Some(r) = pending_serial.lock().unwrap().take() {
            live.serial.thread_states = r.thread_states.clone();
            live.serial.elapsed_us = r.wall_time_us;
            live.serial.done = true;
            live.serial.result = Some(r);

            // Now kick off parallel run if mode requires it.
            if setup.mode != setup::RunMode::Serial {
                let dir = data_dir.to_path_buf();
                let threads = setup.threads;
                let pp = pending_parallel.clone();
                std::thread::spawn(move || {
                    if let Ok(result) = run_scan(&dir, threads) {
                        *pp.lock().unwrap() = Some(result);
                    }
                });
            }
        } else {
            // Serial still running - update elapsed.
            live.serial.elapsed_us = live.started_at.elapsed().as_micros() as u64;
        }
    }

    // Absorb completed parallel result.
    if !live.parallel.done {
        if let Some(r) = pending_parallel.lock().unwrap().take() {
            live.parallel.thread_states = r.thread_states.clone();
            live.parallel.elapsed_us = r.wall_time_us;
            live.parallel.done = true;
            live.parallel.result = Some(r);
        } else if live.serial.done {
            // Parallel is running - update elapsed from wall clock minus serial time.
            let since_serial_done = live.started_at.elapsed().as_micros() as u64;
            if since_serial_done > live.serial.elapsed_us {
                live.parallel.elapsed_us = since_serial_done - live.serial.elapsed_us;
            }
        }
    }
}

fn both_done(live: &LiveState, setup: &SetupState) -> bool {
    match setup.mode {
        setup::RunMode::Serial => live.serial.done,
        setup::RunMode::Parallel => live.parallel.done,
        setup::RunMode::Both => live.serial.done && live.parallel.done,
    }
}

fn build_result(setup: &SetupState, live: &LiveState) -> ResultState {
    let total_bytes = setup.params().keys as u64
        * (setup.params().value_size as u64 + 26 + setup.params().keys.to_string().len() as u64);

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
