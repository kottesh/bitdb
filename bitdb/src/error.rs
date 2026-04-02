use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BitdbError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("record truncated")]
    TruncatedRecord,

    #[error("invalid record magic: {0:#x}")]
    InvalidRecordMagic(u32),

    #[error("invalid record version: {0}")]
    InvalidRecordVersion(u8),

    #[error("invalid record flags: {0}")]
    InvalidRecordFlags(u8),

    #[error("record checksum mismatch: expected={expected:#x}, actual={actual:#x}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("data file not found: {0}")]
    DataFileNotFound(u32),

    #[error("invalid hint file")]
    InvalidHintFile,
}

pub type Result<T> = std::result::Result<T, BitdbError>;
