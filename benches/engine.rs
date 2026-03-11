use criterion::{Criterion, criterion_group, criterion_main};

use bitdb::config::Options;
use bitdb::engine::Engine;
use tempfile::tempdir;

fn engine_scaffold_bench(c: &mut Criterion) {
    c.bench_function("engine_scaffold_noop", |b| b.iter(|| 1 + 1));

    c.bench_function("engine_put_get_serial", |b| {
        b.iter(|| {
            let dir = tempdir().expect("tempdir should be created");
            let mut engine =
                Engine::open(dir.path(), Options::default()).expect("engine should open");
            engine.put(b"k", b"v").expect("put should work");
            let _ = engine.get(b"k").expect("get should work");
        })
    });

    c.bench_function("startup_rebuild_serial", |b| {
        let dir = tempdir().expect("tempdir should be created");
        {
            let mut engine =
                Engine::open(dir.path(), Options::default()).expect("engine should open");
            for i in 0..500 {
                let key = format!("k{i}");
                let val = format!("v{i}");
                engine
                    .put(key.as_bytes(), val.as_bytes())
                    .expect("put should work");
            }
        }

        b.iter(|| {
            let _ = Engine::open(dir.path(), Options::default()).expect("open should work");
        })
    });

    c.bench_function("merge_serial", |b| {
        b.iter(|| {
            let dir = tempdir().expect("tempdir should be created");
            let mut engine =
                Engine::open(dir.path(), Options::default()).expect("engine should open");
            for i in 0..1000 {
                let key = format!("hot-{}", i % 64);
                let val = format!("v{i}");
                engine
                    .put(key.as_bytes(), val.as_bytes())
                    .expect("put should work");
            }
            engine.merge().expect("merge should work");
        })
    });
}

criterion_group!(benches, engine_scaffold_bench);
criterion_main!(benches);
