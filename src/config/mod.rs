//! Configuration loading. Reads `.synrepo/config.toml`.

mod explain;
mod mode;
mod thresholds;

pub use explain::ExplainConfig;
pub use mode::Mode;
pub use thresholds::CrossLinkConfidenceThresholds;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Top-level config read from `.synrepo/config.toml`.
//
// REVIEW NOTE: every field below has `#[serde(default)]` or
// `#[serde(default = "...")]`. This is the backward-compatibility contract:
// an older `config.toml` missing a newer field still deserializes. Any new
// field MUST carry one of those attributes. Container-level
// `#[serde(default)]` is deliberately not used so the author of a new
// field is forced to name the default explicitly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Operational mode. Bootstrap prefers an explicit `--mode`, otherwise
    /// it keeps an existing configured mode or falls back to repository
    /// inspection before defaulting to `auto`.
    #[serde(default)]
    pub mode: Mode,

    /// Roots to index, relative to the repo root. Default is `["."]`.
    #[serde(default = "default_roots")]
    pub roots: Vec<String>,

    /// Directories that contain human-authored concept/decision files.
    /// If empty, concept nodes are disabled in auto mode.
    #[serde(default = "default_concept_dirs")]
    pub concept_directories: Vec<String>,

    /// Git history depth for mining co-change, ownership, blame.
    #[serde(default = "default_git_commit_depth")]
    pub git_commit_depth: u32,

    /// Maximum file size in bytes for indexing. Files above this are skipped.
    #[serde(default = "default_max_file_size")]
    pub max_file_size_bytes: u64,

    /// Advisory ceiling for the in-memory graph snapshot. `0` disables
    /// snapshot publication entirely.
    #[serde(default = "default_max_graph_snapshot_bytes")]
    pub max_graph_snapshot_bytes: usize,

    /// Paths matching these globs are skipped entirely (e.g. secrets).
    #[serde(default = "default_redact_globs")]
    pub redact_globs: Vec<String>,

    /// Approximate token budget per commentary-generation call. Callers
    /// skip generation when the estimated cost exceeds this limit and log
    /// the decision at `warn` level.
    #[serde(default = "default_commentary_cost_limit")]
    pub commentary_cost_limit: u32,

    /// Maximum number of LLM cross-link generation calls the explain pass
    /// may make in one `synrepo sync --generate-cross-links` invocation.
    /// Once the limit is reached, remaining candidate pairs are surfaced as
    /// `blocked` without a model call.
    #[serde(default = "default_cross_link_cost_limit")]
    pub cross_link_cost_limit: u32,

    /// Directory where `synrepo export` writes generated context snapshots,
    /// relative to the repo root. Defaults to `synrepo-context`. Changing
    /// this field does not trigger a graph rebuild (non-compatibility-sensitive).
    #[serde(default = "default_export_dir")]
    pub export_dir: String,

    /// Confidence-tier partition thresholds used by `classify_confidence`.
    /// Changing these does not require a graph rebuild: `synrepo sync
    /// revalidate_links` re-derives the tier for each stored candidate.
    #[serde(default)]
    pub cross_link_confidence_thresholds: CrossLinkConfidenceThresholds,

    /// Number of compile revisions to retain retired observations before
    /// compaction physically deletes them. Compaction runs during `sync`
    /// and `upgrade --apply`, never during the hot reconcile path.
    #[serde(default = "default_retain_retired_revisions")]
    pub retain_retired_revisions: u64,

    /// Enable embedding-based semantic triage for cross-link candidate
    /// generation. When true, an embedding index is built (during init
    /// or reconcile) and used to prefilter candidate pairs based on
    /// cosine similarity.
    #[serde(default)]
    pub enable_semantic_triage: bool,

    /// The embedding model to use for semantic triage. Can be a built-in
    /// model name (all-MiniLM-L6-v2, all-MiniLM-L12-v2, all-mpnet-base-v2),
    /// a Hugging Face model ID (e.g., intfloat/e5-base-v2), or an
    /// absolute path to a local `.onnx` file.
    #[serde(default = "default_semantic_model")]
    pub semantic_model: String,

    /// The expected output dimension of the embedding model. Must match
    /// the model's actual output dimension. Built-in models: 384 for
    /// L6/L12, 768 for mpnet-base.
    #[serde(default = "default_embedding_dim")]
    pub embedding_dim: u16,

    /// Cosine similarity threshold for the semantic prefilter. Pairs exceeding
    /// this threshold are forwarded to LLM evidence extraction.
    #[serde(default = "default_semantic_similarity_threshold")]
    pub semantic_similarity_threshold: f64,

    /// LLM explain configuration. Off by default; opting in is required
    /// even when provider API keys are present in the env. See
    /// [`ExplainConfig`].
    #[serde(default)]
    pub explain: ExplainConfig,
}

fn default_roots() -> Vec<String> {
    vec![".".to_string()]
}

fn default_concept_dirs() -> Vec<String> {
    vec![
        "docs/concepts".to_string(),
        "docs/adr".to_string(),
        "docs/decisions".to_string(),
    ]
}

fn default_git_commit_depth() -> u32 {
    500
}

fn default_max_file_size() -> u64 {
    1024 * 1024 // 1 MB
}

fn default_max_graph_snapshot_bytes() -> usize {
    128 * 1024 * 1024
}

fn default_redact_globs() -> Vec<String> {
    vec![
        "**/secrets/**".to_string(),
        "**/*.env*".to_string(),
        "**/*-private.md".to_string(),
    ]
}

fn default_commentary_cost_limit() -> u32 {
    5000
}

fn default_cross_link_cost_limit() -> u32 {
    200
}

fn default_export_dir() -> String {
    "synrepo-context".to_string()
}

fn default_retain_retired_revisions() -> u64 {
    10
}

fn default_semantic_model() -> String {
    "all-MiniLM-L6-v2".to_string()
}

fn default_embedding_dim() -> u16 {
    384
}

fn default_semantic_similarity_threshold() -> f64 {
    0.6
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            roots: default_roots(),
            concept_directories: default_concept_dirs(),
            git_commit_depth: default_git_commit_depth(),
            max_file_size_bytes: default_max_file_size(),
            max_graph_snapshot_bytes: default_max_graph_snapshot_bytes(),
            redact_globs: default_redact_globs(),
            commentary_cost_limit: default_commentary_cost_limit(),
            cross_link_cost_limit: default_cross_link_cost_limit(),
            cross_link_confidence_thresholds: CrossLinkConfidenceThresholds::default(),
            export_dir: default_export_dir(),
            retain_retired_revisions: default_retain_retired_revisions(),
            enable_semantic_triage: false,
            semantic_model: default_semantic_model(),
            embedding_dim: default_embedding_dim(),
            semantic_similarity_threshold: default_semantic_similarity_threshold(),
            explain: ExplainConfig::default(),
        }
    }
}

impl Config {
    /// Load config from `repo_root/.synrepo/config.toml`, merging with the
    /// global config at `~/.synrepo/config.toml` if it exists. Project
    /// settings override global settings. If neither exists, returns
    /// `Error::NotInitialized`.
    pub fn load(repo_root: &Path) -> crate::Result<Self> {
        let global_path = Self::global_config_path();
        let local_path = repo_root.join(".synrepo/config.toml");

        if !global_path.exists() && !local_path.exists() {
            return Err(crate::Error::NotInitialized(repo_root.to_path_buf()));
        }

        let mut config = if global_path.exists() {
            let text = std::fs::read_to_string(&global_path)?;
            reject_legacy_explain_block(&text, &global_path)?;
            toml::from_str(&text).map_err(|e| crate::Error::Config(e.to_string()))?
        } else {
            Self::default()
        };

        if local_path.exists() {
            let text = std::fs::read_to_string(&local_path)?;
            reject_legacy_explain_block(&text, &local_path)?;
            let local_config: Config =
                toml::from_str(&text).map_err(|e| crate::Error::Config(e.to_string()))?;
            config.merge(local_config);
        }

        Ok(config)
    }

    /// Path to the global config at `~/.synrepo/config.toml`.
    pub fn global_config_path() -> PathBuf {
        home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".synrepo/config.toml")
    }

    /// Path to the `.synrepo/` directory for a given repo root.
    pub fn synrepo_dir(repo_root: &Path) -> PathBuf {
        repo_root.join(".synrepo")
    }

    /// Merge another config into this one. `other` wins on all fields.
    pub fn merge(&mut self, other: Self) {
        // This is a manual merge for now since we want explicit control over
        // which fields are project-scoped.
        self.mode = other.mode;
        // Only override roots if it's not the default ["."]
        if other.roots != default_roots() {
            self.roots = other.roots;
        }
        if other.concept_directories != default_concept_dirs() {
            self.concept_directories = other.concept_directories;
        }
        if other.git_commit_depth != default_git_commit_depth() {
            self.git_commit_depth = other.git_commit_depth;
        }
        if other.max_file_size_bytes != default_max_file_size() {
            self.max_file_size_bytes = other.max_file_size_bytes;
        }
        if other.max_graph_snapshot_bytes != default_max_graph_snapshot_bytes() {
            self.max_graph_snapshot_bytes = other.max_graph_snapshot_bytes;
        }
        if !other.redact_globs.is_empty() && other.redact_globs != default_redact_globs() {
            self.redact_globs = other.redact_globs;
        }
        if other.commentary_cost_limit != default_commentary_cost_limit() {
            self.commentary_cost_limit = other.commentary_cost_limit;
        }
        if other.cross_link_cost_limit != default_cross_link_cost_limit() {
            self.cross_link_cost_limit = other.cross_link_cost_limit;
        }
        if other.export_dir != default_export_dir() {
            self.export_dir = other.export_dir;
        }
        if other.retain_retired_revisions != default_retain_retired_revisions() {
            self.retain_retired_revisions = other.retain_retired_revisions;
        }
        if other.enable_semantic_triage {
            self.enable_semantic_triage = true;
        }
        if other.semantic_model != default_semantic_model() {
            self.semantic_model = other.semantic_model;
        }
        if other.embedding_dim != default_embedding_dim() {
            self.embedding_dim = other.embedding_dim;
        }
        self.cross_link_confidence_thresholds = other.cross_link_confidence_thresholds;

        // Explain merge is more complex (nested)
        self.explain.merge(other.explain);
    }
}

fn reject_legacy_explain_block(text: &str, path: &Path) -> crate::Result<()> {
    let Ok(value) = text.parse::<toml::Value>() else {
        return Ok(());
    };
    if value.get("synthesis").is_some() {
        return Err(crate::Error::Config(format!(
            "{} uses legacy [synthesis]; rename it to [explain]",
            path.display()
        )));
    }
    Ok(())
}

/// Best-effort home-directory resolver: `$HOME` on Unix, `%USERPROFILE%` on
/// Windows, `None` on bare/unsupported targets. Callers should treat `None` as
/// "no per-user state available" and degrade gracefully.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

#[doc(hidden)]
pub mod test_home {
    //! RAII guard that redirects the user's home directory (as read by
    //! [`super::home_dir`]) to a caller-chosen path for the lifetime of the
    //! guard, restoring the prior value on drop.
    //!
    //! Tests that exercise `Config::load` (which merges
    //! `~/.synrepo/config.toml` into the repo-local config) MUST take both
    //! this guard and the shared cross-process test lock
    //! [`HOME_ENV_TEST_LOCK`] so they don't leak the developer's real
    //! persisted credentials into assertions.
    //!
    //! Exposed as `pub #[doc(hidden)]` (not `pub(crate)` or `#[cfg(test)]`)
    //! so bin-crate tests — which compile the library without `cfg(test)` —
    //! can also take the guard. Same pattern as
    //! `pipeline::writer::hold_writer_flock_with_ownership`.

    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::Mutex;

    /// Shared label for `crate::test_support::global_test_lock` — all tests
    /// that mutate the home-directory env var must serialize on this label.
    pub const HOME_ENV_TEST_LOCK: &str = "config-home-env";

    #[cfg(unix)]
    const HOME_VAR: &str = "HOME";
    #[cfg(windows)]
    const HOME_VAR: &str = "USERPROFILE";

    static HOME_ENV_MUTEX: Mutex<()> = Mutex::new(());

    pub struct HomeEnvGuard {
        original: Option<OsString>,
        _thread_guard: std::sync::MutexGuard<'static, ()>,
    }

    impl HomeEnvGuard {
        pub fn redirect_to(path: &Path) -> Self {
            let thread_guard = HOME_ENV_MUTEX
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let original = std::env::var_os(HOME_VAR);
            std::env::set_var(HOME_VAR, path);
            Self {
                original,
                _thread_guard: thread_guard,
            }
        }
    }

    impl Drop for HomeEnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(HOME_VAR, value),
                None => std::env::remove_var(HOME_VAR),
            }
        }
    }
}

#[cfg(test)]
mod tests;
