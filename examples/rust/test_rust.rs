use expman_core::{ExperimentConfig, LoggingEngine, MetricValue, RunStatus};
use std::collections::HashMap;

fn main() {
    let config = ExperimentConfig::new("test_jupyter_tab_rust", "./experiments");
    // Since we updated default to "rust", it should be fine.
    let engine = LoggingEngine::new(config).unwrap();

    let mut m = HashMap::new();
    m.insert("loss".to_string(), MetricValue::Float(0.123));
    engine.log_metrics(m, Some(1));

    engine.close(RunStatus::Finished);
}
