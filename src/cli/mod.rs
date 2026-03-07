#![doc = include_str!("./README.md")]
//! Library backing the [`exp`](../exp/index.html) binary.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Table};

#[cfg(feature = "server")]
use crate::api::{serve, ServerConfig};
use crate::core::storage;
use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .try_init();
}

pub async fn run_cli() -> Result<()> {
    let cli = <Cli as clap::Parser>::parse();
    run_with_cli(cli).await
}

async fn run_with_cli(cli: Cli) -> Result<()> {
    match cli.command {
        #[cfg(feature = "server")]
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
        Commands::Import { dir, input } => {
            cmd_import(dir, input)?;
        }
    }

    Ok(())
}

#[derive(Parser)]
#[command(
    name = "expman",
    about = "⚗️  expman-rs: High-performance experiment manager",
    version,
    author
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[cfg(feature = "server")]
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
        #[arg(long, short, default_value = "csv", value_parser = ["csv", "json", "tensorboard"])]
        format: String,
        /// Output file (default: stdout)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    /// Import logs from a TensorBoard directory
    Import {
        /// Path to the expman experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
        /// Path to the TensorBoard log directory or file
        input: PathBuf,
    },
}

// ─── Command implementations ──────────────────────────────────────────────────

#[cfg(feature = "server")]
pub async fn cmd_serve(dir: PathBuf, host: String, port: u16, live: bool) -> Result<()> {
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

pub fn cmd_list(dir: PathBuf, experiment: Option<String>) -> Result<()> {
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
            let run_dir = exp_dir.join(run_name.as_str());
            let meta = storage::load_run_metadata(&run_dir).unwrap_or_else(|_| {
                crate::core::models::RunMetadata {
                    name: run_name.to_string(),
                    experiment: exp_name.to_string(),
                    status: crate::core::models::RunStatus::Crashed,
                    started_at: chrono::Utc::now(),
                    ..Default::default()
                }
            });

            let duration = meta
                .duration_secs
                .map(format_duration)
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
            let exp_dir: PathBuf = dir.join(exp_name.as_str());
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

pub fn cmd_inspect(run_dir: PathBuf) -> Result<()> {
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

    // Last vectors from parquet
    let vectors_path = run_dir.join("vectors.parquet");
    if vectors_path.exists() {
        let rows = storage::read_vectors(&vectors_path)?;
        let total = rows.len();
        println!("── Last Vectors ({} total rows) ─────────", total);

        if let Some(last) = rows.last() {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(["Vector", "Value"]);
            let mut entries: Vec<(&String, &serde_json::Value)> = last.iter().collect();
            entries.sort_by_key(|(k, _)| k.as_str());
            for (k, v) in entries {
                let k_str: String = k.to_string();
                let v_str: String = v.to_string();
                table.add_row(vec![k_str, v_str]);
            }
            println!("{}", table);
        }
    }

    // Scalars from metadata
    if let Some(scalars) = meta.scalars {
        println!("── Scalars ─────────────────────────────");
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(["Scalar", "Value"]);
        let mut entries: Vec<(&String, &crate::core::models::MetricValue)> =
            scalars.iter().collect();
        entries.sort_by_key(|(k, _): &(&String, &crate::core::models::MetricValue)| k.as_str());
        for (k, v) in entries {
            let k_str: String = k.to_string();
            let v_str: String = v.to_string();
            table.add_row(comfy_table::Row::from(vec![k_str, v_str]));
        }
        println!("{}", table);
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

pub fn cmd_clean(dir: PathBuf, experiment: String, keep: usize, force: bool) -> Result<()> {
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

/// Export metrics from a run to CSV, JSON, or TensorBoard format.
///
/// Reads `vectors.parquet` from the given run directory and converts the
/// data to the requested output format.
///
/// # Supported formats
/// - `csv` — comma-separated values
/// - `json` — pretty-printed JSON array
/// - `tensorboard` — TensorBoard event files (written via `tensorboard-rs`)
///
/// # Arguments
/// * `run_dir` - Path to the run directory containing `vectors.parquet`
/// * `format` - Output format: `"csv"`, `"json"`, or `"tensorboard"`
/// * `output` - Destination path. For CSV/JSON: file path. For TensorBoard:
///   directory path. If `None`, CSV/JSON are printed to stdout.
///
/// # Errors
/// Returns an error if no `vectors.parquet` exists in the run directory.
pub fn cmd_export(run_dir: PathBuf, format: String, output: Option<PathBuf>) -> Result<()> {
    let vectors_path = run_dir.join("vectors.parquet");
    if !vectors_path.exists() {
        anyhow::bail!("No vectors.parquet found in {}", run_dir.display());
    }

    let rows = storage::read_vectors(&vectors_path)?;

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
                        .map(|k| {
                            row.get(k as &String)
                                .map(|v: &serde_json::Value| v.to_string())
                                .unwrap_or_default()
                        })
                        .collect();
                    out += &(vals.join(",") + "\n");
                }
                out
            }
        }
        "tensorboard" => {
            let out_dir = output.clone().unwrap_or_else(|| PathBuf::from("tb_logs"));
            std::fs::create_dir_all(&out_dir)?;
            let out_dir_str = out_dir.to_string_lossy().to_string();
            let mut writer = tensorboard_rs::summary_writer::SummaryWriter::new(&out_dir_str);
            for row in &rows {
                let step = row.get("step").and_then(|v| v.as_i64()).unwrap_or(0);
                for (k, v) in row {
                    if k == "step" || k == "timestamp" {
                        continue;
                    }
                    if let Some(val) = v.as_f64() {
                        writer.add_scalar(k, val as f32, step as usize);
                    }
                }
            }
            writer.flush();
            "TensorBoard logs generated.\n".to_string()
        }
        _ => anyhow::bail!("Unknown format: {}", format),
    };

    match output {
        Some(path) if format != "tensorboard" => {
            std::fs::write(&path, &content)?;
            println!("Exported {} rows to {}", rows.len(), path.display());
        }
        Some(path) => println!("Exported TensorBoard logs to {}", path.display()),
        None => print!("{}", content),
    }

    Ok(())
}

/// Import TensorBoard event logs into an expman experiment.
///
/// Reads scalar summaries from `tfevents` files in the given `input` directory
/// (or a single event file) and creates a new expman run under `dir/<input_basename>`.
///
/// # Arguments
/// * `dir` - Base experiments directory (e.g. `./experiments`)
/// * `input` - Path to a TensorBoard log directory or a single `tfevents` file
///
/// # Errors
/// Returns an error if the input path doesn't exist, no `tfevents` file is found,
/// or the event file cannot be parsed.
pub fn cmd_import(dir: PathBuf, input: PathBuf) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("Input path does not exist: {}", input.display());
    }

    // Try to find an events file if it's a directory
    let file_path = if input.is_dir() {
        let mut events_file = None;
        for entry in std::fs::read_dir(&input)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().contains("tfevents") {
                events_file = Some(entry.path());
                break;
            }
        }
        events_file.ok_or_else(|| anyhow::anyhow!("No tfevents file found in directory"))?
    } else {
        input.clone()
    };

    let exp_name = input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("imported_tb");

    // Create new experiment and run
    let exp_dir = dir.join(exp_name);
    let run_name = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let run_dir = exp_dir.join(&run_name);
    std::fs::create_dir_all(&run_dir)?;

    let file = std::fs::File::open(&file_path)?;
    let reader = tboard::SummaryReader::new(file);
    let mut row_map: std::collections::BTreeMap<
        i64,
        std::collections::HashMap<String, crate::core::models::MetricValue>,
    > = std::collections::BTreeMap::new();

    for event in reader.flatten() {
        let step = event.step;
        let entry = row_map.entry(step).or_default();

        if let Some(tboard::tensorboard::event::What::Summary(summary)) = event.what {
            for value in summary.value {
                if let Some(tboard::tensorboard::summary::value::Value::SimpleValue(val)) =
                    value.value
                {
                    entry.insert(
                        value.tag,
                        crate::core::models::MetricValue::Float(val as f64),
                    );
                }
            }
        }
    }

    let mut rows = Vec::new();
    for (step, map) in row_map {
        rows.push(crate::core::models::VectorRow::new(map, Some(step as u64)));
    }

    if !rows.is_empty() {
        let vectors_path = run_dir.join("vectors.parquet");
        storage::append_vectors(&vectors_path, &rows)?;
        println!(
            "Imported {} steps from TensorBoard to {}/{}",
            rows.len(),
            exp_name,
            run_name
        );
    } else {
        println!("No scalar metrics found in TensorBoard logs.");
    }

    Ok(())
}

// ─── Utilities ────────────────────────────────────────────────────────────────

pub fn format_duration(secs: f64) -> String {
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
