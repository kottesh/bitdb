use std::path::{Path, PathBuf};

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::config::{CorruptionPolicy, Parallelism};
use crate::error::{BitdbError, Result};
use crate::index::keydir::{KeyDir, KeyDirEntry};
use crate::record::RecordFlags;
use crate::storage::file_set::FileSet;
use crate::storage::hint_file::read_hint_file;

/// One decoded entry coming out of a file scan.
/// Fields are `(file_id, byte_offset, keydir_entry, key_bytes)`.
type ScanEntry = (u32, u64, KeyDirEntry, Vec<u8>);

/// Rebuild the in-memory KeyDir by scanning all known data files.
///
/// The scan order determines which value wins for a given key: a record in a
/// later file (higher file_id) or at a later offset within the same file
/// always beats an earlier one.  Tombstones mark deleted keys.
///
/// When `parallelism` is `Serial` the scan is single-threaded.
/// Any other variant uses rayon to scan files concurrently and then merges
/// the results in deterministic (oldest-to-newest) order.
pub fn rebuild_keydir(
    file_set: &FileSet,
    corruption_policy: CorruptionPolicy,
    parallelism: Parallelism,
) -> Result<KeyDir> {
    match parallelism {
        Parallelism::Serial => rebuild_serial(file_set, corruption_policy),
        Parallelism::Fixed(n) => rebuild_parallel(file_set, corruption_policy, Some(n)),
        Parallelism::Auto => rebuild_parallel(file_set, corruption_policy, None),
    }
}

// ---- serial path ------------------------------------------------------------

/// Single-threaded rebuild.  Processes files oldest-to-newest so that the
/// last insert for any key is the winner with no extra bookkeeping.
fn rebuild_serial(file_set: &FileSet, policy: CorruptionPolicy) -> Result<KeyDir> {
    let mut keydir = KeyDir::default();

    for file_id in file_set.file_ids_oldest_to_newest() {
        let entries = scan_file(file_set, file_id, policy)?;
        for (_, _, entry, key) in entries {
            keydir.insert(key, entry);
        }
    }

    Ok(keydir)
}

// ---- parallel path ----------------------------------------------------------

/// Multi-threaded rebuild.
///
/// Each data file is scanned independently on a rayon worker thread.  The
/// per-file results are then merged in a single-threaded pass that processes
/// files strictly oldest-to-newest and offsets strictly lowest-to-highest,
/// guaranteeing the same outcome as the serial path.
fn rebuild_parallel(
    file_set: &FileSet,
    policy: CorruptionPolicy,
    thread_count: Option<usize>,
) -> Result<KeyDir> {
    let file_ids: Vec<u32> = file_set.file_ids_oldest_to_newest();

    // Collect owned paths up front so we can move them into rayon workers
    // without borrowing `file_set` across thread boundaries.
    let file_info: Vec<(u32, PathBuf, Option<PathBuf>)> = file_ids
        .iter()
        .map(|&id| {
            let data_path = file_set
                .file_path(id)
                .expect("file_ids_oldest_to_newest returned id without a path");
            let hint_path = file_set.hint_path(id);
            (id, data_path, hint_path)
        })
        .collect();

    // Scan every file in parallel.  Each task returns either a list of
    // `ScanEntry` tuples or a fatal error.
    let scan_results: Vec<Result<Vec<ScanEntry>>> = {
        let run_scan = |info: &(u32, PathBuf, Option<PathBuf>)| {
            let (file_id, data_path, hint_path) = info;
            scan_file_by_path(*file_id, data_path, hint_path.as_deref(), policy)
        };

        match thread_count {
            None => file_info.par_iter().map(run_scan).collect(),
            Some(n) => {
                // Build a scoped thread pool with a fixed size.  If
                // construction fails (e.g. n == 0) fall back to a serial
                // scan so data correctness is preserved.
                match ThreadPoolBuilder::new().num_threads(n).build() {
                    Ok(pool) => pool.install(|| file_info.par_iter().map(run_scan).collect()),
                    Err(_) => file_info.iter().map(run_scan).collect(),
                }
            }
        }
    };

    // Gather per-file entry lists, propagating the first fatal error.
    let mut all_entries: Vec<ScanEntry> = Vec::new();
    for result in scan_results {
        all_entries.extend(result?);
    }

    // Sort by (file_id ASC, offset ASC) so a linear pass applies entries in
    // the same order as the serial path: last writer wins.
    all_entries.sort_unstable_by_key(|(file_id, offset, _, _)| (*file_id, *offset));

    let mut keydir = KeyDir::default();
    for (_, _, entry, key) in all_entries {
        keydir.insert(key, entry);
    }

    Ok(keydir)
}

// ---- shared file scanning ---------------------------------------------------

/// Scan a single file via the `FileSet` API (used by the serial path).
///
/// Returns entries in file order (offset ascending).  Errors that are not
/// recoverable under `policy` are propagated immediately.
fn scan_file(
    file_set: &FileSet,
    file_id: u32,
    policy: CorruptionPolicy,
) -> Result<Vec<ScanEntry>> {
    let hint_path = file_set
        .hint_path(file_id)
        .ok_or(BitdbError::DataFileNotFound(file_id))?;

    if hint_path.exists()
        && let Ok(hint_entries) = read_hint_file(&hint_path)
    {
        return Ok(hint_entries
            .into_iter()
            .map(|h| {
                let key = h.key.clone();
                let entry = h.to_keydir_entry();
                (file_id, entry.offset, entry, key)
            })
            .collect());
    }

    let path = file_set
        .file_path(file_id)
        .ok_or(BitdbError::DataFileNotFound(file_id))?;
    scan_data_file(file_id, &path, policy)
}

/// Scan a single file given explicit paths (used by the parallel path where
/// we cannot pass `FileSet` across thread boundaries without `Arc`).
///
/// Prefers hint file when available; falls back to full data file scan.
fn scan_file_by_path(
    file_id: u32,
    data_path: &PathBuf,
    hint_path: Option<&Path>,
    policy: CorruptionPolicy,
) -> Result<Vec<ScanEntry>> {
    if let Some(hp) = hint_path
        && hp.exists()
        && let Ok(hint_entries) = read_hint_file(hp)
    {
        return Ok(hint_entries
            .into_iter()
            .map(|h| {
                let key = h.key.clone();
                let entry = h.to_keydir_entry();
                (file_id, entry.offset, entry, key)
            })
            .collect());
    }

    scan_data_file(file_id, data_path, policy)
}

/// Read a data file from disk and decode every record in it.
///
/// Stops on `TruncatedRecord` (safe truncated tail).  Corruption errors are
/// either propagated or cause an early stop depending on `policy`.
fn scan_data_file(file_id: u32, path: &PathBuf, policy: CorruptionPolicy) -> Result<Vec<ScanEntry>> {
    let bytes = std::fs::read(path)?;
    let mut offset = 0usize;
    let mut entries = Vec::new();

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
                entries.push((file_id, offset as u64, entry, decoded.record.key));
                offset += decoded.bytes_read;
            }
            Err(BitdbError::TruncatedRecord) => break,
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

    Ok(entries)
}
