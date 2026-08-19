//! Jupyter notebook service logic (process management and content generation).
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::process::Child;
use tracing::{error, info, warn};

/// Tracks an active Jupyter notebook instance.
pub struct JupyterInstance {
    pub port: u16,
    pub process: Child,
}

/// The interactive backend detected in the user's environment.
///
/// Defined in `core::dto` because it is part of the HTTP response, and the
/// frontend matches on it.
pub use crate::core::dto::InteractiveBackend;

/// The notebook expman generates, per run and per experiment.
const NOTEBOOK_FILE: &str = "interactive.ipynb";

/// The command line `exp serve --jupyter-command` defaults to — what the server
/// always ran, so an unset flag changes nothing.
pub const DEFAULT_JUPYTER_COMMAND: &str = "jupyter";

/// The conventional per-store template location, relative to `base_dir`.
///
/// Checked when `--notebook-template` is not given, so a project can ship a
/// notebook with its experiment store and never pass a flag.
pub const CONVENTIONAL_TEMPLATE: &str = ".expman/notebook.ipynb";

// ─── Notebook templating ─────────────────────────────────────────────────────

/// Every placeholder a notebook template may use, without the `{{…}}`.
///
/// Documented in `docs/content/reference/cli.md`; kept here so the code and the
/// docs can be checked against one list.
pub const NOTEBOOK_PLACEHOLDERS: [&str; 5] =
    ["run_dir", "run_name", "experiment", "store", "project"];

/// Where a generated notebook's content comes from.
///
/// Resolution order is flag, then convention, then the built-in default — so a
/// store with neither behaves exactly as it did before templating existed.
#[derive(Debug, Clone, Default)]
pub struct NotebookTemplateConfig {
    /// `exp serve --notebook-template`, highest priority.
    pub explicit: Option<PathBuf>,
    /// The store root. `<base_dir>/.expman/notebook.ipynb` is the convention.
    pub base_dir: PathBuf,
}

impl NotebookTemplateConfig {
    /// The template to use, or `None` to use the built-in default.
    ///
    /// Absolute, because the path is recorded in the notebook it produces and a
    /// relative one there would only mean anything next to the server's cwd.
    pub fn resolve(&self) -> Option<PathBuf> {
        if let Some(path) = &self.explicit {
            if path.is_file() {
                return Some(absolute(path));
            }
            // An explicit flag pointing at nothing is a typo worth naming, but
            // not worth refusing to serve over: the convention below, or the
            // built-in default, still produces a working notebook.
            warn!(
                "--notebook-template {} is not a file; ignoring it",
                path.display()
            );
        }
        let conventional = self.base_dir.join(CONVENTIONAL_TEMPLATE);
        conventional.is_file().then(|| absolute(&conventional))
    }
}

/// The values a template's placeholders are substituted with, for one run.
#[derive(Debug, Clone)]
pub struct NotebookContext {
    /// Absolute path to the run directory — also where the notebook is written.
    pub run_dir: PathBuf,
    pub run_name: String,
    pub experiment: String,
    /// Absolute path to the store root (`base_dir`).
    pub store: PathBuf,
    /// The run's experiment's project; empty when the experiment is unassigned.
    pub project: String,
}

impl NotebookContext {
    /// Build the context for one run of one experiment.
    ///
    /// Paths are made absolute here rather than at substitution time, because a
    /// template's whole purpose is to be runnable from the notebook's own
    /// directory, where a relative `./experiments/...` would not resolve.
    pub fn new(store: &Path, experiment: &str, run: &str, project: String) -> Self {
        let store = absolute(store);
        Self {
            run_dir: store.join(experiment).join(run),
            run_name: run.to_string(),
            experiment: experiment.to_string(),
            store,
            project,
        }
    }

    /// The placeholder → value bindings, in the documented order.
    fn bindings(&self) -> [(&'static str, String); 5] {
        [
            ("run_dir", self.run_dir.display().to_string()),
            ("run_name", self.run_name.clone()),
            ("experiment", self.experiment.clone()),
            ("store", self.store.display().to_string()),
            ("project", self.project.clone()),
        ]
    }
}

/// Absolute form of `path`, without requiring it to exist.
///
/// `canonicalize` would resolve symlinks but fails on a run directory that has
/// not been created yet, which is the common case on first launch.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Escape `value` for splicing inside a JSON string literal.
///
/// A template is `.ipynb` — JSON — and every placeholder sits inside a string
/// literal, so a value containing `"`, `\`, or a control character has to be
/// escaped or the substitution produces a corrupt notebook. Run directories
/// really can contain such characters. `serde_json` quotes the value correctly;
/// the quotes are then dropped because the surrounding literal supplies its own.
fn json_escape(value: &str) -> String {
    match serde_json::to_string(value) {
        // Safe slice: `to_string` on a `&str` always yields `"…"`, and the
        // quotes are ASCII, so 1..len-1 is on a char boundary.
        Ok(quoted) => quoted[1..quoted.len() - 1].to_string(),
        Err(_) => String::new(),
    }
}

/// Substitute every `{{placeholder}}` in `template`, JSON-escaping the values.
fn substitute(template: &str, ctx: &NotebookContext) -> String {
    let mut out = template.to_string();
    for (name, value) in ctx.bindings() {
        out = out.replace(&format!("{{{{{}}}}}", name), &json_escape(&value));
    }
    out
}

// ─── Staleness ───────────────────────────────────────────────────────────────

/// The metadata block expman stamps into every notebook it writes, namespaced
/// under `metadata` so nbformat and Jupyter both leave it alone.
const EXPMAN_METADATA_KEY: &str = "expman";
const TEMPLATE_HASH_KEY: &str = "template_hash";
const CONTENT_HASH_KEY: &str = "content_hash";
const TEMPLATE_KEY: &str = "template";
const GENERATOR_KEY: &str = "generated_by";
/// `metadata.expman.template` when no template was used.
const BUILTIN_ORIGIN: &str = "builtin";

/// A notebook rendered and ready to write, plus what it was rendered from.
struct RenderedNotebook {
    /// Parsed rather than kept as text, because the expman metadata block is
    /// injected into it before writing.
    value: serde_json::Value,
    /// Hash of the *source* text, so both a template edit and a change to the
    /// built-in default in a new expman version are detectable.
    template_hash: String,
    /// Recorded in the metadata: a template path, or `builtin`.
    origin: String,
}

fn hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}

/// Fingerprint a notebook for the "has the user edited this?" test.
///
/// Taken over the parsed JSON with `metadata.expman.content_hash` removed — the
/// field cannot cover itself — and re-serialised, so the fingerprint does not
/// depend on the file's whitespace. Executing cells *does* change it, which is
/// intended: outputs are the user's work, not ours to overwrite.
fn content_fingerprint(notebook: &serde_json::Value) -> String {
    let mut probe = notebook.clone();
    if let Some(expman) = probe
        .pointer_mut("/metadata/expman")
        .and_then(|v| v.as_object_mut())
    {
        expman.remove(CONTENT_HASH_KEY);
    }
    hash(&serde_json::to_string(&probe).unwrap_or_default())
}

/// Stamp `metadata.expman` into a rendered notebook and serialise it.
fn seal(rendered: RenderedNotebook) -> String {
    let RenderedNotebook {
        mut value,
        template_hash,
        origin,
    } = rendered;

    // A valid .ipynb always has `metadata`, but a template is user-supplied, so
    // create the object rather than discard the template over a missing key.
    if !value.get("metadata").is_some_and(|m| m.is_object()) {
        value["metadata"] = serde_json::json!({});
    }
    value["metadata"][EXPMAN_METADATA_KEY] = serde_json::json!({
        GENERATOR_KEY: format!("expman {}", env!("CARGO_PKG_VERSION")),
        TEMPLATE_KEY: origin,
        TEMPLATE_HASH_KEY: template_hash,
    });

    // The content hash has to be computed with the field absent, then inserted.
    let fingerprint = content_fingerprint(&value);
    value["metadata"][EXPMAN_METADATA_KEY][CONTENT_HASH_KEY] = serde_json::json!(fingerprint);

    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// What a launch should do about the notebook already on disk.
enum Staleness {
    /// Nothing there — write it.
    Absent,
    /// expman wrote it, it is untouched, and the template has moved on.
    Regenerate,
    /// Untouched and current.
    Current,
    /// Edited, or not written by expman. Never overwritten.
    Keep { reason: &'static str },
}

/// Decide whether the notebook at `path` may be rewritten.
async fn classify(path: &Path, template_hash: &str) -> Staleness {
    let Ok(text) = tokio::fs::read_to_string(path).await else {
        // Either absent, or present and unreadable. Treating an unreadable file
        // as absent would overwrite it, so let the write attempt fail instead.
        if path.exists() {
            return Staleness::Keep {
                reason: "it could not be read",
            };
        }
        return Staleness::Absent;
    };

    let Ok(notebook) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Staleness::Keep {
            reason: "it is not valid JSON",
        };
    };

    let expman = notebook.pointer("/metadata/expman");
    let recorded_content = expman
        .and_then(|m| m.get(CONTENT_HASH_KEY))
        .and_then(|h| h.as_str());
    let recorded_template = expman
        .and_then(|m| m.get(TEMPLATE_HASH_KEY))
        .and_then(|h| h.as_str());

    // A hand-made notebook, or one from a version that did not stamp metadata,
    // counts as edited: we have no evidence it is ours to replace.
    let (Some(recorded_content), Some(recorded_template)) = (recorded_content, recorded_template)
    else {
        return Staleness::Keep {
            reason: "it carries no expman metadata, so expman did not write it",
        };
    };

    if content_fingerprint(&notebook) != recorded_content {
        return Staleness::Keep {
            reason: "it has been edited since expman wrote it",
        };
    }
    if recorded_template != template_hash {
        return Staleness::Regenerate;
    }
    Staleness::Current
}

/// Render the notebook for one run: the project template if there is a usable
/// one, else the built-in default.
///
/// A template that cannot be read, or whose substituted form is not valid JSON,
/// is reported by path and the built-in default used instead. Writing a corrupt
/// `.ipynb` would break the very tab the template exists to improve.
async fn render_notebook(
    template: &NotebookTemplateConfig,
    ctx: &NotebookContext,
    is_python: bool,
) -> Result<RenderedNotebook, String> {
    if let Some(path) = template.resolve() {
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => {
                let substituted = substitute(&text, ctx);
                match serde_json::from_str(&substituted) {
                    Ok(value) => {
                        return Ok(RenderedNotebook {
                            value,
                            template_hash: hash(&text),
                            origin: path.display().to_string(),
                        })
                    }
                    Err(e) => error!(
                        "Notebook template {} is not valid JSON after placeholder \
                         substitution ({}); falling back to the built-in default",
                        path.display(),
                        e
                    ),
                }
            }
            Err(e) => error!(
                "Could not read notebook template {}: {}; falling back to the \
                 built-in default",
                path.display(),
                e
            ),
        }
    }
    builtin(generate_notebook_content(is_python))
}

/// Wrap built-in content as a `RenderedNotebook`.
///
/// Fallible rather than `expect`ing, because the built-in content is not a pure
/// literal: the multi-run variant interpolates run names, which are directory
/// names and so are only as well-behaved as the filesystem. A run named with a
/// quote must fail this request, not panic the handler serving it.
fn builtin(text: String) -> Result<RenderedNotebook, String> {
    let value = serde_json::from_str(&text).map_err(|e| {
        format!("expman generated an invalid built-in notebook ({e}); this is a bug")
    })?;
    Ok(RenderedNotebook {
        value,
        template_hash: hash(&text),
        origin: BUILTIN_ORIGIN.to_string(),
    })
}

/// Write `rendered` to `path` unless the file there must be left alone.
///
/// Returns `Ok(true)` when the file was written, `Ok(false)` when the existing
/// one was kept.
async fn write_notebook(path: &Path, rendered: RenderedNotebook) -> Result<bool, String> {
    match classify(path, &rendered.template_hash).await {
        Staleness::Current => return Ok(false),
        Staleness::Keep { reason } => {
            warn!(
                "Leaving {} alone: {}. It may be stale relative to the current \
                 notebook template; delete it to have expman regenerate it.",
                path.display(),
                reason
            );
            return Ok(false);
        }
        Staleness::Regenerate => info!(
            "Regenerating {}: the notebook template changed and the file is unedited",
            path.display()
        ),
        Staleness::Absent => {}
    }

    let content = seal(rendered);
    if let Err(e) = tokio::fs::write(path, content).await {
        error!("Failed to generate {}: {}", NOTEBOOK_FILE, e);
        return Err(format!("Failed to generate {}: {}", NOTEBOOK_FILE, e));
    }
    Ok(true)
}

// ─── Built-in notebook content ───────────────────────────────────────────────

/// Generate the full `.ipynb` JSON content for a default interactive notebook.
///
/// This is the fallback used when a store supplies no template. For Python runs,
/// produces 2 cells:
///   1. Install dependencies (`pip install polars matplotlib`)
///   2. Load and display metrics
///
/// For Rust runs, produces a single cell with a `polars` snippet.
pub fn generate_notebook_content(is_python: bool) -> String {
    let cells = if is_python {
        r##"{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": [
    "# Install required dependencies into this environment\n",
    "import sys\n",
    "!pip --python {sys.executable} install polars matplotlib"
   ]
  },
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": [
    "import polars as pl\n",
    "import matplotlib.pyplot as plt\n",
    "\n",
    "# Load run vectors\n",
    "vectors_path = 'vectors.parquet'\n",
    "df = pl.read_parquet(vectors_path)\n",
    "\n",
    "# Display the latest metrics\n",
    "df.tail()"
   ]
  }"##
        .to_string()
    } else {
        let snippet = "use polars::prelude::*;\n\nfn main() -> Result<(), PolarsError> {\n    // Load run vectors\n    let mut file = std::fs::File::open(\"vectors.parquet\").unwrap();\n    let df = ParquetReader::new(&mut file).finish()?;\n\n    println!(\"{:?}\", df.tail(Some(5)));\n    Ok(())\n}";
        format!(
            r#"{{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    "{}"
   ]
  }}"#,
            snippet.replace('\n', "\\n").replace('"', "\\\"")
        )
    };

    format!(
        r#"{{
 "cells": [
  {}
 ],
 "metadata": {{}},
 "nbformat": 4,
 "nbformat_minor": 5
}}"#,
        cells
    )
}

/// Write `interactive.ipynb` into the run directory.
///
/// Returns `Ok(true)` if the notebook was written, `Ok(false)` if the existing
/// one was kept — either because it is already current, or because it has been
/// edited (see `classify`).
pub async fn generate_notebook(
    ctx: &NotebookContext,
    is_python: bool,
    template: &NotebookTemplateConfig,
) -> Result<bool, String> {
    let path = ctx.run_dir.join(NOTEBOOK_FILE);
    let rendered = render_notebook(template, ctx, is_python).await?;
    write_notebook(&path, rendered).await
}

/// Generate the full `.ipynb` JSON content for a multi-run interactive notebook.
///
/// **Not templatable.** See `generate_multi_run_notebook`.
pub fn generate_multi_run_notebook_content(is_python: bool, runs: &[String]) -> String {
    // Run names are directory names, so they can hold anything the filesystem
    // allows — including a quote, which would close the JSON string literal these
    // snippets are spliced into and yield a notebook Jupyter cannot open. The
    // Python branch splices straight into JSON so it escapes here; the Rust
    // branch escapes its whole snippet at the end, so it must *not* (escaping
    // twice would leave a lone backslash before the quote).
    let cells = if is_python {
        // Escaped once, here, because both snippets below are spliced straight
        // into JSON string literals.
        let escaped: Vec<String> = runs.iter().map(|run| json_escape(run)).collect();
        let load_snippets = escaped.iter().map(|run| {
            format!("df_{} = pl.read_parquet('{}/vectors.parquet').with_columns(pl.lit('{}').alias('run'))", run.replace('-', "_"), run, run)
        }).collect::<Vec<_>>().join("\n");
        let tail_snippets = escaped
            .iter()
            .map(|run| format!("df_{}.tail()", run.replace('-', "_")))
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            r##"{{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    "# Install required dependencies into this environment\n",
    "import sys\n",
    "!pip --python {{sys.executable}} install polars matplotlib"
   ]
  }},
  {{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    "import polars as pl\n",
    "import matplotlib.pyplot as plt\n",
    "\n",
    "# Load run vectors\n",
    "{}\n",
    "\n",
    "# Display the latest metrics\n",
    "{}"
   ]
  }}"##,
            load_snippets.replace('\n', "\\n"),
            tail_snippets.replace('\n', "\\n")
        )
    } else {
        let load_snippets = runs.iter().map(|run| {
            format!("    let df_{} = ParquetReader::new(&mut std::fs::File::open(\"{}/vectors.parquet\").unwrap()).finish()?;\n    // Note: To add a 'run' column in rust polars you would typically use lit(\"{}\") in a select/with_columns, \n    // but for simplicity here we just load them.", run.replace('-', "_"), run, run)
        }).collect::<Vec<_>>().join("\n");

        let snippet = format!("use polars::prelude::*;\n\nfn main() -> Result<(), PolarsError> {{\n    // Load run vectors\n{}\n    Ok(())\n}}", load_snippets);
        format!(
            r#"{{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    "{}"
   ]
  }}"#,
            json_escape(&snippet)
        )
    };

    format!(
        r#"{{
 "cells": [
  {}
 ],
 "metadata": {{}},
 "nbformat": 4,
 "nbformat_minor": 5
}}"#,
        cells
    )
}

/// Write the multi-run `interactive.ipynb` into the experiment directory.
///
/// **Deliberately always the built-in default: `--notebook-template` does not
/// apply here.** A multi-run notebook has no single run, so `{{run_dir}}`,
/// `{{run_name}}` and `{{project}}` have no value to take, and a useful shared
/// template would need a different placeholder set (a `{{runs}}` list, and a
/// rule for rendering it) — a separate design, not a wider version of this one.
/// Recorded in `docs/content/reference/cli.md`.
///
/// The staleness contract *is* shared: the built-in content includes the run
/// names, so adding a run refreshes an unedited notebook, and an edited one is
/// still never overwritten.
pub async fn generate_multi_run_notebook(
    exp_dir: &Path,
    is_python: bool,
    runs: &[String],
) -> Result<bool, String> {
    let path = exp_dir.join(NOTEBOOK_FILE);
    let rendered = builtin(generate_multi_run_notebook_content(is_python, runs))?;
    write_notebook(&path, rendered).await
}

// ─── Launching Jupyter ───────────────────────────────────────────────────────

/// The command line that launches Jupyter, split into program and leading args.
///
/// Configured by `exp serve --jupyter-command`; defaults to a bare `jupyter`.
///
/// **Why a command line rather than a program name.** The kernel a notebook gets
/// is the interpreter Jupyter itself runs under: launched from inside a
/// project's virtualenv, Jupyter's built-in `python3` kernel *is* that venv's
/// interpreter, so `import <project package>` works with no `ipykernel install`
/// and no kernelspec name to keep in sync between expman and the project. That
/// is the whole point of the flag — `--jupyter-command 'uv run --extra nb
/// jupyter'` — and the reason expman deliberately does **not** register a global
/// kernel or write a named kernelspec into the generated notebook. Either of
/// those would mutate the user's `~/.local/share/jupyter` and then have to be
/// kept correct forever.
#[derive(Debug, Clone)]
pub struct JupyterCommand {
    /// The configured line, verbatim, for error messages.
    raw: String,
    program: String,
    leading_args: Vec<String>,
}

impl JupyterCommand {
    /// Split a configured command line into a program and its leading arguments.
    ///
    /// Splitting is POSIX-shell-like (via `shlex`), so a quoted path containing a
    /// space survives. No shell is involved: the program is executed directly, so
    /// pipes, redirections and variable expansion are not available.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let words = shlex::split(raw).ok_or_else(|| {
            format!(
                "--jupyter-command {:?} is not a parsable command line \
                 (unbalanced quote or trailing backslash?)",
                raw
            )
        })?;
        let mut words = words.into_iter();
        let program = words
            .next()
            .ok_or_else(|| "--jupyter-command must name a program".to_string())?;
        Ok(Self {
            raw: raw.to_string(),
            program,
            leading_args: words.collect(),
        })
    }

    /// A `Command` for the configured line, ready for the Jupyter arguments.
    ///
    /// The configured words go first, so `uv run --extra nb jupyter` followed by
    /// `notebook …` composes into the command line a user would type by hand.
    fn base_command(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.program);
        cmd.args(&self.leading_args);
        cmd
    }

    /// The configured line as given, for messages the user has to act on.
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl Default for JupyterCommand {
    fn default() -> Self {
        Self::parse(DEFAULT_JUPYTER_COMMAND).expect("the default jupyter command is one word")
    }
}

/// The arguments that make a Jupyter server iframeable by the dashboard: no
/// browser, our port, and no token/password/XSRF/framing protection.
///
/// See `docs/content/decisions.md` — the dashboard does not proxy Jupyter, so
/// this is only safe because the server is expected to be on localhost.
fn notebook_args(port: u16) -> [String; 7] {
    [
        "notebook".to_string(),
        "--no-browser".to_string(),
        format!("--port={}", port),
        "--ServerApp.token=''".to_string(),
        "--ServerApp.password=''".to_string(),
        "--ServerApp.disable_check_xsrf=True".to_string(),
        "--ServerApp.tornado_settings={\"headers\":{\"Content-Security-Policy\":\"frame-ancestors *\"}}".to_string(),
    ]
}

/// Detect the best available interactive Python backend in the user's environment.
///
/// Checks the configured Jupyter command, then `python3`.
pub async fn detect_backend(jupyter: &JupyterCommand) -> InteractiveBackend {
    // Check jupyter
    match jupyter
        .base_command()
        .args(["notebook", "--version"])
        .output()
        .await
    {
        Ok(output) => {
            info!(
                "`{} notebook --version`: status={}, stdout={}, stderr={}",
                jupyter.as_str(),
                output.status,
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            if output.status.success() {
                return InteractiveBackend::Jupyter;
            }
        }
        Err(e) => {
            info!("`{}` not found: {}", jupyter.as_str(), e);
        }
    }

    // Check python3
    match tokio::process::Command::new("python3")
        .arg("--version")
        .output()
        .await
    {
        Ok(output) => {
            info!(
                "python3 --version: status={}, stdout={}",
                output.status,
                String::from_utf8_lossy(&output.stdout).trim()
            );
            if output.status.success() {
                return InteractiveBackend::Python;
            }
        }
        Err(e) => {
            info!("python3 not found: {}", e);
        }
    }

    InteractiveBackend::None
}

/// Thread-safe manager for spawning and stopping Jupyter Notebooks.
///
/// When Jupyter is available in the user's environment, this manager spawns
/// per-run Jupyter instances. When only ipython/python is available, the
/// frontend shows notebook content with copy-paste guidance instead.
#[derive(Clone, Default)]
pub struct JupyterManager {
    instances: Arc<Mutex<HashMap<String, JupyterInstance>>>,
    /// How to launch Jupyter (`exp serve --jupyter-command`).
    command: JupyterCommand,
    /// Where generated notebooks come from (`exp serve --notebook-template`).
    template: NotebookTemplateConfig,
}

impl JupyterManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// A manager that honours the server's Jupyter configuration.
    pub fn with_config(command: JupyterCommand, template: NotebookTemplateConfig) -> Self {
        Self {
            instances: Arc::default(),
            command,
            template,
        }
    }

    /// The configured Jupyter command, for backend detection.
    pub fn command(&self) -> &JupyterCommand {
        &self.command
    }

    /// The configured notebook template source.
    pub fn template(&self) -> &NotebookTemplateConfig {
        &self.template
    }

    /// Finds an available TCP port.
    fn get_available_port() -> Option<u16> {
        (8888..9999).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
    }

    /// Launch a Jupyter server in `cwd` on `port`, failing if it dies at once.
    async fn spawn_server(&self, cwd: &Path, port: u16) -> Result<Child, String> {
        let mut child = self
            .command
            .base_command()
            .args(notebook_args(port))
            .current_dir(cwd)
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to start Jupyter with the configured command `{}`: {}. \
                     Point `exp serve --jupyter-command` at a command line that \
                     launches Jupyter (for example `uv run --extra nb jupyter`).",
                    self.command.as_str(),
                    e
                )
            })?;

        // Small wait to ensure it hasn't instantly crashed
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "Jupyter (`{}`) crashed immediately with status {}",
                self.command.as_str(),
                status
            ));
        }
        Ok(child)
    }

    /// Spawns a Jupyter Notebook for a given run.
    pub async fn spawn(&self, ctx: &NotebookContext, is_python: bool) -> Result<u16, String> {
        let key = format!("{}:{}", ctx.experiment, ctx.run_name);

        // Already running?
        {
            let instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get(&key) {
                return Ok(instance.port);
            }
        }

        let port = Self::get_available_port()
            .ok_or_else(|| "No available ports for Jupyter".to_string())?;

        // Generate the notebook, or refresh it if the template has moved on
        generate_notebook(ctx, is_python, &self.template).await?;

        info!("Spawning Jupyter Notebook for {} on port {}", key, port);
        let child = self.spawn_server(&ctx.run_dir, port).await?;

        let mut instances = self.instances.lock().unwrap();
        instances.insert(
            key,
            JupyterInstance {
                port,
                process: child,
            },
        );

        Ok(port)
    }

    /// Spawns a multi-run Jupyter Notebook in the experiment directory.
    pub async fn spawn_multi(
        &self,
        exp: &str,
        exp_dir: PathBuf,
        is_python: bool,
        runs: &[String],
    ) -> Result<u16, String> {
        let key = format!("{}:__multi__", exp);

        // Already running?
        {
            let instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get(&key) {
                return Ok(instance.port);
            }
        }

        let port = Self::get_available_port()
            .ok_or_else(|| "No available ports for Jupyter".to_string())?;

        // Generate notebook if it doesn't exist
        generate_multi_run_notebook(&exp_dir, is_python, runs).await?;

        info!(
            "Spawning multi-run Jupyter Notebook for {} on port {}",
            exp, port
        );
        let child = self.spawn_server(&exp_dir, port).await?;

        let mut instances = self.instances.lock().unwrap();
        instances.insert(
            key,
            JupyterInstance {
                port,
                process: child,
            },
        );

        Ok(port)
    }

    /// Returns the port if the notebook is running.
    pub fn status(&self, exp: &str, run: &str) -> Option<u16> {
        let key = format!("{}:{}", exp, run);
        let mut instances = self.instances.lock().unwrap();

        if let Some(instance) = instances.get_mut(&key) {
            match instance.process.try_wait() {
                Ok(Some(_)) => { /* exited */ }
                Ok(None) => return Some(instance.port),
                Err(_) => { /* error polling */ }
            }
        }

        instances.remove(&key);
        None
    }

    /// Stops a running Jupyter instance.
    pub async fn stop(&self, exp: &str, run: &str) -> Result<(), String> {
        let key = format!("{}:{}", exp, run);
        let mut instance = {
            let mut instances = self.instances.lock().unwrap();
            instances.remove(&key)
        };

        if let Some(mut inst) = instance.take() {
            info!("Shutting down Jupyter Notebook for {}", key);
            let _ = inst.process.kill().await;
            let _ = inst.process.wait().await;
        }

        Ok(())
    }

    /// Kill all notebooks (e.g., on server shutdown).
    pub async fn shutdown_all(&self) {
        let all: Vec<_> = {
            let mut instances = self.instances.lock().unwrap();
            instances.drain().map(|(_, inst)| inst).collect()
        };

        for mut inst in all {
            let _ = inst.process.kill().await;
            let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), inst.process.wait())
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context whose run directory is `dir`, so a test can write there.
    fn ctx_in(dir: &Path) -> NotebookContext {
        NotebookContext {
            run_dir: dir.to_path_buf(),
            run_name: "run_1".to_string(),
            experiment: "eval".to_string(),
            store: dir.to_path_buf(),
            project: "study1".to_string(),
        }
    }

    fn read_notebook(path: &Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn cell_sources(notebook: &serde_json::Value) -> String {
        notebook["cells"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|cell| cell["source"].as_array().unwrap())
            .map(|line| line.as_str().unwrap())
            .collect()
    }

    /// A one-cell template that echoes back every placeholder.
    fn template_using_all_placeholders() -> String {
        let lines = NOTEBOOK_PLACEHOLDERS
            .iter()
            .map(|name| format!("\"{0} = '{{{{{0}}}}}'\\n\"", name))
            .collect::<Vec<_>>()
            .join(",\n    ");
        format!(
            r#"{{
 "cells": [
  {{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    {}
   ]
  }}
 ],
 "metadata": {{}},
 "nbformat": 4,
 "nbformat_minor": 5
}}"#,
            lines
        )
    }

    #[test]
    fn test_jupyter_manager_new() {
        let manager = JupyterManager::new();
        assert!(manager.status("eval", "run_1").is_none());
    }

    #[test]
    fn test_get_available_port() {
        let port = JupyterManager::get_available_port();
        assert!(port.is_some());
        let p = port.unwrap();
        assert!((8888..9999).contains(&p));
    }

    #[tokio::test]
    async fn test_stop_non_existent() {
        let manager = JupyterManager::new();
        let res = manager.stop("exp1", "run1").await;
        assert!(res.is_ok());
    }

    #[test]
    fn test_generate_notebook_content_python_has_two_cells() {
        let content = generate_notebook_content(true);
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let cells = parsed["cells"].as_array().unwrap();
        assert_eq!(
            cells.len(),
            2,
            "Python notebook should have exactly 2 cells"
        );
        assert_eq!(parsed["nbformat"], 4);
    }
    #[test]
    fn test_generate_notebook_content_rust_has_one_cell() {
        let content = generate_notebook_content(false);
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let cells = parsed["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 1, "Rust notebook should have exactly 1 cell");
        assert_eq!(parsed["nbformat"], 4);
    }

    #[test]
    fn test_generate_multi_run_notebook_content_python_shows_tails() {
        let runs = vec!["run-1".to_string(), "run-2".to_string()];
        let content = generate_multi_run_notebook_content(true, &runs);
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let cells = parsed["cells"].as_array().unwrap();

        // Find the cell with the tails snippet
        let mut found = false;
        for cell in cells {
            if let Some(source) = cell["source"].as_array() {
                let full_source = source
                    .iter()
                    .map(|s| s.as_str().unwrap())
                    .collect::<String>();
                if full_source.contains("df_run_1.tail()")
                    && full_source.contains("df_run_2.tail()")
                {
                    found = true;
                }
            }
        }
        assert!(found, "Should have found a cell with individual tail calls");
    }

    #[tokio::test]
    async fn test_generate_notebook_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_in(tmp.path());
        let created = generate_notebook(&ctx, true, &NotebookTemplateConfig::default())
            .await
            .unwrap();
        assert!(created);
        assert!(tmp.path().join(NOTEBOOK_FILE).exists());

        let created_again = generate_notebook(&ctx, true, &NotebookTemplateConfig::default())
            .await
            .unwrap();
        assert!(
            !created_again,
            "an up-to-date notebook must not be rewritten"
        );
    }

    #[tokio::test]
    async fn test_detect_backend_returns_something() {
        let backend = detect_backend(&JupyterCommand::default()).await;
        // In CI/test environments, at least python3 should be available
        assert_ne!(backend, InteractiveBackend::None);
    }

    // ─── Template discovery ──────────────────────────────────────────────

    #[test]
    fn test_template_resolution_prefers_the_flag_then_the_convention() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let conventional = store.join(CONVENTIONAL_TEMPLATE);
        std::fs::create_dir_all(conventional.parent().unwrap()).unwrap();
        let explicit = tmp.path().join("explicit.ipynb");

        // Nothing on disk: the built-in default.
        let mut config = NotebookTemplateConfig {
            explicit: None,
            base_dir: store.clone(),
        };
        assert_eq!(config.resolve(), None);

        // The convention alone.
        std::fs::write(&conventional, "{}").unwrap();
        assert_eq!(config.resolve(), Some(conventional.clone()));

        // The flag wins over the convention.
        std::fs::write(&explicit, "{}").unwrap();
        config.explicit = Some(explicit.clone());
        assert_eq!(config.resolve(), Some(explicit));

        // A flag pointing at nothing falls through rather than failing.
        config.explicit = Some(tmp.path().join("absent.ipynb"));
        assert_eq!(config.resolve(), Some(conventional));
    }

    #[tokio::test]
    async fn test_template_placeholders_are_all_substituted() {
        let tmp = tempfile::tempdir().unwrap();
        let template = tmp.path().join("notebook.ipynb");
        std::fs::write(&template, template_using_all_placeholders()).unwrap();

        let run_dir = tmp.path().join("store").join("eval").join("run_1");
        std::fs::create_dir_all(&run_dir).unwrap();
        let ctx =
            NotebookContext::new(&tmp.path().join("store"), "eval", "run_1", "s1".to_string());
        let config = NotebookTemplateConfig {
            explicit: Some(template),
            base_dir: PathBuf::new(),
        };

        assert!(generate_notebook(&ctx, true, &config).await.unwrap());
        let source = cell_sources(&read_notebook(&run_dir.join(NOTEBOOK_FILE)));

        assert!(source.contains(&format!("run_dir = '{}'", run_dir.display())));
        assert!(source.contains("run_name = 'run_1'"));
        assert!(source.contains("experiment = 'eval'"));
        assert!(source.contains(&format!("store = '{}'", tmp.path().join("store").display())));
        assert!(source.contains("project = 's1'"));
        assert!(
            !source.contains("{{"),
            "no placeholder should survive: {source}"
        );
    }

    #[tokio::test]
    async fn test_placeholder_values_are_json_escaped() {
        let tmp = tempfile::tempdir().unwrap();
        let template = tmp.path().join("notebook.ipynb");
        std::fs::write(&template, template_using_all_placeholders()).unwrap();

        // A run name that would corrupt the JSON if spliced in verbatim.
        let nasty = r#"quote"back\slash"#;
        let ctx = NotebookContext {
            run_dir: tmp.path().to_path_buf(),
            run_name: nasty.to_string(),
            experiment: "tab\there".to_string(),
            store: tmp.path().to_path_buf(),
            project: String::new(),
        };
        let config = NotebookTemplateConfig {
            explicit: Some(template),
            base_dir: PathBuf::new(),
        };

        assert!(generate_notebook(&ctx, true, &config).await.unwrap());
        // Parsing at all is the assertion that matters: an unescaped `"` would
        // have ended the string literal and produced invalid JSON.
        let source = cell_sources(&read_notebook(&tmp.path().join(NOTEBOOK_FILE)));
        assert!(
            source.contains(&format!("run_name = '{}'", nasty)),
            "the escaped value must decode back to the original: {source}"
        );
        assert!(source.contains("experiment = 'tab\there'"));
    }

    #[tokio::test]
    async fn test_invalid_template_falls_back_to_the_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let template = tmp.path().join("broken.ipynb");
        std::fs::write(&template, "{ this is not json").unwrap();

        let ctx = ctx_in(tmp.path());
        let config = NotebookTemplateConfig {
            explicit: Some(template),
            base_dir: PathBuf::new(),
        };

        assert!(generate_notebook(&ctx, true, &config).await.unwrap());
        let notebook = read_notebook(&tmp.path().join(NOTEBOOK_FILE));
        assert_eq!(notebook["cells"].as_array().unwrap().len(), 2);
        assert_eq!(
            notebook["metadata"]["expman"]["template"], BUILTIN_ORIGIN,
            "the fallback must record that no template was used"
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_unreadable_template_falls_back_to_the_builtin() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let template = tmp.path().join("locked.ipynb");
        std::fs::write(&template, r#"{"cells": [], "metadata": {}, "nbformat": 4}"#).unwrap();
        std::fs::set_permissions(&template, std::fs::Permissions::from_mode(0o000)).unwrap();

        let ctx = ctx_in(tmp.path());
        let config = NotebookTemplateConfig {
            explicit: Some(template.clone()),
            base_dir: PathBuf::new(),
        };

        let wrote = generate_notebook(&ctx, true, &config).await;
        // Restore before asserting, so a failure does not leave an unremovable
        // temp dir behind.
        std::fs::set_permissions(&template, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(wrote.unwrap(), "an unreadable template must not be fatal");
        let notebook = read_notebook(&tmp.path().join(NOTEBOOK_FILE));
        assert_eq!(notebook["metadata"]["expman"]["template"], BUILTIN_ORIGIN);
    }

    /// The backward-compatibility guarantee: a store with no template gets
    /// byte-for-byte the cells 1.2.1 produced.
    ///
    /// Downstream depends on this — enabling templating must not quietly change
    /// what every existing store already sees. Only the JSON *around* the cells
    /// differs (pretty-printed, plus the `metadata.expman` stamp).
    #[tokio::test]
    async fn test_no_template_reproduces_the_builtin_cells_exactly() {
        for is_python in [true, false] {
            let tmp = tempfile::tempdir().unwrap();
            let ctx = ctx_in(tmp.path());
            // No explicit template, and `base_dir` holds no `.expman/`.
            let config = NotebookTemplateConfig {
                explicit: None,
                base_dir: tmp.path().to_path_buf(),
            };
            assert!(generate_notebook(&ctx, is_python, &config).await.unwrap());

            let written = read_notebook(&tmp.path().join(NOTEBOOK_FILE));
            let expected: serde_json::Value =
                serde_json::from_str(&generate_notebook_content(is_python)).unwrap();

            assert_eq!(
                written["cells"], expected["cells"],
                "is_python={is_python}: the built-in cells must be unchanged"
            );
            assert_eq!(written["nbformat"], expected["nbformat"]);
            assert_eq!(written["nbformat_minor"], expected["nbformat_minor"]);
            assert_eq!(written["metadata"]["expman"]["template"], BUILTIN_ORIGIN);
        }
    }

    #[tokio::test]
    async fn test_a_template_without_metadata_still_gets_stamped() {
        let tmp = tempfile::tempdir().unwrap();
        let template = tmp.path().join("bare.ipynb");
        std::fs::write(&template, r#"{"cells": [], "nbformat": 4}"#).unwrap();

        let ctx = ctx_in(tmp.path());
        let config = NotebookTemplateConfig {
            explicit: Some(template),
            base_dir: PathBuf::new(),
        };
        assert!(generate_notebook(&ctx, true, &config).await.unwrap());

        let notebook = read_notebook(&tmp.path().join(NOTEBOOK_FILE));
        assert!(notebook["metadata"]["expman"]["content_hash"].is_string());
    }

    // ─── Staleness ───────────────────────────────────────────────────────

    /// Write a notebook from `body`, used as a stand-in for a template edit.
    fn template_with(dir: &Path, marker: &str) -> PathBuf {
        let path = dir.join("notebook.ipynb");
        std::fs::write(
            &path,
            format!(
                r#"{{"cells": [{{"cell_type": "code", "execution_count": null,
 "metadata": {{}}, "outputs": [], "source": ["{}"]}}],
 "metadata": {{}}, "nbformat": 4, "nbformat_minor": 5}}"#,
                marker
            ),
        )
        .unwrap();
        path
    }

    #[tokio::test]
    async fn test_unedited_notebook_is_rewritten_when_the_template_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_in(tmp.path());
        let template = template_with(tmp.path(), "first");
        let config = NotebookTemplateConfig {
            explicit: Some(template.clone()),
            base_dir: PathBuf::new(),
        };

        assert!(generate_notebook(&ctx, true, &config).await.unwrap());
        let notebook_path = tmp.path().join(NOTEBOOK_FILE);
        assert!(cell_sources(&read_notebook(&notebook_path)).contains("first"));

        // Unchanged template: left alone.
        assert!(!generate_notebook(&ctx, true, &config).await.unwrap());

        // Changed template, file untouched: rewritten.
        template_with(tmp.path(), "second");
        assert!(generate_notebook(&ctx, true, &config).await.unwrap());
        assert!(cell_sources(&read_notebook(&notebook_path)).contains("second"));
    }

    #[tokio::test]
    async fn test_an_edited_notebook_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_in(tmp.path());
        let config = NotebookTemplateConfig {
            explicit: Some(template_with(tmp.path(), "first")),
            base_dir: PathBuf::new(),
        };
        assert!(generate_notebook(&ctx, true, &config).await.unwrap());

        // The user changes a cell, keeping expman's metadata intact.
        let notebook_path = tmp.path().join(NOTEBOOK_FILE);
        let mut notebook = read_notebook(&notebook_path);
        notebook["cells"][0]["source"] = serde_json::json!(["mine\n"]);
        std::fs::write(
            &notebook_path,
            serde_json::to_string_pretty(&notebook).unwrap(),
        )
        .unwrap();

        // Template moves on; the edit still wins.
        template_with(tmp.path(), "second");
        assert!(!generate_notebook(&ctx, true, &config).await.unwrap());
        assert!(cell_sources(&read_notebook(&notebook_path)).contains("mine"));
    }

    #[tokio::test]
    async fn test_a_notebook_without_expman_metadata_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_in(tmp.path());
        let notebook_path = tmp.path().join(NOTEBOOK_FILE);
        std::fs::write(
            &notebook_path,
            r#"{"cells": [], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}"#,
        )
        .unwrap();

        let config = NotebookTemplateConfig {
            explicit: Some(template_with(tmp.path(), "first")),
            base_dir: PathBuf::new(),
        };
        assert!(!generate_notebook(&ctx, true, &config).await.unwrap());
        assert!(read_notebook(&notebook_path)["metadata"]["expman"].is_null());
    }

    #[test]
    fn test_multi_run_content_survives_a_run_name_with_a_quote() {
        // Run names are directory names; the filesystem permits this.
        let runs = vec![r#"run"one"#.to_string(), r"run\two".to_string()];
        for is_python in [true, false] {
            let content = generate_multi_run_notebook_content(is_python, &runs);
            serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|e| {
                panic!("is_python={is_python} produced invalid JSON: {e}\n{content}")
            });
        }
    }

    #[tokio::test]
    async fn test_multi_run_notebook_tracks_its_run_set() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = vec!["run-1".to_string()];
        assert!(generate_multi_run_notebook(tmp.path(), true, &runs)
            .await
            .unwrap());
        assert!(!generate_multi_run_notebook(tmp.path(), true, &runs)
            .await
            .unwrap());

        let more = vec!["run-1".to_string(), "run-2".to_string()];
        assert!(generate_multi_run_notebook(tmp.path(), true, &more)
            .await
            .unwrap());
        assert!(cell_sources(&read_notebook(&tmp.path().join(NOTEBOOK_FILE))).contains("df_run_2"));
    }

    // ─── Command resolution ──────────────────────────────────────────────

    #[test]
    fn test_default_command_is_a_bare_jupyter() {
        let cmd = JupyterCommand::default();
        assert_eq!(cmd.program, "jupyter");
        assert!(cmd.leading_args.is_empty());
        assert_eq!(cmd.as_str(), DEFAULT_JUPYTER_COMMAND);
    }

    #[test]
    fn test_command_splits_a_command_line() {
        let cmd = JupyterCommand::parse("uv run --extra nb jupyter").unwrap();
        assert_eq!(cmd.program, "uv");
        assert_eq!(cmd.leading_args, ["run", "--extra", "nb", "jupyter"]);
    }

    #[test]
    fn test_command_honours_quoting() {
        let cmd = JupyterCommand::parse(r#""/opt/my tools/uv" run jupyter"#).unwrap();
        assert_eq!(cmd.program, "/opt/my tools/uv");
        assert_eq!(cmd.leading_args, ["run", "jupyter"]);
    }

    #[test]
    fn test_command_rejects_unparsable_and_empty_lines() {
        let err = JupyterCommand::parse("uv \"run").unwrap_err();
        assert!(err.contains("uv \\\"run"), "should quote the value: {err}");
        assert!(JupyterCommand::parse("   ").is_err());
    }

    #[test]
    fn test_notebook_args_follow_the_configured_command() {
        // The composition that has to hold: `uv run … jupyter` + `notebook …`.
        let cmd = JupyterCommand::parse("uv run --extra nb jupyter").unwrap();
        let mut words = cmd.leading_args.clone();
        words.extend(notebook_args(8888));
        assert_eq!(&words[..5], ["run", "--extra", "nb", "jupyter", "notebook"]);
        assert!(words.contains(&"--port=8888".to_string()));
    }

    #[tokio::test]
    async fn test_spawn_names_the_configured_command_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = JupyterManager::with_config(
            JupyterCommand::parse("expman-no-such-jupyter-binary").unwrap(),
            NotebookTemplateConfig::default(),
        );
        let err = manager
            .spawn_server(tmp.path(), 8888)
            .await
            .expect_err("a missing program must fail");
        assert!(
            err.contains("expman-no-such-jupyter-binary"),
            "the error must name the configured command, not just ENOENT: {err}"
        );
        assert!(err.contains("--jupyter-command"));
    }
}
