//! System metrics: hardware utilisation sampled alongside the training metrics.
//!
//! Collection is by **subprocess probe** — we shell out to the vendor's own CLI
//! (`nvidia-smi`, `rocm-smi`, `tpu-info`) rather than linking NVML or ROCm. That
//! trades a few milliseconds per sample for three things worth more: no
//! build-time dependency on a vendor SDK, no runtime `dlopen` that fails
//! differently on every cluster image, and — most importantly — a user can add
//! a probe for hardware we have never heard of by writing a config entry
//! instead of a patch.
//!
//! A probe whose binary is absent is skipped silently and permanently. A laptop
//! with no GPU logs CPU and memory and says nothing about it.
//!
//! ## Parsing strategy
//!
//! Only two parsers, because inventing a schema for each vendor is how this
//! kind of code rots:
//!
//! - **`nvidia-smi`** uses `--format=csv,noheader,nounits` with an explicit
//!   `--query-gpu` column list, so the output shape is one we requested.
//! - **Everything else JSON** is flattened by extracting *every numeric leaf*
//!   into a dotted key. This is deliberately schema-agnostic: it works for
//!   `rocm-smi --json`, for `tpu-info --json`, and for whatever a user's own
//!   tool emits, and it does not break when a vendor adds a field.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::core::models::MetricValue;

/// How to interpret a probe's stdout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeFormat {
    /// `nvidia-smi --format=csv,noheader,nounits`, one row per device.
    NvidiaCsv,
    /// Any JSON. Every numeric leaf becomes a metric, keyed by its dotted path.
    Json,
    /// `key=value` or `key: value` lines, one per line.
    KeyValue,
    /// Markdown-style pipe tables: first column identifies the device, the
    /// remaining column headers name the metrics. What `tpu-info` emits.
    PipeTable,
}

/// A configured probe. Serializable so it can come from a config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeSpec {
    /// Metric key prefix, e.g. `gpu` → `gpu.0.util_pct`.
    pub prefix: String,
    /// Program to run. Skipped entirely if not found on PATH.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub format: ProbeFormat,
    /// Column names for `NvidiaCsv`, positionally matched to the CSV fields.
    #[serde(default)]
    pub columns: Vec<String>,
}

impl ProbeSpec {
    /// NVIDIA GPUs. The query list fixes the column order, so parsing is not
    /// guessing at a format — it is reading back what we asked for.
    ///
    /// Verified end to end against a 2× RTX A6000 host on 2026-07-29; the test
    /// `nvidia_probe_matches_real_a6000_output` pins that exact output.
    pub fn nvidia() -> Self {
        Self {
            prefix: "gpu".to_string(),
            command: "nvidia-smi".to_string(),
            args: vec![
                "--query-gpu=index,utilization.gpu,utilization.memory,memory.used,memory.total,temperature.gpu,power.draw".to_string(),
                "--format=csv,noheader,nounits".to_string(),
            ],
            format: ProbeFormat::NvidiaCsv,
            columns: vec![
                "index".to_string(),
                "util_pct".to_string(),
                "mem_util_pct".to_string(),
                "mem_used_mb".to_string(),
                "mem_total_mb".to_string(),
                "temp_c".to_string(),
                "power_w".to_string(),
            ],
        }
    }

    /// AMD GPUs via `rocm-smi --json`, read through the generic JSON flattener.
    pub fn rocm() -> Self {
        Self {
            prefix: "gpu".to_string(),
            command: "rocm-smi".to_string(),
            args: vec![
                "--showuse".to_string(),
                "--showmemuse".to_string(),
                "--showpower".to_string(),
                "--showtemp".to_string(),
                "--json".to_string(),
            ],
            format: ProbeFormat::Json,
            columns: vec![],
        }
    }

    /// Google TPUs via the `tpu-info` CLI.
    ///
    /// Verified against a v6e host (`tpu-info` 0.0.21, libtpu 0.0.21) on
    /// 2026-07-29 — the earlier `--json` guess was simply wrong: `tpu-info` has
    /// **no JSON output**, and prints Rich-formatted pipe tables, one per
    /// requested metric.
    ///
    /// Only the three metrics that describe utilisation are requested. The tool
    /// exposes ~40, most of them checkpoint and gRPC timings that belong in a
    /// profile rather than in a 15-second sample.
    ///
    /// > `hbm_usage` and `duty_cycle_percent` read `N/A` unless a framework has
    /// > the TPU open — libtpu publishes them only then. That is not an error and
    /// > not worth warning about; the values simply appear once training starts.
    /// > `tensorcore_utilization` reports even when idle, which is what was used
    /// > to confirm numeric parsing against the live host.
    ///
    /// ## Why not talk to libtpu directly?
    ///
    /// Investigated on the v6e host, 2026-07-29. `tpu_info` reaches libtpu over
    /// gRPC at `localhost:8431` (`RuntimeMetricServiceStub`, local channel
    /// credentials), so a native client is conceivable. It was rejected:
    ///
    /// 1. **`ss -ltn` shows nothing on 8431 while the TPU is idle.** libtpu
    ///    starts that server *inside* the training process. A direct client
    ///    would get connection-refused exactly as often as `tpu-info` returns
    ///    `N/A`, so it fixes nothing about availability — the thing worth fixing.
    /// 2. **No `.proto` source ships** — only compiled `_pb2.py`. Using tonic
    ///    means vendoring a reconstructed, Google-internal, unversioned schema
    ///    that a libtpu update can silently break.
    /// 3. `grpc.local_channel_credentials()` has no tonic equivalent.
    /// 4. The saving is small: `tpu-info` costs ~391ms (of which ~100ms is
    ///    Python import), against a default 15s interval — **~2.6% of one core**
    ///    on a background thread that never touches the training loop.
    ///
    /// Revisit only if sub-second sampling is ever needed, which profiling —
    /// not this sampler — is the right tool for.
    pub fn tpu() -> Self {
        Self {
            prefix: "tpu".to_string(),
            command: "tpu-info".to_string(),
            args: vec![
                "--metric".to_string(),
                "duty_cycle_percent".to_string(),
                "--metric".to_string(),
                "hbm_usage".to_string(),
                "--metric".to_string(),
                "tensorcore_utilization".to_string(),
            ],
            format: ProbeFormat::PipeTable,
            columns: vec![],
        }
    }

    /// The probes tried by default. Absent binaries cost one PATH lookup each.
    pub fn defaults() -> Vec<Self> {
        vec![Self::nvidia(), Self::rocm(), Self::tpu()]
    }

    fn is_available(&self) -> bool {
        which(&self.command).is_some()
    }

    fn sample(&self) -> HashMap<String, MetricValue> {
        let output = match Command::new(&self.command).args(&self.args).output() {
            Ok(o) if o.status.success() => o.stdout,
            // A probe that errors once will usually error every time; we simply
            // return nothing rather than filling the log with vendor noise.
            _ => return HashMap::new(),
        };
        let text = String::from_utf8_lossy(&output);
        match self.format {
            ProbeFormat::NvidiaCsv => parse_nvidia_csv(&text, &self.prefix, &self.columns),
            ProbeFormat::Json => serde_json::from_str::<serde_json::Value>(&text)
                .map(|v| flatten_json_numbers(&v, &self.prefix))
                .unwrap_or_default(),
            ProbeFormat::KeyValue => parse_key_value(&text, &self.prefix),
            ProbeFormat::PipeTable => parse_pipe_tables(&text, &self.prefix),
        }
    }
}

/// Minimal PATH lookup — avoids a dependency for six lines.
fn which(program: &str) -> Option<std::path::PathBuf> {
    if program.contains('/') {
        let p = Path::new(program);
        return p.is_file().then(|| p.to_path_buf());
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(program);
            candidate.is_file().then_some(candidate)
        })
    })
}

/// One row per device; `index` (if present) becomes part of the key.
fn parse_nvidia_csv(text: &str, prefix: &str, columns: &[String]) -> HashMap<String, MetricValue> {
    let mut out = HashMap::new();
    for (row_idx, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let fields: Vec<&str> = line.split(',').map(|f| f.trim()).collect();
        let device = columns
            .iter()
            .position(|c| c == "index")
            .and_then(|i| fields.get(i))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(row_idx);

        for (col, value) in columns.iter().zip(&fields) {
            if col == "index" {
                continue;
            }
            // "[N/A]" and "[Not Supported]" are normal for some fields on some
            // cards; skipping keeps them out of the schema entirely rather than
            // writing nulls forever.
            if let Ok(v) = value.parse::<f64>() {
                out.insert(format!("{prefix}.{device}.{col}"), MetricValue::Float(v));
            }
        }
    }
    out
}

/// Extract every numeric leaf, keyed by its dotted path.
///
/// Schema-agnostic on purpose — see the module docs. Strings that parse as
/// numbers count, because `rocm-smi` reports `"GPU use (%)": "42"`.
fn flatten_json_numbers(value: &serde_json::Value, prefix: &str) -> HashMap<String, MetricValue> {
    let mut out = HashMap::new();
    walk_json(value, prefix, &mut out);
    out
}

fn walk_json(value: &serde_json::Value, path: &str, out: &mut HashMap<String, MetricValue>) {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                out.insert(path.to_string(), MetricValue::Float(f));
            }
        }
        serde_json::Value::String(s) => {
            if let Some(f) = parse_leading_number(s) {
                out.insert(path.to_string(), MetricValue::Float(f));
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                walk_json(v, &format!("{path}.{}", sanitize(k)), out);
            }
        }
        serde_json::Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                walk_json(v, &format!("{path}.{i}"), out);
            }
        }
        _ => {}
    }
}

/// Take the leading numeric run of a string.
///
/// Vendors glue units on ("42.0c", "155.0W", "0.00%") and quote their numbers,
/// so the leading run is what carries the value.
fn parse_leading_number(text: &str) -> Option<f64> {
    let numeric: String = text
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    (!numeric.is_empty())
        .then(|| numeric.parse().ok())
        .flatten()
}

/// Parse every markdown-style pipe table in `text`.
///
/// The first column identifies the device (`Core ID`, `Device`, `Chip`); every
/// other column header names a metric. Multiple tables are handled in one pass,
/// which is what `tpu-info` emits when several `--metric` flags are given.
///
/// Rich's surrounding panels use box-drawing characters, not the ASCII `|` these
/// tables use, so they are skipped without needing to be recognised.
fn parse_pipe_tables(text: &str, prefix: &str) -> HashMap<String, MetricValue> {
    let mut out = HashMap::new();
    let mut header: Option<Vec<String>> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        let is_row = trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 1;
        if !is_row {
            // Any non-row line ends the current table, so the next table's
            // first row is read as its header rather than as data.
            header = None;
            continue;
        }

        let cells: Vec<String> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();

        // The |---|---| rule under a header carries no data.
        if cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
        {
            continue;
        }

        match &header {
            None => header = Some(cells),
            Some(names) => {
                let Some(device) = cells.first() else {
                    continue;
                };
                let device = sanitize(device);
                for (name, value) in names.iter().zip(&cells).skip(1) {
                    // "N/A" is normal: libtpu publishes HBM and duty cycle only
                    // while a framework holds the TPU. Skipping keeps the column
                    // out of the schema rather than writing nulls forever.
                    if let Some(v) = parse_leading_number(value) {
                        out.insert(
                            format!("{prefix}.{device}.{}", sanitize(name)),
                            MetricValue::Float(v),
                        );
                    }
                }
            }
        }
    }
    out
}

fn parse_key_value(text: &str, prefix: &str) -> HashMap<String, MetricValue> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=').or_else(|| line.split_once(':')) else {
            continue;
        };
        if let Ok(f) = v.trim().parse::<f64>() {
            out.insert(
                format!("{prefix}.{}", sanitize(k.trim())),
                MetricValue::Float(f),
            );
        }
    }
    out
}

/// Metric keys become Parquet column names, so keep them boring.
fn sanitize(key: &str) -> String {
    key.trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_lowercase()
}

// ─── Host CPU and memory ──────────────────────────────────────────────────────

/// CPU and memory from `/proc`, which needs no external tool at all.
#[derive(Default)]
struct HostProbe {
    /// Previous cumulative jiffies, since CPU utilisation is a delta.
    prev_cpu: Option<(u64, u64)>,
}

impl HostProbe {
    fn sample(&mut self) -> HashMap<String, MetricValue> {
        let mut out = HashMap::new();

        if let Ok(stat) = std::fs::read_to_string("/proc/stat") {
            if let Some(line) = stat.lines().next() {
                let vals: Vec<u64> = line
                    .split_whitespace()
                    .skip(1)
                    .filter_map(|v| v.parse().ok())
                    .collect();
                if vals.len() >= 4 {
                    let idle = vals[3];
                    let total: u64 = vals.iter().sum();
                    // The first sample has no baseline to diff against, so it
                    // reports nothing rather than a meaningless since-boot mean.
                    if let Some((prev_total, prev_idle)) = self.prev_cpu {
                        let dt = total.saturating_sub(prev_total);
                        let di = idle.saturating_sub(prev_idle);
                        if dt > 0 {
                            let used = 100.0 * (dt.saturating_sub(di)) as f64 / dt as f64;
                            out.insert("cpu.util_pct".to_string(), MetricValue::Float(used));
                        }
                    }
                    self.prev_cpu = Some((total, idle));
                }
            }
        }

        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            let field = |name: &str| -> Option<f64> {
                meminfo.lines().find_map(|l| {
                    let rest = l.strip_prefix(name)?.trim().strip_suffix(" kB")?;
                    rest.trim()
                        .parse::<f64>()
                        .ok()
                        .map(|kb| kb / 1024.0 / 1024.0)
                })
            };
            if let (Some(total), Some(available)) = (field("MemTotal:"), field("MemAvailable:")) {
                out.insert("mem.total_gb".to_string(), MetricValue::Float(total));
                out.insert(
                    "mem.used_gb".to_string(),
                    MetricValue::Float(total - available),
                );
                if total > 0.0 {
                    out.insert(
                        "mem.util_pct".to_string(),
                        MetricValue::Float(100.0 * (total - available) / total),
                    );
                }
            }
        }

        // This process's own resident set — usually the number you actually
        // want when chasing a leak in a training loop.
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            if let Some(rss) = status.lines().find_map(|l| {
                l.strip_prefix("VmRSS:")?
                    .trim()
                    .strip_suffix(" kB")?
                    .trim()
                    .parse::<f64>()
                    .ok()
            }) {
                out.insert(
                    "proc.rss_gb".to_string(),
                    MetricValue::Float(rss / 1024.0 / 1024.0),
                );
            }
        }

        out
    }
}

// ─── Sampler ──────────────────────────────────────────────────────────────────

/// Samples every available probe on demand.
pub struct SystemSampler {
    probes: Vec<ProbeSpec>,
    host: HostProbe,
}

impl SystemSampler {
    /// Resolve which probes exist on this machine. Done once, at startup: PATH
    /// does not change under a running job, and re-checking every tick would
    /// mean a stat per probe per sample forever.
    pub fn new(specs: Vec<ProbeSpec>) -> Self {
        let probes: Vec<ProbeSpec> = specs.into_iter().filter(|p| p.is_available()).collect();
        if !probes.is_empty() {
            tracing::debug!(
                probes = ?probes.iter().map(|p| &p.command).collect::<Vec<_>>(),
                "system metric probes enabled"
            );
        }
        Self {
            probes,
            host: HostProbe::default(),
        }
    }

    /// One sample across every probe. Keys collide across probes only if two
    /// probes share a prefix, which is the caller's choice to make.
    ///
    /// Probes run **concurrently**. Each costs a subprocess — `tpu-info` alone is
    /// ~390ms — so a host with several accelerators would otherwise pay their
    /// sum on every tick. One thread per probe is right here: they are
    /// I/O-and-fork bound, there are single digits of them, and they are spawned
    /// once per sampling interval, not per step.
    pub fn sample(&mut self) -> HashMap<String, MetricValue> {
        // The host probe is cheap (a few /proc reads) and holds state across
        // samples, so it stays inline.
        let mut out = self.host.sample();

        if self.probes.len() == 1 {
            out.extend(self.probes[0].sample());
            return out;
        }

        let collected: Vec<HashMap<String, MetricValue>> = std::thread::scope(|scope| {
            let handles: Vec<_> = self
                .probes
                .iter()
                .map(|probe| scope.spawn(move || probe.sample()))
                .collect();
            handles
                .into_iter()
                // A panicking probe must not take the run's logging with it.
                .filter_map(|h| h.join().ok())
                .collect()
        });
        for values in collected {
            out.extend(values);
        }
        out
    }

    /// Names of the external probes actually in use, for reporting.
    pub fn active_probes(&self) -> Vec<String> {
        self.probes.iter().map(|p| p.command.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvidia_csv_is_keyed_by_device_index() {
        let spec = ProbeSpec::nvidia();
        let out = parse_nvidia_csv(
            "0, 42, 17, 1024, 40960, 61, 155.42\n1, 0, 0, 12, 40960, 33, 22.10\n",
            &spec.prefix,
            &spec.columns,
        );
        assert_eq!(out["gpu.0.util_pct"].to_string(), "42");
        assert_eq!(out["gpu.1.mem_total_mb"].to_string(), "40960");
        assert_eq!(out["gpu.0.power_w"].to_string(), "155.42");
        // `index` is a key component, never a metric of its own.
        assert!(!out.contains_key("gpu.0.index"));
    }

    #[test]
    fn nvidia_csv_skips_unsupported_fields_rather_than_writing_nulls() {
        let spec = ProbeSpec::nvidia();
        let out = parse_nvidia_csv(
            "0, 42, 17, 1024, 40960, [N/A], [Not Supported]",
            &spec.prefix,
            &spec.columns,
        );
        assert!(out.contains_key("gpu.0.util_pct"));
        assert!(!out.contains_key("gpu.0.temp_c"));
        assert!(!out.contains_key("gpu.0.power_w"));
    }

    #[test]
    fn json_flattening_is_schema_agnostic() {
        // Shaped like rocm-smi: quoted numbers, units glued on, nested by card.
        let value: serde_json::Value = serde_json::from_str(
            r#"{"card0": {"GPU use (%)": "42", "Temperature (C)": "61.5c", "Average Graphics Package Power (W)": "155.0W", "Card model": "AMD Instinct"}}"#,
        )
        .unwrap();
        let out = flatten_json_numbers(&value, "gpu");
        assert_eq!(out["gpu.card0.gpu_use"].to_string(), "42");
        // Units glued to the number are stripped; inner punctuation survives as
        // underscores so two differently-named fields cannot collide.
        assert_eq!(out["gpu.card0.temperature__c"].to_string(), "61.5");
        assert_eq!(
            out["gpu.card0.average_graphics_package_power__w"].to_string(),
            "155"
        );
        // A non-numeric string contributes nothing rather than a garbage 0.
        assert!(!out.contains_key("gpu.card0.card_model"));
    }

    #[test]
    fn arrays_become_indexed_keys() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{"devices": [{"hbm_used": 4.5}, {"hbm_used": 4.7}]}"#).unwrap();
        let out = flatten_json_numbers(&value, "tpu");
        assert_eq!(out["tpu.devices.0.hbm_used"].to_string(), "4.5");
        assert_eq!(out["tpu.devices.1.hbm_used"].to_string(), "4.7");
    }

    /// Verbatim `tpu-info` output from a v6e host, 2026-07-29. Kept exact —
    /// including the Rich panel and the N/A rows — because the point of the test
    /// is that the parser survives real output, not a tidied version of it.
    const TPU_INFO_V6E: &str = r#"╭───────────────────────── Runtime Utilization Status ─────────────────────────╮
│ WARNING: Libtpu metrics unavailable. Is there a framework using the TPU? See │
│ tpu_info docs for more information.                                          │
╰──────────────────────────────────────────────────────────────────────────────╯
TPU Duty Cycle

| Core ID | Duty Cycle (%) |
|---------|----------------|
| 0       | N/A            |
| 1       | N/A            |

TPU HBM Usage

| Device | HBM Usage (GiB) |
|--------|-----------------|
| 0      | N/A             |
| 1      | N/A             |

TensorCore Utilization

| Core ID | TensorCore Utilization |
|---------|------------------------|
| 0       | 0.00%                  |
| 1       | 12.50%                 |
"#;

    #[test]
    fn tpu_info_pipe_tables_parse() {
        let out = parse_pipe_tables(TPU_INFO_V6E, "tpu");

        // TensorCore utilisation reports even with no framework attached; the
        // trailing % is stripped.
        assert_eq!(out["tpu.0.tensorcore_utilization"].to_string(), "0");
        assert_eq!(out["tpu.1.tensorcore_utilization"].to_string(), "12.5");

        // N/A means libtpu has nothing to publish yet. Skipped, not zero —
        // recording 0 GiB of HBM while a job runs would be a lie.
        assert!(!out.contains_key("tpu.0.duty_cycle"));
        assert!(
            out.keys().all(|k| !k.contains("hbm")),
            "N/A must not be stored: {out:?}"
        );

        // The Rich warning panel uses box-drawing pipes, not ASCII, so it never
        // reaches the parser.
        assert!(out.keys().all(|k| !k.to_lowercase().contains("warning")));
        assert_eq!(out.len(), 2, "only the two real readings: {out:?}");
    }

    #[test]
    fn pipe_tables_keep_each_table_separate() {
        // Two tables whose first column means different things must not bleed
        // into one another — the header has to reset between them.
        let text = "\
Table A

| Core ID | Duty Cycle (%) |
|---------|----------------|
| 0       | 55.0           |

Table B

| Device | HBM Usage (GiB) |
|--------|-----------------|
| 0      | 12.5            |
";
        let out = parse_pipe_tables(text, "tpu");
        assert_eq!(out["tpu.0.duty_cycle"].to_string(), "55");
        assert_eq!(out["tpu.0.hbm_usage__gib"].to_string(), "12.5");
    }

    #[test]
    fn nvidia_probe_matches_real_a6000_output() {
        // Verbatim from an RTX A6000 host, 2026-07-29 — the exact command in
        // ProbeSpec::nvidia(), so this pins the column contract end to end.
        let spec = ProbeSpec::nvidia();
        let out = parse_nvidia_csv(
            "0, 0, 0, 4, 49140, 27, 4.65\n1, 0, 0, 14, 49140, 29, 9.17\n",
            &spec.prefix,
            &spec.columns,
        );
        assert_eq!(out["gpu.0.mem_total_mb"].to_string(), "49140");
        assert_eq!(out["gpu.1.mem_used_mb"].to_string(), "14");
        assert_eq!(out["gpu.0.power_w"].to_string(), "4.65");
        assert_eq!(out["gpu.1.temp_c"].to_string(), "29");
        // 7 columns minus `index`, times 2 GPUs.
        assert_eq!(out.len(), 12);
    }

    #[test]
    fn absent_binaries_are_dropped_at_construction() {
        let sampler = SystemSampler::new(vec![ProbeSpec {
            prefix: "nope".into(),
            command: "expman-definitely-not-a-real-binary".into(),
            args: vec![],
            format: ProbeFormat::Json,
            columns: vec![],
        }]);
        assert!(sampler.active_probes().is_empty());
    }

    #[test]
    fn host_probe_reports_memory_and_needs_two_samples_for_cpu() {
        let mut host = HostProbe::default();
        let first = host.sample();
        if !cfg!(target_os = "linux") {
            return;
        }
        assert!(first.contains_key("mem.total_gb"));
        // No baseline yet, so no CPU figure — better than a since-boot average
        // dressed up as a current reading.
        assert!(!first.contains_key("cpu.util_pct"));

        // Real elapsed time is required: with a zero jiffy delta there is
        // genuinely nothing to report, and the sampler correctly says nothing.
        std::thread::sleep(std::time::Duration::from_millis(60));
        let second = host.sample();
        let util = second
            .get("cpu.util_pct")
            .and_then(|v| v.to_string().parse::<f64>().ok())
            .expect("a second sample with elapsed time yields CPU utilisation");
        assert!(
            (0.0..=100.0).contains(&util),
            "utilisation must be a percentage, got {util}"
        );
    }
}
