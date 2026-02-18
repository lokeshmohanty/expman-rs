//! Storage layer: Parquet/Arrow IPC metrics, YAML config, file system management.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, Float64Array, Int64Array, StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::properties::WriterProperties;
use serde_yaml;

use crate::error::Result;
use crate::models::{
    ExperimentMetadata, MetricRow, MetricValue, RunMetadata, RunStatus,
};

// ─── Directory helpers ────────────────────────────────────────────────────────

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn list_experiments(base_dir: &Path) -> Result<Vec<String>> {
    if !base_dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in fs::read_dir(base_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

pub fn list_runs(experiment_dir: &Path) -> Result<Vec<String>> {
    if !experiment_dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in fs::read_dir(experiment_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                // Avoid "artifacts" if we are inside a run, but here we are at experiment level.
                // However, an experiment folder contains runs (directories).
                // We should probably filter for directories that contain a run.yaml or metrics.parquet
                // But for now, just listing all dirs except maybe some reserved ones.
                if name != "artifacts" {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort_by(|a, b| b.cmp(a)); // newest first
    Ok(names)
}

pub fn list_artifacts(run_dir: &Path) -> Result<Vec<ArtifactInfo>> {
    let mut files = vec![];

    // 1. List default artifacts from run_dir root
    if run_dir.exists() {
        for entry in fs::read_dir(run_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Include specific default files
                if name == "metrics.parquet" || name == "config.yaml" || name == "run.yaml" || name == "run.log" || name == "console.log" {
                    let size = path.metadata()?.len();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    files.push(ArtifactInfo {
                        path: name.to_string(),
                        name: name.to_string(),
                        size,
                        ext,
                        is_default: true,
                    });
                }
            }
        }
    }

    // 2. List user artifacts from artifacts/ subdir
    let artifacts_dir = run_dir.join("artifacts");
    if artifacts_dir.exists() {
        collect_files(&artifacts_dir, &artifacts_dir, &mut files)?;
    }
    
    Ok(files)
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<ArtifactInfo>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let size = path.metadata()?.len();
            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            out.push(ArtifactInfo {
                path: rel.to_string_lossy().to_string(),
                name: path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string(),
                size,
                ext,
                is_default: false,
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ArtifactInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub ext: String,
    pub is_default: bool,
}

// ─── YAML config I/O ─────────────────────────────────────────────────────────

pub fn save_yaml<T: serde::Serialize>(path: &Path, data: &T) -> Result<()> {
    let content = serde_yaml::to_string(data)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn load_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T>
where
    T: Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let content = fs::read_to_string(path)?;
    let val = serde_yaml::from_str(&content)?;
    Ok(val)
}

pub fn load_yaml_value(path: &Path) -> Result<serde_yaml::Value> {
    if !path.exists() {
        return Ok(serde_yaml::Value::Mapping(Default::default()));
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

pub fn save_run_metadata(run_dir: &Path, meta: &RunMetadata) -> Result<()> {
    save_yaml(&run_dir.join("run.yaml"), meta)
}

pub fn load_run_metadata(run_dir: &Path) -> Result<RunMetadata> {
    let path = run_dir.join("run.yaml");
    if !path.exists() {
        // Construct a minimal metadata from directory name
        let name = run_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let exp = run_dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        return Ok(RunMetadata {
            name,
            experiment: exp,
            status: RunStatus::Crashed,
            started_at: Utc::now(),
            finished_at: None,
            duration_secs: None,
            description: None,
        });
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(serde_yaml::from_str(&content)?)
}

pub fn save_experiment_metadata(exp_dir: &Path, meta: &ExperimentMetadata) -> Result<()> {
    save_yaml(&exp_dir.join("experiment.yaml"), meta)
}

pub fn load_experiment_metadata(exp_dir: &Path) -> Result<ExperimentMetadata> {
    load_yaml(&exp_dir.join("experiment.yaml"))
}

// ─── Parquet metrics I/O ─────────────────────────────────────────────────────

/// Append metric rows to a Parquet file.
/// Strategy: read existing → concat → write back.
/// This is called infrequently (batched), so O(n) is acceptable.
/// For very large files, a future optimization is columnar append via IPC.
pub fn append_metrics(path: &Path, rows: &[MetricRow]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    // Build new batch from rows
    let new_batch = rows_to_record_batch(rows)?;

    // If file exists, read and concat
    let final_batch = if path.exists() {
        let existing = read_parquet(path)?;
        concat_batches(&existing, &new_batch)?
    } else {
        new_batch
    };

    write_parquet(path, &final_batch)?;
    Ok(())
}

/// Read all metrics from a Parquet file as a list of row maps.
pub fn read_metrics(path: &Path) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let batch = read_parquet(path)?;
    record_batch_to_rows(&batch)
}

/// Read metrics since a given step (for live streaming).
pub fn read_metrics_since(
    path: &Path,
    since_step: Option<u64>,
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    let all = read_metrics(path)?;
    if let Some(since) = since_step {
        Ok(all
            .into_iter()
            .filter(|row| {
                row.get("step")
                    .and_then(|v| v.as_u64())
                    .map(|s| s > since)
                    .unwrap_or(true)
            })
            .collect())
    } else {
        Ok(all)
    }
}

fn read_parquet(path: &Path) -> Result<RecordBatch> {
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let mut reader = builder.build()?;
    let mut batches = vec![];
    for batch in &mut reader {
        batches.push(batch?);
    }
    if batches.is_empty() {
        // Return empty batch with default schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("step", DataType::Int64, true),
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
        ]));
        return Ok(RecordBatch::new_empty(schema));
    }
    if batches.len() == 1 {
        return Ok(batches.remove(0));
    }
    // Concat multiple batches
    let schema = batches[0].schema();
    Ok(arrow::compute::concat_batches(&schema, &batches)?)
}

fn write_parquet(path: &Path, batch: &RecordBatch) -> Result<()> {
    let file = fs::File::create(path)?;
    let props = WriterProperties::builder()
        .set_compression(parquet::basic::Compression::SNAPPY)
        .build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(batch)?;
    writer.close()?;
    Ok(())
}

fn concat_batches(existing: &RecordBatch, new: &RecordBatch) -> Result<RecordBatch> {
    // Merge schemas: new batch may have columns not in existing (diagonal concat)
    let merged_schema = merge_schemas(existing.schema_ref(), new.schema_ref());
    let merged_schema = Arc::new(merged_schema);

    let existing_aligned = align_batch(existing, &merged_schema)?;
    let new_aligned = align_batch(new, &merged_schema)?;

    Ok(arrow::compute::concat_batches(
        &merged_schema,
        &[existing_aligned, new_aligned],
    )?)
}

fn merge_schemas(a: &Schema, b: &Schema) -> Schema {
    let mut fields: Vec<Field> = a.fields().iter().map(|f| f.as_ref().clone()).collect();
    for field in b.fields() {
        if a.field_with_name(field.name()).is_err() {
            fields.push(field.as_ref().clone());
        }
    }
    Schema::new(fields)
}

fn align_batch(batch: &RecordBatch, target_schema: &Schema) -> Result<RecordBatch> {
    let n = batch.num_rows();
    let mut columns: Vec<ArrayRef> = vec![];

    for field in target_schema.fields() {
        if let Some(col) = batch.column_by_name(field.name()) {
            columns.push(col.clone());
        } else {
            // Fill missing column with nulls
            let null_array: ArrayRef = match field.data_type() {
                DataType::Float64 => Arc::new(Float64Array::from(vec![None::<f64>; n])),
                DataType::Int64 => Arc::new(Int64Array::from(vec![None::<i64>; n])),
                DataType::Timestamp(TimeUnit::Microsecond, _) => {
                    Arc::new(TimestampMicrosecondArray::from(vec![None::<i64>; n])
                        .with_timezone_opt(Some("UTC".to_string())))
                }
                _ => Arc::new(StringArray::from(vec![None::<&str>; n])),
            };
            columns.push(null_array);
        }
    }

    Ok(RecordBatch::try_new(Arc::new(target_schema.clone()), columns)?)
}

fn rows_to_record_batch(rows: &[MetricRow]) -> Result<RecordBatch> {
    if rows.is_empty() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("step", DataType::Int64, true),
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
        ]));
        return Ok(RecordBatch::new_empty(schema));
    }

    // Collect all unique metric keys across all rows
    let mut all_keys: Vec<String> = vec![];
    for row in rows {
        for key in row.values.keys() {
            if !all_keys.contains(key) {
                all_keys.push(key.clone());
            }
        }
    }

    let _n = rows.len();

    // Build columns
    let mut fields = vec![
        Field::new("step", DataType::Int64, true),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ];
    let mut arrays: Vec<ArrayRef> = vec![];

    // step column
    let steps: Vec<Option<i64>> = rows.iter().map(|r| r.step.map(|s| s as i64)).collect();
    arrays.push(Arc::new(Int64Array::from(steps)));

    // timestamp column (microseconds since epoch UTC)
    let timestamps: Vec<Option<i64>> = rows
        .iter()
        .map(|r| Some(r.timestamp.timestamp_micros()))
        .collect();
    arrays.push(Arc::new(
        TimestampMicrosecondArray::from(timestamps)
            .with_timezone_opt(Some("UTC".to_string())),
    ));

    // metric value columns
    for key in &all_keys {
        // Determine type from first non-null value
        let first_val = rows.iter().find_map(|r| r.values.get(key));
        match first_val {
            Some(MetricValue::Float(_)) | Some(MetricValue::Int(_)) => {
                // Store as Float64 for simplicity
                let vals: Vec<Option<f64>> = rows
                    .iter()
                    .map(|r| match r.values.get(key) {
                        Some(MetricValue::Float(f)) => Some(*f),
                        Some(MetricValue::Int(i)) => Some(*i as f64),
                        _ => None,
                    })
                    .collect();
                fields.push(Field::new(key, DataType::Float64, true));
                arrays.push(Arc::new(Float64Array::from(vals)));
            }
            _ => {
                // Store as Utf8
                let vals: Vec<Option<String>> = rows
                    .iter()
                    .map(|r| match r.values.get(key) {
                        Some(MetricValue::Text(s)) => Some(s.clone()),
                        Some(MetricValue::Bool(b)) => Some(b.to_string()),
                        Some(MetricValue::Float(f)) => Some(f.to_string()),
                        Some(MetricValue::Int(i)) => Some(i.to_string()),
                        None => None,
                    })
                    .collect();
                fields.push(Field::new(key, DataType::Utf8, true));
                arrays.push(Arc::new(StringArray::from(vals)));
            }
        }
    }

    let schema = Arc::new(Schema::new(fields));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

fn record_batch_to_rows(
    batch: &RecordBatch,
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    let schema = batch.schema();
    let n = batch.num_rows();
    let mut rows = vec![HashMap::new(); n];

    for (col_idx, field) in schema.fields().iter().enumerate() {
        let col = batch.column(col_idx);
        let name = field.name().clone();

        for row_idx in 0..n {
            use arrow::array::Array;
            if col.is_null(row_idx) {
                rows[row_idx].insert(name.clone(), serde_json::Value::Null);
                continue;
            }
            let val = match field.data_type() {
                DataType::Float64 => {
                    let arr = col.as_any().downcast_ref::<Float64Array>().unwrap();
                    let f = arr.value(row_idx);
                    if f.is_nan() || f.is_infinite() {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!(f)
                    }
                }
                DataType::Int64 => {
                    let arr = col.as_any().downcast_ref::<Int64Array>().unwrap();
                    serde_json::json!(arr.value(row_idx))
                }
                DataType::Timestamp(TimeUnit::Microsecond, _) => {
                    let arr = col
                        .as_any()
                        .downcast_ref::<TimestampMicrosecondArray>()
                        .unwrap();
                    let micros = arr.value(row_idx);
                    let dt = DateTime::<Utc>::from_timestamp_micros(micros)
                        .unwrap_or_default();
                    serde_json::json!(dt.to_rfc3339())
                }
                DataType::Utf8 => {
                    let arr = col.as_any().downcast_ref::<StringArray>().unwrap();
                    serde_json::json!(arr.value(row_idx))
                }
                _ => serde_json::Value::Null,
            };
            rows[row_idx].insert(name.clone(), val);
        }
    }

    Ok(rows)
}
