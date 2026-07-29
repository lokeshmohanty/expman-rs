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
            read_only,
        } => {
            cmd_serve(dir, host, port, !no_live, read_only).await?;
        }
        Commands::Project { command } => {
            cmd_project(command)?;
        }
        Commands::Sweep { command } => {
            cmd_sweep(command)?;
        }
        Commands::Probes { all } => {
            cmd_probes(all)?;
        }
        Commands::List {
            dir,
            experiment,
            project,
            group,
            tag,
            status,
            runs,
        } => {
            cmd_list(dir, experiment, project, group, tag, status, runs)?;
        }
        Commands::Reap {
            dir,
            older_than,
            project,
            experiment,
            force,
        } => {
            cmd_reap(dir, &older_than, project, experiment, force)?;
        }
        Commands::Inspect { run_dir } => {
            cmd_inspect(run_dir)?;
        }
        Commands::Clean {
            experiment,
            dir,
            project,
            group,
            tag,
            keep,
            force,
        } => {
            cmd_clean(dir, experiment, project, group, tag, keep, force)?;
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
        /// Refuse every mutating request — safe to share with a supervisor
        #[arg(long)]
        read_only: bool,
    },
    /// Manage projects: the grouping layer above experiments
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },
    /// Run hyperparameter sweeps
    Sweep {
        #[command(subcommand)]
        command: SweepCommands,
    },
    /// Show which system-metric probes are available and one live sample
    ///
    /// The answer to "why am I not seeing GPU metrics?": a probe whose binary is
    /// absent is skipped silently during a run, which is right for logging and
    /// unhelpful for debugging.
    Probes {
        /// Print every probe considered, including unavailable ones
        #[arg(long)]
        all: bool,
    },
    /// List experiments and their runs
    List {
        /// Path to experiments directory
        #[arg(default_value = "./experiments")]
        dir: PathBuf,
        /// Show runs for a specific experiment
        #[arg(long, short)]
        experiment: Option<String>,
        /// Only show experiments/runs in this project
        #[arg(long, short)]
        project: Option<String>,
        /// Only show runs in this group (a DDP job or sweep cohort)
        #[arg(long, short)]
        group: Option<String>,
        /// Filter runs by tag expression, e.g. "arm:tiered AND (study:1 OR study:2)"
        #[arg(long, short)]
        tag: Option<String>,
        /// Filter runs by status (RUNNING, FINISHED, FAILED, CRASHED)
        #[arg(long, short)]
        status: Option<String>,
        /// List matching runs across all experiments instead of grouping
        #[arg(long)]
        runs: bool,
    },
    /// Mark stale RUNNING runs as CRASHED
    ///
    /// A hard kill leaves a run RUNNING forever, silently inflating the active
    /// count. A run is stale when its heartbeat — or, for runs written before
    /// heartbeats existed, its start time — is older than --older-than.
    Reap {
        /// Path to experiments directory
        #[arg(default_value = "./experiments")]
        dir: PathBuf,
        /// Age threshold, e.g. 90s, 30m, 2h, 3d
        #[arg(long, default_value = "1h")]
        older_than: String,
        /// Only reap runs in this project
        #[arg(long, short)]
        project: Option<String>,
        /// Only reap runs of this experiment
        #[arg(long, short)]
        experiment: Option<String>,
        /// Actually rewrite run.yaml (default: dry run)
        #[arg(long)]
        force: bool,
    },
    /// Inspect a specific run: show config and last metrics
    Inspect {
        /// Path to the run directory (e.g. experiments/my_exp/runs/20240101_120000)
        run_dir: PathBuf,
    },
    /// Remove old runs, keeping the N most recent per experiment
    Clean {
        /// Experiment name. Omit to clean every experiment matched by the filters.
        experiment: Option<String>,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
        /// Only clean experiments in this project
        #[arg(long, short)]
        project: Option<String>,
        /// Only consider runs in this group
        #[arg(long, short)]
        group: Option<String>,
        /// Only consider runs matching this tag expression
        #[arg(long, short)]
        tag: Option<String>,
        /// Number of most recent runs to keep per experiment
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

#[derive(Subcommand)]
pub enum SweepCommands {
    /// Show the trials a config expands to, without running anything
    Preview {
        /// Path to the sweep YAML
        config: PathBuf,
        /// Show at most this many trials
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Run the sweep locally
    Run {
        /// Path to the sweep YAML
        config: PathBuf,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
        /// Trials to run concurrently
        #[arg(long, short = 'j', default_value_t = 1)]
        parallel: usize,
        /// Print the commands instead of executing them
        #[arg(long)]
        dry_run: bool,
    },
    /// Emit an sbatch array script for the sweep
    Slurm {
        /// Path to the sweep YAML
        config: PathBuf,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
        /// Write here instead of stdout
        #[arg(long, short)]
        output: Option<PathBuf>,
        #[arg(long)]
        partition: Option<String>,
        #[arg(long)]
        time: Option<String>,
        #[arg(long)]
        gpus: Option<String>,
        #[arg(long)]
        cpus: Option<String>,
        #[arg(long)]
        mem: Option<String>,
        /// Directory for SLURM stdout files
        #[arg(long)]
        log_dir: Option<String>,
        /// Cap concurrently running array tasks (the %N suffix)
        #[arg(long)]
        max_concurrent: Option<usize>,
        /// Extra verbatim #SBATCH lines, e.g. --sbatch "--account=abc"
        ///
        /// allow_hyphen_values, because the whole point of this flag is to pass
        /// through options that begin with `--`.
        #[arg(long = "sbatch", allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Show trial status and the best result so far
    Status {
        /// Sweep name (the group its trials share)
        name: String,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
        /// Metric to rank by; defaults to the sweep config's metric
        #[arg(long)]
        metric: Option<String>,
        /// Rank ascending (lower is better)
        #[arg(long, default_value_t = true)]
        minimize: bool,
    },
}

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// List all projects
    Ls {
        /// Path to experiments directory
        #[arg(default_value = "./experiments")]
        dir: PathBuf,
    },
    /// Create a new project
    New {
        /// Project id (directory name under .projects/)
        name: String,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
        /// Human-readable name
        #[arg(long)]
        display_name: Option<String>,
        /// One-line description
        #[arg(long)]
        description: Option<String>,
        /// Tag (repeatable)
        #[arg(long)]
        tag: Vec<String>,
    },
    /// Show a project, its metadata, and its experiments
    Show {
        /// Project id
        name: String,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
    },
    /// Assign an experiment to a project
    Assign {
        /// Experiment name
        experiment: String,
        /// Project id
        project: String,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
    },
    /// Remove an experiment from its project
    Unassign {
        /// Experiment name
        experiment: String,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
    },
    /// Delete a project, unassigning its experiments
    Rm {
        /// Project id
        name: String,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
        /// Actually delete (default: dry run)
        #[arg(long)]
        force: bool,
    },
    /// Generate projects from a YAML manifest
    ///
    /// One-way: the manifest is authoritative and each sync overwrites the
    /// project's metadata, README, and experiment membership. Synced projects
    /// are marked generated and are read-only in the dashboard.
    Sync {
        /// Path to the manifest YAML
        manifest: PathBuf,
        /// Path to experiments directory
        #[arg(long, default_value = "./experiments")]
        dir: PathBuf,
    },
}

// ─── Command implementations ──────────────────────────────────────────────────

#[cfg(feature = "server")]
pub async fn cmd_serve(
    dir: PathBuf,
    host: String,
    port: u16,
    live: bool,
    read_only: bool,
) -> Result<()> {
    println!("⚗️  ExpMan Dashboard");
    println!("   Experiments: {}", dir.display());
    println!("   URL:         http://{}:{}", host, port);
    if live {
        println!("   Live mode:   ✓ SSE streaming enabled");
    }
    if read_only {
        println!("   Read-only:   ✓ all mutating requests refused");
    }
    println!();

    let config = ServerConfig {
        base_dir: dir,
        host,
        port,
        live_mode: live,
        read_only,
    };
    serve(config).await?;
    Ok(())
}

/// Build a `RunQuery` from the filter flags shared by list/clean/reap.
fn build_query(
    experiment: Option<String>,
    project: Option<String>,
    group: Option<String>,
    tag: Option<String>,
    status: Option<String>,
) -> Result<storage::RunQuery> {
    let status = match status.as_deref() {
        None => None,
        Some(s) => Some(match s.to_ascii_uppercase().as_str() {
            "RUNNING" => crate::core::models::RunStatus::Running,
            "FINISHED" => crate::core::models::RunStatus::Finished,
            "FAILED" => crate::core::models::RunStatus::Failed,
            "CRASHED" => crate::core::models::RunStatus::Crashed,
            other => anyhow::bail!(
                "Unknown status {other:?}; expected RUNNING, FINISHED, FAILED or CRASHED"
            ),
        }),
    };
    Ok(storage::RunQuery {
        project,
        experiment,
        group,
        status,
        tags: tag
            .as_deref()
            .map(storage::parse_tag_expr)
            .unwrap_or_default(),
    })
}

/// Parse a duration like `90s`, `30m`, `2h`, `3d`. A bare number means seconds.
fn parse_duration(input: &str) -> Result<chrono::Duration> {
    let trimmed = input.trim();
    let (value, unit) = match trimmed.chars().last() {
        Some(c) if c.is_ascii_alphabetic() => (&trimmed[..trimmed.len() - 1], c),
        _ => (trimmed, 's'),
    };
    let value: i64 = value.parse().map_err(|_| {
        anyhow::anyhow!("Invalid duration {input:?}; expected e.g. 90s, 30m, 2h, 3d")
    })?;
    let secs = match unit.to_ascii_lowercase() {
        's' => value,
        'm' => value * 60,
        'h' => value * 3600,
        'd' => value * 86400,
        other => anyhow::bail!("Unknown duration unit {other:?}; expected s, m, h or d"),
    };
    Ok(chrono::Duration::seconds(secs))
}

/// List experiments, or the runs matching a set of filters.
///
/// Grouping mirrors the dashboard: without `--experiment`/`--runs` this is an
/// experiment table carrying the project each belongs to, so the CLI and the
/// dashboard agree on the hierarchy.
#[allow(clippy::too_many_arguments)]
pub fn cmd_list(
    dir: PathBuf,
    experiment: Option<String>,
    project: Option<String>,
    group: Option<String>,
    tag: Option<String>,
    status: Option<String>,
    runs: bool,
) -> Result<()> {
    let filtering_runs = tag.is_some() || status.is_some() || group.is_some();
    let query = build_query(
        experiment.clone(),
        project.clone(),
        group.clone(),
        tag,
        status,
    )?;

    // Run view: an explicit --runs, a chosen experiment, or any run-level filter.
    if runs || experiment.is_some() || filtering_runs {
        let entries = storage::query_runs(&dir, &query)?;
        if entries.is_empty() {
            println!("No runs matched.");
            return Ok(());
        }

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        // Only show the group column when something is actually grouped —
        // an all-"-" column is noise on the common single-process case.
        let show_groups = entries.iter().any(|e| e.group.is_some());
        let mut header = vec!["Run", "Experiment", "Project"];
        if show_groups {
            header.push("Group");
        }
        header.extend(["Status", "Started", "Duration", "Tags"]);
        table.set_header(header);

        for entry in &entries {
            let duration = entry
                .duration_secs
                .map(format_duration)
                .unwrap_or_else(|| "running".to_string());
            let mut row = vec![
                entry.run.clone(),
                entry.experiment.clone(),
                entry.project.clone().unwrap_or_else(|| "-".to_string()),
            ];
            if show_groups {
                row.push(match (&entry.group, entry.rank) {
                    (Some(g), Some(r)) => format!("{g}[{r}]"),
                    (Some(g), None) => g.clone(),
                    _ => "-".to_string(),
                });
            }
            row.extend([
                entry.status.clone(),
                entry.started_at.format("%Y-%m-%d %H:%M").to_string(),
                duration,
                if entry.tags.is_empty() {
                    "-".to_string()
                } else {
                    entry.tags.join(", ")
                },
            ]);
            table.add_row(row);
        }

        println!("{}", table);
        if show_groups {
            let groups: std::collections::BTreeSet<&String> =
                entries.iter().filter_map(|e| e.group.as_ref()).collect();
            println!("{} run(s) across {} group(s)", entries.len(), groups.len());
        } else {
            println!("{} run(s)", entries.len());
        }
        return Ok(());
    }

    // Experiment view.
    let experiments = storage::list_experiments(&dir)?;
    let mut rows = vec![];
    for exp_name in &experiments {
        let exp_dir: PathBuf = dir.join(exp_name.as_str());
        let meta = storage::load_experiment_metadata(&exp_dir).unwrap_or_default();
        if let Some(want) = &project {
            if meta.project.as_deref() != Some(want.as_str()) {
                continue;
            }
        }
        let run_count = storage::list_runs(&exp_dir).unwrap_or_default().len();
        rows.push((exp_name.clone(), meta, run_count));
    }

    if rows.is_empty() {
        match &project {
            Some(p) => println!("No experiments in project '{}'", p),
            None => println!("No experiments found in '{}'", dir.display()),
        }
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(["Experiment", "Project", "Runs", "Display Name"]);
    for (exp_name, meta, run_count) in &rows {
        table.add_row([
            exp_name.as_str(),
            meta.project.as_deref().unwrap_or("-"),
            &run_count.to_string(),
            meta.display_name.as_deref().unwrap_or("-"),
        ]);
    }

    println!("Experiments in: {}", dir.display());
    println!("{}", table);
    Ok(())
}

// ─── Projects ─────────────────────────────────────────────────────────────────

pub fn cmd_project(command: ProjectCommands) -> Result<()> {
    match command {
        ProjectCommands::Ls { dir } => {
            let names = storage::list_projects(&dir)?;
            if names.is_empty() {
                println!("No projects in '{}'", dir.display());
                return Ok(());
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(["Project", "Experiments", "Runs", "Source", "Display Name"]);
            for name in &names {
                let meta = storage::load_project_metadata(&dir, name).unwrap_or_default();
                let exps = storage::list_project_experiments(&dir, name).unwrap_or_default();
                let run_count: usize = exps
                    .iter()
                    .map(|e| storage::list_runs(&dir.join(e)).unwrap_or_default().len())
                    .sum();
                let source = if meta.generated {
                    meta.generated_from.as_deref().unwrap_or("generated")
                } else {
                    "-"
                };
                table.add_row([
                    name.as_str(),
                    &exps.len().to_string(),
                    &run_count.to_string(),
                    source,
                    meta.display_name.as_deref().unwrap_or("-"),
                ]);
            }
            println!("{}", table);
        }

        ProjectCommands::New {
            name,
            dir,
            display_name,
            description,
            tag,
        } => {
            if storage::project_exists(&dir, &name) {
                anyhow::bail!("Project '{}' already exists", name);
            }
            let meta = crate::core::models::ProjectMetadata {
                display_name,
                description,
                tags: tag,
                created_at: Some(chrono::Utc::now()),
                ..Default::default()
            };
            storage::save_project_metadata(&dir, &name, &meta)?;
            println!("✓ Created project '{}'", name);
        }

        ProjectCommands::Show { name, dir } => {
            if !storage::project_exists(&dir, &name) {
                anyhow::bail!("Project '{}' not found in {}", name, dir.display());
            }
            let meta = storage::load_project_metadata(&dir, &name)?;
            println!("Project: {}", name);
            if let Some(dn) = &meta.display_name {
                println!("Display name: {}", dn);
            }
            if let Some(desc) = &meta.description {
                println!("Description: {}", desc);
            }
            if !meta.tags.is_empty() {
                println!("Tags: {}", meta.tags.join(", "));
            }
            if let Some(created) = meta.created_at {
                println!("Created: {}", created.format("%Y-%m-%d %H:%M"));
            }
            if meta.generated {
                println!(
                    "Generated from: {} — read-only, regenerated on each sync",
                    meta.generated_from
                        .as_deref()
                        .unwrap_or("an external source")
                );
            }
            println!();

            let exps = storage::list_project_experiments(&dir, &name)?;
            if exps.is_empty() {
                println!("No experiments assigned.");
                return Ok(());
            }
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(["Experiment", "Runs", "Display Name"]);
            for exp in &exps {
                let exp_dir = dir.join(exp);
                let exp_meta = storage::load_experiment_metadata(&exp_dir).unwrap_or_default();
                let run_count = storage::list_runs(&exp_dir).unwrap_or_default().len();
                table.add_row([
                    exp.as_str(),
                    &run_count.to_string(),
                    exp_meta.display_name.as_deref().unwrap_or("-"),
                ]);
            }
            println!("{}", table);
        }

        ProjectCommands::Assign {
            experiment,
            project,
            dir,
        } => {
            if !storage::project_exists(&dir, &project) {
                anyhow::bail!(
                    "Project '{}' not found. Create it first: exp project new {}",
                    project,
                    project
                );
            }
            storage::set_experiment_project(&dir, &experiment, Some(&project))?;
            println!("✓ Assigned '{}' to project '{}'", experiment, project);
        }

        ProjectCommands::Unassign { experiment, dir } => {
            storage::set_experiment_project(&dir, &experiment, None)?;
            println!("✓ Unassigned '{}'", experiment);
        }

        ProjectCommands::Rm { name, dir, force } => {
            if !storage::project_exists(&dir, &name) {
                anyhow::bail!("Project '{}' not found in {}", name, dir.display());
            }
            let exps = storage::list_project_experiments(&dir, &name)?;
            println!("Will delete project '{}'.", name);
            if exps.is_empty() {
                println!("  No experiments are assigned to it.");
            } else {
                println!(
                    "  {} experiment(s) will be unassigned (runs are NOT deleted):",
                    exps.len()
                );
                for exp in &exps {
                    println!("  - {}", exp);
                }
            }
            if !force {
                println!("\nDry run. Use --force to actually delete.");
                return Ok(());
            }
            storage::delete_project(&dir, &name)?;
            println!("✓ Deleted project '{}'", name);
        }

        ProjectCommands::Sync { manifest, dir } => {
            let parsed = crate::core::projects::load_manifest(&manifest)?;
            if parsed.projects.is_empty() {
                println!("Manifest declares no projects; nothing to do.");
                return Ok(());
            }
            let reports = crate::core::projects::sync_manifest(&dir, &parsed)?;
            for report in &reports {
                println!("✓ {}", report.project);
                if !report.assigned.is_empty() {
                    println!("    assigned:   {}", report.assigned.join(", "));
                }
                if !report.unassigned.is_empty() {
                    println!("    unassigned: {}", report.unassigned.join(", "));
                }
                if !report.missing.is_empty() {
                    // Not fatal: a project may be declared before its first run.
                    println!(
                        "    not yet in store: {} (assignment recorded anyway)",
                        report.missing.join(", ")
                    );
                }
            }
            println!(
                "\nSynced {} project(s) from {}. These are read-only in the dashboard.",
                reports.len(),
                manifest.display()
            );
        }
    }
    Ok(())
}

// ─── Reaping ──────────────────────────────────────────────────────────────────

/// Mark stale `RUNNING` runs as `CRASHED`.
pub fn cmd_reap(
    dir: PathBuf,
    older_than: &str,
    project: Option<String>,
    experiment: Option<String>,
    force: bool,
) -> Result<()> {
    let max_age = parse_duration(older_than)?;
    let now = chrono::Utc::now();

    let query = storage::RunQuery {
        project,
        experiment,
        group: None,
        status: Some(crate::core::models::RunStatus::Running),
        tags: vec![],
    };

    let mut stale = vec![];
    for entry in storage::query_runs(&dir, &query)? {
        let run_dir = PathBuf::from(&entry.path);
        let meta = storage::load_run_metadata(&run_dir)?;
        if storage::is_run_stale(&meta, max_age, now) {
            let last_seen = meta.heartbeat_at.unwrap_or(meta.started_at);
            stale.push((entry, run_dir, meta, now.signed_duration_since(last_seen)));
        }
    }

    if stale.is_empty() {
        println!("No stale runs older than {}.", older_than);
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(["Run", "Experiment", "Project", "Last seen", "Source"]);
    for (entry, _, meta, age) in &stale {
        table.add_row([
            entry.run.as_str(),
            entry.experiment.as_str(),
            entry.project.as_deref().unwrap_or("-"),
            &format_duration(age.num_seconds() as f64),
            if meta.heartbeat_at.is_some() {
                "heartbeat"
            } else {
                "started_at"
            },
        ]);
    }
    println!("{}", table);
    println!("{} stale run(s) older than {}.", stale.len(), older_than);

    if !force {
        println!("\nDry run. Use --force to mark these CRASHED.");
        return Ok(());
    }

    for (entry, run_dir, mut meta, _) in stale {
        meta.status = crate::core::models::RunStatus::Crashed;
        // finished_at is when we last knew it alive, not when we noticed —
        // otherwise a run reaped a week late reports a week-long duration.
        let last_seen = meta.heartbeat_at.unwrap_or(meta.started_at);
        meta.finished_at = Some(last_seen);
        meta.duration_secs = Some(
            last_seen
                .signed_duration_since(meta.started_at)
                .num_milliseconds() as f64
                / 1000.0,
        );
        storage::save_run_metadata(&run_dir, &meta)?;
        println!("  ✓ {}/{} → CRASHED", entry.experiment, entry.run);
    }

    println!("Done.");
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
    if storage::has_metrics(&run_dir, storage::VECTORS_STEM) {
        let rows = storage::read_run_vectors(&run_dir)?;
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

/// Remove old runs, keeping the `keep` most recent **per experiment**.
///
/// With `--tag`, only matching runs are candidates and only they are counted
/// toward `keep` — so `--tag arm:tiered --keep 5` keeps five tiered runs per
/// experiment and never touches the others.
#[allow(clippy::too_many_arguments)]
pub fn cmd_clean(
    dir: PathBuf,
    experiment: Option<String>,
    project: Option<String>,
    group: Option<String>,
    tag: Option<String>,
    keep: usize,
    force: bool,
) -> Result<()> {
    if experiment.is_none() && project.is_none() && group.is_none() && tag.is_none() {
        anyhow::bail!(
            "Refusing to clean the entire store. Narrow it with an experiment name, --project, --group or --tag."
        );
    }

    let query = build_query(experiment, project, group, tag, None)?;
    let entries = storage::query_runs(&dir, &query)?;
    if entries.is_empty() {
        println!("No runs matched.");
        return Ok(());
    }

    // Group by experiment, preserving the newest-first order query_runs gives us.
    let mut by_experiment: std::collections::BTreeMap<String, Vec<&storage::RunEntry>> =
        Default::default();
    for entry in &entries {
        by_experiment
            .entry(entry.experiment.clone())
            .or_default()
            .push(entry);
    }

    let mut to_delete: Vec<&storage::RunEntry> = vec![];
    for (exp_name, runs) in &by_experiment {
        if runs.len() <= keep {
            println!(
                "Nothing to clean in '{}': {} matching run(s) (keep={})",
                exp_name,
                runs.len(),
                keep
            );
            continue;
        }
        to_delete.extend(&runs[keep..]);
    }

    if to_delete.is_empty() {
        return Ok(());
    }

    println!(
        "\nWill delete {} run(s), keeping the {} most recent per experiment:",
        to_delete.len(),
        keep
    );
    for entry in &to_delete {
        println!("  - {}/{}", entry.experiment, entry.run);
    }

    if !force {
        println!("\nDry run. Use --force to actually delete.");
        return Ok(());
    }

    for entry in &to_delete {
        std::fs::remove_dir_all(&entry.path)?;
        println!("  ✓ Deleted {}/{}", entry.experiment, entry.run);
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
    if !storage::has_metrics(&run_dir, storage::VECTORS_STEM) {
        anyhow::bail!("No vectors.parquet found in {}", run_dir.display());
    }

    let rows = storage::read_run_vectors(&run_dir)?;

    let content = match format.as_str() {
        "json" => serde_json::to_string_pretty(&rows)?,
        "csv" => rows_to_csv(&rows),
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

    if rows.is_empty() {
        println!("No scalar metrics found in TensorBoard logs.");
        return Ok(());
    }

    let vectors_path = run_dir.join("vectors.parquet");
    storage::append_vectors(&vectors_path, &rows)?;

    // Without these the run has no run.yaml, so every reader falls back to
    // `minimal_run_metadata` and the import shows up in the dashboard as
    // CRASHED. An import is a completed run, and it should read as one.
    let now = chrono::Utc::now();
    let last_vectors: std::collections::HashMap<String, crate::core::models::MetricValue> = rows
        .last()
        .map(|row| row.values.clone())
        .unwrap_or_default();

    storage::save_run_metadata(
        &run_dir,
        &crate::core::models::RunMetadata {
            name: run_name.clone(),
            experiment: exp_name.to_string(),
            status: crate::core::models::RunStatus::Finished,
            started_at: now,
            finished_at: Some(now),
            duration_secs: Some(0.0),
            description: Some(format!("Imported from TensorBoard: {}", input.display())),
            tags: Some(vec!["imported".to_string(), "tensorboard".to_string()]),
            vectors: Some(last_vectors),
            language: Some("tensorboard".to_string()),
            ..Default::default()
        },
    )?;

    let exp_meta_path = exp_dir.join("experiment.yaml");
    if !exp_meta_path.exists() {
        storage::save_experiment_metadata(
            &exp_dir,
            &crate::core::models::ExperimentMetadata {
                description: Some(format!("Imported from TensorBoard: {}", input.display())),
                tags: vec!["imported".to_string()],
                ..Default::default()
            },
        )?;
    }

    println!(
        "Imported {} steps from TensorBoard to {}/{}",
        rows.len(),
        exp_name,
        run_name
    );

    Ok(())
}

// ─── CSV ──────────────────────────────────────────────────────────────────────

/// Serialise metric rows to RFC 4180 CSV.
///
/// Two things this gets right that a naive implementation does not, and both
/// silently corrupted exports before:
///
/// 1. The header is the **union of every row's keys**, not `rows[0]`'s. A metric
///    first logged at step 500 exists in no earlier row, so a first-row header
///    dropped that column from the file entirely.
/// 2. Values are quoted and escaped. They came from `serde_json::Value::to_string()`,
///    which leaves a string containing a comma to split into two columns, and
///    wraps every string in JSON quotes that then read as part of the data.
fn rows_to_csv(rows: &[std::collections::HashMap<String, serde_json::Value>]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let mut keys: Vec<String> = rows
        .iter()
        .flat_map(|row| row.keys().cloned())
        .collect::<std::collections::BTreeSet<String>>()
        .into_iter()
        .collect();

    // step and timestamp lead; the rest stay alphabetical.
    keys.sort_by_key(|k| match k.as_str() {
        "step" => (0, String::new()),
        "timestamp" => (1, String::new()),
        other => (2, other.to_string()),
    });

    let mut out = String::new();
    out.push_str(
        &keys
            .iter()
            .map(|k| csv_escape(k))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');

    for row in rows {
        let values: Vec<String> = keys
            .iter()
            .map(|k| match row.get(k) {
                // A missing metric and an explicit null are both an empty cell —
                // any placeholder here would be indistinguishable from data.
                None | Some(serde_json::Value::Null) => String::new(),
                Some(serde_json::Value::String(s)) => csv_escape(s),
                Some(other) => csv_escape(&other.to_string()),
            })
            .collect();
        out.push_str(&values.join(","));
        out.push('\n');
    }

    out
}

/// Quote a CSV field if it contains a comma, quote, CR or LF; double inner quotes.
fn csv_escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
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

// ─── Sweeps ───────────────────────────────────────────────────────────────────

pub fn cmd_sweep(command: SweepCommands) -> Result<()> {
    use crate::core::sweep::{render_sbatch, SlurmOptions, SweepConfig};

    match command {
        SweepCommands::Preview { config, limit } => {
            let sweep = SweepConfig::load(&config)?;
            let trials = sweep.expand()?;
            println!(
                "Sweep '{}' → {} trial(s) into experiment '{}'",
                sweep.name,
                trials.len(),
                sweep.experiment
            );
            println!();

            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            let mut header = vec!["#".to_string(), "Run".to_string()];
            header.extend(sweep.params.keys().cloned());
            table.set_header(header);
            for trial in trials.iter().take(limit) {
                let mut row = vec![trial.index.to_string(), trial.run_name.clone()];
                for key in sweep.params.keys() {
                    row.push(
                        trial
                            .params
                            .get(key)
                            .map(|v| match v {
                                serde_yaml::Value::String(s) => s.clone(),
                                other => serde_yaml::to_string(other)
                                    .unwrap_or_default()
                                    .trim()
                                    .to_string(),
                            })
                            .unwrap_or_else(|| "-".to_string()),
                    );
                }
                table.add_row(row);
            }
            println!("{}", table);
            if trials.len() > limit {
                println!("… {} more (use --limit)", trials.len() - limit);
            }
            println!(
                "\nExample command:\n  {}",
                trials[0].command(&sweep.command)
            );
        }

        SweepCommands::Run {
            config,
            dir,
            parallel,
            dry_run,
        } => {
            let sweep = SweepConfig::load(&config)?;
            let trials = sweep.expand()?;
            println!(
                "Sweep '{}': {} trial(s), {} at a time",
                sweep.name,
                trials.len(),
                parallel.max(1)
            );

            if dry_run {
                for trial in &trials {
                    println!("  [{}] {}", trial.index, trial.command(&sweep.command));
                }
                println!("\nDry run. Drop --dry-run to execute.");
                return Ok(());
            }

            storage::ensure_dir(&dir)?;
            // Assign the experiment to the project up front, so the trials are
            // grouped correctly even if the first one fails immediately.
            if let Some(project) = &sweep.project {
                ensure_project(&dir, project)?;
                storage::set_experiment_project(&dir, &sweep.experiment, Some(project))?;
            }

            let mut running: Vec<(usize, std::process::Child)> = vec![];
            let mut failed = 0usize;
            let mut completed = 0usize;

            for trial in &trials {
                // Block until a slot frees up. A bounded number of concurrent
                // trials is the point — an unbounded spawn would thrash a box
                // that a sweep is already saturating.
                while running.len() >= parallel.max(1) {
                    reap_one(&mut running, &mut completed, &mut failed)?;
                }

                let rendered = trial.command(&sweep.command);
                println!("  [{}/{}] {}", trial.index + 1, trials.len(), rendered);

                let mut cmd = std::process::Command::new("sh");
                cmd.arg("-c").arg(&rendered);
                for (key, value) in trial.env(&sweep) {
                    cmd.env(key, value);
                }
                cmd.env("EXPMAN_BASE_DIR", &dir);
                match cmd.spawn() {
                    Ok(child) => running.push((trial.index, child)),
                    Err(e) => {
                        eprintln!("  ✗ trial {} failed to start: {}", trial.index, e);
                        failed += 1;
                    }
                }
            }
            while !running.is_empty() {
                reap_one(&mut running, &mut completed, &mut failed)?;
            }

            println!(
                "\nSweep '{}' finished: {} succeeded, {} failed.",
                sweep.name, completed, failed
            );
            println!("  exp list {} --group {}", dir.display(), sweep.name);
            if failed > 0 {
                anyhow::bail!("{failed} trial(s) failed");
            }
        }

        SweepCommands::Slurm {
            config,
            dir,
            output,
            partition,
            time,
            gpus,
            cpus,
            mem,
            log_dir,
            max_concurrent,
            extra,
        } => {
            let sweep = SweepConfig::load(&config)?;
            let trials = sweep.expand()?;
            let script = render_sbatch(
                &sweep,
                &trials,
                &dir,
                &SlurmOptions {
                    partition,
                    time,
                    gpus,
                    cpus,
                    mem,
                    log_dir,
                    max_concurrent,
                    extra,
                },
            );
            match output {
                Some(path) => {
                    std::fs::write(&path, &script)?;
                    // Without the executable bit the natural next step
                    // (`./sweep.sbatch`) fails confusingly.
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = std::fs::metadata(&path)?.permissions();
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&path, perms);
                    }
                    println!(
                        "Wrote {} ({} trials).\n  sbatch {}",
                        path.display(),
                        trials.len(),
                        path.display()
                    );
                }
                None => print!("{}", script),
            }
        }

        SweepCommands::Status {
            name,
            dir,
            metric,
            minimize,
        } => {
            let query = storage::RunQuery {
                group: Some(name.clone()),
                ..Default::default()
            };
            let entries = storage::query_runs(&dir, &query)?;
            if entries.is_empty() {
                println!("No trials found for sweep '{}' in {}", name, dir.display());
                return Ok(());
            }

            let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
            for entry in &entries {
                *counts.entry(entry.status.clone()).or_default() += 1;
            }
            println!("Sweep '{}': {} trial(s)", name, entries.len());
            for (status, count) in &counts {
                println!("  {status}: {count}");
            }

            // Rank by the requested metric, taking whichever of scalars/vectors
            // carries it — a run may report a final value either way.
            let Some(metric_name) = metric else {
                println!("\nPass --metric <name> to rank trials.");
                return Ok(());
            };
            let mut ranked: Vec<(&storage::RunEntry, f64)> = entries
                .iter()
                .filter_map(|entry| {
                    let value = entry
                        .scalars
                        .get(&metric_name)
                        .or_else(|| entry.vectors.get(&metric_name))?;
                    value.to_string().parse::<f64>().ok().map(|v| (entry, v))
                })
                .collect();
            if ranked.is_empty() {
                println!("\nNo trial reported metric '{}'.", metric_name);
                return Ok(());
            }
            ranked.sort_by(|a, b| {
                if minimize {
                    a.1.total_cmp(&b.1)
                } else {
                    b.1.total_cmp(&a.1)
                }
            });

            println!();
            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(["Rank", "Trial", &metric_name, "Status", "Tags"]);
            for (position, (entry, value)) in ranked.iter().take(20).enumerate() {
                table.add_row([
                    (position + 1).to_string(),
                    entry.run.clone(),
                    format!("{value}"),
                    entry.status.clone(),
                    entry.tags.join(", "),
                ]);
            }
            println!("{}", table);
            println!(
                "Best: {} with {} = {}",
                ranked[0].0.run, metric_name, ranked[0].1
            );
        }
    }
    Ok(())
}

/// Create a project if it does not exist yet.
///
/// A sweep config naming a project should not leave the experiment pointing at
/// a project that `exp project ls` cannot see — a dangling reference that shows
/// up as a grouped experiment with no group to belong to.
fn ensure_project(dir: &std::path::Path, project: &str) -> Result<()> {
    if storage::project_exists(dir, project) {
        return Ok(());
    }
    storage::save_project_metadata(
        dir,
        project,
        &crate::core::models::ProjectMetadata {
            created_at: Some(chrono::Utc::now()),
            ..Default::default()
        },
    )?;
    println!("  created project '{project}'");
    Ok(())
}

/// Wait for any one running trial to exit, recording whether it succeeded.
fn reap_one(
    running: &mut Vec<(usize, std::process::Child)>,
    completed: &mut usize,
    failed: &mut usize,
) -> Result<()> {
    if running.is_empty() {
        return Ok(());
    }
    // Poll rather than blocking on the first child: whichever finishes first
    // should free its slot, not whichever happens to be at index 0.
    loop {
        for idx in 0..running.len() {
            if let Some(status) = running[idx].1.try_wait()? {
                let (trial_idx, _) = running.remove(idx);
                if status.success() {
                    *completed += 1;
                } else {
                    *failed += 1;
                    eprintln!("  ✗ trial {trial_idx} exited with {status}");
                }
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// ─── Probes ───────────────────────────────────────────────────────────────────

/// Report probe availability and take one sample.
pub fn cmd_probes(show_all: bool) -> Result<()> {
    use crate::core::sysmetrics::{ProbeSpec, SystemSampler};

    let specs = ProbeSpec::defaults();
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(["Probe", "Prefix", "Available", "Command"]);

    let mut sampler = SystemSampler::new(specs.clone());
    let active = sampler.active_probes();

    for spec in &specs {
        let available = active.contains(&spec.command);
        if !available && !show_all {
            continue;
        }
        table.add_row([
            spec.command.clone(),
            spec.prefix.clone(),
            if available { "yes" } else { "no" }.to_string(),
            format!("{} {}", spec.command, spec.args.join(" ")),
        ]);
    }
    println!("{}", table);
    if active.is_empty() {
        println!("No hardware probes found on PATH; only CPU and memory are sampled.");
        if !show_all {
            println!("Use --all to see every probe that was considered.");
        }
    }

    // Two samples: CPU utilisation is a delta and has no value on the first.
    println!("\nSampling…");
    let _ = sampler.sample();
    std::thread::sleep(std::time::Duration::from_millis(600));
    let sample = sampler.sample();

    if sample.is_empty() {
        println!("No metrics returned.");
        return Ok(());
    }

    let mut values: Vec<(&String, &crate::core::models::MetricValue)> = sample.iter().collect();
    values.sort_by_key(|(k, _)| k.as_str());
    let mut out = Table::new();
    out.load_preset(UTF8_FULL);
    out.set_header(["Metric", "Value"]);
    for (key, value) in values {
        out.add_row([key.clone(), value.to_string()]);
    }
    println!("{}", out);
    Ok(())
}
