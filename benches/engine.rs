use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use bitdb::config::{Options, Parallelism};
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

    // Compare serial vs parallel (Auto) startup rebuild on a medium dataset.
    // The dataset is written once; each iteration reopens and rebuilds.
    {
        let dir = tempdir().expect("tempdir should be created");
        {
            let mut engine =
                Engine::open(dir.path(), Options::default()).expect("engine should open");
            for i in 0..2000 {
                let key = format!("bench_key_{i:06}");
                let val = format!("bench_val_{i:06}");
                engine
                    .put(key.as_bytes(), val.as_bytes())
                    .expect("put should work");
            }
        }

        let mut group = c.benchmark_group("startup_rebuild");

        group.bench_with_input(BenchmarkId::new("mode", "serial"), &dir, |b, d| {
            b.iter(|| {
                let opts = Options {
                    parallelism: Parallelism::Serial,
                    ..Options::default()
                };
                Engine::open(d.path(), opts).expect("open should work");
            });
        });

        group.bench_with_input(BenchmarkId::new("mode", "parallel_auto"), &dir, |b, d| {
            b.iter(|| {
                let opts = Options {
                    parallelism: Parallelism::Auto,
                    ..Options::default()
                };
                Engine::open(d.path(), opts).expect("open should work");
            });
        });

        group.finish();
    }

    c.bench_function("merge_serial", |b| {
        b.iter(|| {
            let dir = tempdir().expect("tempdir should be created");
            let mut engine = Engine::open(dir.path(), Options {
                parallelism: Parallelism::Serial,
                ..Options::default()
            })
            .expect("engine should open");
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

    // Compare serial vs parallel merge on a dataset with many unique live keys
    // so the read phase has meaningful work to parallelise.
    {
        let mut group = c.benchmark_group("merge_pipeline");

        for (label, parallelism) in [
            ("serial", Parallelism::Serial),
            ("parallel_auto", Parallelism::Auto),
        ] {
            group.bench_with_input(
                BenchmarkId::new("mode", label),
                &parallelism,
                |b, &par| {
                    b.iter(|| {
                        let dir = tempdir().expect("tempdir should be created");
                        let mut engine = Engine::open(
                            dir.path(),
                            Options {
                                parallelism: par,
                                ..Options::default()
                            },
                        )
                        .expect("engine should open");
                        // Write 500 unique keys with a few overwrites to
                        // simulate realistic compaction input.
                        for i in 0..500 {
                            let key = format!("mk{i:04}");
                            let val = format!("mv{i:04}");
                            engine
                                .put(key.as_bytes(), val.as_bytes())
                                .expect("put should work");
                        }
                        for i in 0..100 {
                            let key = format!("mk{i:04}");
                            let val = format!("mv{i:04}_v2");
                            engine
                                .put(key.as_bytes(), val.as_bytes())
                                .expect("overwrite should work");
                        }
                        engine.merge().expect("merge should work");
                    });
                },
            );
        }

        group.finish();
    }
}

criterion_group!(benches, engine_scaffold_bench);
criterion_main!(benches);
