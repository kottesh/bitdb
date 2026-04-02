# bitdb Architecture

## Overview

bitdb is a Bitcask-style append-only key-value store implemented in Rust.
All writes append to an active data file.  An in-memory hash map (the
KeyDir) tracks the latest byte offset for every key.  Reads are a single
random-access seek.  Startup rebuilds the KeyDir by replaying all data
files from oldest to newest (or by reading compact hint files).
Compaction (merge) rewrites only live keys into fresh data files and then
atomically replaces the old files.

---

## On-disk file format

### Data files

Each record occupies a contiguous byte sequence on disk:

```
+----------+---------+---------+----------+-----------+----------+------------+-----+-------+
| magic(4) | ver(1)  | flags(1)| crc32(4) | ts(8)     | klen(4)  | vlen(4)    | key | value |
+----------+---------+---------+----------+-----------+----------+------------+-----+-------+
```

- `magic`: `0x42444231` (ASCII "BDB1") - identifies a bitdb record.
- `ver`: record format version, currently `1`.
- `flags`: `0x00` = normal record, `0x01` = tombstone (deleted key).
- `crc32`: CRC-32 over all fields except the crc32 field itself.
- `ts`: Unix timestamp in seconds when the record was written.
- `klen`/`vlen`: byte lengths of the key and value.
- `key`/`value`: raw bytes; any byte sequence is valid.

Tombstone records have `flags = 0x01` and a zero-length value.  They mark
a key as deleted without requiring any in-place mutation.

Data files are named `%08d.data` where the number is the file ID.  IDs
are assigned in monotonically increasing order.  The highest-numbered
file is the active (writable) file; all others are immutable.

### Hint files

Hint files parallel data files: `%08d.hint`.  They contain one entry per
record in the corresponding data file, storing the key plus location
metadata (file_id, offset, size_bytes, timestamp, is_tombstone) but not
the value.  Reading hint files on startup avoids scanning value bytes,
making rebuild faster at the cost of one extra file per data file.

Hint file layout:

```
+----------+---------+----------+
| magic(4) | ver(1)  | count(4) |  <- 9-byte file header
+----------+---------+----------+
[ per-entry: klen(4) fid(4) off(8) size(4) ts(8) tomb(1) key(klen) ]*
```

---

## In-memory structure: KeyDir

The KeyDir is a `HashMap<Vec<u8>, KeyDirEntry>` keyed on raw key bytes.
Each entry holds:

- `file_id`: which data file contains the latest value.
- `offset`: byte offset of the record within that file.
- `size_bytes`: total encoded record length (used to size reads).
- `timestamp`: record timestamp (used during merge to preserve ts).
- `is_tombstone`: whether the latest write was a delete.

Only the latest entry for each key is kept.  Tombstone entries remain in
the KeyDir so that gets for deleted keys are answered without touching
disk.

---

## Write path

1. Encode the record (header + CRC + key + value).
2. Append the encoded bytes to the active `DataFile`.
3. If the active file exceeds `max_data_file_size_bytes`, rotate to a new
   file with `id = current_id + 1`.
4. Insert/overwrite the KeyDir entry with the new file_id and offset.

All writes are append-only.  No in-place mutation occurs.

---

## Read path

1. Look up the key in the KeyDir.
2. If absent or `is_tombstone`, return `None`.
3. Seek to `(file_id, offset)` in the data file and decode the record.
4. Return the value bytes.

---

## Startup rebuild

Startup iterates data file IDs from oldest (lowest) to newest.  For each
file it checks whether a valid hint file exists.  If so, it reads the hint
entries directly.  Otherwise it scans the raw data file record-by-record.
The KeyDir is built by inserting entries in order; because newer files are
processed last, the final entry for any key is always the latest write.

### Parallelism

When `Options::parallelism` is `Auto` or `Fixed(n)`, each file is scanned
on a separate rayon worker thread.  The collected `(file_id, offset,
entry, key)` tuples are then sorted by `(file_id ASC, offset ASC)` and
merged in a single linear pass that is identical in semantics to the
serial path.

### Corruption policy

- `CorruptionPolicy::Fail`: any CRC mismatch or invalid header returns
  an error and aborts startup.
- `CorruptionPolicy::SkipCorruptedTail`: a corrupt record stops scanning
  the current file at that point (treats it as a truncated tail) and
  continues with the next file.

---

## Merge / compaction

Merge eliminates dead records (overwritten or deleted keys):

1. Collect all non-tombstone KeyDir entries.
2. Load the value for each entry from the current data files.  This read
   phase runs in parallel when `parallelism != Serial`.
3. Write all live records into a temporary directory (`.merge_tmp`) using
   a fresh FileSet.  Hint files are written for the new files.
4. Sync the merged FileSet to disk.
5. Delete the old `.data` and `.hint` files from the data directory.
6. Move the merged files from `.merge_tmp` into the data directory.
7. Remove the temporary directory.

Steps 5-7 constitute the atomic install.  If the process crashes after
step 5 but before step 7, the data directory will be empty; a subsequent
startup will open an empty database.  If it crashes after step 4 but
before step 5, the original files are intact.

---

## Parallelism model

The `Parallelism` enum controls both startup rebuild and merge:

| Variant     | Rebuild                     | Merge read phase              |
|-------------|-----------------------------|-------------------------------|
| `Serial`    | single-threaded file scan   | single-threaded record reads  |
| `Fixed(n)`  | rayon pool of n threads     | rayon pool of n threads       |
| `Auto`      | rayon global pool (all CPUs)| rayon global pool (all CPUs)  |

The write phase of merge is always single-threaded (append-only writer).
The final install is always single-threaded and atomic.

---

## Error handling

All errors are represented by `BitdbError` (`src/error.rs`):

- `Io`: wraps `std::io::Error` from any file operation.
- `TruncatedRecord`: record header or body is cut short.
- `InvalidRecordMagic`: magic bytes do not match.
- `InvalidRecordVersion`: version byte is unrecognised.
- `InvalidRecordFlags`: flags byte is unrecognised.
- `ChecksumMismatch { expected, actual }`: CRC-32 mismatch.
- `DataFileNotFound(file_id)`: a referenced file does not exist.
- `InvalidHintFile`: hint file header is malformed.

Corruption is never silently swallowed on the core read/write paths.
The startup rebuild respects `CorruptionPolicy` for tail truncation and
CRC failures; everything else propagates as `Err`.

---

## Module layout

```
src/
  main.rs          binary entrypoint; CLI dispatch
  lib.rs           module declarations
  config.rs        Options, CorruptionPolicy, Parallelism
  error.rs         BitdbError, Result alias
  record.rs        encode/decode, RecordFlags
  storage/
    mod.rs
    data_file.rs   DataFile append/read, file-naming helpers
    file_set.rs    FileSet: active writer + known immutable files
    hint_file.rs   HintEntry, read_hint_file, write_hint_file
  index/
    mod.rs
    keydir.rs      KeyDir, KeyDirEntry
  recovery.rs      rebuild_keydir (serial + parallel paths)
  merge.rs         run_merge (serial + parallel read phase)
  engine.rs        Engine: public API (open/get/put/delete/sync/stats/merge)
  cli.rs           Clap CLI types
  bench.rs         CLI benchmark helpers

tests/
  basic.rs         engine open/put/get/delete/reopen
  record.rs        record codec unit tests
  storage.rs       DataFile/FileSet unit tests
  rebuild.rs       KeyDir rebuild unit tests
  hint.rs          hint file unit tests
  recovery.rs      corruption/truncation recovery unit tests
  merge.rs         serial merge correctness tests
  parallel_rebuild.rs  parallel rebuild parity tests (Phase 11)
  parallel_merge.rs    parallel merge parity tests (Phase 12)
  cli.rs           CLI command integration tests
  bench_cli.rs     CLI bench command smoke tests

benches/
  engine.rs        criterion benchmarks: put/get, startup, merge
```
