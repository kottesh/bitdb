# bitdb

## What this tool is

`bitdb` is a Bitcask-style key-value store in Rust.

- append-only data files
- in-memory keydir for latest key position
- tombstone delete model
- startup rebuild from data/hint files
- serial merge compaction
- CLI operations + benchmark commands

Use it for simple durable key-value workflows, local testing, storage experiments.

## Operations you can perform

All examples run from repo root.

### Build and run

```bash
cargo build
cargo run -- --help
```

### Put a key

```bash
cargo run -- --data-dir ./data put user:1 alice
```

Expected output:

```text
OK
```

### Get a key

```bash
cargo run -- --data-dir ./data get user:1
```

Expected output when found:

```text
alice
```

Expected output when missing:

```text
NOT_FOUND
```

### Delete a key

```bash
cargo run -- --data-dir ./data delete user:1
```

Expected output:

```text
OK
```

### Show stats

```bash
cargo run -- --data-dir ./data stats
```

Example output:

```text
live_keys=10 tombstones=3
```

### Run merge (compaction)

```bash
cargo run -- --data-dir ./data merge
```

Expected output:

```text
OK
```

### Run CLI benchmarks

Startup benchmark:

```bash
cargo run -- --data-dir ./bench-data bench startup --mode serial
```

Output shape:

```text
startup_ms=...
```

Merge benchmark:

```bash
cargo run -- --data-dir ./bench-data bench merge --mode serial
```

Output shape:

```text
merge_ms=...
```

Workload benchmark:

```bash
cargo run -- --data-dir ./bench-data bench workload --ops 5000 --mode serial --threads 1
```

Output shape:

```text
ops_per_sec=...
```

### Run criterion benchmarks

```bash
cargo bench --bench engine -- --sample-size 10
```

Baseline serial numbers tracked in `BENCHMARK_BASELINE.md`.

## Dev quality commands

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## Single test examples

```bash
cargo test --test record
cargo test --test merge
cargo test cli_put_get_delete_flow -- --exact
```
