# bitdb

## What this tool is

`bitdb` is a Bitcask-style key-value store in Rust.

- Append-only data files; no in-place mutation.
- In-memory KeyDir (hash map) for O(1) key lookups.
- Tombstone delete model.
- Startup rebuild from data files or compact hint files.
- Configurable parallelism for startup rebuild and merge.
- Merge/compaction that rewrites only live keys.
- CLI for put, get, delete, stats, merge, and benchmarks.
- Interactive TUI with a scrollable session and a live stats bar.

See `docs/ARCHITECTURE.md` for the full design including file format,
recovery semantics, and the parallel pipeline model.

---

## Build and run

```bash
cargo build
cargo run -- --help
```

---

## TUI (interactive mode)

Launch the interactive terminal interface:

```bash
cargo run -- tui
cargo run -- --data-dir ./data tui
```

Layout:

```
+----------------------------------------------------------------------+
| live_keys: 3   tombstones: 1   data_dir: ./data                      |  <- cement grey stats bar
+----------------------------------------------------------------------+
|                                                                       |
|  > help                                                               |
|  commands:                                                            |
|    put <key> <value>   insert or overwrite a key                     |
|    get <key>           retrieve a value                              |
|    delete <key>        delete a key                                  |
|    stats               show live_keys and tombstones                 |
|    merge               run compaction                                |
|    clear               clear this output                             |
|    quit / exit         exit the TUI                                  |
|                                                                       |
|  > put hello world                                                    |
|  OK                                                                   |
|                                                                       |
|  > get hello                                                          |
|  world                                                                |
|                                                                       |
|  > _                                                                  |  <- input follows last output
|                                                                       |
+----------------------------------------------------------------------+
```

The stats bar (top) refreshes automatically after every mutating command.
The output area scrolls naturally as content grows.
The input prompt is not pinned; it lives right after the last output line.

### TUI keybindings

| Key | Action |
|---|---|
| `Enter` | Submit the current command |
| `Backspace` | Delete the last character |
| `Up` | Recall previous command from history |
| `Down` | Move forward in history (empty line at end) |
| `Ctrl-C` / `Ctrl-D` | Exit the TUI |

### TUI commands

| Command | Description |
|---|---|
| `help` | List all available commands |
| `put <key> <value>` | Insert or overwrite a key |
| `get <key>` | Retrieve a value (`NOT_FOUND` if absent) |
| `delete <key>` | Delete a key (tombstone) |
| `stats` | Print current live_keys and tombstones |
| `merge` | Run compaction (removes dead records) |
| `clear` | Clear the output history |
| `quit` / `exit` | Exit the TUI |

---

## CLI operations

All examples run from repo root.

### Put a key

```bash
cargo run -- --data-dir ./data put user:1 alice
```

```text
OK
```

### Get a key

```bash
cargo run -- --data-dir ./data get user:1
```

```text
alice
```

Missing key:

```text
NOT_FOUND
```

### Delete a key

```bash
cargo run -- --data-dir ./data delete user:1
```

```text
OK
```

### Show stats

```bash
cargo run -- --data-dir ./data stats
```

```text
live_keys=10 tombstones=3
```

### Run merge (compaction)

```bash
cargo run -- --data-dir ./data merge
```

```text
OK
```

---

## Benchmarks

### CLI benchmarks

Startup (serial):

```bash
cargo run -- --data-dir ./bench-data bench startup --mode serial
```

Startup (parallel):

```bash
cargo run -- --data-dir ./bench-data bench startup --mode parallel
```

Output shape:

```text
startup_ms=...
```

Merge (serial):

```bash
cargo run -- --data-dir ./bench-data bench merge --mode serial
```

Merge (parallel):

```bash
cargo run -- --data-dir ./bench-data bench merge --mode parallel
```

Output shape:

```text
merge_ms=...
```

Workload (serial):

```bash
cargo run -- --data-dir ./bench-data bench workload --ops 5000 --mode serial
```

Workload (parallel):

```bash
cargo run -- --data-dir ./bench-data bench workload --ops 5000 --mode parallel
```

Output shape:

```text
ops_per_sec=...
```

### Criterion benchmarks

```bash
cargo bench --bench engine -- --sample-size 10
```

Benchmarks included:

- `engine_scaffold_noop` - baseline noop
- `engine_put_get_serial` - single put + get cycle
- `startup_rebuild_serial` - full rebuild of 500-key database
- `startup_rebuild/mode/serial` - rebuild of 2000-key database, serial
- `startup_rebuild/mode/parallel_auto` - same, parallel
- `merge_serial` - write + compact 1000 writes over 64 hot keys, serial
- `merge_pipeline/mode/serial` - write 500 unique keys + 100 overwrites + compact, serial
- `merge_pipeline/mode/parallel_auto` - same, parallel

Baseline results are tracked in `BENCHMARK_BASELINE.md`.

---

## Parallelism configuration

The engine accepts an `Options::parallelism` value:

| Value | Behaviour |
|---|---|
| `Serial` | All operations run single-threaded. |
| `Auto` | rayon uses all available logical CPUs. |
| `Fixed(n)` | rayon uses exactly `n` threads. |

Parallel paths affect startup rebuild and the merge read phase. The
merge write phase and final install are always single-threaded.

---

## Dev quality commands

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

### Targeted test commands

```bash
cargo test --test record
cargo test --test merge
cargo test --test parallel_rebuild
cargo test --test parallel_merge
cargo test --test tui
cargo test cli_put_get_delete_flow -- --exact
```
