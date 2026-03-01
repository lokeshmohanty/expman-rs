# Expman Py

PyO3 Python extension module for `expman-rs`.

## Overview

This module exposes the `Experiment` class to Python, allowing Python applications and scripts to leverage the high-performance logging capabilities of `expman-rs`. All I/O operations are non-blocking, ensuring that Python's Global Interpreter Lock (GIL) and the main experiment loop are never blocked.

## Key Features

- **No Performance Penalty:** `log_metrics()` executes in ~100ns as it just sends data across a Rust channel. It does not wait for disk I/O.
- **Pythonic Interface:** Built with PyO3, the API looks and feels like native Python, supporting both object-oriented mapping and context managers.
- **Automatic Environment Tracking:** Automatically records the Python executable's path and environment details.
- **Safe Resource Management:** Automatically flushes queued metrics and marks runs as finished or failed through Python's `__del__` and `__exit__` hooks.

## Usage Examples

### Method 1: Using a Context Manager (Recommended)

Using a context manager ensures a clear scope for your experiment. When the block exits, the experiment is automatically closed and pending data is safely flushed. If an exception occurs within the block, the run will be marked as "Failed".

```python
from expman import Experiment
import time
import random

# Initialize the experiment
# name: "mnist_classifier"
# base_dir: "./experiments" (default)
with Experiment("mnist_classifier") as exp:
    
    # 1. Log configuration parameters
    exp.log_params({
        "learning_rate": 0.005,
        "batch_size": 64,
        "model": "CNN"
    })
    
    exp.info("Starting training loop...")

    # 2. Main training loop
    for epoch in range(10):
        # Simulate training time
        time.sleep(0.1)
        
        loss = 1.0 / (epoch + 1) + random.uniform(0, 0.1)
        acc = epoch * 10 + random.uniform(0, 5)

        # 3. Log metrics (non-blocking!)
        exp.log_metrics({
            "train_loss": loss,
            "train_acc": acc
        }, step=epoch)
        
    exp.info("Training complete!")
    
    # Optional: You can access the specific run directory generated
    print(f"Run data saved to: {exp.run_dir}")
```

### Method 2: Global Singleton / Object Instantiation

If you have a complex script where a context manager is inconvenient, you can instantiate the `Experiment` directly. It will attempt to flush cleanly when the script exits or when the object is garbage-collected.

```python
import expman

# Initialize globally
# You can optionally specify a custom 'run_name' instead of relying on the auto-generated timestamp
exp = expman.Experiment("resnet_cifar10", run_name="test_run_01")

exp.log_params({"lr": 0.001})

for step in range(100):
    exp.log_metrics({"loss": 0.5 - (step * 0.001)}, step=step)

# The run will auto-close on script exit.
# Alternatively, you can explicitly close it:
# exp.close()
```

### Saving Artifacts

You can also save arbitrary files (e.g., model checkpoints, generated images) to the experiment's artifact directory asynchronously:

```python
from expman import Experiment

with Experiment("artifact_demo") as exp:
    # ... training code ...
    
    # Save a model weights file
    # This copies the file locally to the run's artifacts/ folder
    exp.save_artifact("model_weights.pt")
```
