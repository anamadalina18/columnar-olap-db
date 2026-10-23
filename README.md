# Columnar OLAP Engine with Iceberg-style Metadata

A small analytical database in Rust: columnar storage, vectorized (chunk-at-a-time) operators and a
versioned, Iceberg-inspired metadata layer with column-level statistics.

**Context.** This is my implementation for **Lab 3** of **Scalable Data Management Systems**
(TU Darmstadt, FG Data Management, 2025/26, Erasmus+ year). The course provided the project skeleton:
the `Value`/`TableChunk`/operator traits, file-storage plumbing, benchmarks and the test suite.
This repository contains **only the files I implemented**, so it does not build standalone.

## Architecture

### On-disk layout

A table is a set of immutable data files, one per chunk. A file is self-describing:

```
magic "SDMS\x19\x03JS" | rows: u64 | columns: u64 | N × (type: 8B, start: 8B) | column data...
```

The header stores, for every column, its type and its byte offset, which is what makes **projection
pushdown** possible: `parse_columns` seeks straight to the requested columns and never reads the
others. Values inside a column are stored consecutively (`Vec<Value>`, with `Option` for nulls in a
chunk), so a scan hands whole columns to the next operator.

### Execution

Operators are pull-based and work on chunks, never on single rows:

```
ColumnTableScan → ColumnFilter → ColumnAggregate
                               → ColumnEqJoin
```

- `ColumnTableScan` iterates the file handles selected by the metadata layer, with a column
  projection mask.
- `ColumnFilter` evaluates conjunctive predicates over a chunk and emits the surviving rows.
- `ColumnAggregate` applies a boxed `AggFunc` to a column.
- `ColumnEqJoin` builds a hash table on one side and probes with the other.

### Metadata and versioning

`Catalog` holds `TableMetadata`, which holds an ordered `Vec<Manifest>` — one manifest per committed
modification, i.e. one per version. A manifest records `added` and `deleted` file handles plus the
`FileStats` of every added file.

Reading a version means replaying manifests from the beginning: add the files of each manifest, then
remove the ones it deleted. `TableMetadata::files(version)` does exactly that, so a snapshot of any
past version can be reconstructed without copying data.

Pruning happens at two levels: `TableMetadata::contains` / `contains_range` walk the manifests of the
requested version and keep only files whose statistics overlap the predicates; `Manifest::contains`
does the same per manifest. Statistics are `MinMax` per column, so a file is skipped when its
`[min, max]` does not intersect the predicate.

### Write path

`start_table_modification(table_id)` opens a modification; `insert`, `update`, `delete` and
`delete_chunks` accumulate changes into the current manifest; `commit()` appends the manifest and
increments the version. `insert` writes a new file per chunk and computes its stats; `update` and
`delete` read the affected chunk, rewrite it as a new file with fresh statistics, and record the old
file as deleted. Any attempt to modify the same file twice in one version is rejected with
`EngineError`, which keeps a manifest duplicate-free by construction.

## Limitations

- **Single-threaded, no concurrency control.** There is no transaction manager, no latch on the
  catalog, and no isolation between readers and writers — the engine is a lab prototype of the
  storage/metadata layer, not a DBMS.
- **Copy-on-write updates.** Updating one row rewrites the whole chunk to a new file; the same holds
  for row deletes. Write amplification is proportional to the chunk size.
- **No compaction.** Deleted files stay in the catalog history forever and old versions are never
  garbage-collected, so both metadata and storage grow with the number of modifications.
- **MinMax statistics only.** No null counts, no distinct counts, no bloom filters, and pruning is
  file-level — there are no finer row-group zone maps.
- **No compression or encoding.** Columns are stored raw (no dictionary, RLE or bit-packing), so
  files are larger and scans read more bytes than a real columnar format would.
- **Full-file materialization.** Projection pushdown reduces the number of columns read, but the
  selected columns are always read completely; there is no late materialization or vectorized
  decoding step.
- **No predicate pushdown into the reader.** Predicates are evaluated by `ColumnFilter`, not by the
  storage layer, so the scan cannot stop early.

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

- One manifest per committed modification keeps the history append-only: the effective file set is
  always a fold over the manifest list.
- The data-file layout and the pruning path are also exercised by the course benchmarks, where file
  pruning via statistics is the main win.
