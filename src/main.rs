#![doc = include_str!("../README.md")]
//! expman CLI: friendly command-line interface for experiment management.

#[tokio::main]
async fn main() {
    expman::cli::init_tracing();
    if let Err(e) = expman::cli::run_cli().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
