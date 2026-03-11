use std::fs;
use std::path::Path;

use crate::config::Options;
use crate::error::Result;
use crate::index::keydir::KeyDir;
use crate::record::{Record, RecordFlags};
use crate::storage::file_set::FileSet;

pub fn run_merge(
    data_dir: &Path,
    keydir: &KeyDir,
    file_set: &FileSet,
    options: &Options,
) -> Result<()> {
    let merge_dir = data_dir.join(".merge_tmp");
    if merge_dir.exists() {
        fs::remove_dir_all(&merge_dir)?;
    }
    fs::create_dir_all(&merge_dir)?;

    {
        let mut merged = FileSet::open(&merge_dir, options)?;
        for (key, entry) in keydir.iter() {
            if entry.is_tombstone {
                continue;
            }
            let decoded = file_set.read_at(entry.file_id, entry.offset)?;
            let merged_record = Record::new(
                entry.timestamp,
                key.to_vec(),
                decoded.record.value,
                RecordFlags::Normal,
            );
            merged.append(&merged_record)?;
        }
        merged.sync_active()?;
        merged.write_hint_files()?;
    }

    for entry in fs::read_dir(data_dir)? {
        let entry = entry?;
        let path = entry.path();
        if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("data" | "hint")
        ) {
            fs::remove_file(path)?;
        }
    }

    for entry in fs::read_dir(&merge_dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name() {
            fs::rename(&path, data_dir.join(name))?;
        }
    }
    fs::remove_dir_all(&merge_dir)?;

    Ok(())
}
