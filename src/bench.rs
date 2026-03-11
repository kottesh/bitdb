use std::time::Instant;

use crate::cli::BenchMode;
use crate::config::Options;
use crate::engine::Engine;
use crate::error::Result;

pub fn bench_startup(data_dir: &std::path::Path, _mode: BenchMode) -> Result<String> {
    let start = Instant::now();
    let _engine = Engine::open(data_dir, Options::default())?;
    let ms = start.elapsed().as_millis();
    Ok(format!("startup_ms={ms}"))
}

pub fn bench_merge(data_dir: &std::path::Path, _mode: BenchMode) -> Result<String> {
    let mut engine = Engine::open(data_dir, Options::default())?;
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

pub fn bench_workload(
    data_dir: &std::path::Path,
    ops: u64,
    _mode: BenchMode,
    _threads: usize,
) -> Result<String> {
    let mut engine = Engine::open(data_dir, Options::default())?;
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
