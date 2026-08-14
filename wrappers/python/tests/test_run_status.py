"""Terminal run status: explicit, automatic, and the normal case.

The bug these guard: a Python run that died was recorded as FINISHED, so a
crashed run and a successful one were indistinguishable to everything that
later read the store.
"""

import os
import subprocess
import sys
import textwrap

import pytest
import yaml

import expman


def read_status(run_dir):
    with open(os.path.join(run_dir, "run.yaml")) as f:
        return yaml.safe_load(f)["status"]


def read_run_log(run_dir):
    path = os.path.join(run_dir, "run.log")
    if not os.path.exists(path):
        return ""
    with open(path) as f:
        return f.read()


# ─── The normal case must not move ───────────────────────────────────────────


def test_plain_close_is_still_finished(tmp_path):
    """The backwards-compatibility guarantee: `close()` with no argument."""
    exp = expman.Experiment("ok_exp", base_dir=str(tmp_path))
    run_dir = exp.run_dir
    exp.log_vector({"loss": 0.1}, step=0)
    exp.close()

    assert read_status(run_dir) == "FINISHED"


def test_singleton_close_is_still_finished(tmp_path):
    run_dir = expman.init("ok_singleton", base_dir=str(tmp_path)).run_dir
    expman.close()

    assert read_status(run_dir) == "FINISHED"


def test_context_manager_success_is_finished(tmp_path):
    with expman.Experiment("ctx_ok", base_dir=str(tmp_path)) as exp:
        run_dir = exp.run_dir

    assert read_status(run_dir) == "FINISHED"


# ─── Explicit failure ────────────────────────────────────────────────────────


def test_close_accepts_an_explicit_status(tmp_path):
    exp = expman.Experiment("explicit_fail", base_dir=str(tmp_path))
    run_dir = exp.run_dir
    exp.close(status="failed")  # lowercase on purpose

    assert read_status(run_dir) == "FAILED"


def test_close_accepts_crashed(tmp_path):
    exp = expman.Experiment("explicit_crash", base_dir=str(tmp_path))
    run_dir = exp.run_dir
    exp.close(status="CRASHED")

    assert read_status(run_dir) == "CRASHED"


def test_fail_records_status_and_reason(tmp_path):
    exp = expman.Experiment("fail_reason", base_dir=str(tmp_path))
    run_dir = exp.run_dir
    exp.fail(reason="loss diverged at step 4200")

    assert read_status(run_dir) == "FAILED"
    # The reason goes to run.log, which the dashboard console renders. run.yaml
    # has no field for it and description belongs to the user.
    assert "loss diverged at step 4200" in read_run_log(run_dir)


def test_singleton_fail(tmp_path):
    run_dir = expman.init("singleton_fail", base_dir=str(tmp_path)).run_dir
    expman.fail(reason="OOM")

    assert read_status(run_dir) == "FAILED"
    assert "OOM" in read_run_log(run_dir)


def test_singleton_close_takes_a_status(tmp_path):
    run_dir = expman.init("singleton_status", base_dir=str(tmp_path)).run_dir
    expman.close(status="crashed")

    assert read_status(run_dir) == "CRASHED"


def test_context_manager_failure_records_the_exception(tmp_path):
    run_dir = None
    with pytest.raises(RuntimeError):
        with expman.Experiment("ctx_fail", base_dir=str(tmp_path)) as exp:
            run_dir = exp.run_dir
            raise RuntimeError("intentional")

    assert read_status(run_dir) == "FAILED"
    log = read_run_log(run_dir)
    assert "RuntimeError: intentional" in log


# ─── A bad status is an error, not a silent FINISHED ─────────────────────────


def test_unknown_status_raises(tmp_path):
    exp = expman.Experiment("bad_status", base_dir=str(tmp_path))
    run_dir = exp.run_dir
    with pytest.raises(ValueError, match="invalid terminal run status"):
        exp.close(status="FINSIHED")

    # Still open — the typo did not close it as anything.
    assert read_status(run_dir) == "RUNNING"
    exp.close(status="FAILED")
    assert read_status(run_dir) == "FAILED"


def test_running_is_not_a_terminal_status(tmp_path):
    exp = expman.Experiment("no_running", base_dir=str(tmp_path))
    with pytest.raises(ValueError, match="invalid terminal run status"):
        exp.close(status="RUNNING")
    exp.close()


def test_pyo3_layer_rejects_a_bad_status_too(tmp_path):
    """The guard lives in Rust as well, so any binding gets it."""
    exp = expman.Experiment("pyo3_guard", base_dir=str(tmp_path))
    with pytest.raises(ValueError, match="invalid terminal run status"):
        exp._exp.close(status="nonsense")
    exp.close()


# ─── The automatic path ──────────────────────────────────────────────────────


def test_atexit_close_infers_failed_from_an_uncaught_exception(tmp_path):
    """Unit-level: the exit hook reads the interpreter's dying exception."""
    exp = expman.Experiment("infer_fail", base_dir=str(tmp_path))
    run_dir = exp.run_dir

    try:
        raise ValueError("boom")
    except ValueError as e:
        # What CPython does to sys before running atexit callbacks.
        sys.last_exc = e
        sys.last_value = e
        try:
            exp._close_at_exit()
        finally:
            del sys.last_exc
            del sys.last_value

    assert read_status(run_dir) == "FAILED"
    assert "ValueError: boom" in read_run_log(run_dir)


def test_a_stale_traceback_does_not_taint_a_clean_run(tmp_path):
    """sys.last_exc is process-global and a REPL keeps it forever.

    A run created *after* an unrelated traceback must still finish FINISHED,
    which is why Experiment snapshots the value at construction.
    """
    try:
        raise ValueError("stale, from before the run existed")
    except ValueError as e:
        sys.last_exc = e
        sys.last_value = e

    try:
        exp = expman.Experiment("stale_exc", base_dir=str(tmp_path))
        run_dir = exp.run_dir
        exp._close_at_exit()
    finally:
        del sys.last_exc
        del sys.last_value

    assert read_status(run_dir) == "FINISHED"


# ─── End to end, in a real interpreter ───────────────────────────────────────

_SCRIPT = """
import expman

exp = expman.Experiment(
    {name!r}, base_dir={base_dir!r}, heartbeat_interval_secs=0, capture_provenance=False
)
print("RUN_DIR=" + exp.run_dir, flush=True)
exp.log_vector({{"loss": 0.5}}, step=0)
{body}
"""


def run_script(tmp_path, name, body):
    """Run a real interpreter and hand back (completed process, run_dir)."""
    script = tmp_path / f"{name}.py"
    script.write_text(
        _SCRIPT.format(name=name, base_dir=str(tmp_path / "store"), body=textwrap.dedent(body))
    )
    proc = subprocess.run(
        [sys.executable, str(script)],
        capture_output=True,
        text=True,
        cwd=str(tmp_path),
    )
    marker = [ln for ln in proc.stdout.splitlines() if ln.startswith("RUN_DIR=")]
    assert marker, f"script never started:\n{proc.stdout}\n{proc.stderr}"
    return proc, marker[0][len("RUN_DIR=") :]


def test_uncaught_exception_in_a_real_process_is_not_finished(tmp_path):
    """The reported bug, end to end.

    No `close()` anywhere: the script dies, atexit runs, and the run must not
    be recorded as FINISHED.
    """
    proc, run_dir = run_script(tmp_path, "crashing", 'raise RuntimeError("training died")')

    assert proc.returncode != 0, proc.stdout + proc.stderr
    assert "RuntimeError: training died" in proc.stderr
    assert read_status(run_dir) == "FAILED"
    assert "RuntimeError: training died" in read_run_log(run_dir)


def test_explicit_fail_then_reraise(tmp_path):
    """The explicit idiom: end the run honestly, then let the traceback out."""
    proc, run_dir = run_script(
        tmp_path,
        "explicit_fail_reraise",
        """
        try:
            raise RuntimeError("died")
        except Exception as e:
            exp.fail(reason=str(e))
            raise
        """,
    )

    assert proc.returncode != 0
    assert read_status(run_dir) == "FAILED"
    assert "died" in read_run_log(run_dir)


def test_a_finally_close_still_wins_and_says_finished(tmp_path):
    """The remaining sharp edge, pinned rather than papered over.

    `try/finally: close()` runs *before* the interpreter's exception handling,
    so it closes the run FINISHED and the exit hook has nothing left to correct.
    Callers who use that idiom must close in an `except` branch instead. If this
    test ever starts failing because the status came out FAILED, the automatic
    path grew the ability to override an explicit close — which would be a
    bigger behaviour change than it looks and needs a deliberate decision.
    """
    proc, run_dir = run_script(
        tmp_path,
        "finally_close",
        """
        try:
            raise RuntimeError("died")
        finally:
            exp.close()
        """,
    )

    assert proc.returncode != 0
    assert read_status(run_dir) == "FINISHED"


def test_clean_exit_in_a_real_process_is_finished(tmp_path):
    """The automatic path must not invent failures."""
    proc, run_dir = run_script(tmp_path, "clean", "print('done')")

    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert read_status(run_dir) == "FINISHED"


def test_explicit_close_in_a_real_process_is_finished(tmp_path):
    proc, run_dir = run_script(tmp_path, "explicit_clean", "exp.close()")

    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert read_status(run_dir) == "FINISHED"


def test_keyboard_interrupt_is_not_finished(tmp_path):
    """Ctrl-C is not success either."""
    proc, run_dir = run_script(tmp_path, "interrupted", "raise KeyboardInterrupt")

    assert proc.returncode != 0
    assert read_status(run_dir) == "FAILED"
