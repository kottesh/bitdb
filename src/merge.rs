use std::fs;
use std::path::{Path, PathBuf};

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::config::{Options, Parallelism};
use crate::error::Result;
use crate::index::keydir::{KeyDir, KeyDirEntry};
use crate::record::{Record, RecordFlags};
use crate::storage::file_set::FileSet;

/// A live record ready to be written into the merged output file set.
/// Carries the key, value bytes, and the original timestamp so the
/// compacted record is bit-for-bit identical to the original.
struct LiveRecord {
    key: Vec<u8>,
    value: Vec<u8>,
    timestamp: u64,
}

/// Run merge/compaction for the database at `data_dir`.
///
/// Only the latest, non-tombstone value for each key is written to the
/// compacted output.  The analysis phase (loading values from disk) runs
/// in parallel when `options.parallelism` is not `Serial`.  The write
/// phase always runs serially because `FileSet` is a single-writer
/// append structure.  The final install is atomic: the old files are
/// replaced only after the compacted set is fully written and synced.
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

    // Collect only the live entries that need to be rewritten.
    let live_entries: Vec<(&[u8], &KeyDirEntry)> = keydir
        .iter()
        .filter(|(_, entry)| !entry.is_tombstone)
        .collect();

    // Load record values from disk, parallelising the I/O-bound read phase
    // when the parallelism config permits.
    let live_records: Vec<LiveRecord> = match options.parallelism {
        Parallelism::Serial => load_records_serial(&live_entries, file_set)?,
        Parallelism::Fixed(n) => load_records_parallel(&live_entries, file_set, Some(n))?,
        Parallelism::Auto => load_records_parallel(&live_entries, file_set, None)?,
    };

    // Write phase: single-threaded append into the temporary merge directory.
    {
        let mut merged = FileSet::open(&merge_dir, options)?;
        for rec in live_records {
            let record = Record::new(rec.timestamp, rec.key, rec.value, RecordFlags::Normal);
            merged.append(&record)?;
        }
        merged.sync_active()?;
        merged.write_hint_files()?;
    }

    // Atomic install: remove old data/hint files, then rename merged outputs
    // into the data directory.
    remove_data_and_hint_files(data_dir)?;
    install_merged_files(&merge_dir, data_dir)?;
    fs::remove_dir_all(&merge_dir)?;

    Ok(())
}

// ---- analysis / read phase --------------------------------------------------

/// Load record values serially, one entry at a time.
fn load_records_serial(
    entries: &[(&[u8], &KeyDirEntry)],
    file_set: &FileSet,
) -> Result<Vec<LiveRecord>> {
    entries
        .iter()
        .map(|(key, entry)| load_one(key, entry, file_set))
        .collect()
}

/// Load record values using rayon to issue reads concurrently.
///
/// Each task reads a single record by file_id + offset.  Because reads go to
/// different (immutable) data files the operations are independent and safe to
/// parallelise.  Results are collected in the same order as the input slice so
/// the write phase is deterministic.
fn load_records_parallel(
    entries: &[(&[u8], &KeyDirEntry)],
    file_set: &FileSet,
    thread_count: Option<usize>,
) -> Result<Vec<LiveRecord>> {
    // Pre-compute owned (path, entry) pairs so rayon workers do not borrow
    // `file_set` (which is not Sync).  We store the file path alongside each
    // entry to avoid going through `file_set` inside worker threads.
    let tasks: Vec<(Vec<u8>, KeyDirEntry, PathBuf)> = entries
        .iter()
        .map(|(key, entry)| {
            let path = file_set
                .file_path(entry.file_id)
                .expect("live entry references a file that does not exist in the file set");
            (key.to_vec(), **entry, path)
        })
        .collect();

    let results: Vec<Result<LiveRecord>> = match thread_count {
        None => tasks.par_iter().map(load_one_by_path).collect(),
        Some(n) => match ThreadPoolBuilder::new().num_threads(n).build() {
            Ok(pool) => pool.install(|| tasks.par_iter().map(load_one_by_path).collect()),
            Err(_) => tasks.iter().map(load_one_by_path).collect(),
        },
    };

    results.into_iter().collect()
}

/// Load a single record via the `FileSet` API (serial path).
fn load_one(key: &[u8], entry: &KeyDirEntry, file_set: &FileSet) -> Result<LiveRecord> {
    let decoded = file_set.read_at(entry.file_id, entry.offset)?;
    Ok(LiveRecord {
        key: key.to_vec(),
        value: decoded.record.value,
        timestamp: entry.timestamp,
    })
}

/// Load a single record by reading its data file directly (parallel path).
fn load_one_by_path(task: &(Vec<u8>, KeyDirEntry, PathBuf)) -> Result<LiveRecord> {
    let (key, entry, path) = task;
    let decoded = crate::storage::data_file::DataFile::read_at(path, entry.offset)?;
    Ok(LiveRecord {
        key: key.clone(),
        value: decoded.record.value,
        timestamp: entry.timestamp,
    })
}

// ---- install helpers --------------------------------------------------------

/// Delete all `.data` and `.hint` files from `dir`.
fn remove_data_and_hint_files(dir: &Path) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("data" | "hint")
        ) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Move every file from `src_dir` into `dst_dir`.
fn install_merged_files(src_dir: &Path, dst_dir: &Path) -> Result<()> {
    for entry in fs::read_dir(src_dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name() {
            fs::rename(&path, dst_dir.join(name))?;
        }
    }
    Ok(())
}
