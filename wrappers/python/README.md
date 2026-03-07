# ExpMan Python Wrapper

The `expman` Python package provides a seamless, idiomatic interface for logging experiments in Python projects while leveraging the high-performance Rust core.

## Key Features

- **Idiomatic Python**: Uses context managers and singletons for clean integration into scripts and notebooks.
- **Fast Logging**: Native calls to the Rust engine ensure sub-microsecond logging latency.
- **Automatic Cleanup**: Ensures all file handles are closed and data is flushed upon script completion.
- **TensorBoard Drop-in**: Replace `from torch.utils.tensorboard import SummaryWriter` with `from expman import SummaryWriter`.

## Installation

```bash
pip install expman-rs
```

## Basic Usage

### Global Singleton

```python
import expman as exp

exp.init("my_model")
exp.log_vector({"loss": 0.42})
```

### Context Manager

```python
from expman import Experiment

with Experiment("my_model") as exp:
    exp.log_vector({"loss": 0.42})
```

### TensorBoard SummaryWriter (Drop-in Replacement)

Replace your existing TensorBoard import with expman's drop-in replacement.
All metrics are stored in expman's high-performance Parquet format instead of
TensorBoard event files.

```python
# Before:
# from torch.utils.tensorboard import SummaryWriter

# After:
from expman import SummaryWriter

writer = SummaryWriter(log_dir="runs/my_experiment")

for epoch in range(100):
    loss = 1.0 / (epoch + 1)
    writer.add_scalar("train/loss", loss, epoch)

writer.add_scalars("metrics", {"accuracy": 0.95, "f1": 0.92}, 100)
writer.add_hparams({"lr": 0.001, "batch_size": 32}, {"hparam/accuracy": 0.95})

writer.close()
```

**Supported methods:**

| Method | Status | Notes |
|--------|--------|-------|
| `add_scalar` | ✅ Full | Logs as expman vector metric |
| `add_scalars` | ✅ Full | Auto-prefixes tags with main_tag |
| `add_text` | ✅ Full | Logged as info message |
| `add_hparams` | ✅ Full | Logs params + initial metrics |
| `flush` | ✅ No-op | Expman auto-flushes asynchronously |
| `close` | ✅ Full | Graceful shutdown |
| `add_histogram` | ⚠️ Stub | No-op, won't raise errors |
| `add_image/images` | ⚠️ Stub | No-op, won't raise errors |
| `add_figure` | ⚠️ Stub | No-op, won't raise errors |
| `add_video` | ⚠️ Stub | No-op, won't raise errors |
| `add_audio` | ⚠️ Stub | No-op, won't raise errors |
| `add_graph` | ⚠️ Stub | No-op, won't raise errors |
| `add_embedding` | ⚠️ Stub | No-op, won't raise errors |
| `add_pr_curve` | ⚠️ Stub | No-op, won't raise errors |
| `add_custom_scalars` | ⚠️ Stub | No-op, won't raise errors |
| `add_mesh` | ⚠️ Stub | No-op, won't raise errors |

## Internal Architecture

The wrapper uses `PyO3` to create native Python extension modules that link directly to the Rust `expman` library.
