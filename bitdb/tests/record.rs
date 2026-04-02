use bitdb::error::BitdbError;
use bitdb::record::{self, Record, RecordFlags};

#[test]
fn record_roundtrip_normal() {
    let record = Record::new(
        42,
        b"key-normal".to_vec(),
        b"value-normal".to_vec(),
        RecordFlags::Normal,
    );

    let encoded = record::encode(&record);
    let decoded = record::decode_one(&encoded).expect("record should decode");

    assert_eq!(decoded.record, record);
    assert_eq!(decoded.bytes_read, encoded.len());
}

#[test]
fn record_roundtrip_tombstone() {
    let record = Record::new(
        99,
        b"key-delete".to_vec(),
        Vec::new(),
        RecordFlags::Tombstone,
    );

    let encoded = record::encode(&record);
    let decoded = record::decode_one(&encoded).expect("record should decode");

    assert_eq!(decoded.record.flags, RecordFlags::Tombstone);
    assert_eq!(decoded.record.value, Vec::<u8>::new());
}

#[test]
fn decode_rejects_crc_mismatch() {
    let record = Record::new(1, b"key".to_vec(), b"value".to_vec(), RecordFlags::Normal);
    let mut encoded = record::encode(&record);

    let last = encoded.len() - 1;
    encoded[last] ^= 0xFF;

    let err = record::decode_one(&encoded).expect_err("decode must fail");
    assert!(matches!(err, BitdbError::ChecksumMismatch { .. }));
}

#[test]
fn decode_rejects_invalid_magic() {
    let record = Record::new(1, b"k".to_vec(), b"v".to_vec(), RecordFlags::Normal);
    let mut encoded = record::encode(&record);

    encoded[0] = 0;
    encoded[1] = 0;
    encoded[2] = 0;
    encoded[3] = 0;

    let err = record::decode_one(&encoded).expect_err("decode must fail");
    assert!(matches!(err, BitdbError::InvalidRecordMagic(_)));
}

#[test]
fn decode_rejects_invalid_version() {
    let record = Record::new(1, b"k".to_vec(), b"v".to_vec(), RecordFlags::Normal);
    let mut encoded = record::encode(&record);

    encoded[4] = 2;

    let err = record::decode_one(&encoded).expect_err("decode must fail");
    assert!(matches!(err, BitdbError::InvalidRecordVersion(2)));
}

#[test]
fn decode_rejects_truncated_record() {
    let record = Record::new(7, b"abc".to_vec(), b"xyz".to_vec(), RecordFlags::Normal);
    let mut encoded = record::encode(&record);
    encoded.truncate(encoded.len() - 2);

    let err = record::decode_one(&encoded).expect_err("decode must fail");
    assert!(matches!(err, BitdbError::TruncatedRecord));
}
