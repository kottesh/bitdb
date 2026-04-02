use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::error::{BitdbError, Result};
use crate::index::keydir::KeyDirEntry;

const HINT_MAGIC: u32 = 0x4849_4e54;
const HINT_VERSION: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HintEntry {
    pub key: Vec<u8>,
    pub file_id: u32,
    pub offset: u64,
    pub size_bytes: u32,
    pub timestamp: u64,
    pub is_tombstone: bool,
}

impl HintEntry {
    pub fn to_keydir_entry(&self) -> KeyDirEntry {
        KeyDirEntry {
            file_id: self.file_id,
            offset: self.offset,
            size_bytes: self.size_bytes,
            timestamp: self.timestamp,
            is_tombstone: self.is_tombstone,
        }
    }
}

pub fn write_hint_file(path: &Path, entries: &[HintEntry]) -> Result<()> {
    let mut out = Vec::new();
    out.extend_from_slice(&HINT_MAGIC.to_le_bytes());
    out.push(HINT_VERSION);
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());

    for entry in entries {
        out.extend_from_slice(&(entry.key.len() as u32).to_le_bytes());
        out.extend_from_slice(&entry.file_id.to_le_bytes());
        out.extend_from_slice(&entry.offset.to_le_bytes());
        out.extend_from_slice(&entry.size_bytes.to_le_bytes());
        out.extend_from_slice(&entry.timestamp.to_le_bytes());
        out.push(if entry.is_tombstone { 1 } else { 0 });
        out.extend_from_slice(&entry.key);
    }

    let mut file = File::create(path)?;
    file.write_all(&out)?;
    file.sync_data()?;
    Ok(())
}

pub fn read_hint_file(path: &Path) -> Result<Vec<HintEntry>> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    if buf.len() < 9 {
        return Err(BitdbError::InvalidHintFile);
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != HINT_MAGIC {
        return Err(BitdbError::InvalidHintFile);
    }
    if buf[4] != HINT_VERSION {
        return Err(BitdbError::InvalidHintFile);
    }

    let count = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]) as usize;
    let mut offset = 9usize;
    let mut entries = Vec::with_capacity(count);

    for _ in 0..count {
        if offset + 29 > buf.len() {
            return Err(BitdbError::InvalidHintFile);
        }
        let key_len = u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]) as usize;
        offset += 4;
        let file_id = u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]);
        offset += 4;
        let file_offset = u64::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
            buf[offset + 4],
            buf[offset + 5],
            buf[offset + 6],
            buf[offset + 7],
        ]);
        offset += 8;
        let size_bytes = u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]);
        offset += 4;
        let timestamp = u64::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
            buf[offset + 4],
            buf[offset + 5],
            buf[offset + 6],
            buf[offset + 7],
        ]);
        offset += 8;
        let is_tombstone = match buf[offset] {
            0 => false,
            1 => true,
            _ => return Err(BitdbError::InvalidHintFile),
        };
        offset += 1;
        if offset + key_len > buf.len() {
            return Err(BitdbError::InvalidHintFile);
        }
        let key = buf[offset..offset + key_len].to_vec();
        offset += key_len;

        entries.push(HintEntry {
            key,
            file_id,
            offset: file_offset,
            size_bytes,
            timestamp,
            is_tombstone,
        });
    }

    Ok(entries)
}
