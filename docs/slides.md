---
title: "BitDB — A Bitcask-Style Key-Value Store"
subtitle: "Parallel Computing Laboratory (20MSSL02)"
author: "Shree Kottes J (7176 22 31 050)"
date: "April 2026"
theme: default
navigation: horizontal
fonttheme: professionalfonts
fontsize: 11pt
header-includes:
  - \usepackage{tikz}
  - \usepackage{graphicx}
  - \usepackage{hyperref}
  - \graphicspath{{./docs/attachments/}}
  - \usenavigationsymbolstemplate{\insertslidenavigationsymbol\insertframenavigationsymbol\insertsubsectionnavigationsymbol\insertsectionnavigationsymbol\insertdocnavigationsymbol\insertbackfindforwardnavigationsymbol}
  - \setbeamerfont{title}{size=\normalsize,series=\mdseries}
  - \setbeamerfont{subtitle}{size=\small}
---

# Problem Statement

## The Cold-Start Problem in Key-Value Stores

- Disk-based KV stores lose their in-memory index on every restart
- On startup, the **entire dataset must be scanned** to rebuild the index
- With millions of records across hundreds of files, this is **sequential I/O**
- A 1 M-key database at default settings requires decoding every record — **one file at a time**

> **Cold-start latency grows linearly with dataset size — can we do better?**

---

# Existing Approach

## Bitcask — The Original Design

Bitcask (Riak, 2010) introduced the *append-only + in-memory hash index* model:

\medskip

\begin{tabular}{p{2.2cm} p{7.2cm}}
\textbf{Concept} & \textbf{How it works} \\
\hline
Data files  & Append-only segments; never modified in place \\
KeyDir      & In-memory \texttt{HashMap<key -> (file\_id, offset, size)>} \\
Reads       & O(1) — one hash lookup + one disk seek \\
Writes      & O(1) — append to active file \\
Startup     & Scan \textbf{all} data files top-to-bottom, \textbf{sequentially} \\
Hint files  & Optional index sidecars to skip full record decoding \\
\end{tabular}

\medskip

**The bottleneck:** startup rebuild is strictly serial — file N cannot start until file N-1 finishes.

---

# What We Are Solving

## Serial Startup Rebuild is Unnecessarily Slow

Each data file is **independent** — records in `data.0003` have no dependency on `data.0002`.

\medskip

Yet the classic implementation processes them one at a time:

```
Thread 0: [f0]->[f1]->[f2]->[f3]->  done
```

\medskip

**Observation:** File scanning is embarrassingly parallel.

- No shared mutable state between files during the read phase
- The only ordering constraint is in the *merge step* — last writer wins per key
- Same applies to **compaction (merge)** — reading live values from disk

---

# Our Solution

## Parallel KeyDir Rebuild with Rayon

Split the work across all available CPU cores:

```
Thread 0: [f0]-[f4]-[f8]  -> entries[]
Thread 1: [f1]-[f5]-[f9]  -> entries[]
Thread 2: [f2]-[f6]-[f10] -> entries[]
Thread 3: [f3]-[f7]-[f11] -> entries[]
                   |
           sort by (file_id, offset)
                   |
           single-pass KeyDir build
```

\medskip

**Correctness preserved:** entries are sorted by `(file_id ASC, offset ASC)` before insertion — identical outcome to the serial path; *last writer always wins*.

---

# Implementation

## Startup Rebuild (`recovery.rs`)

```rust
// Each file scanned independently on a rayon worker
file_info.par_iter().map(|f| scan_file(f)).collect()

// Sort by (file_id, offset) → deterministic KeyDir build
all_entries.sort_unstable_by_key(
    |(fid, off, ..)| (*fid, *off)
);
```

---

# Implementation (cont.)

## Merge / Compaction (`merge.rs`)

```rust
// Read phase: parallel value loading from disk
live_entries.par_iter().map(|e| load_value(e)).collect()
// Write phase: always serial (single FileSet writer)
```

\medskip

## Parallelism Config (`Options`)

\begin{tabular}{p{2.0cm} p{6.5cm}}
\textbf{Value} & \textbf{Behaviour} \\
\hline
\texttt{Serial}   & Single-threaded, deterministic \\
\texttt{Auto}     & Rayon uses all logical CPUs \\
\texttt{Fixed(n)} & Exactly \textit{n} threads \\
\end{tabular}

---

# The Tracer

## Visualising the Parallel Rebuild — Live

`tracer` is a companion TUI that makes the parallelism **visible**:

\medskip

- Generates a configurable dataset (keys, value size, file size)
- Runs **serial rebuild** then **parallel rebuild** on the same dataset
- Shows each thread's file assignments as live progress bars
- Displays wall-clock timing, per-file duration, and speedup in real time

\medskip

```
+-- SERIAL (1 thread) - done ----------------+
| v Thread 0                                 |
|   f:000 [####################] 5,714 keys  |
|   f:001 [####################] 5,714 keys  |
|   ...                                      |
+--------------------------------------------+
```

---

# Results — Tracer Screenshot

## Live Side-by-Side View

\begin{center}
\includegraphics[width=0.95\textwidth,height=0.75\textheight,keepaspectratio]{tracer_1.png}
\end{center}

---

# Results — Tracer Screenshot (2)

## Parallel Column — Thread Toggle View

\begin{center}
\includegraphics[width=0.95\textwidth,height=0.75\textheight,keepaspectratio]{tracer_2.png}
\end{center}

---

# Results — Startup Rebuild

## Serial vs Parallel: 5,000,000 keys across 2839 files

\begin{center}
\includegraphics[width=0.95\textwidth,height=0.75\textheight,keepaspectratio]{tracer_result.png}
\end{center}

---

# Results — Merge Pipeline

## Compaction: Serial vs Parallel Read Phase

\begin{center}
\includegraphics[width=0.95\textwidth,height=0.75\textheight,keepaspectratio]{merge_bench.png}
\end{center}

---

# Summary

## What Was Built

\medskip

**`bitdb`** — a production-quality Bitcask implementation in Rust

- Append-only storage, O(1) reads and writes
- Hint files for fast startup bypass
- Parallel startup rebuild and merge via Rayon
- Configurable parallelism: `Serial` / `Auto` / `Fixed(n)`
- Full test suite + Criterion benchmarks

\medskip

**`tracer`** — an interactive parallel visualiser

- Live per-thread, per-file progress bars
- Side-by-side serial vs parallel comparison
- Real-time speedup measurement

\medskip

> Parallel file scanning turns an O(N) sequential bottleneck into a task that scales with core count — with **zero change** to correctness or the on-disk format.

---

# {.plain}

\begin{center}
{\Large \usebeamercolor[fg]{title}\color{fg} Thank You}\\[1.5em]
Shree Kottes J\\
7176 22 31 050\\[1em]
\href{https://github.com/kottesh/bitdb}{\texttt{github.com/kottesh/bitdb}}\\[2em]
{\small Parallel Computing Laboratory (20MSSL02)}
\end{center}
