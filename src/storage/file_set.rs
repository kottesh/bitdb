use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Options;
use crate::error::{BitdbError, Result};
use crate::record::{DecodeResult, Record};
use crate::storage::data_file::{DataFile, data_file_path, parse_data_file_id};
use crate::storage::hint_file::{HintEntry, write_hint_file};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RecordLocation {
    pub file_id: u32,
    pub offset: u64,
    pub size_bytes: u32,
}

#[derive(Debug)]
pub struct FileSet {
    dir: PathBuf,
    options: Options,
    known_file_ids: BTreeSet<u32>,
    active: DataFile,
}

impl FileSet {
    pub fn open(dir: &Path, options: &Options) -> Result<Self> {
        fs::create_dir_all(dir)?;

        let mut known_file_ids = BTreeSet::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if let Some(file_id) = parse_data_file_id(&entry.path()) {
                known_file_ids.insert(file_id);
            }
        }

        let active_id = known_file_ids.last().copied().unwrap_or(1);
        known_file_ids.insert(active_id);
        let active = DataFile::open_append(dir, active_id)?;

        Ok(Self {
            dir: dir.to_path_buf(),
            options: options.clone(),
            known_file_ids,
            active,
        })
    }

    pub fn append(&mut self, record: &Record) -> Result<RecordLocation> {
        let encoded_len = crate::record::encode(record).len() as u64;

        if !self.active.is_empty()
            && self.active.len().saturating_add(encoded_len) > self.options.max_data_file_size_bytes
        {
            let next_id = self.active.id() + 1;
            self.active = DataFile::open_append(&self.dir, next_id)?;
            self.known_file_ids.insert(next_id);
        }

        let file_id = self.active.id();
        let (offset, size_bytes) = self.active.append(record)?;

        Ok(RecordLocation {
            file_id,
            offset,
            size_bytes: size_bytes as u32,
        })
    }

    pub fn read_at(&self, file_id: u32, offset: u64) -> Result<DecodeResult> {
        if !self.known_file_ids.contains(&file_id) {
            return Err(BitdbError::DataFileNotFound(file_id));
        }

        let path = data_file_path(&self.dir, file_id);
        if !path.exists() {
            return Err(BitdbError::DataFileNotFound(file_id));
        }

        DataFile::read_at(&path, offset)
    }

    pub fn sync_active(&self) -> Result<()> {
        self.active.sync()
    }

    pub fn file_ids_oldest_to_newest(&self) -> Vec<u32> {
        self.known_file_ids.iter().copied().collect()
    }

    pub fn file_path(&self, file_id: u32) -> Option<PathBuf> {
        if self.known_file_ids.contains(&file_id) {
            Some(data_file_path(&self.dir, file_id))
        } else {
            None
        }
    }

    pub fn hint_path(&self, file_id: u32) -> Option<PathBuf> {
        if self.known_file_ids.contains(&file_id) {
            Some(self.dir.join(format!("{file_id:08}.hint")))
        } else {
            None
        }
    }

    pub fn write_hint_files(&self) -> Result<()> {
        for file_id in self.file_ids_oldest_to_newest() {
            self.write_hint_file_for(file_id)?;
        }
        Ok(())
    }

    fn write_hint_file_for(&self, file_id: u32) -> Result<()> {
        let Some(path) = self.file_path(file_id) else {
            return Err(BitdbError::DataFileNotFound(file_id));
        };
        let bytes = fs::read(path)?;
        let mut offset = 0usize;
        let mut entries = Vec::new();

        while offset < bytes.len() {
            match crate::record::decode_one(&bytes[offset..]) {
                Ok(decoded) => {
                    entries.push(HintEntry {
                        key: decoded.record.key,
                        file_id,
                        offset: offset as u64,
                        size_bytes: decoded.bytes_read as u32,
                        timestamp: decoded.record.timestamp,
                        is_tombstone: decoded.record.flags == crate::record::RecordFlags::Tombstone,
                    });
                    offset += decoded.bytes_read;
                }
                Err(crate::error::BitdbError::TruncatedRecord) => break,
                Err(err) => return Err(err),
            }
        }

        let hint_path = self
            .hint_path(file_id)
            .ok_or(BitdbError::DataFileNotFound(file_id))?;
        write_hint_file(&hint_path, &entries)?;
        Ok(())
    }
}
