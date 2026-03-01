#![doc = include_str!("../README.md")]
//! expman CLI: friendly command-line interface for experiment management.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use expman_cli::{cmd_clean, cmd_export, cmd_inspect, cmd_list, cmd_serve, Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            dir,
            host,
            port,
            no_live,
        } => {
            cmd_serve(dir, host, port, !no_live).await?;
        }
        Commands::List { dir, experiment } => {
            cmd_list(dir, experiment)?;
        }
        Commands::Inspect { run_dir } => {
            cmd_inspect(run_dir)?;
        }
        Commands::Clean {
            experiment,
            dir,
            keep,
            force,
        } => {
            cmd_clean(dir, experiment, keep, force)?;
        }
        Commands::Export {
            run_dir,
            format,
            output,
        } => {
            cmd_export(run_dir, format, output)?;
        }
    }

    Ok(())
}
