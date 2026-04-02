use bitdb::config::Options;
use tempfile::tempdir;

#[test]
fn merge_keeps_only_live_latest_values() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options {
        max_data_file_size_bytes: 80,
        ..Options::default()
    };
    let mut engine = bitdb::engine::Engine::open(dir.path(), options).expect("open should work");

    engine.put(b"a", b"1").expect("put should work");
    engine.put(b"a", b"2").expect("put should work");
    engine.put(b"b", b"1").expect("put should work");
    engine.delete(b"b").expect("delete should work");
    engine.put(b"c", b"3").expect("put should work");

    engine.merge().expect("merge should work");

    assert_eq!(
        engine.get(b"a").expect("get should work"),
        Some(b"2".to_vec())
    );
    assert_eq!(engine.get(b"b").expect("get should work"), None);
    assert_eq!(
        engine.get(b"c").expect("get should work"),
        Some(b"3".to_vec())
    );
}

#[test]
fn merge_cleans_old_files_and_installs_compacted_files() {
    let dir = tempdir().expect("tempdir should be created");
    let options = Options {
        max_data_file_size_bytes: 64,
        ..Options::default()
    };
    let mut engine = bitdb::engine::Engine::open(dir.path(), options).expect("open should work");

    for i in 0..20 {
        let key = "hot-key";
        let value = format!("payload-{i}");
        engine
            .put(key.as_bytes(), value.as_bytes())
            .expect("put should work");
    }

    let before = std::fs::read_dir(dir.path())
        .expect("read_dir should work")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|v| v.to_str()) == Some("data"))
        .count();
    assert!(before > 1);

    engine.merge().expect("merge should work");

    let after = std::fs::read_dir(dir.path())
        .expect("read_dir should work")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|v| v.to_str()) == Some("data"))
        .count();
    assert!(after > 0);
    assert!(after < before);
    assert!(!dir.path().join(".merge_tmp").exists());
}
