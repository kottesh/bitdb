use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Options;
use crate::error::Result;
use crate::index::keydir::KeyDir;
use crate::merge::run_merge;
use crate::record::{Record, RecordFlags};
use crate::recovery::rebuild_keydir;
use crate::storage::file_set::FileSet;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EngineStats {
    pub live_keys: usize,
    pub tombstones: usize,
}

#[derive(Debug)]
pub struct Engine {
    data_dir: Box<Path>,
    options: Options,
    file_set: FileSet,
    keydir: KeyDir,
}

impl Engine {
    pub fn open(data_dir: &Path, options: Options) -> Result<Self> {
        let file_set = FileSet::open(data_dir, &options)?;
        let keydir = rebuild_keydir(&file_set, options.corruption_policy, options.parallelism)?;

        Ok(Self {
            data_dir: data_dir.into(),
            options,
            file_set,
            keydir,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self.keydir.get(key) else {
            return Ok(None);
        };

        if entry.is_tombstone {
            return Ok(None);
        }

        let decoded = self.file_set.read_at(entry.file_id, entry.offset)?;
        Ok(Some(decoded.record.value))
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        let record = Record::new(
            unix_timestamp_secs(),
            key.to_vec(),
            value.to_vec(),
            RecordFlags::Normal,
        );
        let location = self.file_set.append(&record)?;
        self.keydir.insert(
            key.to_vec(),
            crate::index::keydir::KeyDirEntry {
                file_id: location.file_id,
                offset: location.offset,
                size_bytes: location.size_bytes,
                timestamp: record.timestamp,
                is_tombstone: false,
            },
        );
        Ok(())
    }

    pub fn delete(&mut self, key: &[u8]) -> Result<()> {
        let record = Record::new(
            unix_timestamp_secs(),
            key.to_vec(),
            Vec::new(),
            RecordFlags::Tombstone,
        );
        let location = self.file_set.append(&record)?;
        self.keydir.insert(
            key.to_vec(),
            crate::index::keydir::KeyDirEntry {
                file_id: location.file_id,
                offset: location.offset,
                size_bytes: location.size_bytes,
                timestamp: record.timestamp,
                is_tombstone: true,
            },
        );
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        self.file_set.sync_active()
    }

    pub fn stats(&self) -> EngineStats {
        let mut live_keys = 0usize;
        let mut tombstones = 0usize;
        for entry in self.keydir.values() {
            if entry.is_tombstone {
                tombstones += 1;
            } else {
                live_keys += 1;
            }
        }

        EngineStats {
            live_keys,
            tombstones,
        }
    }

    pub fn merge(&mut self) -> Result<()> {
        run_merge(self.data_dir(), &self.keydir, &self.file_set, &self.options)?;
        self.file_set = FileSet::open(self.data_dir(), &self.options)?;
        self.keydir = rebuild_keydir(
            &self.file_set,
            self.options.corruption_policy,
            self.options.parallelism,
        )?;
        Ok(())
    }
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
