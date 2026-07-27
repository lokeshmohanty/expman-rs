# Reference — the `exp` CLI

*Hand-written from `src/cli/mod.rs`, 2026-07-27.*

Binary name is **`exp`** (`Cargo.toml` `default-run`), though clap's `name` is
set to `"expman"` (`cli/mod.rs:71`) so `--help` output says `expman`. Requires
the `cli` feature. Entry: `src/main.rs:6-8`.

Logging: `init_tracing` (`cli/mod.rs:15-23`) reads `RUST_LOG` via `EnvFilter`,
defaults to `info`, compact format without targets.

Errors surface as `Error: {e}` + `exit(1)`. The CLI uses `anyhow`, not
`ExpmanError`.

---

## `exp serve [DIR]`

Gated on the `server` feature. Starts the dashboard.

| arg | default |
|---|---|
| `DIR` | `./experiments` |
| `--host` | `127.0.0.1` |
| `--port`, `-p` | `8000` |
| `--no-live` | off |

Implementation `cli/mod.rs:152-169`. Note `live_mode` is passed into
`ServerConfig` but then dropped — `/api/config` reports it hardcoded as `true`.

## `exp list [DIR]`

| arg | default |
|---|---|
| `DIR` | `./experiments` |
| `--experiment`, `-e` | none |

Without `-e`: a table of `Experiment | Runs | Display Name`. With `-e`: a table
of `Run | Status | Started | Duration | Description`, where a `None` duration
prints literally as `running`. Both use `comfy_table` with the `UTF8_FULL`
preset (`cli/mod.rs:171-243`).

## `exp inspect <RUN_DIR>`

Prints run/experiment/status/started/duration, then `config.yaml` verbatim, then
a `Vector | Value` table of the **last** Parquet row plus a total row count, then
a `Scalar | Value` table from `run.yaml.scalars`, then every artifact as
`path (N bytes)` (`cli/mod.rs:245-318`).

> The help text example (`cli/mod.rs:111`) shows
> `experiments/my_exp/runs/20240101_120000` — the `runs/` segment is **spurious**
> and does not match the real layout. See
> [storage-layout.md](storage-layout.md).

## `exp clean <EXPERIMENT>`

| arg | default |
|---|---|
| `EXPERIMENT` | — |
| `--dir` | `./experiments` |
| `--keep`, `-k` | `5` |
| `--force` | off |

**Dry-run by default** — prints what it would delete and
`"Dry run. Use --force to actually delete."`. With `--force`, `remove_dir_all`s
each victim. Relies on `list_runs` being newest-first plus `split_off(keep)`
(`cli/mod.rs:320-360`).

## `exp export <RUN_DIR>`

| arg | default |
|---|---|
| `RUN_DIR` | — |
| `--format`, `-f` | `csv` — one of `csv`, `json`, `tensorboard` |
| `--output`, `-o` | stdout for csv/json; `./tb_logs` for tensorboard |

Bails if `vectors.parquet` is absent (`cli/mod.rs:380-443`).

- **json** — `serde_json::to_string_pretty` of the row maps.
- **csv** — header keys come from **`rows[0]` only**, sorted; keys appearing only
  in later rows are dropped. Values are `serde_json::Value::to_string()`, so
  strings keep their quotes and **there is no escaping or quoting logic**. Treat
  this as a convenience export, not a robust CSV writer.
- **tensorboard** — writes via `tensorboard_rs::summary_writer::SummaryWriter`.
  Only `as_f64()`-able columns are emitted, as `add_scalar(k, v as f32, step)`;
  `step` and `timestamp` are skipped.

## `exp import <INPUT>`

| arg | default |
|---|---|
| `INPUT` | — (a TB log dir or a single event file) |
| `--dir` | `./experiments` |

If `INPUT` is a directory, takes the **first** entry whose filename contains
`tfevents`. Experiment name is `INPUT`'s file name (fallback `imported_tb`); run
name is a UTC timestamp. Parses with `tboard::SummaryReader`, folding
`SimpleValue` summaries into a `BTreeMap<step, ...>` so rows come out
step-ordered, then writes via `storage::append_vectors` (`cli/mod.rs:457-532`).

> **Gotcha:** it creates the run directory but writes **no `run.yaml` or
> `experiment.yaml`**, so imported runs read back as `CRASHED` via
> `minimal_run_metadata`.

---

## Via Python

`pip install expman-rs` installs an `exp` console script
(`pyproject.toml:24-25`) pointing at `expman.cli:main` — a 31-line shim that
execs the binary bundled at `<site-packages>/expman/bin/exp` and forwards the
exit code (`wrappers/python/expman/cli.py`).

> **Nix caveat:** `nix build .#python3Packages.expman-rs` has no step that
> populates `expman/bin/`, so that console script will report *"Bundled binary
> not found"* and return 1. Nix users should get the CLI from `.#expman`.
