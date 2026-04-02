use crc32fast::Hasher;

use crate::error::{BitdbError, Result};

const RECORD_MAGIC: u32 = 0x4244_4231;
const RECORD_VERSION: u8 = 1;
const HEADER_LEN: usize = 26;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RecordFlags {
    Normal,
    Tombstone,
}

impl RecordFlags {
    fn to_u8(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Tombstone => 1,
        }
    }

    fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Tombstone),
            _ => Err(BitdbError::InvalidRecordFlags(value)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub timestamp: u64,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub flags: RecordFlags,
}

impl Record {
    pub fn new(timestamp: u64, key: Vec<u8>, value: Vec<u8>, flags: RecordFlags) -> Self {
        Self {
            timestamp,
            key,
            value,
            flags,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodeResult {
    pub record: Record,
    pub bytes_read: usize,
}

pub fn encode(record: &Record) -> Vec<u8> {
    let key_len = record.key.len() as u32;
    let value_len = record.value.len() as u32;

    let mut out = Vec::with_capacity(HEADER_LEN + record.key.len() + record.value.len());
    out.extend_from_slice(&RECORD_MAGIC.to_le_bytes());
    out.push(RECORD_VERSION);
    out.push(record.flags.to_u8());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&record.timestamp.to_le_bytes());
    out.extend_from_slice(&key_len.to_le_bytes());
    out.extend_from_slice(&value_len.to_le_bytes());
    out.extend_from_slice(&record.key);
    out.extend_from_slice(&record.value);

    let crc = compute_crc(&out);
    out[6..10].copy_from_slice(&crc.to_le_bytes());

    out
}

pub fn decode_one(input: &[u8]) -> Result<DecodeResult> {
    if input.len() < HEADER_LEN {
        return Err(BitdbError::TruncatedRecord);
    }

    let magic = u32::from_le_bytes([input[0], input[1], input[2], input[3]]);
    if magic != RECORD_MAGIC {
        return Err(BitdbError::InvalidRecordMagic(magic));
    }

    let version = input[4];
    if version != RECORD_VERSION {
        return Err(BitdbError::InvalidRecordVersion(version));
    }

    let flags = RecordFlags::from_u8(input[5])?;
    let expected_crc = u32::from_le_bytes([input[6], input[7], input[8], input[9]]);
    let timestamp = u64::from_le_bytes([
        input[10], input[11], input[12], input[13], input[14], input[15], input[16], input[17],
    ]);
    let key_len = u32::from_le_bytes([input[18], input[19], input[20], input[21]]) as usize;
    let value_len = u32::from_le_bytes([input[22], input[23], input[24], input[25]]) as usize;
    let total_len = HEADER_LEN
        .checked_add(key_len)
        .and_then(|v| v.checked_add(value_len))
        .ok_or(BitdbError::TruncatedRecord)?;

    if input.len() < total_len {
        return Err(BitdbError::TruncatedRecord);
    }

    let actual_crc = compute_crc(&input[..total_len]);
    if expected_crc != actual_crc {
        return Err(BitdbError::ChecksumMismatch {
            expected: expected_crc,
            actual: actual_crc,
        });
    }

    let key_start = HEADER_LEN;
    let value_start = key_start + key_len;
    let record = Record {
        timestamp,
        key: input[key_start..value_start].to_vec(),
        value: input[value_start..total_len].to_vec(),
        flags,
    };

    Ok(DecodeResult {
        record,
        bytes_read: total_len,
    })
}

fn compute_crc(input: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(&input[..6]);
    hasher.update(&input[10..]);
    hasher.finalize()
}
