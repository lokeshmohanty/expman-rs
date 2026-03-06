# ExpMan Python Wrapper

The `expman` Python package provides a seamless, idiomatic interface for logging experiments in Python projects while leveraging the high-performance Rust core.

## Key Features

- **Idiomatic Python**: Uses context managers and singletons for clean integration into scripts and notebooks.
- **Fast Logging**: Native calls to the Rust engine ensure sub-microsecond logging latency.
- **Automatic Cleanup**: Ensures all file handles are closed and data is flushed upon script completion.

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

## Internal Architecture

The wrapper uses `PyO3` to create native Python extension modules that link directly to the Rust `expman` library.
