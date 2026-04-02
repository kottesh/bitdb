/// Integration tests for Phase 12 - parallel merge pipeline.
///
/// Every test verifies that the parallel merge path produces exactly the same
/// on-disk state as the serial merge path.  We check live-key retention,
/// tombstone exclusion, correctness across multiple input files, and
/// idempotency after a second merge.
use bitdb::config::{Options, Parallelism};
use bitdb::engine::Engine;
use tempfile::TempDir;

fn serial_opts() -> Options {
    Options {
        parallelism: Parallelism::Serial,
        ..Options::default()
    }
}

fn parallel_opts() -> Options {
    Options {
        parallelism: Parallelism::Auto,
        ..Options::default()
    }
}

fn small_file_opts(parallelism: Parallelism) -> Options {
    Options {
        max_data_file_size_bytes: 512,
        parallelism,
        ..Options::default()
    }
}

// ---- helpers ----------------------------------------------------------------

/// Write `count` unique keys, overwrite the first half, delete the last
/// quarter.  Returns the engine with data on disk.
fn write_varied_dataset(dir: &TempDir, count: usize, opts: Options) -> Engine {
    let mut eng = Engine::open(dir.path(), opts).unwrap();
    for i in 0..count {
        let key = format!("k{i:04}").into_bytes();
        let val = format!("v{i:04}_orig").into_bytes();
        eng.put(&key, &val).unwrap();
    }
    // Overwrite the first half with a new value.
    for i in 0..count / 2 {
        let key = format!("k{i:04}").into_bytes();
        let val = format!("v{i:04}_new").into_bytes();
        eng.put(&key, &val).unwrap();
    }
    // Delete the last quarter.
    for i in (count * 3 / 4)..count {
        let key = format!("k{i:04}").into_bytes();
        eng.delete(&key).unwrap();
    }
    eng.sync().unwrap();
    eng
}

// ---- parity tests -----------------------------------------------------------

#[test]
fn parallel_merge_live_keys_match_serial_merge() {
    // Write data, merge with serial, collect results.
    let dir_s = TempDir::new().unwrap();
    {
        let mut eng = write_varied_dataset(&dir_s, 80, serial_opts());
        eng.merge().unwrap();
    }

    // Write identical data, merge with parallel, collect results.
    let dir_p = TempDir::new().unwrap();
    {
        let mut eng = write_varied_dataset(&dir_p, 80, parallel_opts());
        eng.merge().unwrap();
    }

    // Both databases must expose identical get() results for every key.
    let eng_s = Engine::open(dir_s.path(), serial_opts()).unwrap();
    let eng_p = Engine::open(dir_p.path(), parallel_opts()).unwrap();

    for i in 0..80usize {
        let key = format!("k{i:04}").into_bytes();
        assert_eq!(
            eng_s.get(&key).unwrap(),
            eng_p.get(&key).unwrap(),
            "mismatch for key index {i}"
        );
    }
}

#[test]
fn parallel_merge_tombstones_are_excluded() {
    let dir = TempDir::new().unwrap();

    {
        let mut eng = Engine::open(dir.path(), parallel_opts()).unwrap();
        eng.put(b"alive", b"yes").unwrap();
        eng.put(b"gone", b"here").unwrap();
        eng.delete(b"gone").unwrap();
        eng.merge().unwrap();
    }

    let eng = Engine::open(dir.path(), parallel_opts()).unwrap();
    assert_eq!(eng.get(b"alive").unwrap().unwrap(), b"yes");
    assert!(
        eng.get(b"gone").unwrap().is_none(),
        "tombstoned key must be absent after merge"
    );
    // After compaction there must be no tombstone entries in stats.
    assert_eq!(eng.stats().tombstones, 0, "merge must discard tombstones");
}

#[test]
fn parallel_merge_retains_latest_overwrite() {
    let dir = TempDir::new().unwrap();

    {
        let mut eng = Engine::open(dir.path(), parallel_opts()).unwrap();
        for rev in 0..6u32 {
            eng.put(b"hotkey", format!("rev{rev}").as_bytes()).unwrap();
        }
        eng.merge().unwrap();
    }

    let eng = Engine::open(dir.path(), parallel_opts()).unwrap();
    assert_eq!(eng.get(b"hotkey").unwrap().unwrap(), b"rev5");
}

#[test]
fn parallel_merge_across_multiple_input_files() {
    // Force file rotation so we have several input files going into merge.
    let dir = TempDir::new().unwrap();
    let count = 120;

    {
        let mut eng = Engine::open(dir.path(), small_file_opts(Parallelism::Auto)).unwrap();
        for i in 0..count {
            let key = format!("m{i:04}").into_bytes();
            let val = format!("val{i:04}").into_bytes();
            eng.put(&key, &val).unwrap();
        }
        eng.merge().unwrap();
    }

    let eng = Engine::open(dir.path(), parallel_opts()).unwrap();
    for i in 0..count {
        let key = format!("m{i:04}").into_bytes();
        let expected = format!("val{i:04}").into_bytes();
        assert_eq!(
            eng.get(&key).unwrap().unwrap(),
            expected,
            "wrong value for key index {i}"
        );
    }
    assert_eq!(eng.stats().tombstones, 0);
}

#[test]
fn parallel_merge_stats_match_serial_merge() {
    let dir_s = TempDir::new().unwrap();
    {
        let mut eng = write_varied_dataset(&dir_s, 60, serial_opts());
        eng.merge().unwrap();
    }

    let dir_p = TempDir::new().unwrap();
    {
        let mut eng = write_varied_dataset(&dir_p, 60, parallel_opts());
        eng.merge().unwrap();
    }

    let s = Engine::open(dir_s.path(), serial_opts()).unwrap().stats();
    let p = Engine::open(dir_p.path(), parallel_opts()).unwrap().stats();

    assert_eq!(s.live_keys, p.live_keys, "live_keys differ after merge");
    assert_eq!(s.tombstones, p.tombstones, "tombstones differ after merge");
}

#[test]
fn parallel_merge_is_idempotent() {
    // Running merge twice on a parallel engine must leave data unchanged.
    let dir = TempDir::new().unwrap();

    {
        let mut eng = Engine::open(dir.path(), parallel_opts()).unwrap();
        for i in 0..40 {
            eng.put(format!("ik{i}").as_bytes(), format!("iv{i}").as_bytes())
                .unwrap();
        }
        eng.merge().unwrap();
        eng.merge().unwrap();
    }

    let eng = Engine::open(dir.path(), parallel_opts()).unwrap();
    for i in 0..40 {
        let key = format!("ik{i}").into_bytes();
        let expected = format!("iv{i}").into_bytes();
        assert_eq!(eng.get(&key).unwrap().unwrap(), expected);
    }
    assert_eq!(eng.stats().tombstones, 0);
}

#[test]
fn parallel_merge_empty_database_is_safe() {
    let dir = TempDir::new().unwrap();
    let mut eng = Engine::open(dir.path(), parallel_opts()).unwrap();
    // Merging an empty database must not return an error.
    eng.merge().unwrap();
    assert_eq!(eng.stats().live_keys, 0);
}
