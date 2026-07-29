//! Provenance: what code and what environment produced a run.
//!
//! Written once at run creation into `provenance.yaml`. This is most of what
//! makes a run reproducible, and it is the thing people notice missing only
//! months later, when the answer to "which commit was this?" has to be guessed
//! from a timestamp.
//!
//! ## The diff is opt-in, deliberately
//!
//! The commit SHA, branch and dirty flag are always captured: they are small,
//! and they cannot leak anything that is not already in the repository. The
//! working-tree **diff** is captured only on request, because a dirty tree
//! routinely contains things that should not be copied into a store you might
//! later share — an edited `.env`, a pasted API key, an unpublished data path.
//! Knowing a run was dirty is cheap and safe; knowing exactly how is a choice
//! the user should make per-run.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Everything captured about the code and environment behind a run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Provenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentInfo>,
    /// Command line the run was launched with, best-effort.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Scheduler identifiers, when running under one. Lets a run be traced back
    /// to its scheduler logs, which is where the OOM kill will be recorded.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub scheduler: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitInfo {
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// True when the working tree had uncommitted changes. A run with
    /// `dirty: true` and no `diff` is *not* reproducible, and says so.
    pub dirty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    /// Working-tree diff, only when explicitly requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// Set when a diff was requested but exceeded the size cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_truncated_bytes: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// `pip freeze` output. Captured on request — it costs a subprocess and can
    /// run to hundreds of lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packages: Option<String>,
}

/// A captured diff larger than this is truncated. Big enough for real work,
/// small enough that a stray notebook checkpoint cannot bloat every run.
const MAX_DIFF_BYTES: usize = 1024 * 1024;

/// Run a git command in `dir`, returning trimmed stdout on success.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Capture git state for the repository containing `dir`.
///
/// Returns `None` when `dir` is not in a repository or git is unavailable —
/// running outside version control is normal, not an error.
pub fn capture_git(dir: &Path, include_diff: bool) -> Option<GitInfo> {
    let commit = git(dir, &["rev-parse", "HEAD"])?;

    // `--porcelain` is empty exactly when the tree is clean.
    let dirty = git(dir, &["status", "--porcelain"]).is_some();

    let branch = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD");

    let mut info = GitInfo {
        commit,
        branch,
        dirty,
        remote: git(dir, &["config", "--get", "remote.origin.url"]),
        diff: None,
        diff_truncated_bytes: None,
    };

    if include_diff && dirty {
        // HEAD rather than the index, so staged and unstaged changes both land.
        if let Some(diff) = git(dir, &["diff", "HEAD"]) {
            if diff.len() > MAX_DIFF_BYTES {
                info.diff_truncated_bytes = Some(diff.len());
                info.diff = Some(diff.chars().take(MAX_DIFF_BYTES).collect());
            } else {
                info.diff = Some(diff);
            }
        }
    }

    Some(info)
}

/// Scheduler identifiers from the environment.
///
/// Only variables that identify *this* job — enough to find it in the
/// scheduler's own records. Not a dump of the environment, which would sweep up
/// credentials.
pub fn capture_scheduler() -> std::collections::BTreeMap<String, String> {
    const KEYS: &[&str] = &[
        "SLURM_JOB_ID",
        "SLURM_ARRAY_JOB_ID",
        "SLURM_ARRAY_TASK_ID",
        "SLURM_JOB_NAME",
        "SLURM_JOB_NODELIST",
        "SLURM_PROCID",
        "SLURM_NTASKS",
        "PBS_JOBID",
        "LSB_JOBID",
        "KUBERNETES_SERVICE_HOST",
    ];
    KEYS.iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect()
}

/// Best-effort hostname, from the environment or `hostname(1)`.
pub fn capture_hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.is_empty())
        .or_else(|| {
            Command::new("hostname")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|h| !h.is_empty())
        })
}

/// The command line this process was started with.
pub fn capture_command() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    (!args.is_empty()).then(|| args.join(" "))
}

impl Provenance {
    /// Capture everything cheap, plus the diff only if asked.
    pub fn capture(working_dir: &Path, include_diff: bool) -> Self {
        Self {
            git: capture_git(working_dir, include_diff),
            environment: None,
            command: capture_command(),
            hostname: capture_hostname(),
            scheduler: capture_scheduler(),
        }
    }

    /// True when this run could be reconstructed from what was captured.
    pub fn is_reproducible(&self) -> bool {
        match &self.git {
            Some(git) => !git.dirty || git.diff.is_some(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_outside_a_repo_is_none_not_an_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        // A bare temp dir is not in a repo (unless /tmp is, which it is not).
        let prov = Provenance::capture(tmp.path(), false);
        assert!(prov.git.is_none());
        // The rest still populates — a run outside git is perfectly normal.
        assert!(prov.command.is_some());
    }

    #[test]
    fn a_clean_repo_is_reproducible_without_a_diff() {
        let prov = Provenance {
            git: Some(GitInfo {
                commit: "abc123".into(),
                dirty: false,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(prov.is_reproducible());
    }

    #[test]
    fn a_dirty_repo_without_a_diff_is_not_reproducible() {
        // The point of tracking this: a dirty run is honest about the fact that
        // its code cannot be recovered.
        let mut prov = Provenance {
            git: Some(GitInfo {
                commit: "abc123".into(),
                dirty: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!prov.is_reproducible());

        prov.git.as_mut().unwrap().diff = Some("diff --git ...".into());
        assert!(prov.is_reproducible());
    }

    #[test]
    fn scheduler_capture_only_takes_identifying_keys() {
        // Not a dump of the environment — a dump would sweep up credentials.
        unsafe {
            std::env::set_var("SLURM_JOB_ID", "12345");
            std::env::set_var("MY_SECRET_TOKEN", "hunter2");
        }
        let sched = capture_scheduler();
        assert_eq!(sched.get("SLURM_JOB_ID").map(String::as_str), Some("12345"));
        assert!(!sched.contains_key("MY_SECRET_TOKEN"));
        unsafe {
            std::env::remove_var("SLURM_JOB_ID");
            std::env::remove_var("MY_SECRET_TOKEN");
        }
    }

    #[test]
    fn this_repo_is_detected_as_a_git_checkout() {
        // Runs inside the expman working tree, which is a repo by construction.
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let git = capture_git(here, false).expect("expman's own tree is a git repo");
        assert_eq!(git.commit.len(), 40, "a full SHA: {}", git.commit);
        assert!(git.diff.is_none(), "diff must stay opt-in");
    }
}
