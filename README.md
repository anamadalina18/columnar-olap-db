# Columnar OLAP Engine with Iceberg-style Metadata

A small analytical database in Rust: columnar storage, vectorized (chunk-at-a-time) operators and a
versioned, Iceberg-inspired metadata layer with column-level statistics.

**Context.** This is my implementation for **Lab 3** of **Scalable Data Management Systems**
(TU Darmstadt, FG Data Management, 2025/26, Erasmus+ year). The course provided the project skeleton:
the `Value`/`TableChunk`/operator traits, file-storage plumbing, benchmarks and the test suite.
This repository contains **only the files I implemented**, so it does not build standalone.

## What I implemented

**Storage format (`storage/data.rs`).** Serialization and deserialization of data files: an 8-byte
magic header, row/column counts, and a per-column descriptor (type + byte offset) followed by the
column values. Two read paths are implemented — full parse and **projected parse** for a subset of
columns, which only reads the requested columns.

**Vectorized operators (`engine/operators.rs`).** Chunk-at-a-time execution with:

- `ColumnTableScan` — scans a set of data files with column projection;
- `ColumnFilter` — conjunctive predicate evaluation over chunks;
- `ColumnAggregate` — pluggable `AggFunc` over a column;
- `ColumnEqJoin` — hash join on a single column.

**Iceberg-style metadata.**

- Column `MinMax` statistics and `FileStats` (`iceberg/stats.rs`), including overlap checks for
  equality and range predicates;
- `Manifest` (`iceberg/manifest.rs`) — added/deleted file lists per version plus the stats of added
  files, and iterator-returning predicate filtering (`contains`, `contains_range`);
- `TableMetadata` (`iceberg/table_metadata.rs`) — snapshot at an arbitrary `Version`, effective file
  set (added minus deleted, applied manifest by manifest), predicate-based pruning at a version, and
  version creation.

**Write path (`engine/db_engine.rs`).** `start_table_modification` / `commit` plus
`insert`, `update`, `delete` and `delete_chunks`: chunks are rewritten to new files, statistics are
recomputed, changes are recorded in the manifest, and conflicting or duplicate modifications of the
same file inside one version are rejected.

**Value semantics (`value_cmp.rs`).** `PartialOrd`/`Ord` for `Value` across `Int`, `UInt`, `RowID`
and `Varchar`.

## Files

| Path | Contents |
|---|---|
| `src/storage/data.rs` | Data-file format, `to_bytes`, `parse`, `parse_columns` |
| `src/engine/operators.rs` | Table scan, filter, aggregate and hash join operators |
| `src/engine/db_engine.rs` | Versioned insert/update/delete/delete_chunks and commit |
| `src/iceberg/stats.rs` | `MinMax` and `FileStats` predicate overlap checks |
| `src/iceberg/manifest.rs` | Manifest with added/deleted files and stats |
| `src/iceberg/table_metadata.rs` | Snapshots, effective file set, predicate pruning, versioning |
| `src/value_cmp.rs` | Ordering for `Value` |

## Notes

- The engine keeps one metadata version per committed modification, so a table can be queried at any
  past version.
- The data-file layout is also exercised by the course benchmarks (`query_bench`), where file pruning
  via statistics is the main win.
