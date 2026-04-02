/// Tests for dataset generation and tracer_meta.json correctness.
use tempfile::TempDir;
use tracer::dataset::{DatasetParams, GenerateProgress, generate, params_match};

fn default_params() -> DatasetParams {
    DatasetParams {
        keys: 500,
        value_size: 16,
        file_size_bytes: 32 * 1024,
    }
}

#[test]
fn generate_writes_correct_key_count() {
    let dir = TempDir::new().unwrap();
    let params = default_params();
    let progress = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));

    generate(dir.path(), &params, progress).unwrap();

    // Open with bitdb and verify key count via stats.
    let opts = bitdb::config::Options {
        max_data_file_size_bytes: params.file_size_bytes,
        ..bitdb::config::Options::default()
    };
    let engine = bitdb::engine::Engine::open(dir.path(), opts).unwrap();
    assert_eq!(engine.stats().live_keys, params.keys);
}

#[test]
fn generate_writes_meta_file() {
    let dir = TempDir::new().unwrap();
    let params = default_params();
    let progress = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));

    generate(dir.path(), &params, progress).unwrap();

    assert!(
        dir.path().join("tracer_meta.json").exists(),
        "tracer_meta.json must be written after generation"
    );
}

#[test]
fn params_match_returns_true_when_meta_matches() {
    let dir = TempDir::new().unwrap();
    let params = default_params();
    let progress = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));

    generate(dir.path(), &params, progress).unwrap();

    assert!(
        params_match(dir.path(), &params),
        "params_match should return true when meta matches"
    );
}

#[test]
fn params_match_returns_false_when_key_count_differs() {
    let dir = TempDir::new().unwrap();
    let params = default_params();
    let progress = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));

    generate(dir.path(), &params, progress).unwrap();

    let different = DatasetParams {
        keys: 999,
        ..params
    };
    assert!(
        !params_match(dir.path(), &different),
        "params_match should return false when key count differs"
    );
}

#[test]
fn params_match_returns_false_when_no_meta_file() {
    let dir = TempDir::new().unwrap();
    let params = default_params();
    assert!(
        !params_match(dir.path(), &params),
        "params_match should return false when no meta file exists"
    );
}

#[test]
fn keys_are_formatted_correctly() {
    let dir = TempDir::new().unwrap();
    let params = DatasetParams {
        keys: 5,
        value_size: 8,
        file_size_bytes: 64 * 1024,
    };
    let progress = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));

    generate(dir.path(), &params, progress).unwrap();

    let opts = bitdb::config::Options {
        max_data_file_size_bytes: params.file_size_bytes,
        ..bitdb::config::Options::default()
    };
    let engine = bitdb::engine::Engine::open(dir.path(), opts).unwrap();
    for i in 0..5usize {
        let key = format!("key:{i:08}");
        assert!(
            engine.get(key.as_bytes()).unwrap().is_some(),
            "key {key} should exist"
        );
    }
}

#[test]
fn fixed_seed_produces_identical_values_on_two_runs() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();
    let params = DatasetParams {
        keys: 10,
        value_size: 16,
        file_size_bytes: 64 * 1024,
    };

    let p1 = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));
    let p2 = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));
    generate(dir1.path(), &params, p1).unwrap();
    generate(dir2.path(), &params, p2).unwrap();

    let opts = bitdb::config::Options {
        max_data_file_size_bytes: params.file_size_bytes,
        ..bitdb::config::Options::default()
    };
    let e1 = bitdb::engine::Engine::open(dir1.path(), opts.clone()).unwrap();
    let e2 = bitdb::engine::Engine::open(dir2.path(), opts).unwrap();

    for i in 0..10usize {
        let key = format!("key:{i:08}");
        assert_eq!(
            e1.get(key.as_bytes()).unwrap(),
            e2.get(key.as_bytes()).unwrap(),
            "fixed seed must produce identical value for {key}"
        );
    }
}

#[test]
fn progress_is_updated_during_generation() {
    let dir = TempDir::new().unwrap();
    let params = DatasetParams {
        keys: 200,
        value_size: 16,
        file_size_bytes: 64 * 1024,
    };
    let progress = std::sync::Arc::new(std::sync::Mutex::new(GenerateProgress::default()));
    let progress_clone = progress.clone();

    generate(dir.path(), &params, progress).unwrap();

    let p = progress_clone.lock().unwrap();
    assert_eq!(p.keys_written, 200);
    assert_eq!(p.total_keys, 200);
}
