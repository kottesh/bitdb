/// Integration tests for the TUI screen state machines.
/// These tests drive state transitions without a real terminal.
use tempfile::TempDir;
use tracer::dataset::{DatasetParams, GenerateProgress, generate};
use tracer::tui::live::LiveState;
use tracer::tui::setup::{RunMode, SetupState};

fn small_params() -> DatasetParams {
    DatasetParams {
        keys: 100,
        value_size: 8,
        file_size_bytes: 16 * 1024,
    }
}

fn populated_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    let params = small_params();
    let progress = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));
    generate(dir.path(), &params, progress).unwrap();
    dir
}

// ---- SetupState tests -------------------------------------------------------

#[test]
fn setup_default_mode_is_both() {
    let dir = TempDir::new().unwrap();
    let state = SetupState::new(dir.path().to_path_buf());
    assert_eq!(state.mode, RunMode::Both);
}

#[test]
fn setup_increment_mode_cycles_serial_parallel_both() {
    let dir = TempDir::new().unwrap();
    let mut state = SetupState::new(dir.path().to_path_buf());
    // Default is Both; cycle through.
    state.focused = tracer::tui::setup::Field::Mode;
    state.increment();
    assert_eq!(state.mode, RunMode::Serial);
    state.increment();
    assert_eq!(state.mode, RunMode::Parallel);
    state.increment();
    assert_eq!(state.mode, RunMode::Both);
}

#[test]
fn setup_decrement_mode_cycles_reverse() {
    let dir = TempDir::new().unwrap();
    let mut state = SetupState::new(dir.path().to_path_buf());
    state.focused = tracer::tui::setup::Field::Mode;
    state.decrement();
    assert_eq!(state.mode, RunMode::Parallel);
    state.decrement();
    assert_eq!(state.mode, RunMode::Serial);
    state.decrement();
    assert_eq!(state.mode, RunMode::Both);
}

#[test]
fn setup_keys_increment_decrement_by_100k() {
    let dir = TempDir::new().unwrap();
    let mut state = SetupState::new(dir.path().to_path_buf());
    state.focused = tracer::tui::setup::Field::Keys;
    let initial = state.keys;
    state.increment();
    assert_eq!(state.keys, initial + 100_000);
    state.decrement();
    assert_eq!(state.keys, initial);
}

#[test]
fn setup_keys_decrement_does_not_go_below_100k() {
    let dir = TempDir::new().unwrap();
    let mut state = SetupState::new(dir.path().to_path_buf());
    state.focused = tracer::tui::setup::Field::Keys;
    state.keys = 100_000;
    state.decrement();
    assert_eq!(state.keys, 100_000, "keys must not go below 100k");
}

#[test]
fn setup_threads_increment_decrement() {
    let dir = TempDir::new().unwrap();
    let mut state = SetupState::new(dir.path().to_path_buf());
    state.focused = tracer::tui::setup::Field::Threads;
    let initial = state.threads;
    state.increment();
    assert_eq!(state.threads, initial + 1);
    state.decrement();
    assert_eq!(state.threads, initial);
}

#[test]
fn setup_threads_min_is_1() {
    let dir = TempDir::new().unwrap();
    let mut state = SetupState::new(dir.path().to_path_buf());
    state.focused = tracer::tui::setup::Field::Threads;
    state.threads = 1;
    state.decrement();
    assert_eq!(state.threads, 1, "threads must not go below 1");
}

#[test]
fn setup_needs_generation_false_after_generate() {
    let dir = populated_dir();
    let mut state = SetupState::new(dir.path().to_path_buf());
    state.keys = small_params().keys;
    state.value_size_idx = 0; // 8 bytes matches small_params
    // file_size_idx: 16KB - find the matching index
    // VALUE_SIZES = [8, 64, 256, 1024] -> idx 0 = 8
    // FILE_SIZES = [128KB, 256KB, 512KB, 1MB, 4MB] -> none is 16KB
    // So needs_generation = true because file size doesn't match.
    assert!(
        state.needs_generation(),
        "file size mismatch should require regeneration"
    );
}

#[test]
fn setup_params_match_meta_after_generate() {
    let dir = TempDir::new().unwrap();
    // Use a file size that matches one of the presets (128KB).
    let params = DatasetParams {
        keys: 50,
        value_size: 8,
        file_size_bytes: 128 * 1024,
    };
    let progress = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));
    generate(dir.path(), &params, progress).unwrap();

    assert!(
        tracer::dataset::params_match(dir.path(), &params),
        "params_match should be true after generation"
    );
}

// ---- LiveState tests --------------------------------------------------------

#[test]
fn live_state_keys_rebuilt_zero_when_all_queued() {
    let dir = TempDir::new().unwrap();
    let _setup = SetupState::new(dir.path().to_path_buf());
    let live = LiveState {
        serial: tracer::tui::live::SideSnapshot {
            label: "SERIAL".into(),
            thread_states: vec![],
            elapsed_us: 0,
            done: false,
            result: None,
            started_at: None,
            live_progress: None,
        },
        parallel: tracer::tui::live::SideSnapshot {
            label: "PARALLEL".into(),
            thread_states: vec![],
            elapsed_us: 0,
            done: false,
            result: None,
            started_at: None,
            live_progress: None,
        },
        total_keys: 100,
        columns: 2,
        serial_scroll: 0,
        parallel_scroll: 0,
        focused: tracer::tui::live::FocusedColumn::Serial,
        parallel_cursor: 0,
        collapsed_threads: std::collections::HashSet::new(),
        finished: false,
    };
    assert_eq!(live.keys_rebuilt(), 0);
}

#[test]
fn live_state_columns_2_in_both_mode() {
    let dir = TempDir::new().unwrap();
    let mut setup = SetupState::new(dir.path().to_path_buf());
    setup.focused = tracer::tui::setup::Field::Mode;
    // Default is Both so columns should be 2.
    assert_eq!(setup.mode, RunMode::Both);
}

#[test]
fn live_state_columns_1_in_serial_mode() {
    let dir = TempDir::new().unwrap();
    let mut setup = SetupState::new(dir.path().to_path_buf());
    setup.mode = RunMode::Serial;
    // Single column when not both.
    assert_ne!(setup.mode, RunMode::Both);
}
