use std::time::Instant;

use crate::cli::BenchMode;
use crate::config::{Options, Parallelism};
use crate::engine::Engine;
use crate::error::Result;

/// Convert a CLI `BenchMode` into the corresponding `Parallelism` config value.
fn parallelism_for(mode: BenchMode) -> Parallelism {
    match mode {
        BenchMode::Serial => Parallelism::Serial,
        BenchMode::Parallel => Parallelism::Auto,
    }
}

/// Measure how long it takes to open (rebuild) the database from disk.
///
/// The data directory is expected to contain previously written data files.
/// An empty directory is valid and will produce a near-zero startup time.
pub fn bench_startup(data_dir: &std::path::Path, mode: BenchMode) -> Result<String> {
    let opts = Options {
        parallelism: parallelism_for(mode),
        ..Options::default()
    };
    let start = Instant::now();
    let _engine = Engine::open(data_dir, opts)?;
    let ms = start.elapsed().as_millis();
    Ok(format!("startup_ms={ms}"))
}

/// Write a small burst of records and measure how long merge/compaction takes.
///
/// The write burst uses key churn (20 hot keys, 200 writes) so the input
/// has significant dead data to compact away.
pub fn bench_merge(data_dir: &std::path::Path, mode: BenchMode) -> Result<String> {
    let opts = Options {
        parallelism: parallelism_for(mode),
        ..Options::default()
    };
    let mut engine = Engine::open(data_dir, opts)?;
    for i in 0..200 {
        let key = format!("mkey-{}", i % 20);
        let value = format!("mval-{i}");
        engine.put(key.as_bytes(), value.as_bytes())?;
    }
    let start = Instant::now();
    engine.merge()?;
    let ms = start.elapsed().as_millis();
    Ok(format!("merge_ms={ms}"))
}

/// Run a mixed put+get workload and report throughput in ops/sec.
///
/// Each "op" is one put followed by one get for the same key, so the total
/// number of I/O operations is `2 * ops`.
pub fn bench_workload(
    data_dir: &std::path::Path,
    ops: u64,
    mode: BenchMode,
    _threads: usize,
) -> Result<String> {
    let opts = Options {
        parallelism: parallelism_for(mode),
        ..Options::default()
    };
    let mut engine = Engine::open(data_dir, opts)?;
    let start = Instant::now();
    for i in 0..ops {
        let key = format!("wkey-{i}");
        engine.put(key.as_bytes(), b"value")?;
        let _ = engine.get(key.as_bytes())?;
    }
    let secs = start.elapsed().as_secs_f64();
    let ops_per_sec = if secs == 0.0 {
        ops as f64
    } else {
        ops as f64 / secs
    };
    Ok(format!("ops_per_sec={ops_per_sec:.2}"))
}
