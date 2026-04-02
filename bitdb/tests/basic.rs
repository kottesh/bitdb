use tempfile::tempdir;

#[test]
fn open_empty_database_succeeds() {
    let dir = tempdir().expect("tempdir should be created");
    let options = bitdb::config::Options::default();

    let engine = bitdb::engine::Engine::open(dir.path(), options);

    assert!(engine.is_ok());
}

#[test]
fn engine_put_get_delete_and_reopen() {
    let dir = tempdir().expect("tempdir should be created");
    let options = bitdb::config::Options::default();

    {
        let mut engine =
            bitdb::engine::Engine::open(dir.path(), options.clone()).expect("open should succeed");

        engine.put(b"alpha", b"1").expect("put should succeed");
        let got = engine.get(b"alpha").expect("get should succeed");
        assert_eq!(got, Some(b"1".to_vec()));

        engine
            .put(b"alpha", b"2")
            .expect("overwrite should succeed");
        let got = engine.get(b"alpha").expect("get should succeed");
        assert_eq!(got, Some(b"2".to_vec()));

        engine.delete(b"alpha").expect("delete should succeed");
        let got = engine.get(b"alpha").expect("get should succeed");
        assert_eq!(got, None);

        engine.sync().expect("sync should succeed");
    }

    let mut reopened =
        bitdb::engine::Engine::open(dir.path(), options).expect("reopen should succeed");
    let got = reopened.get(b"alpha").expect("get should succeed");
    assert_eq!(got, None);

    reopened
        .put(b"beta", b"persist")
        .expect("put should succeed");
    let stats = reopened.stats();
    assert_eq!(stats.live_keys, 1);
}

#[test]
fn engine_supports_binary_keys_and_values() {
    let dir = tempdir().expect("tempdir should be created");
    let options = bitdb::config::Options::default();
    let mut engine = bitdb::engine::Engine::open(dir.path(), options).expect("open should work");

    let key = [0, 255, 1, 2];
    let value = [9, 8, 0, 7];

    engine.put(&key, &value).expect("put should work");
    let got = engine.get(&key).expect("get should work");

    assert_eq!(got, Some(value.to_vec()));
}
