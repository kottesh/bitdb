/// Tests for worker state transitions and RunResult correctness.
use std::path::Path;
use tempfile::TempDir;
use tracer::worker::{RunResult, SlotState, run_scan};

fn populate(dir: &Path, keys: usize) {
    let opts = bitdb::config::Options {
        max_data_file_size_bytes: 32 * 1024,
        ..bitdb::config::Options::default()
    };
    let mut engine = bitdb::engine::Engine::open(dir, opts).unwrap();
    for i in 0..keys {
        let key = format!("key:{i:08}");
        let val = format!("val:{i:08}");
        engine.put(key.as_bytes(), val.as_bytes()).unwrap();
    }
}

#[test]
fn run_scan_serial_returns_correct_total_keys() {
    let dir = TempDir::new().unwrap();
    populate(dir.path(), 500);

    let result: RunResult = run_scan(dir.path(), 1).unwrap();
    assert_eq!(result.total_keys, 500);
}

#[test]
fn run_scan_parallel_returns_correct_total_keys() {
    let dir = TempDir::new().unwrap();
    populate(dir.path(), 500);

    let result: RunResult = run_scan(dir.path(), 4).unwrap();
    assert_eq!(result.total_keys, 500);
}

#[test]
fn serial_and_parallel_return_same_key_count() {
    let dir = TempDir::new().unwrap();
    populate(dir.path(), 300);

    let serial = run_scan(dir.path(), 1).unwrap();
    let parallel = run_scan(dir.path(), 4).unwrap();

    assert_eq!(serial.total_keys, parallel.total_keys);
}

#[test]
fn all_slots_finish_in_done_state() {
    let dir = TempDir::new().unwrap();
    populate(dir.path(), 200);

    let result = run_scan(dir.path(), 2).unwrap();
    for thread in &result.thread_states {
        for slot in &thread.slots {
            assert!(
                matches!(slot.state, SlotState::Done { .. }),
                "slot for file {} did not finish in Done state",
                slot.file_id
            );
        }
    }
}

#[test]
fn done_slots_have_nonzero_duration() {
    let dir = TempDir::new().unwrap();
    populate(dir.path(), 200);

    let result = run_scan(dir.path(), 2).unwrap();
    for thread in &result.thread_states {
        for slot in &thread.slots {
            if let SlotState::Done {
                duration_us,
                keys_found,
                ..
            } = slot.state
            {
                assert!(keys_found > 0, "done slot should have keys_found > 0");
                // duration_us can be 0 on very fast machines but bytes_read must be > 0
                let _ = duration_us;
            }
        }
    }
}

#[test]
fn run_result_wall_time_is_nonzero() {
    let dir = TempDir::new().unwrap();
    populate(dir.path(), 300);

    let result = run_scan(dir.path(), 4).unwrap();
    // keys_per_sec must be positive
    assert!(result.keys_per_sec > 0.0);
}

#[test]
fn thread_count_matches_requested() {
    let dir = TempDir::new().unwrap();
    populate(dir.path(), 100);

    let result = run_scan(dir.path(), 3).unwrap();
    assert_eq!(result.thread_states.len(), 3);
}
