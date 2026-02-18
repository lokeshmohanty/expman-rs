//! expman CLI: friendly command-line interface for experiment management.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Table};
use tracing_subscriber::EnvFilter;

use expman_core::storage;
use expman_server::{serve, ServerConfig};

#[derive(Parser)]
#[command(
    name = "expman",
    about = "⚗️  expman-rs: High-performance experiment manager",
    version,
    author
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the web dashboard server
    Serve {
        /// Path to experiments directory
        #[arg(default_value = "./experiments")]
        dir: PathBuf,
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind to
        #[arg(long, short, default_value_t = 8000)]
        port: u16,
        /// Disable live SSE streaming
        #[arg(long)]
        no_live: bool,
    },
    /// List experiments and their runs
    List {
        /// Path to experiments directory
        #[arg(default_value = "./experiments")]
        dir: PathBuf,
        /// Show runs for a specific experiment
        #[arg(long, short)]
        experiment: Option<String>,
    },
    /// Inspect a specific run: show config and last metrics
    Inspect {
        /// Path to the run directory (e.g. experiments/my_exp/runs/20240101_120000)
        run_dir: PathBuf,
    },
    /// Remove old runs, keeping the N most recent
    Clean {
        /// Experiment name
        experiment: String,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
        /// Number of most recent runs to keep
        #[arg(long, short, default_value_t = 5)]
        keep: usize,
        /// Actually delete (default: dry run)
        #[arg(long)]
        force: bool,
    },
    /// Export metrics from a run to CSV or JSON
    Export {
        /// Path to the run directory
        run_dir: PathBuf,
        /// Output format
        #[arg(long, short, default_value = "csv", value_parser = ["csv", "json"])]
        format: String,
        /// Output file (default: stdout)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
}

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
        Commands::Serve { dir, host, port, no_live } => {
            cmd_serve(dir, host, port, !no_live).await?;
        }
        Commands::List { dir, experiment } => {
            cmd_list(dir, experiment)?;
        }
        Commands::Inspect { run_dir } => {
            cmd_inspect(run_dir)?;
        }
        Commands::Clean { experiment, dir, keep, force } => {
            cmd_clean(dir, experiment, keep, force)?;
        }
        Commands::Export { run_dir, format, output } => {
            cmd_export(run_dir, format, output)?;
        }
    }

    Ok(())
}

// ─── Command implementations ──────────────────────────────────────────────────

async fn cmd_serve(dir: PathBuf, host: String, port: u16, live: bool) -> Result<()> {
    println!("⚗️  ExpMan Dashboard");
    println!("   Experiments: {}", dir.display());
    println!("   URL:         http://{}:{}", host, port);
    if live {
        println!("   Live mode:   ✓ SSE streaming enabled");
    }
    println!();

    let config = ServerConfig {
        base_dir: dir,
        host,
        port,
        live_mode: live,
    };
    serve(config).await?;
    Ok(())
}

fn cmd_list(dir: PathBuf, experiment: Option<String>) -> Result<()> {
    if let Some(exp_name) = experiment {
        // List runs for a specific experiment
        let exp_dir = dir.join(&exp_name);
        let runs = storage::list_runs(&exp_dir)?;

        if runs.is_empty() {
            println!("No runs found for experiment '{}'", exp_name);
            return Ok(());
        }

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(["Run", "Status", "Started", "Duration", "Description"]);

        for run_name in &runs {
            let run_dir = exp_dir.join(run_name);
            let meta = storage::load_run_metadata(&run_dir)
                .unwrap_or_else(|_| expman_core::models::RunMetadata {
                    name: run_name.clone(),
                    experiment: exp_name.clone(),
                    status: expman_core::models::RunStatus::Crashed,
                    started_at: chrono::Utc::now(),
                    finished_at: None,
                    duration_secs: None,
                    description: None,
                });

            let duration = meta
                .duration_secs
                .map(|d| format_duration(d))
                .unwrap_or_else(|| "running".to_string());

            table.add_row([
                run_name.as_str(),
                &meta.status.to_string(),
                &meta.started_at.format("%Y-%m-%d %H:%M").to_string(),
                &duration,
                meta.description.as_deref().unwrap_or("-"),
            ]);
        }

        println!("Experiment: {}", exp_name);
        println!("{}", table);
    } else {
        // List all experiments
        let experiments = storage::list_experiments(&dir)?;

        if experiments.is_empty() {
            println!("No experiments found in '{}'", dir.display());
            return Ok(());
        }

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(["Experiment", "Runs", "Display Name"]);

        for exp_name in &experiments {
            let exp_dir = dir.join(exp_name);
            let runs = storage::list_runs(&exp_dir).unwrap_or_default();
            let meta = storage::load_experiment_metadata(&exp_dir).unwrap_or_default();
            table.add_row([
                exp_name.as_str(),
                &runs.len().to_string(),
                meta.display_name.as_deref().unwrap_or("-"),
            ]);
        }

        println!("Experiments in: {}", dir.display());
        println!("{}", table);
    }

    Ok(())
}

fn cmd_inspect(run_dir: PathBuf) -> Result<()> {
    if !run_dir.exists() {
        anyhow::bail!("Run directory not found: {}", run_dir.display());
    }

    let meta = storage::load_run_metadata(&run_dir)?;
    println!("Run: {}", meta.name);
    println!("Experiment: {}", meta.experiment);
    println!("Status: {}", meta.status);
    println!("Started: {}", meta.started_at.format("%Y-%m-%d %H:%M:%S"));
    if let Some(d) = meta.duration_secs {
        println!("Duration: {}", format_duration(d));
    }
    println!();

    // Config
    let config_path = run_dir.join("config.yaml");
    if config_path.exists() {
        println!("── Config ──────────────────────────────");
        let content = std::fs::read_to_string(&config_path)?;
        println!("{}", content.trim());
        println!();
    }

    // Last metrics
    let metrics_path = run_dir.join("metrics.parquet");
    if metrics_path.exists() {
        let rows = storage::read_metrics(&metrics_path)?;
        let total = rows.len();
        println!("── Last Metrics ({} total rows) ─────────", total);

        if let Some(last) = rows.last() {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(["Metric", "Value"]);
            let mut entries: Vec<_> = last.iter().collect();
            entries.sort_by_key(|(k, _)| k.as_str());
            for (k, v) in entries {
                table.add_row([k.as_str(), &v.to_string()]);
            }
            println!("{}", table);
        }
    }

    // Artifacts
    let artifacts = storage::list_artifacts(&run_dir)?;
    if !artifacts.is_empty() {
        println!("── Artifacts ({}) ──────────────────────", artifacts.len());
        for a in &artifacts {
            println!("  {} ({} bytes)", a.path, a.size);
        }
    }

    Ok(())
}

fn cmd_clean(dir: PathBuf, experiment: String, keep: usize, force: bool) -> Result<()> {
    let exp_dir = dir.join(&experiment);
    let mut runs = storage::list_runs(&exp_dir)?;

    if runs.len() <= keep {
        println!(
            "Nothing to clean: {} has {} runs (keep={})",
            experiment,
            runs.len(),
            keep
        );
        return Ok(());
    }

    // Runs are sorted newest-first; remove the oldest ones (tail)
    let to_delete = runs.split_off(keep);

    println!(
        "Will delete {} run(s) from '{}' (keeping {} most recent):",
        to_delete.len(),
        experiment,
        keep
    );
    for run in &to_delete {
        println!("  - {}", run);
    }

    if !force {
        println!("\nDry run. Use --force to actually delete.");
        return Ok(());
    }

    for run in &to_delete {
        let run_dir = exp_dir.join(run);
        std::fs::remove_dir_all(&run_dir)?;
        println!("  ✓ Deleted {}", run);
    }

    println!("Done.");
    Ok(())
}

fn cmd_export(run_dir: PathBuf, format: String, output: Option<PathBuf>) -> Result<()> {
    let metrics_path = run_dir.join("metrics.parquet");
    if !metrics_path.exists() {
        anyhow::bail!("No metrics.parquet found in {}", run_dir.display());
    }

    let rows = storage::read_metrics(&metrics_path)?;

    let content = match format.as_str() {
        "json" => serde_json::to_string_pretty(&rows)?,
        "csv" => {
            if rows.is_empty() {
                String::new()
            } else {
                let mut keys: Vec<String> = rows[0].keys().cloned().collect();
                keys.sort();
                let mut out = keys.join(",") + "\n";
                for row in &rows {
                    let vals: Vec<String> = keys
                        .iter()
                        .map(|k| row.get(k).map(|v| v.to_string()).unwrap_or_default())
                        .collect();
                    out += &(vals.join(",") + "\n");
                }
                out
            }
        }
        _ => anyhow::bail!("Unknown format: {}", format),
    };

    match output {
        Some(path) => {
            std::fs::write(&path, &content)?;
            println!("Exported {} rows to {}", rows.len(), path.display());
        }
        None => print!("{}", content),
    }

    Ok(())
}

// ─── Utilities ────────────────────────────────────────────────────────────────

fn format_duration(secs: f64) -> String {
    let secs = secs as u64;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}h {}m", h, m)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}
