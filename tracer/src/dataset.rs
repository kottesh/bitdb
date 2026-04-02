use std::path::Path;
use std::sync::{Arc, Mutex};

use rand::RngCore;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

use bitdb::config::Options;
use bitdb::engine::Engine;

/// Fixed RNG seed so every run on every machine produces the same dataset.
const SEED: u64 = 42;

/// Parameters that fully describe a generated dataset.
/// Stored in `tracer_meta.json` alongside the data files.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatasetParams {
    pub keys: usize,
    pub value_size: usize,
    pub file_size_bytes: u64,
}

/// Live progress state updated by `generate` and polled by the TUI.
#[derive(Clone, Debug, Default)]
pub struct GenerateProgress {
    pub keys_written: usize,
    pub total_keys: usize,
    pub files_created: usize,
    pub elapsed_ms: u64,
}

/// Generate a dataset at `data_dir` using `params`.
///
/// Keys are formatted as `key:{i:08}`.  Values are random bytes of
/// `params.value_size` produced by a seeded RNG so the output is
/// reproducible.  Progress is written to `progress` after every key so
/// the TUI can display a live fill bar.
///
/// On completion writes `tracer_meta.json` next to the data files.
pub fn generate(
    data_dir: &Path,
    params: &DatasetParams,
    progress: Arc<Mutex<GenerateProgress>>,
) -> std::io::Result<()> {
    {
        let mut p = progress.lock().unwrap();
        p.total_keys = params.keys;
        p.keys_written = 0;
    }

    let opts = Options {
        max_data_file_size_bytes: params.file_size_bytes,
        ..Options::default()
    };

    let mut engine =
        Engine::open(data_dir, opts).map_err(|e| std::io::Error::other(e.to_string()))?;

    let mut rng = StdRng::seed_from_u64(SEED);
    let mut value_buf = vec![0u8; params.value_size];
    let start = std::time::Instant::now();

    for i in 0..params.keys {
        let key = format!("key:{i:08}");
        rng.fill_bytes(&mut value_buf);
        engine
            .put(key.as_bytes(), &value_buf)
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        let mut p = progress.lock().unwrap();
        p.keys_written = i + 1;
        p.elapsed_ms = start.elapsed().as_millis() as u64;
    }

    // Write meta file so future runs can detect whether regeneration is needed.
    write_meta(data_dir, params)?;
    Ok(())
}

/// Return true if `tracer_meta.json` in `data_dir` matches `params`.
pub fn params_match(data_dir: &Path, params: &DatasetParams) -> bool {
    let Some(stored) = read_meta(data_dir) else {
        return false;
    };
    &stored == params
}

/// Read `tracer_meta.json` from `data_dir`.  Returns `None` on any error.
pub fn read_meta(data_dir: &Path) -> Option<DatasetParams> {
    let path = data_dir.join("tracer_meta.json");
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_meta(data_dir: &Path, params: &DatasetParams) -> std::io::Result<()> {
    let path = data_dir.join("tracer_meta.json");
    let json =
        serde_json::to_vec_pretty(params).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(path, json)
}
