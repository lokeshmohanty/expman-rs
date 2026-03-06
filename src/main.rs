#![doc = include_str!("../README.md")]
//! expman CLI: friendly command-line interface for experiment management.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    expman::cli::init_tracing();
    expman::cli::run_cli().await
}
