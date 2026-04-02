# tracer Architecture

## Purpose

`tracer` is a standalone diagnostic crate that lives alongside `bitdb` in the
same Cargo workspace.  It visually demonstrates the difference between serial
and parallel execution for two core `bitdb` operations:

- **Startup rebuild** - scanning data files to reconstruct the in-memory KeyDir.
- **Merge read phase** - loading live record values from disk before compaction.

It never modifies any `bitdb` source file.  It depends on `bitdb` as a library
crate and uses only its public API.

---

## Workspace layout

```
/                               workspace root
  Cargo.toml                    [workspace] manifest: members = ["bitdb", "tracer"]
  bitdb/                        existing crate, zero changes
  tracer/                       this crate
    Cargo.toml
    docs/
      ARCHITECTURE.md           this file
    src/
      main.rs                   CLI entry point, screen router bootstrap
      dataset.rs                dataset generation, meta file read/write
      worker.rs                 work assignment, shared state, instrumented scan
      tui/
        mod.rs                  terminal setup/teardown, screen router loop
        setup.rs                setup screen (parameter selection)
        generate.rs             dataset generation progress screen
        live.rs                 live side-by-side serial vs parallel view
        result.rs               frozen final result + summary screen
    tests/
      assignment.rs             TDD: file distribution across threads
      worker.rs                 TDD: state transitions and timing
      dataset.rs                TDD: key generation and meta file correctness
```

---

## Dependencies

```toml
bitdb      = { path = "../bitdb" }   public API only
ratatui    = "0.29"                  TUI rendering
crossterm  = "0.28"                  terminal raw mode, key events
rayon      = "1.10"                  parallel file scanning
rand       = "0.9"                   random value generation (fixed seed 42)
serde      = { version = "1", features = ["derive"] }
serde_json = "1"                     tracer_meta.json read/write
```

---

## Dataset parameters

All tunable on the setup screen.  Stored in `tracer_meta.json` next to the
data files so the tracer can detect whether regeneration is needed.

| Parameter | Default | Range |
|---|---|---|
| keys | 1,000,000 | 100k steps, 100k to 5M |
| value size (bytes) | 64 | 8 / 64 / 256 / 1024 |
| file size | 512 KB | 128KB / 256KB / 512KB / 1MB / 4MB |
| parallel threads | 4 | 1 to 16 |
| mode | both | serial / parallel / both |

At defaults (1M keys, 64b values, 512KB files):
- ~100 MB total data on disk
- ~200 data files
- 4 threads → 50 files each
- Expected serial rebuild: 400-800 ms
- Expected parallel rebuild: 100-200 ms
- Expected speedup: 3-5x

File size is intentionally kept small (512KB default) so the number of data
files is large enough for parallelism to show a dramatic visual and numeric
difference.  Hint files are deleted before every benchmark run so the full
record-decode path is always exercised rather than the fast hint-read shortcut.

---

## Dataset generation (`dataset.rs`)

```rust
pub struct DatasetParams {
    pub keys: usize,
    pub value_size: usize,
    pub file_size_bytes: u64,
}

pub struct GenerateProgress {
    pub keys_written: usize,
    pub total_keys: usize,
    pub files_created: usize,
    pub elapsed_ms: u64,
}
```

- Uses `rand` with fixed seed `42` so every run on every machine produces the
  same dataset.
- Keys are formatted as `key:{i:08}` (e.g. `key:00000001`).
- Values are random bytes of `value_size` length.
- Writes via `bitdb::engine::Engine::open` + `engine.put()`.
- Progress is reported via `Arc<Mutex<GenerateProgress>>` polled by the TUI
  generation screen at 16ms intervals.
- On completion writes `tracer_meta.json`:

```json
{
  "keys": 1000000,
  "value_size": 64,
  "file_size_bytes": 524288
}
```

On startup the tracer reads `tracer_meta.json`.  If it matches the current
setup-screen parameters the existing dataset is reused.  If it is missing or
mismatched, generation is triggered.

---

## Work assignment (`worker.rs`)

### Types

```rust
pub enum SlotState {
    Queued,
    Processing { bytes_done: u64, keys_found: usize },
    Done { duration_us: u64, keys_found: usize, bytes_read: u64 },
}

pub struct FileSlot {
    pub file_id: u32,
    pub file_size_bytes: u64,
    pub state: SlotState,
}

pub struct ThreadState {
    pub thread_id: usize,
    pub slots: Vec<FileSlot>,
}

pub struct RunResult {
    pub thread_states: Vec<ThreadState>,
    pub total_keys: usize,
    pub wall_time_us: u64,
    pub keys_per_sec: f64,
}
```

### File distribution

Files are assigned round-robin across T threads before any scanning begins.
Thread `i` receives files at indices `i, i+T, i+2T, ...` from the
oldest-to-newest ordered file list.  This assignment is visible in the TUI
as the full queued slot list per thread before the run starts.

Serial mode is T=1: one thread receives all files in order.

### Instrumented scan loop

Each rayon task:
1. Marks its current slot as `Processing`.
2. Reads the full file into a byte buffer (`std::fs::read`).
3. Loops `bitdb::record::decode_one` over the buffer - same decode path as
   the real engine's recovery, but with timing and progress writes inserted.
4. Every 5,000 records writes `bytes_done` and `keys_found` back to the
   shared `Arc<Mutex<Vec<ThreadState>>>` so the TUI can show fill progress.
5. On file completion marks the slot as `Done` with wall duration and totals.

### Shared state

```
Arc<Mutex<Vec<ThreadState>>>
```

No channels, no async.  The TUI render loop acquires the lock on each 16ms
tick, clones the state snapshot, releases the lock, and redraws.  Workers
hold the lock only for a single `Vec` index write - contention is negligible.

### Serial vs parallel runs

`both` mode runs serial (T=1) first to completion, captures `RunResult`,
then runs parallel (T=N) to completion, captures `RunResult`.  Both runs
operate on the same data files.  Hint files are deleted before the serial
run so neither run benefits from them.

---

## TUI screens (`tui/`)

### Screen router (`tui/mod.rs`)

```rust
pub enum Screen {
    Setup(SetupState),
    Generating(GenerateState),
    Live(LiveState),
    Result(ResultState),
}
```

A single `run()` function owns the terminal lifecycle:
- `enable_raw_mode` + `EnterAlternateScreen` on entry.
- `disable_raw_mode` + `LeaveAlternateScreen` unconditionally on exit.
- 16ms poll loop: handle key event → update screen state → redraw.

---

### Setup screen (`tui/setup.rs`)

Five navigable fields (Up/Down to select, Left/Right or `-`/`+` to change):

```
  dataset
  > keys          1,000,000
    value size    64 bytes
    file size     512 KB

  threads
    parallel      4

  mode            both

  dataset status  ready (1,000,000 keys, 512KB files, 200 files)

  [ Enter: run ]   [ g: regenerate ]   [ q: quit ]
```

The `dataset status` line reads `tracer_meta.json` at render time and shows
whether the current parameters match the existing data.

---

### Generation screen (`tui/generate.rs`)

```
  generating dataset

  [████████████████████░░░░░░░░░░░░░░░░░░░░]  482,310 / 1,000,000

  files created   94
  elapsed         18.4s
  estimated       ~20s remaining
```

Progress bar driven by `Arc<Mutex<GenerateProgress>>`.  Transitions
automatically to `Live` on completion.

---

### Live screen (`tui/live.rs`)

Split into two columns when mode is `both`.  Single full-width panel for
`serial` or `parallel` mode.

Left column: serial run (T=1), all files assigned to Thread 0.
Right column: parallel run (T=N), files round-robin across N thread rows.

Each file slot is rendered as:

```
  f:042 [████████████░░░░░░░░]
```

Bar fill is proportional to `bytes_done / file_size_bytes`.

Slot colors:
- Grey fill `░` - queued, not yet started.
- Yellow fill `█` - currently processing.
- Green fill `█` - done.

Footer (always visible, never scrolls away):

```
  serial    12.4ms  [████████████░░░░░░░░░░░░░░░░░░░░]  in progress
  parallel   3.8ms  [████░░░░░░░░░░░░░░░░░░░░░░░░░░░░]  in progress
  speedup    3.3x                keys rebuilt: 312,400 / 1,000,000
```

Both elapsed timers tick live.  Speedup ratio appears as soon as both
runs have non-zero elapsed time.  Thread file lists scroll within each
column if they overflow the available height.

Transitions automatically to `Result` when both runs complete.

---

### Result screen (`tui/result.rs`)

Frozen snapshot of the live view (all bars green) plus a summary panel:

```
  startup rebuild
    serial        623ms    1,604,000 keys/sec
    parallel      178ms    5,617,000 keys/sec    3.5x faster

  dataset
    keys          1,000,000
    files         200
    total size    98.4 MB
    threads       4

  [ r: back to setup ]        [ q: quit ]
```

---

## Error handling

All engine errors from dataset generation are printed to the result area and
do not crash the TUI.  Terminal is always restored before the process exits,
even on panic (via the screen router's unconditional teardown).

---

## TDD test plan

### `tests/assignment.rs`
- 200 files across 4 threads → each thread gets exactly 50 files.
- 201 files across 4 threads → thread 0 gets 51, threads 1-3 get 50.
- 1 file across 4 threads → thread 0 gets 1 file, threads 1-3 get none.
- Round-robin order is correct: thread 0 gets indices 0,4,8,...
- Serial mode (T=1) gives all files to thread 0.

### `tests/worker.rs`
- Fresh assignment has all slots in `Queued` state.
- After processing, slot transitions `Queued -> Processing -> Done`.
- `Done` slot carries `keys_found > 0` and `duration_us > 0`.
- `RunResult.total_keys` equals sum of all slot `keys_found` values.
- `RunResult.wall_time_us` is less than sum of slot durations in parallel mode.

### `tests/dataset.rs`
- Given params writes correct number of keys.
- `tracer_meta.json` is written with correct field values.
- Params-match detection returns true when meta matches settings.
- Params-match detection returns false when meta differs.
- Keys are formatted as `key:{i:08}`.
- Fixed seed `42` produces identical dataset on two consecutive runs.

---

## What the instructor sees

1. `cargo run -p tracer` - opens setup screen, tweak keys/threads/file-size.
2. Press Enter - dataset generates (one time), then live view plays.
3. Left column: one thread crawling through 200 files sequentially.
4. Right column: 4 threads each chewing through 50 files in parallel.
5. Footer: both clocks ticking, speedup ratio updating live.
6. Result screen: clean numeric summary with keys/sec and Nx faster.
7. Press `r` - back to setup, change thread count to 1/2/4/8, rerun, compare.
