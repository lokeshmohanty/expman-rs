//! Project sync: projecting an external manifest into the projects layer.
//!
//! An expman project can be a **one-way generated projection** of a source that
//! lives elsewhere — a `studies.yaml` in a thesis repo, say. In that mode expman
//! is not the authority: it carries only *general information* (display name,
//! description, tags, a README frontpage) and *experiments information* (which
//! experiments belong to the project). Hypotheses, task lists and sign-off state
//! stay in the source.
//!
//! The consequence is that a generated project is **overwritten wholesale by the
//! next sync**. Anything edited through the dashboard in the meantime is lost.
//! That is why `sync` stamps `generated: true` on the project and writes a
//! do-not-edit marker into the README: the API refuses edits to such a project
//! rather than accepting a write it is about to destroy.

use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::core::error::{ExpmanError, Result};
use crate::core::models::ProjectMetadata;
use crate::core::storage;

/// Marker written at the top of every generated README.
///
/// Also the signal `is_generated_readme` looks for, so a README that predates
/// the `generated` flag is still recognised.
pub const GENERATED_MARKER: &str =
    "<!-- expman:generated — do not edit. Regenerated on each sync; edits here are lost. -->";

/// A manifest describing one or more projects to project into the store.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectManifest {
    #[serde(default)]
    pub projects: Vec<ProjectSpec>,
}

/// One project's worth of the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSpec {
    /// Project id — the directory name under `.projects/`.
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Human-readable pointer to whatever is authoritative.
    #[serde(default)]
    pub generated_from: Option<String>,
    /// Experiments belonging to this project. Experiments previously assigned
    /// here but absent from this list are unassigned.
    #[serde(default)]
    pub experiments: Vec<String>,
    /// Optional structured frontpage. Rendered to README.md.
    #[serde(default)]
    pub frontpage: Option<Frontpage>,
    /// Raw README body, used verbatim instead of rendering `frontpage`.
    #[serde(default)]
    pub readme: Option<String>,
}

/// The structured frontpage of a project, rendered into README.md.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Frontpage {
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub target_venue: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    /// Any further sections, rendered in order as `## <heading>`.
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Section {
    pub heading: String,
    pub body: String,
}

/// What one project's sync changed. Returned so the caller can report it.
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub project: String,
    pub assigned: Vec<String>,
    pub unassigned: Vec<String>,
    /// Experiments named by the manifest that have no directory in the store.
    /// Not an error: the project may be declared before its first run.
    pub missing: Vec<String>,
}

/// Load a manifest from a YAML file.
///
/// Accepts either the full `{projects: [...]}` form or a bare list of specs.
pub fn load_manifest(path: &Path) -> Result<ProjectManifest> {
    let content = std::fs::read_to_string(path)?;
    // A bare list is the natural thing to write by hand; accept both rather
    // than failing on a shape that is obviously intended.
    if let Ok(manifest) = serde_yaml::from_str::<ProjectManifest>(&content) {
        if !manifest.projects.is_empty() {
            return Ok(manifest);
        }
    }
    let projects: Vec<ProjectSpec> = serde_yaml::from_str(&content)
        .map_err(|e| ExpmanError::Other(format!("could not parse manifest: {e}")))?;
    Ok(ProjectManifest { projects })
}

/// Project one spec into the store: metadata, README, and experiment assignment.
pub fn sync_project(base_dir: &Path, spec: &ProjectSpec) -> Result<SyncReport> {
    let now = Utc::now();

    // Preserve created_at across syncs — it describes the project, not this run.
    let previous = storage::load_project_metadata(base_dir, &spec.name).unwrap_or_default();
    let meta = ProjectMetadata {
        display_name: spec.display_name.clone(),
        description: spec.description.clone(),
        tags: spec.tags.clone(),
        created_at: previous.created_at.or(Some(now)),
        generated: true,
        generated_from: spec.generated_from.clone(),
        generated_at: Some(now),
    };
    storage::save_project_metadata(base_dir, &spec.name, &meta)?;

    let readme = match &spec.readme {
        Some(raw) => format!("{GENERATED_MARKER}\n\n{}", raw.trim_end()),
        None => render_readme(spec, &meta),
    };
    storage::save_project_readme(base_dir, &spec.name, &readme)?;

    // Reconcile membership: assign everything listed, unassign anything that
    // was in this project but has dropped out of the manifest.
    let mut report = SyncReport {
        project: spec.name.clone(),
        ..Default::default()
    };
    let previously = storage::list_project_experiments(base_dir, &spec.name)?;

    for exp in &spec.experiments {
        if !base_dir.join(exp).is_dir() {
            report.missing.push(exp.clone());
        }
        storage::set_experiment_project(base_dir, exp, Some(&spec.name))?;
        if !previously.contains(exp) {
            report.assigned.push(exp.clone());
        }
    }

    for exp in previously {
        if !spec.experiments.contains(&exp) {
            storage::set_experiment_project(base_dir, &exp, None)?;
            report.unassigned.push(exp);
        }
    }

    Ok(report)
}

/// Sync every project in a manifest.
pub fn sync_manifest(base_dir: &Path, manifest: &ProjectManifest) -> Result<Vec<SyncReport>> {
    manifest
        .projects
        .iter()
        .map(|spec| sync_project(base_dir, spec))
        .collect()
}

/// True when a README was written by a sync and will be clobbered by the next one.
pub fn is_generated_readme(content: &str) -> bool {
    content.trim_start().starts_with(GENERATED_MARKER)
}

/// Render a project's frontpage to Markdown.
fn render_readme(spec: &ProjectSpec, meta: &ProjectMetadata) -> String {
    let mut out = String::new();
    out.push_str(GENERATED_MARKER);
    out.push_str("\n\n# ");
    out.push_str(spec.display_name.as_deref().unwrap_or(&spec.name));
    out.push_str("\n\n");

    if let Some(desc) = &spec.description {
        out.push_str(desc.trim());
        out.push_str("\n\n");
    }

    let fp = spec.frontpage.clone().unwrap_or_default();

    if let Some(question) = &fp.question {
        out.push_str("## Question\n\n");
        out.push_str(question.trim());
        out.push_str("\n\n");
    }
    if let Some(scope) = &fp.scope {
        out.push_str("## Scope\n\n");
        out.push_str(scope.trim());
        out.push_str("\n\n");
    }
    if !fp.domains.is_empty() {
        out.push_str("## Domains\n\n");
        for domain in &fp.domains {
            out.push_str(&format!("- {domain}\n"));
        }
        out.push('\n');
    }

    // Target and status read as a small fact table rather than prose.
    let facts: Vec<(&str, String)> = [
        ("Target venue", fp.target_venue.clone()),
        ("Target date", fp.target_date.clone()),
        ("Status", fp.status.clone()),
        (
            "Tags",
            (!spec.tags.is_empty()).then(|| spec.tags.join(", ")),
        ),
        (
            "Experiments",
            (!spec.experiments.is_empty()).then(|| spec.experiments.len().to_string()),
        ),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k, v)))
    .collect();

    if !facts.is_empty() {
        out.push_str("| | |\n|---|---|\n");
        for (key, value) in facts {
            out.push_str(&format!("| {key} | {value} |\n"));
        }
        out.push('\n');
    }

    for section in &fp.sections {
        out.push_str(&format!(
            "## {}\n\n{}\n\n",
            section.heading,
            section.body.trim()
        ));
    }

    if !spec.experiments.is_empty() {
        out.push_str("## Experiments\n\n");
        for exp in &spec.experiments {
            out.push_str(&format!("- `{exp}`\n"));
        }
        out.push('\n');
    }

    if let Some(source) = &meta.generated_from {
        out.push_str(&format!(
            "---\n\nGenerated from {source}{}.\n",
            meta.generated_at
                .map(|t| format!(" on {}", t.format("%Y-%m-%d %H:%M UTC")))
                .unwrap_or_default()
        ));
    }

    out
}
