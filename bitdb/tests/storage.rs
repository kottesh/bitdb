use bitdb::config::Options;
use bitdb::error::BitdbError;
use bitdb::record::{Record, RecordFlags};
use bitdb::storage::file_set::FileSet;
use tempfile::tempdir;

#[test]
fn append_and_read_record_by_offset() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options {
        max_data_file_size_bytes: 1024,
        ..Options::default()
    };

    let mut file_set = FileSet::open(dir.path(), &options).expect("fileset open should succeed");
    let record = Record::new(1, b"k1".to_vec(), b"v1".to_vec(), RecordFlags::Normal);

    let location = file_set.append(&record).expect("append should succeed");
    let decoded = file_set
        .read_at(location.file_id, location.offset)
        .expect("read should succeed");

    assert_eq!(decoded.record, record);
}

#[test]
fn append_multiple_records_are_readable_in_order() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options {
        max_data_file_size_bytes: 1024,
        ..Options::default()
    };
    let mut file_set = FileSet::open(dir.path(), &options).expect("fileset open should succeed");

    let r1 = Record::new(1, b"a".to_vec(), b"1".to_vec(), RecordFlags::Normal);
    let r2 = Record::new(2, b"b".to_vec(), b"2".to_vec(), RecordFlags::Normal);

    let l1 = file_set.append(&r1).expect("append should succeed");
    let l2 = file_set.append(&r2).expect("append should succeed");

    assert_eq!(l1.file_id, l2.file_id);
    assert!(l2.offset > l1.offset);

    let d1 = file_set
        .read_at(l1.file_id, l1.offset)
        .expect("read should succeed");
    let d2 = file_set
        .read_at(l2.file_id, l2.offset)
        .expect("read should succeed");

    assert_eq!(d1.record, r1);
    assert_eq!(d2.record, r2);
}

#[test]
fn rotates_data_file_when_size_limit_reached() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options {
        max_data_file_size_bytes: 64,
        ..Options::default()
    };

    let mut file_set = FileSet::open(dir.path(), &options).expect("fileset open should succeed");
    let first = Record::new(1, b"first".to_vec(), b"value".to_vec(), RecordFlags::Normal);
    let second = Record::new(
        2,
        b"second".to_vec(),
        b"value".to_vec(),
        RecordFlags::Normal,
    );

    let l1 = file_set.append(&first).expect("append should succeed");
    let l2 = file_set.append(&second).expect("append should succeed");

    assert!(l2.file_id > l1.file_id);
}

#[test]
fn reopen_existing_files_preserves_reads() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options {
        max_data_file_size_bytes: 1024,
        ..Options::default()
    };

    let location = {
        let mut file_set =
            FileSet::open(dir.path(), &options).expect("fileset open should succeed");
        let record = Record::new(1, b"persist".to_vec(), b"ok".to_vec(), RecordFlags::Normal);
        file_set.append(&record).expect("append should succeed")
    };

    let reopened = FileSet::open(dir.path(), &options).expect("fileset reopen should succeed");
    let decoded = reopened
        .read_at(location.file_id, location.offset)
        .expect("read should succeed");

    assert_eq!(decoded.record.key, b"persist".to_vec());
    assert_eq!(decoded.record.value, b"ok".to_vec());
}

#[test]
fn read_from_unknown_file_id_fails() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options::default();
    let file_set = FileSet::open(dir.path(), &options).expect("fileset open should succeed");

    let err = file_set.read_at(999, 0).expect_err("read must fail");
    assert!(matches!(err, BitdbError::DataFileNotFound(999)));
}
