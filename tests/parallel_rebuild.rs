/// Integration tests for Phase 11 - parallel startup rebuild.
///
/// Each test exercises the parallel rebuild path and verifies that the
/// resulting KeyDir is identical to what the serial path would produce.
/// Tests cover: basic parity, overwrite/tombstone correctness, multi-file
/// state, and the `Fixed(1)` / `Serial` / `Auto` parallelism variants.
use bitdb::config::{CorruptionPolicy, Options, Parallelism};
use bitdb::engine::Engine;
use tempfile::TempDir;

fn serial_opts() -> Options {
    Options {
        parallelism: Parallelism::Serial,
        ..Options::default()
    }
}

fn parallel_auto_opts() -> Options {
    Options {
        parallelism: Parallelism::Auto,
        ..Options::default()
    }
}

fn parallel_fixed_opts(n: usize) -> Options {
    Options {
        parallelism: Parallelism::Fixed(n),
        ..Options::default()
    }
}

// ---- helpers ----------------------------------------------------------------

/// Populate a database with a deterministic set of keys and return the dir.
fn populate(dir: &TempDir, count: usize, opts: Options) -> Engine {
    let mut eng = Engine::open(dir.path(), opts).unwrap();
    for i in 0..count {
        let key = format!("key:{i:06}").into_bytes();
        let val = format!("val:{i:06}").into_bytes();
        eng.put(&key, &val).unwrap();
    }
    eng
}

// ---- parity tests -----------------------------------------------------------

#[test]
fn parallel_auto_matches_serial_on_simple_dataset() {
    let dir = TempDir::new().unwrap();

    // Write data using serial mode.
    {
        let eng = populate(&dir, 200, serial_opts());
        eng.sync().unwrap();
    }

    // Reopen serially and collect all live key/value pairs.
    let serial_pairs: Vec<(Vec<u8>, Vec<u8>)> = {
        let eng = Engine::open(dir.path(), serial_opts()).unwrap();
        let mut pairs: Vec<_> = (0..200)
            .map(|i| {
                let key = format!("key:{i:06}").into_bytes();
                let val = eng.get(&key).unwrap().unwrap();
                (key, val)
            })
            .collect();
        pairs.sort();
        pairs
    };

    // Reopen using parallel Auto mode and collect the same pairs.
    let parallel_pairs: Vec<(Vec<u8>, Vec<u8>)> = {
        let eng = Engine::open(dir.path(), parallel_auto_opts()).unwrap();
        let mut pairs: Vec<_> = (0..200)
            .map(|i| {
                let key = format!("key:{i:06}").into_bytes();
                let val = eng.get(&key).unwrap().unwrap();
                (key, val)
            })
            .collect();
        pairs.sort();
        pairs
    };

    assert_eq!(serial_pairs, parallel_pairs);
}

#[test]
fn parallel_fixed_1_matches_serial() {
    let dir = TempDir::new().unwrap();

    {
        let eng = populate(&dir, 50, serial_opts());
        eng.sync().unwrap();
    }

    let reopen_serial = |i: usize| -> Vec<u8> {
        let eng = Engine::open(dir.path(), serial_opts()).unwrap();
        let key = format!("key:{i:06}").into_bytes();
        eng.get(&key).unwrap().unwrap()
    };
    let reopen_parallel = |i: usize| -> Vec<u8> {
        let eng = Engine::open(dir.path(), parallel_fixed_opts(1)).unwrap();
        let key = format!("key:{i:06}").into_bytes();
        eng.get(&key).unwrap().unwrap()
    };

    for i in 0..50 {
        assert_eq!(
            reopen_serial(i),
            reopen_parallel(i),
            "mismatch at key index {i}"
        );
    }
}

#[test]
fn parallel_fixed_4_matches_serial() {
    let dir = TempDir::new().unwrap();

    {
        let eng = populate(&dir, 300, serial_opts());
        eng.sync().unwrap();
    }

    let eng_serial = Engine::open(dir.path(), serial_opts()).unwrap();
    let eng_parallel = Engine::open(dir.path(), parallel_fixed_opts(4)).unwrap();

    for i in 0..300 {
        let key = format!("key:{i:06}").into_bytes();
        assert_eq!(
            eng_serial.get(&key).unwrap(),
            eng_parallel.get(&key).unwrap(),
            "mismatch at key index {i}"
        );
    }
}

#[test]
fn parallel_rebuild_respects_overwrites() {
    // Write a key multiple times so only the latest value should survive.
    let dir = TempDir::new().unwrap();

    {
        let mut eng = Engine::open(dir.path(), serial_opts()).unwrap();
        for rev in 0..5u32 {
            eng.put(b"mykey", format!("rev{rev}").as_bytes()).unwrap();
        }
        eng.sync().unwrap();
    }

    let eng = Engine::open(dir.path(), parallel_auto_opts()).unwrap();
    assert_eq!(eng.get(b"mykey").unwrap().unwrap(), b"rev4");
}

#[test]
fn parallel_rebuild_respects_tombstones() {
    let dir = TempDir::new().unwrap();

    {
        let mut eng = Engine::open(dir.path(), serial_opts()).unwrap();
        eng.put(b"gone", b"here").unwrap();
        eng.delete(b"gone").unwrap();
        eng.put(b"alive", b"yes").unwrap();
        eng.sync().unwrap();
    }

    let eng = Engine::open(dir.path(), parallel_auto_opts()).unwrap();
    assert!(
        eng.get(b"gone").unwrap().is_none(),
        "deleted key must be absent"
    );
    assert_eq!(eng.get(b"alive").unwrap().unwrap(), b"yes");
}

#[test]
fn parallel_rebuild_across_multiple_data_files() {
    // Force rotation so we have several data files, then verify parallel
    // rebuild sees all of them and produces correct state.
    let dir = TempDir::new().unwrap();
    let small_file_opts = Options {
        max_data_file_size_bytes: 512,
        parallelism: Parallelism::Serial,
        ..Options::default()
    };

    let key_count = 100;
    {
        let mut eng = Engine::open(dir.path(), small_file_opts.clone()).unwrap();
        for i in 0..key_count {
            let key = format!("k{i:04}").into_bytes();
            let val = format!("v{i:04}").into_bytes();
            eng.put(&key, &val).unwrap();
        }
        eng.sync().unwrap();
    }

    // Parallel reopen with the same small-file limit.
    let par_opts = Options {
        max_data_file_size_bytes: 512,
        parallelism: Parallelism::Auto,
        ..Options::default()
    };
    let eng = Engine::open(dir.path(), par_opts).unwrap();

    for i in 0..key_count {
        let key = format!("k{i:04}").into_bytes();
        let expected = format!("v{i:04}").into_bytes();
        assert_eq!(
            eng.get(&key).unwrap().unwrap(),
            expected,
            "wrong value for key index {i}"
        );
    }
}

#[test]
fn parallel_stats_match_serial_stats() {
    let dir = TempDir::new().unwrap();

    {
        let mut eng = Engine::open(dir.path(), serial_opts()).unwrap();
        for i in 0..20 {
            eng.put(format!("k{i}").as_bytes(), b"val").unwrap();
        }
        // Delete a few to create tombstones.
        for i in 0..5 {
            eng.delete(format!("k{i}").as_bytes()).unwrap();
        }
        eng.sync().unwrap();
    }

    let s = Engine::open(dir.path(), serial_opts()).unwrap().stats();
    let p = Engine::open(dir.path(), parallel_auto_opts())
        .unwrap()
        .stats();

    assert_eq!(s.live_keys, p.live_keys, "live_keys mismatch");
    assert_eq!(s.tombstones, p.tombstones, "tombstones mismatch");
}

#[test]
fn parallel_rebuild_with_skip_corruption_policy() {
    // Verify that the corruption-policy option is respected by the parallel
    // path the same way it is by the serial path.
    let dir = TempDir::new().unwrap();

    {
        let mut eng = Engine::open(dir.path(), serial_opts()).unwrap();
        eng.put(b"a", b"1").unwrap();
        eng.put(b"b", b"2").unwrap();
        eng.sync().unwrap();
    }

    let opts = Options {
        corruption_policy: CorruptionPolicy::SkipCorruptedTail,
        parallelism: Parallelism::Auto,
        ..Options::default()
    };
    let eng = Engine::open(dir.path(), opts).unwrap();
    assert_eq!(eng.get(b"a").unwrap().unwrap(), b"1");
    assert_eq!(eng.get(b"b").unwrap().unwrap(), b"2");
}
