use crate::config::CorruptionPolicy;
use crate::error::{BitdbError, Result};
use crate::index::keydir::{KeyDir, KeyDirEntry};
use crate::record::RecordFlags;
use crate::storage::file_set::FileSet;
use crate::storage::hint_file::read_hint_file;

pub fn rebuild_keydir(file_set: &FileSet, policy: CorruptionPolicy) -> Result<KeyDir> {
    let mut keydir = KeyDir::default();

    for file_id in file_set.file_ids_oldest_to_newest() {
        let Some(hint_path) = file_set.hint_path(file_id) else {
            return Err(BitdbError::DataFileNotFound(file_id));
        };

        if hint_path.exists()
            && let Ok(entries) = read_hint_file(&hint_path)
        {
            for entry in entries {
                let key = entry.key.clone();
                keydir.insert(key, entry.to_keydir_entry());
            }
            continue;
        }

        let path = file_set
            .file_path(file_id)
            .ok_or(BitdbError::DataFileNotFound(file_id))?;
        let bytes = std::fs::read(path)?;

        let mut offset = 0usize;
        while offset < bytes.len() {
            match crate::record::decode_one(&bytes[offset..]) {
                Ok(decoded) => {
                    let entry = KeyDirEntry {
                        file_id,
                        offset: offset as u64,
                        size_bytes: decoded.bytes_read as u32,
                        timestamp: decoded.record.timestamp,
                        is_tombstone: decoded.record.flags == RecordFlags::Tombstone,
                    };
                    keydir.insert(decoded.record.key, entry);
                    offset += decoded.bytes_read;
                }
                Err(BitdbError::TruncatedRecord) => {
                    break;
                }
                Err(
                    err @ BitdbError::ChecksumMismatch { .. }
                    | err @ BitdbError::InvalidRecordMagic(_)
                    | err @ BitdbError::InvalidRecordVersion(_)
                    | err @ BitdbError::InvalidRecordFlags(_),
                ) => match policy {
                    CorruptionPolicy::Fail => return Err(err),
                    CorruptionPolicy::SkipCorruptedTail => break,
                },
                Err(err) => return Err(err),
            }
        }
    }

    Ok(keydir)
}
