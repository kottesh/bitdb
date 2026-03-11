use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

use bitdb::config::{CorruptionPolicy, Options};
use tempfile::tempdir;

#[test]
fn truncated_tail_is_ignored_on_reopen() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();

    {
        let mut engine =
            bitdb::engine::Engine::open(dir.path(), options.clone()).expect("open should work");
        engine.put(b"k1", b"v1").expect("put should work");
        engine.put(b"k2", b"v2").expect("put should work");
        engine.sync().expect("sync should work");
    }

    let path = dir.path().join("00000001.data");
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("data file should open");
    let len = file.seek(SeekFrom::End(0)).expect("seek should work");
    file.set_len(len - 4).expect("truncate should work");

    let reopened = bitdb::engine::Engine::open(
        dir.path(),
        Options {
            corruption_policy: CorruptionPolicy::SkipCorruptedTail,
            ..options
        },
    )
    .expect("open should succeed with truncated tail policy");

    assert_eq!(
        reopened.get(b"k1").expect("get should work"),
        Some(b"v1".to_vec())
    );
    assert_eq!(reopened.get(b"k2").expect("get should work"), None);
}

#[test]
fn bad_crc_fails_open_in_fail_policy() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();

    {
        let mut engine =
            bitdb::engine::Engine::open(dir.path(), options.clone()).expect("open should work");
        engine.put(b"k", b"v").expect("put should work");
        engine.sync().expect("sync should work");
    }

    let path = dir.path().join("00000001.data");
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("data file should open");
    file.seek(SeekFrom::Start(12)).expect("seek should work");
    file.write_all(&[0xFF]).expect("write should work");
    file.sync_data().expect("sync should work");

    let result = bitdb::engine::Engine::open(
        dir.path(),
        Options {
            corruption_policy: CorruptionPolicy::Fail,
            ..options
        },
    );

    assert!(result.is_err());
}

#[test]
fn bad_crc_is_ignored_in_skip_corrupted_tail_policy() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();

    {
        let mut engine =
            bitdb::engine::Engine::open(dir.path(), options.clone()).expect("open should work");
        engine.put(b"k", b"v").expect("put should work");
        engine.sync().expect("sync should work");
    }

    let path = dir.path().join("00000001.data");
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("data file should open");
    file.seek(SeekFrom::Start(12)).expect("seek should work");
    file.write_all(&[0xEE]).expect("write should work");
    file.sync_data().expect("sync should work");

    let reopened = bitdb::engine::Engine::open(
        dir.path(),
        Options {
            corruption_policy: CorruptionPolicy::SkipCorruptedTail,
            ..options
        },
    )
    .expect("open should succeed");

    assert_eq!(reopened.get(b"k").expect("get should work"), None);
}
