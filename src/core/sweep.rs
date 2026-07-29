//! Hyperparameter sweeps: expand a config into trials, then run them.
//!
//! A sweep is a **group** of runs (see `RunMetadata::group`), one per trial. It
//! needs no new storage concept and no server: expanding the grid is pure
//! computation, and each trial is an ordinary run that happens to share a group
//! and carry its parameters as tags.
//!
//! ## Two backends, because a laptop and a cluster are different problems
//!
//! `exp sweep run` executes trials locally with a concurrency cap. `exp sweep
//! slurm` emits an sbatch array and stops — the scheduler is better at queuing,
//! preemption and fair-share than any agent we would write, and on a cluster it
//! is the only thing allowed to place work.
//!
//! ## Determinism
//!
//! Random search uses a seeded SplitMix64 implemented here rather than the
//! `rand` crate. That is not NIH: it means a sweep re-expanded next year yields
//! the same trials, which a dependency's version bump cannot promise.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::error::{ExpmanError, Result};

/// How to explore the parameter space.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchMethod {
    /// Every combination of every `values` list.
    #[default]
    Grid,
    /// `trials` independent samples.
    Random,
}

/// One parameter's domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Discrete choices. Required for grid search.
    #[serde(default)]
    pub values: Option<Vec<serde_yaml::Value>>,
    /// Continuous lower bound (random search only).
    #[serde(default)]
    pub min: Option<f64>,
    /// Continuous upper bound (random search only).
    #[serde(default)]
    pub max: Option<f64>,
    /// Sample the exponent rather than the value — right for learning rates and
    /// weight decays, where 1e-5..1e-2 should not be dominated by the top decade.
    #[serde(default)]
    pub log: bool,
}

/// Which metric decides that one trial beat another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricGoal {
    pub name: String,
    #[serde(default)]
    pub goal: Goal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Goal {
    #[default]
    Minimize,
    Maximize,
}

/// A sweep definition, as written in YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepConfig {
    /// Sweep id. Becomes the group every trial belongs to.
    pub name: String,
    /// Experiment the trials are logged under.
    pub experiment: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub method: SearchMethod,
    /// Number of trials for random search. Ignored by grid.
    #[serde(default)]
    pub trials: Option<usize>,
    #[serde(default)]
    pub seed: Option<u64>,
    /// Command template. `{param}` placeholders are substituted per trial.
    pub command: String,
    pub params: BTreeMap<String, ParamSpec>,
    #[serde(default)]
    pub metric: Option<MetricGoal>,
}

/// One point in the space, with everything needed to launch it.
#[derive(Debug, Clone, Serialize)]
pub struct Trial {
    pub index: usize,
    pub run_name: String,
    pub params: BTreeMap<String, serde_yaml::Value>,
}

impl Trial {
    /// The command to execute, with `{param}` placeholders filled in.
    pub fn command(&self, template: &str) -> String {
        let mut out = template.to_string();
        for (key, value) in &self.params {
            out = out.replace(&format!("{{{key}}}"), &scalar_to_string(value));
        }
        out
    }

    /// Environment for the trial process.
    ///
    /// Params arrive both ways deliberately: the template suits an existing
    /// argparse script unchanged, while the env vars suit a shell wrapper or an
    /// sbatch script that never sees the command string.
    pub fn env(&self, sweep: &SweepConfig) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        for (key, value) in &self.params {
            env.insert(
                format!("EXPMAN_PARAM_{}", key.to_uppercase().replace('-', "_")),
                scalar_to_string(value),
            );
        }
        env.insert("EXPMAN_SWEEP".to_string(), sweep.name.clone());
        // Params as tags: this is what makes a sweep filterable and faceted in
        // the dashboard rather than a flat list of opaque trial names.
        env.insert("EXPMAN_TAGS".to_string(), self.tags().join(","));
        // The Python side reads these to place the run in the sweep's group
        // without the training script needing to know it is in a sweep.
        env.insert("EXPMAN_GROUP".to_string(), sweep.name.clone());
        env.insert("EXPMAN_RANK".to_string(), self.index.to_string());
        env.insert("EXPMAN_RUN_NAME".to_string(), self.run_name.clone());
        env.insert("EXPMAN_EXPERIMENT".to_string(), sweep.experiment.clone());
        if let Some(project) = &sweep.project {
            env.insert("EXPMAN_PROJECT".to_string(), project.clone());
        }
        env
    }

    /// Tags describing this point, so the runs table can be faceted by param.
    pub fn tags(&self) -> Vec<String> {
        self.params
            .iter()
            .map(|(k, v)| format!("{k}:{}", scalar_to_string(v)))
            .collect()
    }
}

/// Render a YAML scalar the way a command line expects it.
fn scalar_to_string(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

/// Deterministic PRNG. See the module docs for why this is not `rand`.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    fn next_f64(&mut self) -> f64 {
        // Top 53 bits: exactly the mantissa width of an f64.
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl SweepConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_yaml::from_str(&content)
            .map_err(|e| ExpmanError::Other(format!("could not parse sweep config: {e}")))
    }

    /// Expand into concrete trials.
    pub fn expand(&self) -> Result<Vec<Trial>> {
        if self.params.is_empty() {
            return Err(ExpmanError::Other("sweep declares no params".into()));
        }
        let points = match self.method {
            SearchMethod::Grid => self.expand_grid()?,
            SearchMethod::Random => self.expand_random()?,
        };
        Ok(points
            .into_iter()
            .enumerate()
            .map(|(index, params)| Trial {
                index,
                run_name: format!("{}-{index:04}", self.name),
                params,
            })
            .collect())
    }

    fn expand_grid(&self) -> Result<Vec<BTreeMap<String, serde_yaml::Value>>> {
        let mut combos: Vec<BTreeMap<String, serde_yaml::Value>> = vec![BTreeMap::new()];
        for (name, spec) in &self.params {
            let Some(values) = &spec.values else {
                return Err(ExpmanError::Other(format!(
                    "grid search needs explicit `values` for '{name}'; \
                     min/max is continuous and only works with method: random"
                )));
            };
            if values.is_empty() {
                return Err(ExpmanError::Other(format!("'{name}' has no values")));
            }
            combos = combos
                .into_iter()
                .flat_map(|base| {
                    values.iter().map(move |v| {
                        let mut next = base.clone();
                        next.insert(name.clone(), v.clone());
                        next
                    })
                })
                .collect();
        }
        Ok(combos)
    }

    fn expand_random(&self) -> Result<Vec<BTreeMap<String, serde_yaml::Value>>> {
        let trials = self
            .trials
            .ok_or_else(|| ExpmanError::Other("random search needs `trials: N`".into()))?;
        let mut rng = SplitMix64(self.seed.unwrap_or(0).wrapping_add(0x5EED));
        let mut out = Vec::with_capacity(trials);

        for _ in 0..trials {
            let mut point = BTreeMap::new();
            for (name, spec) in &self.params {
                let value = if let Some(values) = &spec.values {
                    if values.is_empty() {
                        return Err(ExpmanError::Other(format!("'{name}' has no values")));
                    }
                    let idx = (rng.next_f64() * values.len() as f64) as usize;
                    values[idx.min(values.len() - 1)].clone()
                } else if let (Some(min), Some(max)) = (spec.min, spec.max) {
                    if max < min {
                        return Err(ExpmanError::Other(format!(
                            "'{name}' has max ({max}) below min ({min})"
                        )));
                    }
                    let u = rng.next_f64();
                    let raw = if spec.log {
                        if min <= 0.0 {
                            return Err(ExpmanError::Other(format!(
                                "'{name}' is log-scaled so min must be > 0, got {min}"
                            )));
                        }
                        (min.ln() + u * (max.ln() - min.ln())).exp()
                    } else {
                        min + u * (max - min)
                    };
                    serde_yaml::Value::Number(serde_yaml::Number::from(round_significant(raw, 6)))
                } else {
                    return Err(ExpmanError::Other(format!(
                        "'{name}' needs either `values` or both `min` and `max`"
                    )));
                };
                point.insert(name.clone(), value);
            }
            out.push(point);
        }
        Ok(out)
    }
}

/// Trim float noise so a sampled `0.0030000000000000005` reads as `0.003` in a
/// command line, a tag and a directory name.
fn round_significant(value: f64, digits: i32) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    let magnitude = value.abs().log10().floor();
    let factor = 10f64.powi(digits - 1 - magnitude as i32);
    (value * factor).round() / factor
}

/// Render an sbatch array script for a sweep.
///
/// One array task per trial: SLURM then owns queueing and placement, and
/// `scancel` on the array id kills the whole sweep.
pub fn render_sbatch(
    sweep: &SweepConfig,
    trials: &[Trial],
    base_dir: &Path,
    options: &SlurmOptions,
) -> String {
    let mut out = String::new();
    out.push_str("#!/bin/bash\n");
    out.push_str(&format!("#SBATCH --job-name={}\n", sweep.name));
    out.push_str(&format!(
        "#SBATCH --array=0-{}{}\n",
        trials.len().saturating_sub(1),
        options
            .max_concurrent
            .map(|n| format!("%{n}"))
            .unwrap_or_default()
    ));
    out.push_str(&format!(
        "#SBATCH --output={}/%x-%A_%a.out\n",
        options.log_dir.as_deref().unwrap_or("slurm-logs")
    ));
    for (flag, value) in [
        ("partition", &options.partition),
        ("time", &options.time),
        ("gpus", &options.gpus),
        ("cpus-per-task", &options.cpus),
        ("mem", &options.mem),
    ] {
        if let Some(v) = value {
            out.push_str(&format!("#SBATCH --{flag}={v}\n"));
        }
    }
    for extra in &options.extra {
        out.push_str(&format!("#SBATCH {extra}\n"));
    }

    out.push_str("\nset -euo pipefail\n\n");
    out.push_str("# Generated by `exp sweep slurm`. Each array task is one trial.\n");
    out.push_str(&format!("export EXPMAN_SWEEP={}\n", sweep.name));
    out.push_str(&format!("export EXPMAN_GROUP={}\n", sweep.name));
    out.push_str(&format!("export EXPMAN_EXPERIMENT={}\n", sweep.experiment));
    if let Some(project) = &sweep.project {
        out.push_str(&format!("export EXPMAN_PROJECT={project}\n"));
    }
    out.push_str(&format!("export EXPMAN_BASE_DIR={}\n", base_dir.display()));
    out.push_str("export EXPMAN_RANK=$SLURM_ARRAY_TASK_ID\n\n");

    // A case rather than a params file: the script stays self-contained, so it
    // still runs correctly if the sweep config is edited or moved afterwards.
    out.push_str("case $SLURM_ARRAY_TASK_ID in\n");
    for trial in trials {
        out.push_str(&format!("  {})\n", trial.index));
        out.push_str(&format!("    export EXPMAN_RUN_NAME={}\n", trial.run_name));
        // Params and the derived tags. Without EXPMAN_TAGS a SLURM sweep would
        // produce runs that cannot be faceted, unlike the same sweep run locally.
        for (key, value) in trial.env(sweep) {
            if key.starts_with("EXPMAN_PARAM_") || key == "EXPMAN_TAGS" {
                out.push_str(&format!("    export {key}={}\n", shell_quote(&value)));
            }
        }
        out.push_str(&format!("    {}\n", trial.command(&sweep.command)));
        out.push_str("    ;;\n");
    }
    out.push_str(
        "  *)\n    echo \"No such trial: $SLURM_ARRAY_TASK_ID\" >&2\n    exit 1\n    ;;\n",
    );
    out.push_str("esac\n");
    out
}

/// Single-quote for the shell, escaping any embedded quote.
fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_alphanumeric() || "._-/=:".contains(c))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

/// sbatch directives for a generated sweep script.
#[derive(Debug, Clone, Default)]
pub struct SlurmOptions {
    pub partition: Option<String>,
    pub time: Option<String>,
    pub gpus: Option<String>,
    pub cpus: Option<String>,
    pub mem: Option<String>,
    pub log_dir: Option<String>,
    /// `%N` on the array — caps how many trials run at once.
    pub max_concurrent: Option<usize>,
    /// Verbatim extra `#SBATCH` lines.
    pub extra: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(method: SearchMethod) -> SweepConfig {
        let mut params = BTreeMap::new();
        params.insert(
            "lr".to_string(),
            ParamSpec {
                values: Some(vec![
                    serde_yaml::Value::Number(0.1.into()),
                    serde_yaml::Value::Number(0.01.into()),
                ]),
                min: None,
                max: None,
                log: false,
            },
        );
        params.insert(
            "bs".to_string(),
            ParamSpec {
                values: Some(vec![
                    serde_yaml::Value::Number(16.into()),
                    serde_yaml::Value::Number(32.into()),
                ]),
                min: None,
                max: None,
                log: false,
            },
        );
        SweepConfig {
            name: "s1".into(),
            experiment: "e1".into(),
            project: None,
            method,
            trials: Some(5),
            seed: Some(42),
            command: "python train.py --lr {lr} --bs {bs}".into(),
            params,
            metric: None,
        }
    }

    #[test]
    fn grid_is_the_full_cartesian_product() {
        let trials = config(SearchMethod::Grid).expand().unwrap();
        assert_eq!(trials.len(), 4);
        let mut seen: Vec<String> = trials
            .iter()
            .map(|t| {
                format!(
                    "{}/{}",
                    t.params["lr"].as_f64().unwrap(),
                    t.params["bs"].as_i64().unwrap()
                )
            })
            .collect();
        seen.sort();
        assert_eq!(seen, vec!["0.01/16", "0.01/32", "0.1/16", "0.1/32"]);
    }

    #[test]
    fn grid_refuses_a_continuous_param_instead_of_guessing() {
        let mut cfg = config(SearchMethod::Grid);
        cfg.params.insert(
            "dropout".into(),
            ParamSpec {
                values: None,
                min: Some(0.0),
                max: Some(0.5),
                log: false,
            },
        );
        let err = cfg.expand().unwrap_err().to_string();
        assert!(err.contains("dropout"), "{err}");
        assert!(err.contains("random"), "error should say what to do: {err}");
    }

    #[test]
    fn random_search_is_reproducible_from_its_seed() {
        let cfg = config(SearchMethod::Random);
        let a = cfg.expand().unwrap();
        let b = cfg.expand().unwrap();
        assert_eq!(a.len(), 5);
        let key =
            |t: &[Trial]| -> Vec<String> { t.iter().map(|t| format!("{:?}", t.params)).collect() };
        assert_eq!(key(&a), key(&b), "same seed must give the same trials");

        let mut other = config(SearchMethod::Random);
        other.seed = Some(43);
        assert_ne!(key(&a), key(&other.expand().unwrap()));
    }

    #[test]
    fn log_scale_sampling_stays_in_range_and_spans_decades() {
        let mut cfg = config(SearchMethod::Random);
        cfg.trials = Some(200);
        cfg.params.clear();
        cfg.params.insert(
            "lr".into(),
            ParamSpec {
                values: None,
                min: Some(1e-5),
                max: Some(1e-1),
                log: true,
            },
        );
        let trials = cfg.expand().unwrap();
        let values: Vec<f64> = trials
            .iter()
            .map(|t| t.params["lr"].as_f64().unwrap())
            .collect();
        assert!(
            values.iter().all(|v| (1e-5..=1e-1).contains(v)),
            "out of range"
        );
        // Log sampling should put roughly a quarter of the mass in each decade;
        // a uniform sampler would put ~90% in the top one.
        let bottom_decade = values.iter().filter(|v| **v < 1e-4).count();
        assert!(
            bottom_decade > 20,
            "log scale should reach small values, got {bottom_decade}/200"
        );
    }

    #[test]
    fn log_scale_rejects_a_non_positive_minimum() {
        let mut cfg = config(SearchMethod::Random);
        cfg.params.clear();
        cfg.params.insert(
            "lr".into(),
            ParamSpec {
                values: None,
                min: Some(0.0),
                max: Some(0.1),
                log: true,
            },
        );
        assert!(cfg
            .expand()
            .unwrap_err()
            .to_string()
            .contains("min must be > 0"));
    }

    #[test]
    fn command_template_and_env_carry_the_same_values() {
        let cfg = config(SearchMethod::Grid);
        let trial = &cfg.expand().unwrap()[0];
        let cmd = trial.command(&cfg.command);
        assert!(cmd.starts_with("python train.py --lr "));
        assert!(!cmd.contains('{'), "every placeholder substituted: {cmd}");

        let env = trial.env(&cfg);
        assert_eq!(env["EXPMAN_GROUP"], "s1");
        assert_eq!(env["EXPMAN_RANK"], "0");
        // The same value reaches the script both ways.
        assert!(cmd.contains(&env["EXPMAN_PARAM_LR"]));
    }

    #[test]
    fn sbatch_covers_every_trial_and_quotes_values() {
        let cfg = config(SearchMethod::Grid);
        let trials = cfg.expand().unwrap();
        let script = render_sbatch(
            &cfg,
            &trials,
            Path::new("./experiments"),
            &SlurmOptions {
                partition: Some("gpu".into()),
                max_concurrent: Some(2),
                ..Default::default()
            },
        );
        assert!(script.contains("#SBATCH --array=0-3%2"));
        assert!(script.contains("#SBATCH --partition=gpu"));
        for trial in &trials {
            assert!(script.contains(&format!("EXPMAN_RUN_NAME={}", trial.run_name)));
        }
        assert!(script.contains("export EXPMAN_RANK=$SLURM_ARRAY_TASK_ID"));
        // Tags must reach SLURM trials too, or a cluster sweep is unfacetable
        // while the same sweep run locally is not.
        assert!(script.contains("EXPMAN_TAGS="), "sbatch must export tags");
    }

    #[test]
    fn shell_quoting_protects_values_with_spaces() {
        assert_eq!(shell_quote("0.001"), "0.001");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn tags_describe_the_point_for_faceting() {
        let cfg = config(SearchMethod::Grid);
        let tags = cfg.expand().unwrap()[0].tags();
        assert!(tags.iter().any(|t| t.starts_with("lr:")));
        assert!(tags.iter().any(|t| t.starts_with("bs:")));
    }
}
