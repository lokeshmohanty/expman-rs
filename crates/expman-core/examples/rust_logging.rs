//! Example of using expman-core directly from Rust.

use expman_core::{ExperimentConfig, LoggingEngine, MetricValue, RunStatus};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup configuration
    let config = ExperimentConfig::new("rust_benchmark", "./experiments");

    // 2. Initialize the logging engine
    // This starts the background tokio task
    let engine = LoggingEngine::new(config)?;
    println!("Started Rust experiment: {}", engine.config().run_name);

    // 3. Log some initial parameters
    let mut params = HashMap::new();
    params.insert(
        "language".to_string(),
        serde_yaml::Value::String("Rust".to_string()),
    );
    params.insert("threads".to_string(), serde_yaml::Value::Number(1.into()));
    engine.log_params(params);

    // 4. Simulate metrics logging
    for i in 0..50 {
        let mut metrics = HashMap::new();
        metrics.insert(
            "sine".to_string(),
            MetricValue::Float((i as f64 * 0.1).sin()),
        );
        metrics.insert(
            "cosine".to_string(),
            MetricValue::Float((i as f64 * 0.1).cos()),
        );

        // Non-blocking log_metrics
        engine.log_metrics(metrics, Some(i));

        thread::sleep(Duration::from_millis(50));
    }

    // 5. Graceful shutdown
    println!("Finishing experiment...");
    engine.close(RunStatus::Finished);

    Ok(())
}
