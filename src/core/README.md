# ExpMan Core Logging Engine

The `core` module contains the high-performance, asynchronous logging engine that forms the heart of `expman-rs`.

## Key Features

- **Non-blocking Architecture**: All logging operations (`log_vector`, `log_scalar`, etc.) are performed via asynchronous channels, ensuring that your experiment's execution is never delayed by I/O.
- **Efficient Storage**: Data is stored using the Apache Arrow format and persisted as Parquet files, providing excellent compression and fast analytical query performance.
- **Batched I/O**: A background task manages batched writes to disk, optimizing throughput and reducing filesystem overhead.

## Components

- **`engine`**: The main `LoggingEngine` implementation.
- **`storage`**: Lower-level primitives for interacting with the experiment filesystem and Parquet/Arrow data.
- **`models`**: Shared data structures used across the entire crate.
- **`error`**: Centralized error handling for the core engine.

## Usage

```rust
use expman::core::{ExperimentConfig, LoggingEngine, RunStatus};

fn main() -> anyhow::Result<()> {
   let config = ExperimentConfig::new("my_experiment", "./experiments");
   let engine = LoggingEngine::new(config)?;

   engine.log_vector([("accuracy".to_string(), 0.95.into())].into(), Some(1));

   engine.close(RunStatus::Finished);
   Ok(())
}
```
