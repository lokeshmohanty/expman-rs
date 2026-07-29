"""Tests for the projects layer and the read API.

Both exist so that a compute node with no dashboard — a SLURM batch job, a tmux
session — can use expman as the single source of truth, rather than reading
Parquet directly and keeping a side-car manifest as the durable record.
"""

import json
import os

import pytest
import yaml

import expman


def _run(base_dir, name, *, project=None, tags=None, steps=3, run_name=None):
    """Create and close one run, returning its directory."""
    with expman.Experiment(
        name,
        base_dir=str(base_dir),
        run_name=run_name,
        project=project,
        tags=tags,
        heartbeat_interval_secs=0,
    ) as exp:
        exp.log_params({"lr": 0.01})
        for step in range(steps):
            exp.log_vector({"loss": 1.0 - step * 0.1}, step=step)
        return exp.run_dir


# ─── Creation-time project / tags / description ──────────────────────────────


def test_project_is_written_offline_at_creation(tmp_path):
    """No server involved: the project lands in experiment.yaml on disk."""
    base = tmp_path / "experiments"
    _run(base, "e1", project="study-1", tags=["arm:tiered", "seed:1"])

    with open(base / "e1" / "experiment.yaml") as f:
        meta = yaml.safe_load(f)
    assert meta["project"] == "study-1"


def test_tags_and_description_at_creation(tmp_path):
    base = tmp_path / "experiments"
    with expman.Experiment(
        "e1",
        base_dir=str(base),
        tags=["arm:tiered"],
        description="first run",
        heartbeat_interval_secs=0,
    ) as exp:
        run_dir = exp.run_dir

    meta = expman.load_run(run_dir)
    assert meta["tags"] == ["arm:tiered"]
    assert meta["description"] == "first run"


def test_set_project_and_tags_after_creation(tmp_path):
    base = tmp_path / "experiments"
    with expman.Experiment("e1", base_dir=str(base), heartbeat_interval_secs=0) as exp:
        assert exp.project is None
        exp.set_project("study-2")
        exp.add_tags(["arm:flat"])
        exp.add_tags(["arm:flat", "seed:7"])  # idempotent on the duplicate
        exp.set_description("patched")
        assert exp.project == "study-2"
        run_dir = exp.run_dir

    meta = expman.load_run(run_dir)
    assert meta["tags"] == ["arm:flat", "seed:7"]
    assert meta["description"] == "patched"


def test_init_accepts_project_and_tags(tmp_path):
    base = tmp_path / "experiments"
    expman.init(
        "e1",
        base_dir=str(base),
        project="study-1",
        tags=["study:1"],
        heartbeat_interval_secs=0,
    )
    expman.log_vector({"loss": 0.5}, step=0)
    expman.close()

    runs = expman.load_runs(base_dir=str(base), project="study-1")
    assert len(runs) == 1
    assert runs[0]["tags"] == ["study:1"]


# ─── Read API ────────────────────────────────────────────────────────────────


def test_read_metrics_returns_logged_rows(tmp_path):
    base = tmp_path / "experiments"
    run_dir = _run(base, "e1", steps=5)

    rows = expman.read_metrics(run_dir)
    assert len(rows) == 5
    assert rows[0]["step"] == 0
    assert "loss" in rows[0]
    # Plain builtins, so a bare environment can consume them.
    assert isinstance(rows, list) and isinstance(rows[0], dict)


def test_load_config_returns_logged_params(tmp_path):
    base = tmp_path / "experiments"
    run_dir = _run(base, "e1")
    assert expman.load_config(run_dir)["lr"] == 0.01


def test_load_runs_filters_by_project_tag_and_status(tmp_path):
    base = tmp_path / "experiments"
    _run(base, "e1", project="study-1", tags=["arm:tiered", "study:1"], run_name="r1")
    _run(base, "e1", project="study-1", tags=["arm:flat", "study:1"], run_name="r2")
    _run(base, "e2", project="study-2", tags=["arm:tiered", "study:2"], run_name="r3")

    assert len(expman.load_runs(base_dir=str(base))) == 3
    assert len(expman.load_runs(base_dir=str(base), project="study-1")) == 2
    assert len(expman.load_runs(base_dir=str(base), experiment="e2")) == 1
    assert len(expman.load_runs(base_dir=str(base), status="FINISHED")) == 3

    # A list of tags is a conjunction.
    tiered_s1 = expman.load_runs(base_dir=str(base), tags=["arm:tiered", "study:1"])
    assert [r["run"] for r in tiered_s1] == ["r1"]

    # An expression string supports OR — the facet scheme this exists for.
    either = expman.load_runs(
        base_dir=str(base), tags="arm:tiered AND (study:1 OR study:2)"
    )
    assert sorted(r["run"] for r in either) == ["r1", "r3"]


def test_load_runs_path_composes_with_read_metrics(tmp_path):
    """The whole point: query, then read, with no path arithmetic by the caller."""
    base = tmp_path / "experiments"
    _run(base, "e1", tags=["arm:tiered"], steps=4)

    run = expman.load_runs(base_dir=str(base), tags=["arm:tiered"])[0]
    assert os.path.isdir(run["path"])
    assert len(expman.read_metrics(run["path"])) == 4


def test_assign_project_without_an_open_run(tmp_path):
    base = tmp_path / "experiments"
    _run(base, "e1")

    expman.assign_project("e1", "study-9", base_dir=str(base))
    assert len(expman.load_runs(base_dir=str(base), project="study-9")) == 1

    projects = expman.load_projects(base_dir=str(base))
    # The project directory is only created by `exp project new`/`sync`; the
    # assignment on the experiment is what makes the run queryable.
    assert isinstance(projects, list)

    expman.assign_project("e1", None, base_dir=str(base))
    assert expman.load_runs(base_dir=str(base), project="study-9") == []


def test_to_pandas_is_optional(tmp_path):
    """pandas stays an extra: the default path must not import it."""
    base = tmp_path / "experiments"
    run_dir = _run(base, "e1")

    rows = expman.read_metrics(run_dir)
    assert isinstance(rows, list)

    pd = __import__("importlib").util.find_spec("pandas")
    if pd is not None:
        df = expman.read_metrics(run_dir, to_pandas=True)
        assert len(df) == len(rows)


# ─── Media, histograms, sweeps ───────────────────────────────────────────────

_TINY_PNG = __import__("base64").b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
)


def test_log_image_accepts_bytes_and_paths(tmp_path):
    base = tmp_path / "experiments"
    png_path = tmp_path / "sample.png"
    png_path.write_bytes(_TINY_PNG)

    with expman.Experiment(
        "media", base_dir=str(base), system_metrics_interval_secs=0, capture_provenance=False
    ) as e:
        e.log_image("from_bytes", _TINY_PNG, step=0)
        e.log_image("from_path", str(png_path), step=1)
        run_dir = e.run_dir

    media = expman.read_media(run_dir)
    assert sorted(m["tag"] for m in media) == ["from_bytes", "from_path"]
    for entry in media:
        assert (tmp_path.parent / os.path.join(run_dir, entry["file"])).exists() or os.path.exists(
            os.path.join(run_dir, entry["file"])
        )
        assert entry["bytes"] == len(_TINY_PNG)


def test_log_image_rejects_nonsense_in_the_native_api(tmp_path):
    # The native API raises where the TensorBoard compat layer warns: this is
    # new code, and a clear error is the fastest route to a fix.
    base = tmp_path / "experiments"
    with expman.Experiment(
        "media", base_dir=str(base), system_metrics_interval_secs=0, capture_provenance=False
    ) as e:
        with pytest.raises(TypeError, match="cannot log"):
            e.log_image("bad", object())


def test_log_histogram_bins_values(tmp_path):
    base = tmp_path / "experiments"
    with expman.Experiment(
        "hist", base_dir=str(base), system_metrics_interval_secs=0, capture_provenance=False
    ) as e:
        e.log_histogram("weights", [0.0, 0.1, 0.5, 0.9, 1.0], step=0, bins=4)
        e.log_histogram("preset", edges=[0.0, 1.0, 2.0], counts=[3, 7], step=1)
        run_dir = e.run_dir

    rows = {h["tag"]: h for h in expman.read_histograms(run_dir)}
    assert rows["weights"]["total"] == 5
    assert len(json.loads(rows["weights"]["counts"])) == 4
    # edges is always one longer than counts, which is what makes it plottable.
    assert len(json.loads(rows["weights"]["edges"])) == 5
    assert rows["preset"]["total"] == 10


def test_log_histogram_rejects_mismatched_bins(tmp_path):
    base = tmp_path / "experiments"
    with expman.Experiment(
        "hist", base_dir=str(base), system_metrics_interval_secs=0, capture_provenance=False
    ) as e:
        with pytest.raises(ValueError, match="one more element"):
            e.log_histogram("bad", edges=[0.0, 1.0], counts=[1, 2, 3])


def test_sweep_params_reads_the_environment(monkeypatch):
    monkeypatch.setenv("EXPMAN_PARAM_LR", "0.001")
    monkeypatch.setenv("EXPMAN_PARAM_BS", "32")
    monkeypatch.setenv("EXPMAN_PARAM_NAME", "baseline")
    monkeypatch.setenv("EXPMAN_SWEEP", "s1")

    params = expman.sweep_params()
    # Types are recovered from strings, so a script can use them directly.
    assert params == {"lr": 0.001, "bs": 32, "name": "baseline"}
    assert isinstance(params["bs"], int)
    assert expman.sweep_name() == "s1"


def test_sweep_env_places_the_run_without_script_changes(tmp_path, monkeypatch):
    base = tmp_path / "experiments"
    monkeypatch.setenv("EXPMAN_BASE_DIR", str(base))
    monkeypatch.setenv("EXPMAN_GROUP", "sweep-a")
    monkeypatch.setenv("EXPMAN_RANK", "3")
    monkeypatch.setenv("EXPMAN_RUN_NAME", "sweep-a-0003")
    monkeypatch.setenv("EXPMAN_TAGS", "lr:0.01,bs:32")

    # base_dir deliberately wrong: the sweep's env must win.
    with expman.Experiment(
        "e1", base_dir="ignored", system_metrics_interval_secs=0, capture_provenance=False
    ) as e:
        e.log_vector({"loss": 1.0}, step=0)

    runs = expman.load_runs(base_dir=str(base), group="sweep-a")
    assert len(runs) == 1
    assert runs[0]["run"] == "sweep-a-0003"
    assert runs[0]["rank"] == 3
    assert sorted(runs[0]["tags"]) == ["bs:32", "lr:0.01"]
