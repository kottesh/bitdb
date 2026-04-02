use bitdb::config::{Options, Parallelism};
use bitdb::record::{Record, RecordFlags};
use bitdb::recovery::rebuild_keydir;
use bitdb::storage::file_set::FileSet;
use tempfile::tempdir;

#[test]
fn rebuild_empty_directory_has_empty_keydir() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();
    let file_set = FileSet::open(dir.path(), &options).expect("open should succeed");

    let keydir = rebuild_keydir(&file_set, options.corruption_policy, Parallelism::Serial)
        .expect("rebuild should succeed");

    assert_eq!(keydir.len(), 0);
}

#[test]
fn rebuild_keeps_latest_overwrite() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();

    {
        let mut file_set = FileSet::open(dir.path(), &options).expect("open should succeed");
        file_set
            .append(&Record::new(
                1,
                b"alpha".to_vec(),
                b"first".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
        file_set
            .append(&Record::new(
                2,
                b"alpha".to_vec(),
                b"second".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
    }

    let reopened = FileSet::open(dir.path(), &options).expect("open should succeed");
    let keydir = rebuild_keydir(&reopened, options.corruption_policy, Parallelism::Serial)
        .expect("rebuild should succeed");
    let entry = keydir.get(b"alpha").expect("key should exist");

    assert!(!entry.is_tombstone);
    let decoded = reopened
        .read_at(entry.file_id, entry.offset)
        .expect("read should succeed");
    assert_eq!(decoded.record.value, b"second".to_vec());
}

#[test]
fn rebuild_tombstone_hides_previous_value() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();

    {
        let mut file_set = FileSet::open(dir.path(), &options).expect("open should succeed");
        file_set
            .append(&Record::new(
                1,
                b"dead".to_vec(),
                b"value".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
        file_set
            .append(&Record::new(
                2,
                b"dead".to_vec(),
                Vec::new(),
                RecordFlags::Tombstone,
            ))
            .expect("append should succeed");
    }

    let reopened = FileSet::open(dir.path(), &options).expect("open should succeed");
    let keydir = rebuild_keydir(&reopened, options.corruption_policy, Parallelism::Serial)
        .expect("rebuild should succeed");
    let entry = keydir.get(b"dead").expect("key should exist");

    assert!(entry.is_tombstone);
}

#[test]
fn rebuild_scans_across_multiple_files() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options {
        max_data_file_size_bytes: 64,
        ..Options::default()
    };

    {
        let mut file_set = FileSet::open(dir.path(), &options).expect("open should succeed");
        file_set
            .append(&Record::new(
                1,
                b"k1".to_vec(),
                b"v1".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
        file_set
            .append(&Record::new(
                2,
                b"k2".to_vec(),
                b"v2".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
        file_set
            .append(&Record::new(
                3,
                b"k1".to_vec(),
                b"v3".to_vec(),
                RecordFlags::Normal,
            ))
            .expect("append should succeed");
    }

    let reopened = FileSet::open(dir.path(), &options).expect("open should succeed");
    let keydir = rebuild_keydir(&reopened, options.corruption_policy, Parallelism::Serial)
        .expect("rebuild should succeed");

    assert_eq!(keydir.len(), 2);
    let k1 = keydir.get(b"k1").expect("k1 should exist");
    let r1 = reopened
        .read_at(k1.file_id, k1.offset)
        .expect("read should work");
    assert_eq!(r1.record.value, b"v3".to_vec());
}
