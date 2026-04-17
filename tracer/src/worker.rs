use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rayon::prelude::*;

use bitdb::record::decode_one;
use bitdb::storage::data_file::{data_file_path, parse_data_file_id};

/// Progress state for a single file slot.
#[derive(Clone, Debug)]
pub enum SlotState {
    /// Assigned but not yet started.
    Queued,
    /// Currently being scanned; carries live counters.
    Processing { bytes_done: u64, keys_found: usize },
    /// Finished; carries final measurements.
    Done {
        duration_us: u64,
        keys_found: usize,
        bytes_read: u64,
    },
}

/// One file assigned to a thread.
#[derive(Clone, Debug)]
pub struct FileSlot {
    pub file_id: u32,
    /// Size on disk in bytes; used to compute bar fill percentage.
    pub file_size_bytes: u64,
    pub state: SlotState,
}

/// All slots assigned to one thread.
#[derive(Clone, Debug)]
pub struct ThreadState {
    pub thread_id: usize,
    pub slots: Vec<FileSlot>,
}

/// Result of one full scan run (serial or parallel).
#[derive(Clone, Debug)]
pub struct RunResult {
    pub thread_states: Vec<ThreadState>,
    pub total_keys: usize,
    pub wall_time_us: u64,
    pub keys_per_sec: f64,
}

/// A handle to the live progress of an in-flight `run_scan` call.
/// The TUI can clone the inner `Vec<ThreadState>` each frame to get a
/// consistent snapshot without blocking the workers for long.
pub type LiveProgress = Arc<Mutex<Vec<ThreadState>>>;

/// Distribute `file_ids` across `thread_count` threads round-robin.
///
/// Thread `i` receives file ids at positions `i, i+N, i+2N, ...`
/// where `N = thread_count`.  All slot states start as `Queued`.
pub fn assign_files(file_ids: &[u32], thread_count: usize) -> Vec<ThreadState> {
    let count = thread_count.max(1);
    let mut states: Vec<ThreadState> = (0..count)
        .map(|i| ThreadState {
            thread_id: i,
            slots: Vec::new(),
        })
        .collect();

    for (pos, &file_id) in file_ids.iter().enumerate() {
        let thread_idx = pos % count;
        // File size will be filled in by run_scan when paths are known.
        states[thread_idx].slots.push(FileSlot {
            file_id,
            file_size_bytes: 0,
            state: SlotState::Queued,
        });
    }

    states
}

/// Run a full instrumented scan of all data files in `data_dir` using
/// `thread_count` worker threads.
///
/// Hint files are deleted before scanning so the full record-decode path
/// is always exercised.  Returns a `RunResult` with per-thread slot states
/// and aggregate timing.
///
/// The caller can pass a `LiveProgress` arc that will be populated with
/// the initial file assignment before any scanning begins, and updated
/// in real time by the workers throughout the scan.  Pass `None` when
/// live progress is not needed.
pub fn run_scan(
    data_dir: &Path,
    thread_count: usize,
    live: Option<&LiveProgress>,
) -> std::io::Result<RunResult> {
    // Collect all data file ids in ascending order.
    let mut file_ids: Vec<u32> = std::fs::read_dir(data_dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| parse_data_file_id(&e.path()))
        .collect();
    file_ids.sort_unstable();

    // Remove hint files so the full decode path is benchmarked.
    for entry in std::fs::read_dir(data_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("hint") {
            let _ = std::fs::remove_file(path);
        }
    }

    // Build file paths and sizes.
    let file_info: Vec<(u32, PathBuf, u64)> = file_ids
        .iter()
        .map(|&id| {
            let path = data_file_path(data_dir, id);
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            (id, path, size)
        })
        .collect();

    // Assign files to threads.
    let mut assignment = assign_files(&file_ids, thread_count);

    // Fill in file sizes now that we have them.
    for thread in &mut assignment {
        for slot in &mut thread.slots {
            if let Some((_, _, size)) = file_info.iter().find(|(id, _, _)| *id == slot.file_id) {
                slot.file_size_bytes = *size;
            }
        }
    }

    // Shared state written by workers, read by the TUI render loop.
    // If the caller supplied a LiveProgress arc, reuse it so the TUI can
    // poll it without any extra channel; otherwise create a local one.
    let shared: Arc<Mutex<Vec<ThreadState>>> = if let Some(lp) = live {
        *lp.lock().unwrap() = assignment;
        lp.clone()
    } else {
        Arc::new(Mutex::new(assignment))
    };

    let wall_start = Instant::now();

    // Build per-thread task lists (owned paths + sizes) before launching rayon.
    let tasks: Vec<Vec<(u32, PathBuf, u64)>> = {
        let states = shared.lock().unwrap();
        states
            .iter()
            .map(|ts| {
                ts.slots
                    .iter()
                    .map(|slot| {
                        let path = data_file_path(data_dir, slot.file_id);
                        (slot.file_id, path, slot.file_size_bytes)
                    })
                    .collect()
            })
            .collect()
    };

    // Scan all threads in parallel.
    tasks
        .par_iter()
        .enumerate()
        .for_each(|(thread_idx, file_list)| {
            for (slot_idx, (file_id, path, _file_size)) in file_list.iter().enumerate() {
                let file_start = Instant::now();

                // Mark slot as Processing.
                {
                    let mut states = shared.lock().unwrap();
                    states[thread_idx].slots[slot_idx].state = SlotState::Processing {
                        bytes_done: 0,
                        keys_found: 0,
                    };
                }

                // Read the entire file then decode record by record.
                let bytes = match std::fs::read(path) {
                    Ok(b) => b,
                    Err(_) => {
                        let mut states = shared.lock().unwrap();
                        states[thread_idx].slots[slot_idx].state = SlotState::Done {
                            duration_us: file_start.elapsed().as_micros() as u64,
                            keys_found: 0,
                            bytes_read: 0,
                        };
                        continue;
                    }
                };

                let total_bytes = bytes.len() as u64;
                let mut offset = 0usize;
                let mut keys_found = 0usize;
                // Report progress every 500 records so the TUI sees
                // movement even on small / fast files.
                let mut since_last_report = 0usize;

                while offset < bytes.len() {
                    match decode_one(&bytes[offset..]) {
                        Ok(decoded) => {
                            offset += decoded.bytes_read;
                            keys_found += 1;
                            since_last_report += 1;

                            if since_last_report >= 500 {
                                since_last_report = 0;
                                let mut states = shared.lock().unwrap();
                                states[thread_idx].slots[slot_idx].state = SlotState::Processing {
                                    bytes_done: offset as u64,
                                    keys_found,
                                };
                            }
                        }
                        // Truncated tail or corruption: stop scanning this file.
                        Err(_) => break,
                    }
                }

                let duration_us = file_start.elapsed().as_micros() as u64;
                {
                    let mut states = shared.lock().unwrap();
                    states[thread_idx].slots[slot_idx].state = SlotState::Done {
                        duration_us,
                        keys_found,
                        bytes_read: total_bytes,
                    };
                }

                let _ = file_id;
            }
        });

    let wall_time_us = wall_start.elapsed().as_micros() as u64;

    let final_states = Arc::try_unwrap(shared)
        .unwrap_or_else(|arc| (*arc.lock().unwrap()).clone().into())
        .into_inner()
        .unwrap();

    let total_keys: usize = final_states
        .iter()
        .flat_map(|ts| ts.slots.iter())
        .map(|slot| match slot.state {
            SlotState::Done { keys_found, .. } => keys_found,
            _ => 0,
        })
        .sum();

    let keys_per_sec = if wall_time_us == 0 {
        total_keys as f64
    } else {
        total_keys as f64 / (wall_time_us as f64 / 1_000_000.0)
    };

    Ok(RunResult {
        thread_states: final_states,
        total_keys,
        wall_time_us,
        keys_per_sec,
    })
}
