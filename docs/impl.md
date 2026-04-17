---
header-includes:
  - \usepackage{geometry}
  - \usepackage{graphicx}
  - \usepackage{array}
  - \usepackage{tabularx}
  - \usepackage{booktabs}
  - \usepackage{listings}
  - \usepackage{xcolor}
  - \usepackage{fancyhdr}
  - \usepackage{setspace}
  - |
    \definecolor{codebg}{RGB}{30,30,30}
    \definecolor{codefg}{RGB}{220,220,220}
    \definecolor{codecomment}{RGB}{106,153,85}
    \definecolor{codekeyword}{RGB}{86,156,214}
    \definecolor{codestring}{RGB}{206,145,120}
    \definecolor{codenumber}{RGB}{181,206,168}
    \definecolor{linkblue}{RGB}{30,100,200}
    \lstset{
      backgroundcolor=\color{codebg},
      basicstyle=\ttfamily\footnotesize\color{codefg},
      keywordstyle=\color{codekeyword}\bfseries,
      commentstyle=\color{codecomment}\itshape,
      stringstyle=\color{codestring},
      breaklines=true,
      breakatwhitespace=true,
      frame=single,
      framerule=0.5pt,
      rulecolor=\color{gray!50},
      xleftmargin=1em,
      xrightmargin=0.5em,
      aboveskip=0.8em,
      belowskip=0.8em,
      showstringspaces=false,
      numbers=left,
      numberstyle=\tiny\color{gray},
      numbersep=8pt,
      tabsize=4
    }
  - |
    \pagestyle{fancy}
    \fancyhf{}
    \fancyhead[L]{\small\textit{BitDB --- Implementation}}
    \fancyhead[R]{\small\textit{Parallel Computing Laboratory}}
    \fancyfoot[C]{\thepage}
    \renewcommand{\headrulewidth}{0.4pt}
  - |
    \AtBeginDocument{
      \let\oldtoc\tableofcontents
      \renewcommand{\tableofcontents}{
        \pagestyle{empty}
        \oldtoc
        \cleardoublepage
        \pagestyle{fancy}
        \setcounter{page}{1}
      }
    }
geometry: "top=1in, bottom=1in, left=1.25in, right=1.25in"
papersize: a4
fontsize: 12pt
numbersections: true
toc: true
toc-depth: 3
colorlinks: true
linkcolor: blue
urlcolor: blue
---

\newpage

# Overview

BitDB is a persistent key-value store in Rust modelled on the Bitcask
storage engine. The entire codebase lives at:

\begin{center}
\href{https://github.com/kottesh/bitdb}{\texttt{https://github.com/kottesh/bitdb}}
\end{center}

The workspace contains two crates:

| Crate | Path | Purpose |
|---|---|---|
| `bitdb` | `bitdb/` | Storage engine, CLI, TUI |
| `tracer` | `tracer/` | Parallel-execution visualiser |

The central idea is simple: on startup, every data file on disk must be
scanned to rebuild the in-memory index. In the original Bitcask design
this is sequential. BitDB makes it parallel using Rayon's work-stealing
thread pool, achieving a **5.69× speedup** on a 5 million-key, 1.3 GB
dataset.

\newpage

# Project Structure

```
bitdb/
  bitdb/
    src/
      config.rs        Options, Parallelism, CorruptionPolicy
      engine.rs        Engine: open/get/put/delete/merge/stats
      record.rs        Binary record format + CRC32 encode/decode
      recovery.rs      Serial and parallel KeyDir rebuild
      merge.rs         Serial and parallel compaction pipeline
      error.rs         BitdbError enum + Result alias
      storage/
        data_file.rs   DataFile: append-only writer + seek reader
        file_set.rs    FileSet: multi-file manager
        hint_file.rs   Hint file write/read (compact index sidecars)
      index/
        keydir.rs      KeyDir: HashMap<Vec<u8>, KeyDirEntry>
      cli.rs           CLI subcommands (put/get/delete/stats/merge/bench)
      tui/             Interactive terminal session
    benches/
      engine.rs        Criterion benchmarks
    tests/             Integration tests
  tracer/
    src/
      dataset.rs       Synthetic dataset generator
      worker.rs        Instrumented parallel scan + LiveProgress
      tui/
        setup.rs       Setup screen
        generate.rs    Generation progress screen
        live.rs        Live side-by-side progress screen
        result.rs      Final result screen
  docs/
    report.md          Project report
    impl.md            This document
    slides.md          Beamer presentation
    attachments/       Screenshots
  Justfile             Task runner
  flake.nix            Nix dev shell
```

\newpage

# Configuration (`config.rs`)

All engine behaviour is controlled by a single `Options` struct.

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum Parallelism {
    #[default]
    Auto,           // rayon uses all logical CPUs
    Fixed(usize),   // exactly n worker threads
    Serial,         // single-threaded, no rayon
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum CorruptionPolicy {
    #[default]
    Fail,                // return error on any corrupt record
    SkipCorruptedTail,   // stop scanning at first corruption
}

#[derive(Clone, Debug)]
pub struct Options {
    pub create_if_missing:         bool,
    pub max_data_file_size_bytes:  u64,
    pub corruption_policy:         CorruptionPolicy,
    pub parallelism:               Parallelism,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            create_if_missing:        true,
            max_data_file_size_bytes: 1024 * 1024,  // 1 MB
            corruption_policy:        CorruptionPolicy::Fail,
            parallelism:              Parallelism::Auto,
        }
    }
}
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/config.rs}{\texttt{bitdb/src/config.rs}}

\newpage

# Record Format (`record.rs`)

Every value written to disk is wrapped in a self-describing record.
The binary layout is:

\begin{center}
\begin{tabular}{|c|c|l|}
\hline
\textbf{Bytes} & \textbf{Field} & \textbf{Description} \\
\hline
0--3   & Magic     & \texttt{0x42444231} --- identifies a BitDB record \\
4      & Version   & \texttt{0x01} \\
5      & Flags     & \texttt{0x00} = Normal, \texttt{0x01} = Tombstone \\
6--9   & CRC32     & Checksum (see below) \\
10--17 & Timestamp & Unix seconds, little-endian u64 \\
18--21 & key\_len  & Key length, little-endian u32 \\
22--25 & value\_len & Value length, little-endian u32 \\
26\ldots & Key    & Raw key bytes \\
26+key\_len\ldots & Value & Raw value bytes \\
\hline
\end{tabular}
\end{center}

The CRC32 is computed over `bytes[0..6]` (header before the CRC
field) and `bytes[10..]` (timestamp + lengths + key + value).
This detects both header and body corruption with one checksum.

```rust
pub fn encode(record: &Record) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        HEADER_LEN + record.key.len() + record.value.len()
    );
    out.extend_from_slice(&RECORD_MAGIC.to_le_bytes()); // 0..4
    out.push(RECORD_VERSION);                           // 4
    out.push(record.flags.to_u8());                     // 5
    out.extend_from_slice(&0u32.to_le_bytes());         // 6..10 (CRC placeholder)
    out.extend_from_slice(&record.timestamp.to_le_bytes());
    out.extend_from_slice(&(record.key.len()   as u32).to_le_bytes());
    out.extend_from_slice(&(record.value.len() as u32).to_le_bytes());
    out.extend_from_slice(&record.key);
    out.extend_from_slice(&record.value);

    // Fill in CRC after the full record is assembled.
    let crc = compute_crc(&out);
    out[6..10].copy_from_slice(&crc.to_le_bytes());
    out
}

fn compute_crc(input: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&input[..6]);    // header before CRC
    hasher.update(&input[10..]);   // everything after CRC
    hasher.finalize()
}
```

Decoding reads the magic, version, flags, and CRC first, then
verifies the checksum before returning the key/value bytes.
A `TruncatedRecord` error is returned when the buffer is too
short to hold the full record -- this is the normal condition
at the tail of a file after a crash.

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/record.rs}{\texttt{bitdb/src/record.rs}}

\newpage

# Storage Layer

## DataFile (`storage/data_file.rs`)

`DataFile` wraps a single append-only file on disk. Writes always
go to the end; reads seek to an explicit byte offset.

```rust
pub struct DataFile {
    id:     u32,
    path:   PathBuf,
    writer: File,   // opened with O_APPEND
    len:    u64,    // tracks current file size without extra stat() calls
}

impl DataFile {
    pub fn append(&mut self, record: &Record) -> Result<(u64, usize)> {
        let encoded = record::encode(record);
        let offset = self.len;
        self.writer.write_all(&encoded)?;
        self.len += encoded.len() as u64;
        Ok((offset, encoded.len()))
    }

    pub fn read_at(path: &Path, offset: u64) -> Result<DecodeResult> {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        record::decode_one(&buf)
    }
}

// Files are named  00000001.data, 00000002.data, ...
pub fn data_file_path(dir: &Path, id: u32) -> PathBuf {
    dir.join(format!("{id:08}.data"))
}
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/storage/data_file.rs}{\texttt{bitdb/src/storage/data\_file.rs}}

## FileSet (`storage/file_set.rs`)

`FileSet` manages a directory of numbered data files. It tracks
all known file IDs in a `BTreeSet` (always sorted) and keeps one
active `DataFile` open for appending.

```rust
pub struct FileSet {
    dir:             PathBuf,
    options:         Options,
    known_file_ids:  BTreeSet<u32>,   // sorted set of all data file IDs
    active:          DataFile,        // current write target
}
```

When an `append` would push the active file over
`max_data_file_size_bytes`, a new file with `id = active_id + 1`
is created and becomes the new active file:

```rust
pub fn append(&mut self, record: &Record) -> Result<RecordLocation> {
    let encoded_len = record::encode(record).len() as u64;

    if !self.active.is_empty()
        && self.active.len().saturating_add(encoded_len)
            > self.options.max_data_file_size_bytes
    {
        let next_id = self.active.id() + 1;
        self.active = DataFile::open_append(&self.dir, next_id)?;
        self.known_file_ids.insert(next_id);
    }

    let (offset, size_bytes) = self.active.append(record)?;
    Ok(RecordLocation {
        file_id:    self.active.id(),
        offset,
        size_bytes: size_bytes as u32,
    })
}
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/storage/file_set.rs}{\texttt{bitdb/src/storage/file\_set.rs}}

## Hint Files (`storage/hint_file.rs`)

A hint file is a compact sidecar written alongside each data file
after a merge. It contains only the KeyDir metadata for every record
in that data file -- no values -- so startup can skip decoding the
full records.

**Hint file binary layout:**

\begin{center}
\begin{tabular}{|c|c|l|}
\hline
\textbf{Bytes} & \textbf{Field} & \textbf{Description} \\
\hline
0--3  & Magic   & \texttt{0x48494E54} (\texttt{HINT}) \\
4     & Version & \texttt{0x01} \\
5--8  & Count   & Number of entries, little-endian u32 \\
\multicolumn{3}{|c|}{\textit{Repeated per entry:}} \\
+0--3  & key\_len    & u32 \\
+4--7  & file\_id    & u32 \\
+8--15 & offset      & u64 \\
+16--19 & size\_bytes & u32 \\
+20--27 & timestamp  & u64 \\
+28    & is\_tombstone & u8 (0 or 1) \\
+29\ldots & key      & raw bytes \\
\hline
\end{tabular}
\end{center}

```rust
pub fn write_hint_file(path: &Path, entries: &[HintEntry]) -> Result<()> {
    let mut out = Vec::new();
    out.extend_from_slice(&HINT_MAGIC.to_le_bytes());
    out.push(HINT_VERSION);
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());

    for entry in entries {
        out.extend_from_slice(&(entry.key.len() as u32).to_le_bytes());
        out.extend_from_slice(&entry.file_id.to_le_bytes());
        out.extend_from_slice(&entry.offset.to_le_bytes());
        out.extend_from_slice(&entry.size_bytes.to_le_bytes());
        out.extend_from_slice(&entry.timestamp.to_le_bytes());
        out.push(if entry.is_tombstone { 1 } else { 0 });
        out.extend_from_slice(&entry.key);
    }

    let mut file = File::create(path)?;
    file.write_all(&out)?;
    file.sync_data()?;
    Ok(())
}
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/storage/hint_file.rs}{\texttt{bitdb/src/storage/hint\_file.rs}}

\newpage

# KeyDir (`index/keydir.rs`)

The KeyDir is the in-memory index: a `HashMap` mapping every live key
to where its latest value lives on disk.

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct KeyDirEntry {
    pub file_id:      u32,   // which data file
    pub offset:       u64,   // byte offset within that file
    pub size_bytes:   u32,   // total encoded record size
    pub timestamp:    u64,   // Unix seconds from record header
    pub is_tombstone: bool,  // true = key is deleted
}

#[derive(Clone, Debug, Default)]
pub struct KeyDir {
    entries: HashMap<Vec<u8>, KeyDirEntry>,
}
```

A `get` is one hash lookup followed by one `pread`-style seek ---
O(1) with no metadata reads.

```rust
// Engine::get
pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
    let Some(entry) = self.keydir.get(key) else {
        return Ok(None);
    };
    if entry.is_tombstone {
        return Ok(None);
    }
    let decoded = self.file_set.read_at(entry.file_id, entry.offset)?;
    Ok(Some(decoded.record.value))
}
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/index/keydir.rs}{\texttt{bitdb/src/index/keydir.rs}}

\newpage

# Engine (`engine.rs`)

`Engine` is the public API. It owns a `FileSet` and a `KeyDir`
and coordinates all operations.

```rust
pub struct Engine {
    data_dir:  Box<Path>,
    options:   Options,
    file_set:  FileSet,
    keydir:    KeyDir,
}
```

## Open

```rust
pub fn open(data_dir: &Path, options: Options) -> Result<Self> {
    let file_set = FileSet::open(data_dir, &options)?;
    let keydir = rebuild_keydir(
        &file_set,
        options.corruption_policy,
        options.parallelism,   // Serial / Auto / Fixed(n)
    )?;
    Ok(Self { data_dir: data_dir.into(), options, file_set, keydir })
}
```

## Put

```rust
pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
    let record = Record::new(
        unix_timestamp_secs(), key.to_vec(), value.to_vec(),
        RecordFlags::Normal,
    );
    let location = self.file_set.append(&record)?;
    self.keydir.insert(key.to_vec(), KeyDirEntry {
        file_id:      location.file_id,
        offset:       location.offset,
        size_bytes:   location.size_bytes,
        timestamp:    record.timestamp,
        is_tombstone: false,
    });
    Ok(())
}
```

## Delete

Deletion writes a **tombstone** record to disk. The key is not removed
from the KeyDir -- it is marked `is_tombstone = true`. The actual
reclamation of disk space happens during merge.

```rust
pub fn delete(&mut self, key: &[u8]) -> Result<()> {
    let record = Record::new(
        unix_timestamp_secs(), key.to_vec(), vec![],
        RecordFlags::Tombstone,
    );
    let location = self.file_set.append(&record)?;
    self.keydir.insert(key.to_vec(), KeyDirEntry {
        file_id:      location.file_id,
        offset:       location.offset,
        size_bytes:   location.size_bytes,
        timestamp:    record.timestamp,
        is_tombstone: true,
    });
    Ok(())
}
```

## Merge

```rust
pub fn merge(&mut self) -> Result<()> {
    run_merge(self.data_dir(), &self.keydir,
              &self.file_set, &self.options)?;
    // Reopen after install
    self.file_set = FileSet::open(self.data_dir(), &self.options)?;
    self.keydir = rebuild_keydir(
        &self.file_set,
        self.options.corruption_policy,
        self.options.parallelism,
    )?;
    Ok(())
}
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/engine.rs}{\texttt{bitdb/src/engine.rs}}

\newpage

# Startup Rebuild (`recovery.rs`)

This is the core parallel operation. On `Engine::open`, every data
file must be scanned to reconstruct the KeyDir.

## Serial Path

```rust
fn rebuild_serial(
    file_set: &FileSet,
    policy: CorruptionPolicy,
) -> Result<KeyDir> {
    let mut keydir = KeyDir::default();
    for file_id in file_set.file_ids_oldest_to_newest() {
        let entries = scan_file(file_set, file_id, policy)?;
        for (_, _, entry, key) in entries {
            keydir.insert(key, entry);
        }
    }
    Ok(keydir)
}
```

Files are processed strictly oldest-to-newest so the last write for
any key always wins without extra bookkeeping.

## Parallel Path

```rust
fn rebuild_parallel(
    file_set: &FileSet,
    policy: CorruptionPolicy,
    thread_count: Option<usize>,
) -> Result<KeyDir> {
    // Pre-compute owned (file_id, data_path, hint_path) triples so
    // rayon workers can move them without borrowing file_set.
    let file_info: Vec<(u32, PathBuf, Option<PathBuf>)> =
        file_set.file_ids_oldest_to_newest()
            .iter()
            .map(|&id| (id,
                file_set.file_path(id).unwrap(),
                file_set.hint_path(id)))
            .collect();

    // Each file is scanned independently on a rayon worker.
    let scan_results: Vec<Result<Vec<ScanEntry>>> = match thread_count {
        None    => file_info.par_iter().map(run_scan).collect(),
        Some(n) => match ThreadPoolBuilder::new().num_threads(n).build() {
            Ok(pool) => pool.install(||
                file_info.par_iter().map(run_scan).collect()),
            Err(_)   => file_info.iter().map(run_scan).collect(),
        },
    };

    // Flatten all per-file entry lists.
    let mut all_entries: Vec<ScanEntry> = Vec::new();
    for result in scan_results {
        all_entries.extend(result?);
    }

    // Sort by (file_id ASC, offset ASC) to restore deterministic order.
    // Without this sort, a thread finishing a newer file first would
    // insert a newer value that could later be overwritten by an older
    // value from a slower thread -- corrupting the KeyDir.
    all_entries.sort_unstable_by_key(|(fid, off, _, _)| (*fid, *off));

    let mut keydir = KeyDir::default();
    for (_, _, entry, key) in all_entries {
        keydir.insert(key, entry);
    }
    Ok(keydir)
}
```

## File Scan (hint-first)

Each file scan checks for a hint file first. If a hint file exists,
only KeyDir metadata is read (no value bytes) -- much faster for
large values.

```rust
fn scan_file_by_path(
    file_id: u32,
    data_path: &PathBuf,
    hint_path: Option<&Path>,
    policy: CorruptionPolicy,
) -> Result<Vec<ScanEntry>> {
    // Prefer hint file if available.
    if let Some(hp) = hint_path
        && hp.exists()
        && let Ok(entries) = read_hint_file(hp)
    {
        return Ok(entries.into_iter().map(|h| {
            let key = h.key.clone();
            let entry = h.to_keydir_entry();
            (file_id, entry.offset, entry, key)
        }).collect());
    }
    // Fall back to full data file decode.
    scan_data_file(file_id, data_path, policy)
}
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/recovery.rs}{\texttt{bitdb/src/recovery.rs}}

\newpage

# Merge / Compaction (`merge.rs`)

Merge rewrites only the latest non-tombstone value for each key into
a clean set of data files, then atomically replaces the old files.

## Pipeline

```
┌─────────────────────────────────┐
│  Collect live KeyDir entries     │  (serial, in memory)
└──────────────┬──────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│  Load values from disk          │  PARALLEL (rayon par_iter)
│  Each entry: read_at(fid,off)   │  independent disk seeks
└──────────────┬──────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│  Write merged output            │  serial (single FileSet writer)
│  + write hint files             │
└──────────────┬──────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│  Atomic install                 │  rename tmp/ -> data dir
└─────────────────────────────────┘
```

## Parallel Read Phase

```rust
fn load_records_parallel(
    entries: &[(&[u8], &KeyDirEntry)],
    file_set: &FileSet,
    thread_count: Option<usize>,
) -> Result<Vec<LiveRecord>> {
    // Pre-compute owned paths -- FileSet is not Sync so workers
    // cannot borrow it across thread boundaries.
    let tasks: Vec<(Vec<u8>, KeyDirEntry, PathBuf)> = entries
        .iter()
        .map(|(key, entry)| {
            let path = file_set.file_path(entry.file_id).unwrap();
            (key.to_vec(), **entry, path)
        })
        .collect();

    // Each task reads exactly one record from disk -- fully independent.
    let results: Vec<Result<LiveRecord>> = match thread_count {
        None    => tasks.par_iter().map(load_one_by_path).collect(),
        Some(n) => match ThreadPoolBuilder::new().num_threads(n).build() {
            Ok(pool) => pool.install(||
                tasks.par_iter().map(load_one_by_path).collect()),
            Err(_)   => tasks.iter().map(load_one_by_path).collect(),
        },
    };

    results.into_iter().collect()
}

fn load_one_by_path(
    task: &(Vec<u8>, KeyDirEntry, PathBuf)
) -> Result<LiveRecord> {
    let (key, entry, path) = task;
    let decoded = DataFile::read_at(path, entry.offset)?;
    Ok(LiveRecord {
        key:       key.clone(),
        value:     decoded.record.value,
        timestamp: entry.timestamp,
    })
}
```

## Atomic Install

```rust
// Remove old .data and .hint files
remove_data_and_hint_files(data_dir)?;
// Move every file from .merge_tmp/ into data_dir
install_merged_files(&merge_dir, data_dir)?;
fs::remove_dir_all(&merge_dir)?;
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/merge.rs}{\texttt{bitdb/src/merge.rs}}

\newpage

# Tracer (`tracer/`)

The tracer is a standalone TUI that makes the parallel execution
visible by running instrumented serial and parallel scans on a
generated dataset and showing per-thread, per-file progress bars
in real time.

## Dataset Generator (`dataset.rs`)

```rust
pub fn generate(
    data_dir: &Path,
    params: &DatasetParams,
    progress: Arc<Mutex<GenerateProgress>>,
) -> io::Result<()> {
    let opts = Options {
        max_data_file_size_bytes: params.file_size_bytes,
        ..Options::default()
    };
    let mut engine = Engine::open(data_dir, opts)?;
    // Fixed seed -- same dataset on every machine
    let mut rng = StdRng::seed_from_u64(42);
    let mut value_buf = vec![0u8; params.value_size];

    for i in 0..params.keys {
        let key = format!("key:{i:08}");
        rng.fill_bytes(&mut value_buf);
        engine.put(key.as_bytes(), &value_buf)?;
        progress.lock().unwrap().keys_written = i + 1;
    }
    write_meta(data_dir, params)?; // tracer_meta.json
    Ok(())
}
```

## LiveProgress and Worker (`worker.rs`)

The `LiveProgress` type is an `Arc<Mutex<Vec<ThreadState>>>`.
Workers hold the lock only briefly (for a single slot update),
so the TUI render loop (16 ms ticks) can always acquire it without
visible blocking.

```rust
pub type LiveProgress = Arc<Mutex<Vec<ThreadState>>>;

pub fn run_scan(
    data_dir: &Path,
    thread_count: usize,
    live: Option<&LiveProgress>,
) -> io::Result<RunResult> {
    // Build initial assignment, fill into the shared arc
    let mut assignment = assign_files(&file_ids, thread_count);
    // ... fill file sizes ...
    let shared: Arc<Mutex<Vec<ThreadState>>> = if let Some(lp) = live {
        *lp.lock().unwrap() = assignment;
        lp.clone()
    } else {
        Arc::new(Mutex::new(assignment))
    };

    // Scan all files across threads in parallel
    tasks.par_iter().enumerate().for_each(|(t_idx, files)| {
        for (s_idx, (_, path, _)) in files.iter().enumerate() {
            // Mark slot as Processing
            shared.lock().unwrap()[t_idx].slots[s_idx].state =
                SlotState::Processing { bytes_done: 0, keys_found: 0 };

            let bytes = std::fs::read(path).unwrap_or_default();
            let mut offset = 0;
            let mut keys_found = 0;
            let mut since_last_report = 0;

            while offset < bytes.len() {
                match decode_one(&bytes[offset..]) {
                    Ok(decoded) => {
                        offset += decoded.bytes_read;
                        keys_found += 1;
                        since_last_report += 1;
                        // Report progress every 500 records
                        if since_last_report >= 500 {
                            since_last_report = 0;
                            shared.lock().unwrap()[t_idx].slots[s_idx].state =
                                SlotState::Processing {
                                    bytes_done: offset as u64,
                                    keys_found,
                                };
                        }
                    }
                    Err(_) => break,
                }
            }

            // Mark slot as Done
            shared.lock().unwrap()[t_idx].slots[s_idx].state =
                SlotState::Done { duration_us, keys_found, bytes_read };
        }
    });
    // ...
}
```

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/tracer/src/worker.rs}{\texttt{tracer/src/worker.rs}}

## TUI Screens

The tracer has four screens managed by a simple `Screen` enum:

| Screen | File | Description |
|---|---|---|
| Setup | `tui/setup.rs` | Configure keys, value size, file size, threads |
| Generating | `tui/generate.rs` | Live fill-bar for dataset write progress |
| Live | `tui/live.rs` | Side-by-side serial vs parallel progress bars |
| Result | `tui/result.rs` | Final wall-clock time, keys/sec, and speedup |

\newpage

# Screenshots

## Live Screen --- Side-by-Side View

\begin{center}
\includegraphics[width=0.95\textwidth,keepaspectratio]{tracer_1.png}
\end{center}

The left column shows the single serial thread scanning all 2,839 files.
The right column shows 7 parallel threads, each assigned a round-robin
subset of files. Green bars indicate completed files.

## Live Screen --- Thread Toggle View

\begin{center}
\includegraphics[width=0.95\textwidth,keepaspectratio]{tracer_2.png}
\end{center}

The parallel column supports per-thread collapsing. A collapsed header
shows `> Thread N  (done/total files)`.

## Result Screen

\begin{center}
\includegraphics[width=0.95\textwidth,keepaspectratio]{tracer_result.png}
\end{center}

Serial: 5605 ms at 923,021 keys/sec. Parallel (7 threads): 985 ms at
5,252,224 keys/sec. Speedup: **5.69x**.

\newpage

# Benchmarks

## Criterion Results

Run with:

```
cargo bench --bench engine -- merge_pipeline startup_rebuild
```

### Startup Rebuild

| Mode | Median | CI |
|---|---|---|
| `serial` | ~180 µs | tight |
| `parallel_auto` | ~120 µs | tight |

The criterion dataset uses 2000 keys (small). The real-world speedup
shows on large datasets -- see tracer results above (5.69×).

### Merge Pipeline

\begin{center}
\includegraphics[width=0.95\textwidth,keepaspectratio]{merge_bench.png}
\end{center}

| Mode | Median | Speedup |
|---|---|---|
| `serial` | 6.31 ms | 1.00× |
| `parallel_auto` | 3.16 ms | **~2×** |

500 unique keys + 100 overwrites. Speedup is modest on small data
due to thread-pool overhead. Scales with dataset size.

## Benchmark Source

Full benchmark code:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/benches/engine.rs}{\texttt{bitdb/benches/engine.rs}}

\newpage

# Error Handling (`error.rs`)

All fallible operations return `Result<T>` using a custom error type:

```rust
#[derive(Debug, Error)]
pub enum BitdbError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("record truncated")]
    TruncatedRecord,

    #[error("invalid record magic: {0:#x}")]
    InvalidRecordMagic(u32),

    #[error("invalid record version: {0}")]
    InvalidRecordVersion(u8),

    #[error("invalid record flags: {0}")]
    InvalidRecordFlags(u8),

    #[error("record checksum mismatch: expected={expected:#x}, actual={actual:#x}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("data file not found: {0}")]
    DataFileNotFound(u32),

    #[error("invalid hint file")]
    InvalidHintFile,
}

pub type Result<T> = std::result::Result<T, BitdbError>;
```

`TruncatedRecord` is the expected end-of-file condition and is
handled silently in all scan loops. The `CorruptionPolicy` controls
what happens on `ChecksumMismatch`, `InvalidRecordMagic` etc:
`Fail` propagates the error; `SkipCorruptedTail` stops scanning and
moves on.

Full source:
\href{https://github.com/kottesh/bitdb/blob/master/bitdb/src/error.rs}{\texttt{bitdb/src/error.rs}}

\newpage

# Summary

BitDB implements the Bitcask storage model in Rust: an append-only log
for writes and a fully in-memory index (KeyDir) for O(1) reads. Every
value lives at a fixed offset in a numbered data file; a get is one
hash lookup plus one seek, with no tree traversal or B-tree page splits.

The project's parallel contribution is in two places. First, startup
rebuild: on `Engine::open`, all data files are scanned in parallel
using Rayon's work-stealing thread pool. Each file is an independent
unit of work, so threads never contend on the scan itself. After all
files are scanned, the combined entry list is sorted by `(file_id,
offset)` before insertion into the KeyDir, which restores the
deterministic serial ordering and guarantees the last write always wins.
On a 5 million-key, 1.3 GB dataset this delivers a **5.69× speedup**
(5605 ms serial → 985 ms parallel on 7 threads).

Second, merge compaction: the read phase that loads live values from
disk before rewriting them is also parallelised with `par_iter`. Each
record read is a fully independent disk seek, so the parallel speedup
scales cleanly with I/O concurrency. The Criterion benchmark shows a
**~2× speedup** on 500 keys with parallelism overhead included;
real-world datasets see larger gains.

Hint files act as a compact fast-path: after each merge, the engine
writes a sidecar `.hint` file containing only KeyDir metadata (no
value bytes). On the next startup, the parallel scan reads hint files
instead of full data files, dramatically reducing I/O for large-value
workloads.

The tracer crate makes all of this visible. It generates a synthetic
dataset, runs both paths with instrumented workers, and renders a
real-time split-screen TUI showing per-thread, per-file progress bars
and a final comparison of wall-clock time, keys/sec, and speedup.

\vspace{\fill}

\noindent\rule{\textwidth}{0.4pt}

\begingroup
\normalsize
\setlength{\parindent}{0pt}
\vspace{2pt}
\noindent\textbf{References}
\vspace{2pt}

\noindent [1] T. Sheehy and D. Smith, \textit{Bitcask: A Log-Structured Hash Table for Fast Key/Value Data}, Basho Technologies, 2010.

\noindent [2] Rayon Contributors, \textit{Rayon: Data-parallelism library for Rust}, \url{https://github.com/rayon-rs/rayon}, 2015--present.

\noindent [3] B. Pereira, \textit{Criterion.rs: Statistics-driven micro-benchmarking in Rust}, \url{https://github.com/bheisler/criterion.rs}, 2014--present.

\noindent [4] S. Klabnik and C. Nichols, \textit{The Rust Programming Language}, No Starch Press, 2019. \url{https://doc.rust-lang.org/book/}

\noindent [5] Ratatui Contributors, \textit{Ratatui: Terminal UI library for Rust}, \url{https://github.com/ratatui-org/ratatui}, 2023--present.
\endgroup
