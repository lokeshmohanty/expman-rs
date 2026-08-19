+++
title = "CLI Reference"
description = "Every subcommand and flag of the exp CLI tool."
weight = 1
+++

# Reference — the `exp` CLI

*Hand-written from `src/cli/mod.rs`, updated 2026-07-28.*

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
| `--read-only` | off |
| `--notebook-template <PATH>` | none — see discovery order below |
| `--jupyter-command <CMD>` | `jupyter` |

`--read-only` refuses **every** non-GET/HEAD/OPTIONS request with `403
{"error": "read_only"}`. It is enforced as middleware over the whole router
(`api/mod.rs`, `enforce_read_only`) rather than per handler, so routes added
later are covered by default — including the Jupyter and TensorBoard start/stop
POSTs, which spawn processes on the host. Use it to share a store with someone
who should not be able to change it.

`live_mode` and `read_only` are stored in `AppState` and reported truthfully by
`/api/config`. The two Jupyter flags are stored on the `JupyterManager` and are
**not** reported by `/api/config` — nothing in the frontend needs them.

### `--notebook-template` — the Interactive tab's cells

*Added 1.3.0.* The notebook a run's Interactive tab opens comes from a
project-supplied `.ipynb`, so the cells can do project-specific work — load this
run's checkpoint, render a rollout the training host could not — instead of the
built-in "read `vectors.parquet` and plot it".

**Discovery order** (`jupyter_service.rs`, `NotebookTemplateConfig::resolve`):

1. `--notebook-template <PATH>`
2. `<DIR>/.expman/notebook.ipynb` — the convention, so a store can carry its own
   notebook and the flag can stay unset
3. the built-in default

A `--notebook-template` that does not exist is **warned about and skipped**,
falling through to 2 and 3; a typo in the flag does not stop the server.

The template is itself a `.ipynb` — valid JSON — and expman substitutes these
placeholders **anywhere in the file**, cells or metadata alike:

| placeholder | value |
|---|---|
| `{{run_dir}}` | absolute path to the run directory (also the notebook's cwd) |
| `{{run_name}}` | the run's directory name |
| `{{experiment}}` | the experiment's directory name |
| `{{store}}` | absolute path to `DIR`, the store root |
| `{{project}}` | the run's experiment's `project`, or `""` when unassigned |

Paths are absolute because a relative one would only resolve against the
server's cwd, which the notebook has no reason to share.

Values are **JSON-escaped** before substitution. Placeholders sit inside JSON
string literals, so a run directory containing `"` or `\` would otherwise close
the literal and corrupt the notebook. This is tested, not assumed
(`test_placeholder_values_are_json_escaped`).

If the substituted template does not parse as JSON, expman **logs an error
naming the template path and writes the built-in default instead** — a broken
`.ipynb` would break the tab the template exists to improve.

**The multi-run notebook is deliberately not templatable.** `POST
/experiments/{exp}/jupyter/start` writes an `interactive.ipynb` into the
*experiment* directory, where `{{run_dir}}`, `{{run_name}}` and `{{project}}`
have no value to take; a useful shared template would need its own placeholder
set (a `{{runs}}` list plus a rule for rendering it), which is a separate design
rather than a wider version of this one. It always gets the built-in default,
and always will until that design exists.

### Notebook staleness — regenerated unless you edited it

*Added 1.3.0. Before then, an existing `interactive.ipynb` was kept forever, so a
run generated under an old template never picked up a fix.*

Every notebook expman writes carries a namespaced `metadata.expman` block:

```json
"metadata": {
  "expman": {
    "generated_by": "expman 1.3.0",
    "template": "/abs/path/.expman/notebook.ipynb",
    "template_hash": "sha256:…",
    "content_hash": "sha256:…"
  }
}
```

On each launch (`POST …/jupyter/start`, or the notebook POST):

| state on disk | what happens |
|---|---|
| absent | written |
| matches `content_hash`, `template_hash` unchanged | left alone, silently |
| matches `content_hash`, `template_hash` differs | **rewritten**, logged at info |
| does not match `content_hash` — you edited it | left alone, logged at **warn** |
| no `metadata.expman` — hand-made, or pre-1.3.0 | left alone, logged at warn |

`content_hash` covers the parsed JSON with `content_hash` itself removed (a field
cannot hash itself), so a whitespace reformat still counts as unedited.
**Executing cells does not** — outputs and `execution_count` are your work, and
expman will not overwrite them. Delete the file to opt back into regeneration.

The rule applies to the multi-run notebook too, whose "template" is the built-in
content — which names the runs, so changing the selected run set refreshes an
unedited notebook.

### `--jupyter-command` — which interpreter the kernel is

*Added 1.3.0.* A command **line**, not a program name; split POSIX-shell-style
(via `shlex`), so a quoted path with a space survives. No shell runs, so pipes,
redirection and `$VAR` are not available. expman appends
`notebook --no-browser --port=… …` after it:

```bash
exp serve ./experiments --jupyter-command 'uv run --extra nb jupyter'
# actually executed:  uv run --extra nb jupyter notebook --no-browser --port=8888 …
```

**Why this rather than a kernelspec.** The kernel a notebook gets is the
interpreter Jupyter itself runs under. Launch Jupyter from inside the project's
virtualenv and its built-in `python3` kernel *is* that venv's interpreter, so
`import <your package>` works with **no `ipykernel install` step and no kernelspec
name for expman and the project to keep in sync**. expman therefore does not
register a global kernel and does not write a named kernelspec into the generated
notebook — either would mutate your `~/.local/share/jupyter` and then have to
stay correct forever.

Two failure modes, both with messages that name the configured value:

- unparsable (unbalanced quote) or empty → `exp serve` **refuses to start**,
  rather than failing at the first click on the Interactive tab
- the program does not exist → the start request fails with
  ``Failed to start Jupyter with the configured command `…`: No such file or
  directory``, not a bare ENOENT

**The trap.** expman runs the command with the **run directory** as cwd, because
that is where Jupyter must serve `interactive.ipynb` from. A command that
discovers its environment from cwd — `uv run` walking up for a `pyproject.toml` —
therefore only works when the store lives inside the project. When it does not,
name the project explicitly instead of changing directory:

```bash
--jupyter-command 'uv run --project /path/to/project --extra nb jupyter'
```

## `exp list [DIR]`

| arg | default |
|---|---|
| `DIR` | `./experiments` |
| `--experiment`, `-e` | none |
| `--project`, `-p` | none |
| `--group`, `-g` | none |
| `--tag`, `-t` | none |
| `--status`, `-s` | none |
| `--runs` | off |

Two views. **Experiment view** (the default) is a table of `Experiment |
Project | Runs | Display Name`, narrowed by `--project`. **Run view** is a flat
cross-experiment table of `Run | Experiment | Project | Status | Started |
Duration | Tags`; a `None` duration prints as `running`.

Run view is selected by `--runs`, by `--experiment`, or by any run-level filter
(`--tag`, `--status`, `--group`) — asking a question about runs gives you runs.
A `Group` column appears only when something is actually grouped, shown as
`group[rank]`.

`--tag` takes an expression: clauses are ANDed, alternatives within a clause are
ORed.

```bash
exp list experiments --tag "arm:tiered AND (study:1 OR study:2)"
exp list experiments --tag "arm:tiered,study:1"   # , is AND, | is OR
```

Operators are matched case-insensitively but only as bare whitespace-delimited
words, so a tag like `brand:ORACLE` is not torn in half.

## `exp project <SUBCOMMAND>`

The projects layer without a server — a cluster node or tmux session has no
dashboard, which is what made this layer unreachable from code before.

| subcommand | what |
|---|---|
| `ls [DIR]` | `Project \| Experiments \| Runs \| Source \| Display Name` |
| `new <NAME> [--display-name] [--description] [--tag ...]` | create a project |
| `show <NAME>` | metadata plus its experiments |
| `assign <EXPERIMENT> <PROJECT>` | set `project:` in `experiment.yaml` |
| `unassign <EXPERIMENT>` | clear it |
| `rm <NAME> [--force]` | delete the project, unassigning its experiments (runs are never deleted); dry run without `--force` |
| `sync <MANIFEST> [--dir]` | generate projects from a YAML manifest |

All take `--dir` (default `./experiments`), except `ls` which takes `DIR`
positionally.

### `exp project sync`

**One-way.** The manifest is authoritative; each sync overwrites the project's
metadata, README, and experiment membership, and marks it `generated: true`.
The dashboard then refuses edits to it with `409 generated_project` and hides
the edit affordances — accepting a write it is about to destroy would be worse
than refusing it.

```yaml
projects:
  - name: study-1
    display_name: "Study 1 — Drift regret"
    description: "Does tiered allocation reduce drift regret?"
    tags: [thesis, study1]
    generated_from: "studies.yaml (thesis repo)"
    experiments: [e1-drift-regret-slope, x2-shift-sweep]
    frontpage:
      question: "Does a tiered arm dominate a flat arm under covariate shift?"
      scope: "3 domains, 5 seeds, 4 shift levels"
      domains: [vision, tabular, text]
      target_venue: "NeurIPS"
      target_date: "2026-05-15"
      status: "running"
      sections:
        - heading: "Notes"
          body: "Free-form markdown."
```

A bare top-level list of specs is accepted too. `readme:` supplies a raw body
instead of rendering `frontpage:`. Membership is *reconciled*: experiments
listed are assigned, and experiments previously in the project but absent from
the manifest are unassigned. Naming an experiment that does not exist yet is
reported, not an error — a project can be declared before its first run.

## `exp sweep <SUBCOMMAND>`

Hyperparameter search. A sweep is a **group** of runs, one per trial — it needs
no new storage concept and no server.

| subcommand | what |
|---|---|
| `preview <CONFIG> [--limit]` | expand the config and print the trials; runs nothing |
| `run <CONFIG> [--dir] [-j N] [--dry-run]` | execute trials locally, `N` at a time |
| `slurm <CONFIG> [--dir] [-o FILE] [--partition] [--time] [--gpus] [--cpus] [--mem] [--log-dir] [--max-concurrent] [--sbatch ...]` | emit an sbatch array |
| `status <NAME> [--dir] [--metric M] [--minimize]` | trial states and a ranked leaderboard |

```yaml
name: lr-sweep            # becomes the group every trial belongs to
experiment: e1-drift      # trials are logged under this experiment
project: study-1          # optional; created if it does not exist
method: grid              # or: random
trials: 40                # random only
seed: 0                   # random only; sweeps are reproducible from this
command: "python train.py --lr {lr} --bs {bs}"
params:
  lr:    {values: [0.1, 0.01, 0.001]}
  bs:    {values: [16, 32]}
  wd:    {min: 1.0e-5, max: 1.0e-2, log: true}   # random only
metric:
  name: val_loss
  goal: minimize
```

**Params reach the trial two ways**, deliberately: substituted into `command`
(so an existing argparse script needs no edits) and exported as
`EXPMAN_PARAM_LR` etc. (so a shell wrapper or sbatch script can read them
without seeing the command string). `expman.sweep_params()` returns them as a
typed dict.

The sweep also exports `EXPMAN_GROUP`, `EXPMAN_RANK`, `EXPMAN_RUN_NAME`,
`EXPMAN_EXPERIMENT`, `EXPMAN_PROJECT`, `EXPMAN_BASE_DIR` and `EXPMAN_TAGS`, all
of which the Python `Experiment` picks up automatically. A training script needs
no sweep-specific code at all.

> **Grid needs `values`.** `min`/`max` is a continuous domain and only works with
> `method: random`; grid says so rather than silently sampling. `log: true`
> samples the exponent, which is what you want for learning rates — a uniform
> sampler puts ~90% of its mass in the top decade.

Random search is reproducible: the same `seed` yields the same trials, using an
internal SplitMix64 rather than the `rand` crate so a dependency bump cannot
change results.

## `exp probes`

Shows which system-metric probes exist on this machine and takes one live
sample. `--all` lists every probe considered, including unavailable ones.

This is the answer to *"why am I not seeing GPU metrics?"* — during a run an
absent probe is skipped silently, which is right for logging and useless for
debugging.

| host | probe | result |
|---|---|---|
| 2× RTX A6000 | `nvidia-smi` | 6 metrics per GPU: `gpu.N.util_pct`, `mem_used_mb`, `mem_total_mb`, `mem_util_pct`, `temp_c`, `power_w` |
| TPU v6e (8 chips) | `tpu-info` | `tpu.N.tensorcore_utilization`; HBM and duty cycle appear once a framework opens the TPU |
| any Linux | `/proc` | `cpu.util_pct`, `mem.total_gb`, `mem.used_gb`, `mem.util_pct`, `proc.rss_gb` |

> **`tpu-info` has no JSON output.** It prints Rich pipe tables, one per
> `--metric`. expman parses those directly. `hbm_usage` and `duty_cycle_percent`
> read `N/A` until a framework has the TPU open — libtpu only publishes them
> then — and are skipped rather than recorded as zero.

Custom probes need no rebuild; add them to config with `format: nvidia_csv`,
`json`, `pipe_table` or `key_value`.

## `exp reap [DIR]`

| arg | default |
|---|---|
| `DIR` | `./experiments` |
| `--older-than` | `1h` (`90s`, `30m`, `2h`, `3d`; bare number = seconds) |
| `--project`, `-p` | none |
| `--experiment`, `-e` | none |
| `--force` | off (dry run) |

Marks stale `RUNNING` runs as `CRASHED`. A hard kill leaves a run `RUNNING`
forever, which silently inflates `/stats.active_runs`.

Staleness comes from `heartbeat_at`, which the engine refreshes every
`heartbeat_interval_secs` (default 30s). Runs written before heartbeats existed
have no `heartbeat_at` and fall back to `started_at` — the conservative
direction, since such a run is only reaped once it is genuinely old.

`finished_at` is set to the **last heartbeat**, not to when you noticed;
otherwise a run reaped a week late would report a week-long duration.

Reaping also **compacts** the run (`storage::compact_run`), folding its live
`.arrow` segments into Parquet. A hard-killed run never reached the engine's
close path, so without this it stayed terminal *and* slow — every later read
re-parses the whole segment heap. Compaction runs **before** the status is
written: this command only selects `RUNNING` runs, so an interrupted reap leaves
the run `RUNNING` and the next one finishes the job, whereas marking first would
strand it terminal and uncompacted forever. Compaction failures are logged, not
fatal — the status still gets recorded.

## `exp inspect <RUN_DIR>`

Prints run/experiment/status/started/duration, then `config.yaml` verbatim, then
a `Vector | Value` table of the **last** Parquet row plus a total row count, then
a `Scalar | Value` table from `run.yaml.scalars`, then every artifact as
`path (N bytes)` (`cli/mod.rs:245-318`).

> The help text example (`cli/mod.rs:111`) shows
> `experiments/my_exp/runs/20240101_120000` — the `runs/` segment is **spurious**
> and does not match the real layout. See
> [storage-layout](/reference/storage-layout/).

## `exp clean [EXPERIMENT]`

| arg | default |
|---|---|
| `EXPERIMENT` | optional |
| `--dir` | `./experiments` |
| `--project`, `-p` | none |
| `--group`, `-g` | none |
| `--tag`, `-t` | none |
| `--keep`, `-k` | `5` |
| `--force` | off |

Keeps the `--keep` most recent runs **per experiment**. At least one of
`EXPERIMENT`, `--project`, `--group` or `--tag` is required — with no scope at all it
refuses rather than cleaning the whole store.

With `--tag`, only matching runs are candidates *and* only they count toward
`--keep`, so `--tag arm:tiered --keep 5` keeps five tiered runs per experiment
and leaves the others untouched.

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
- **csv** — RFC 4180. The header is the **union of every row's keys** (`step` and
  `timestamp` first, the rest alphabetical), and fields containing a comma,
  quote, CR or LF are quoted with inner quotes doubled. A missing metric and an
  explicit null both render as an empty cell, since any placeholder would be
  indistinguishable from data.

  > Both of those were previously wrong, and silently: the header came from
  > `rows[0]`, so a metric first logged mid-run was dropped from the file
  > entirely, and unescaped values let a string containing a comma split into an
  > extra column. Regression tests:
  > `test_cli_export_csv_includes_metrics_first_logged_mid_run`.
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
step-ordered, then writes via `storage::append_vectors`.

It also writes a `run.yaml` with status `FINISHED`, the tags `imported` and
`tensorboard`, and the last row's values, plus an `experiment.yaml` if absent.
Without those an imported run had no metadata at all and every reader fell back
to `minimal_run_metadata`, so the import showed up in the dashboard as
`CRASHED`.

---

## Via Python

`pip install expman-rs` installs an `exp` console script
(`pyproject.toml:24-25`) pointing at `expman.cli:main` — a 31-line shim that
execs the binary bundled at `<site-packages>/expman/bin/exp` and forwards the
exit code (`wrappers/python/expman/cli.py`).

> **Nix caveat:** `nix build .#python3Packages.expman-rs` has no step that
> populates `expman/bin/`, so that console script will report *"Bundled binary
> not found"* and return 1. Nix users should get the CLI from `.#expman`.
