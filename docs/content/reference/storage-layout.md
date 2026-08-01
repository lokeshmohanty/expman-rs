+++
title = "Storage Layout Reference"
description = "On-disk layout, data formats, YAML metadata schemas, and Parquet specs."
weight = 4
+++

# Reference — data model and on-disk layout

*Hand-written from `src/core/models.rs` and `src/core/storage.rs`, 2026-07-27.*

## Directory layout

```
<base_dir>/                              default "./experiments"
├── .projects/                           Project storage directory
│   └── <project_name>/                  Project directory
│       ├── project.yaml                 ProjectMetadata
│       └── README.md                    Markdown frontpage
└── <experiment_name>/                   ExperimentConfig::experiment_dir()
    ├── experiment.yaml                  ExperimentMetadata (created if absent)
    └── <run_name>/                      ExperimentConfig::run_dir()
        ├── run.yaml                     RunMetadata
        ├── config.yaml                  merged hyperparameters
        ├── vectors.parquet              metric rows
        ├── run.log                      engine log lines
        ├── console.log                  stdout/stderr tee (Python only)
        ├── artifacts/                   user files, recursive
        └── tensorboard/                 only if Experiment.tensorboard_dir used
```

`<run_name>` defaults to `chrono::Local::now().format("%Y%m%d_%H%M%S")`
(`models.rs:32`).

**Run discovery is directory-name based**, not index-driven. `list_runs`
(`storage.rs:59`) returns every subdirectory except literal `artifacts` and
`.ipynb_checkpoints`, sorted **descending** — which is newest-first only because
of the timestamp naming convention. `list_experiments` (`:27`) ignores all dot-directories
(e.g., `.projects`, `.ipynb_checkpoints`) and sorts ascending.

## Types

All derive `Debug, Clone, Serialize, Deserialize`. Note there is **no**
`Experiment`, `Run`, `Metric`, or `Artifact` struct — those are directories on
disk plus the metadata structs below.

### `ExperimentConfig` (`models.rs:10-25`)

Passed to the engine; **not itself persisted**.

| field | type | default |
|---|---|---|
| `name` | `String` | — |
| `run_name` | `String` | local timestamp |
| `base_dir` | `PathBuf` | — |
| `flush_interval_rows` | `usize` | `50` |
| `flush_interval_ms` | `u64` | `500` |
| `language` | `String` | `"rust"` (PyO3 sets `"python"`) |
| `env_path` | `Option<String>` | PyO3 fills from `sys.executable` |

Builder: `new(name, base_dir)`, `.with_run_name(run_name)`.

### `MetricValue` (`models.rs:58-63`)

`#[serde(untagged)]` — serializes as the bare scalar.

```rust
enum MetricValue { Float(f64), Int(i64), Bool(bool), Text(String) }
```

`From` impls for `f64, f32, i64, i32, usize, bool, String, &str`. **No
`From<u64>`.** From Python, `Int`/`Bool` are effectively unreachable — see
[architecture](/architecture/#python-bridge).

### `VectorRow` (`models.rs:119-123`)

`step: Option<u64>`, `timestamp: DateTime<Utc>`, `values: HashMap<String, MetricValue>`.

### `RunStatus` (`models.rs:138-143`)

`#[serde(rename_all = "UPPERCASE")]` → `RUNNING | FINISHED | FAILED | CRASHED`.

### Files in a run directory

| file | what |
|---|---|
| `vectors.parquet` | compacted metrics, written at close |
| `vectors-NNNN.arrow` | **live** append-only segments; folded into the Parquet at close |
| `system.parquet` / `system-NNNN.arrow` | sampled hardware metrics |
| `histograms.parquet` / `histograms-NNNN.arrow` | one row per (tag, step): JSON `edges`, `counts`, `total` |
| `media/` + `media.jsonl` | logged images/audio/video and their manifest |
| `provenance.yaml` | git commit/branch/dirty, command, hostname, scheduler ids |
| `run.yaml`, `config.yaml`, `run.log`, `console.log` | as before |
| `.run.lock` | advisory lock file for `run.yaml` updates |

**Metrics are append-only while a run is live.** Each flush appends an Arrow IPC
batch rather than rewriting the Parquet, so cost is proportional to the rows
flushed rather than to the run's history. Rewriting made total write volume grow
with the *square* of the step count; measured on 10k steps, the old path took
**48s** and the new one **0.4s**.

A metric first logged mid-run rolls a new segment, because one IPC stream carries
one schema. Readers union the segments with the Parquet and merge rows sharing a
step, so `read_run_vectors` sees a live run's data — a reader that opened only
the Parquet would see nothing until the run closed.

A segment truncated by a hard kill yields every batch completely written and
stops. Compaction writes the Parquet *before* deleting segments, so an
interruption leaves both and readers union them to the same result.

### `RunMetadata` → `run.yaml`

`name`, `experiment`, `status`, `started_at`, `finished_at`, `duration_secs`,
`description`, `tags`, plus `#[serde(default)]` fields: `heartbeat_at`,
`scalars` (latest scalar value per key), `vectors` (latest value per vector key
— a summary, not the series), `language`, `env_path`.

`heartbeat_at` is refreshed by the engine every `heartbeat_interval_secs`
(default 30s) for as long as the run is `RUNNING`. It exists so a hard-killed
run — which stays `RUNNING` forever — is distinguishable from a legitimately
long one. `None` on runs written before heartbeats existed; readers fall back to
`started_at`, which is the conservative direction. See `exp reap`,
`storage::is_run_stale`, and `storage::looks_alive`.

`description` and `tags` are now written **at creation** when supplied to
`Experiment(...)`, rather than requiring a hand-patch after `close()`.

`group` and `rank` place a run in a cohort — the N ranks of a DDP job, or the
trials of a sweep. Both are auto-detected from the launcher's environment.

**All mutations go through `storage::update_run_metadata`**, which takes an
exclusive advisory lock on `.run.lock`. Under DDP every rank ticks its own
metadata update; a bare load-mutate-save races and silently drops the loser's
fields. `save_yaml` also writes atomically (temp file + rename), so a reader
never sees a half-written `run.yaml` and misreports the run as CRASHED.

`Default` sets `status: Crashed` (`models.rs:181`). That is deliberate: a
missing or unparseable `run.yaml` degrades to `minimal_run_metadata`
(`storage.rs:193-212`), which infers name/experiment from the path and reports
`CRASHED`.

### `ExperimentMetadata` → `experiment.yaml`

`display_name: Option<String>`, `description: Option<String>`,
`tags: Vec<String>`, `project: Option<String>`.

`project` is the whole projects hierarchy: a run's project is resolved through
its experiment, so no run data ever moves when a project is created or
reassigned. It can be written offline — `storage::set_experiment_project`
rewrites only that field — via `Experiment(project=...)`,
`Experiment.set_project()`, `expman.assign_project()`, `exp project assign`, or
`exp project sync`.

### `ProjectMetadata` → `project.yaml`

`display_name: Option<String>`, `description: Option<String>`,
`tags: Vec<String>`, `created_at: Option<DateTime<Utc>>`, plus the
generated-projection marker: `generated: bool`,
`generated_from: Option<String>`, `generated_at: Option<DateTime<Utc>>`.

A project with `generated: true` is a **one-way projection** of a source outside
expman (see `core/projects.rs`). It is overwritten wholesale by the next
`exp project sync`, so the HTTP API refuses writes to it with `409` and the
dashboard hides its edit affordances. Its `README.md` carries a matching
`<!-- expman:generated ... -->` marker on the first line.

### `ArtifactInfo` (`storage.rs:144-150`)

`Serialize` only; lives in `storage.rs`, not `models.rs`. Fields: `path`
(relative), `name`, `size`, `ext` (lowercased), `is_default`.

`list_artifacts` (`:69-111`) returns two classes: whitelisted run-root files
(`vectors.parquet`, `config.yaml`, `run.yaml`, `run.log`, `console.log`) with
`is_default: true`, plus a recursive walk of `artifacts/` with `is_default: false`.

## Parquet schema

Inferred per batch (`rows_to_record_batch`, `storage.rs:448-535`), not declared:

| column | type | nullable |
|---|---|---|
| `step` | `Int64` | yes |
| `timestamp` | `Timestamp(Microsecond, "UTC")` | **no** |
| *each metric key* | `Float64` or `Utf8` | yes |

Metric column order is first-seen across the batch. Type comes from the first
non-null occurrence: `Float`/`Int` → `Float64` (ints widen); `Text`, `Bool`, or
a leading null → `Utf8` (`Bool`/`Float`/`Int` stringified as fallback).

Compression is **SNAPPY** (`:379-388`).

Schema evolution across batches goes through `merge_schemas` (`:404-417`) —
existing fields first, then new-only fields appended — and `align_batch`
(`:419-446`), which back-fills missing columns with typed nulls.

> **Known defect (verified 2026-07-27).** `storage.rs:272` downcasts the existing
> `step` column to `UInt64Array` while `:475`/`:486` write it as `Int64`. The
> downcast always returns `None`, the `else` at `:286` keeps the existing batch
> unfiltered, and the cross-batch step-dedup at `:270-291` is dead code.
> Re-logging the same step in two different flush batches yields duplicate rows.
> In-batch dedup (`:234-252`) works fine.

## Read functions

| function | line | behavior |
|---|---|---|
| `read_vectors(path)` | `:303` | all rows as `Vec<HashMap<String, serde_json::Value>>`; `vec![]` if the file is absent |
| `read_vectors_since(path, since)` | `:312` | reads **everything**, then filters `step > since` in memory. Rows without a parseable step are kept. Used by the 500 ms SSE loop. |
| `read_latest_scalar_metrics(path)` | `:334` | reads all rows, takes `.last()` in **file order, not step order**, drops `step`/`timestamp`, keeps only `as_f64()`-able values |

`record_batch_to_rows` (`:537-588`) converts to JSON: nulls → `Value::Null`;
`Float64` NaN/Inf → `Value::Null` (keeps the JSON valid); timestamps → RFC3339
strings; unhandled Arrow types → `Null`.

All of these load the full Parquet file into memory — there is no predicate or
row-group pushdown.

## Error type

`ExpmanError` (`src/core/error.rs:6-35`), `thiserror`:

`Io` · `Arrow` (not wasm32) · `Parquet` (not wasm32) · `Yaml` · `Json` ·
`ChannelClosed` · `RunNotFound(String)` · `ExperimentNotFound(String)` ·
`Other(String)`

The two `NotFound` variants are declared but **never constructed** — the code
returns empty defaults instead.
