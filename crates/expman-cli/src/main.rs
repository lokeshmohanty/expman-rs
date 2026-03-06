#![doc = include_str!("../README.md")]
//! expman CLI: friendly command-line interface for experiment management.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    expman_cli::init_tracing();
    expman_cli::run_cli().await
}
