import os

from expman.tensorboard import SummaryWriter


def test_summary_writer_creates_expman_run(tmp_path):
    # tb creates log dir
    log_dir = str(tmp_path / "test_tb_run")
    writer = SummaryWriter(log_dir=log_dir)

    # log scalars
    writer.add_scalar("loss", 0.5, 0)
    writer.add_scalar("loss", 0.4, 1)

    writer.add_scalars("metrics", {"accuracy": 0.9, "precision": 0.8}, 0)

    writer.add_text("test", "hello world", 0)

    writer.close()

    # Check if expman directories were created
    assert os.path.isdir(str(tmp_path))
    # It should have created 'test_tb_run' or something similar inside tmp_path

    exp_dir = os.path.join(tmp_path, "test_tb_run")
    assert os.path.exists(exp_dir)
    # The directory should contain the runs and 'experiment.yaml'
    runs = [d for d in os.listdir(exp_dir) if os.path.isdir(os.path.join(exp_dir, d))]
    assert len(runs) == 1

    run_dir = os.path.join(exp_dir, runs[0])
    assert os.path.exists(os.path.join(run_dir, "vectors.parquet"))
