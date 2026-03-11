use bitdb::config::Options;
use bitdb::record::{Record, RecordFlags};
use bitdb::recovery::rebuild_keydir;
use bitdb::storage::file_set::FileSet;
use tempfile::tempdir;

#[test]
fn hint_file_is_written_and_loaded_for_rebuild() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();

    {
        let mut file_set = FileSet::open(dir.path(), &options).expect("open should succeed");
        file_set
            .append(&Record::new(
                1,
                b"h1".to_vec(),
                b"v1".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
        file_set
            .append(&Record::new(
                2,
                b"h2".to_vec(),
                b"v2".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
        file_set.write_hint_files().expect("hints should write");
    }

    let reopened = FileSet::open(dir.path(), &options).expect("open should succeed");
    let keydir = rebuild_keydir(&reopened, options.corruption_policy).expect("rebuild should work");

    assert_eq!(keydir.len(), 2);
    assert!(dir.path().join("00000001.hint").exists());
}

#[test]
fn rebuild_falls_back_to_data_scan_when_hint_missing() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();

    {
        let mut file_set = FileSet::open(dir.path(), &options).expect("open should succeed");
        file_set
            .append(&Record::new(
                1,
                b"k".to_vec(),
                b"v".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
    }

    let reopened = FileSet::open(dir.path(), &options).expect("open should succeed");
    let keydir = rebuild_keydir(&reopened, options.corruption_policy).expect("rebuild should work");

    let entry = keydir.get(b"k").expect("key should exist");
    let decoded = reopened
        .read_at(entry.file_id, entry.offset)
        .expect("read should work");
    assert_eq!(decoded.record.value, b"v".to_vec());
}

#[test]
fn rebuild_falls_back_to_data_scan_when_hint_corrupt() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();

    {
        let mut file_set = FileSet::open(dir.path(), &options).expect("open should succeed");
        file_set
            .append(&Record::new(
                1,
                b"x".to_vec(),
                b"y".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
        file_set.write_hint_files().expect("hints should write");
    }

    std::fs::write(dir.path().join("00000001.hint"), b"not a valid hint")
        .expect("corrupt write should work");

    let reopened = FileSet::open(dir.path(), &options).expect("open should succeed");
    let keydir = rebuild_keydir(&reopened, options.corruption_policy).expect("rebuild should work");
    assert!(keydir.get(b"x").is_some());
}
