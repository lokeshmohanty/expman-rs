# Expman Core

Core storage and logging engine for `expman-rs`.

## Overview 

The `expman-core` module provides the foundational components for tracking, storing, and managing experiment data with high performance. Its central design principle ensures that logging operations (e.g., `log_metrics()`) are extremely fast (~100ns channel sends) and never block the main experiment process. 
All I/O operations are handled asynchronously via a background tokio task.

## Key Features

- **Asynchronous Logging:** Uses a tokio background task and mpsc channels to offload I/O operations, ensuring the main application thread is never blocked.
- **Efficient Storage:** Writes metrics using Apache Arrow and Parquet formats in batches, rather than inefficient per-step read-append-write cycles.
- **Strong Typing:** Provides robust data models for experiment configurations, run states, and metric values.

## Usage Example

Below is a basic example of how to use `expman-core` directly in a Rust application to track an experiment:

```rust
use expman_core::{ExperimentConfig, LoggingEngine, RunStatus};

fn main() -> anyhow::Result<()> {
    // 1. Define the experiment configuration
    // This sets up an experiment named "my_rust_exp" in the "./experiments" directory
    let config = ExperimentConfig::new("my_rust_exp", "./experiments");
    
    // 2. Initialize the logging engine
    // This spawns the background tokio task for handling I/O
    let engine = LoggingEngine::new(config)?;
    
    // 3. Log some parameters (hyperparameters, configuration, etc.)
    engine.log_params([
        ("learning_rate".to_string(), 0.001.into()),
        ("batch_size".to_string(), 32.into()),
        ("optimizer".to_string(), "Adam".into())
    ].into());

    // 4. Log metrics during your training or simulation loop
    for step in 0..100 {
        let loss = 1.0 / (step as f64 + 1.0);
        let accuracy = step as f64 / 100.0;
        
        // log_metrics takes a HashMap of metrics and an optional step counter.
        // This operation takes ~100ns and does not block!
        engine.log_metrics([
            ("loss".to_string(), loss.into()),
            ("accuracy".to_string(), accuracy.into())
        ].into(), Some(step));
    }
    
    // 5. Gracefully close the engine, ensuring all pending metrics are flushed to disk
    engine.close(RunStatus::Finished);
    
    Ok(())
}
```

## Architecture Map

- **`ExperimentConfig`**: Configures the base directory, experiment name, flush intervals, and environment metadata.
- **`LoggingEngine`**: The main entry point. It manages the background writer task and exposes non-blocking methods to send data.
- **`models`**: Contains definitions for `RunStatus`, `MetricValue` (which supports Floats, Ints, Bools, and Strings), and `MetricRow`.
- **`storage`**: Internal module that handles the actual file I/O, writing `config.yaml`, `run.yaml`, `run.log`, and `metrics.parquet`.
