//! Configuration loading. Reads `.synrepo/config.toml`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod synthesis;

pub use synthesis::SynthesisConfig;

/// Which operational mode synrepo runs in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Bootstrap defaults here when repository inspection does not find
    /// rationale markdown under the configured concept directories.
    /// Synthesis runs automatically in the background and writes to the
    /// overlay. Concept nodes are disabled unless human-authored concept
    /// directories exist.
    #[default]
    Auto,
    /// Bootstrap recommends or selects this when repository inspection
    /// finds rationale markdown under the configured concept directories,
    /// unless an explicit or already-configured mode is kept instead.
    /// Synthesis proposals go to a review queue. Concept nodes are
    /// enabled when human-authored ADR directories exist.
    Curated,
}

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

    /// Maximum number of LLM cross-link generation calls the synthesis pass
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

    /// LLM synthesis configuration. Off by default; opting in is required
    /// even when provider API keys are present in the env. See
    /// [`SynthesisConfig`].
    #[serde(default)]
    pub synthesis: SynthesisConfig,
}

/// TOML-friendly mirror of `overlay::ConfidenceThresholds`. Lives in this
/// module so config loading does not pull the overlay types into the config
/// layer; `From` conversions in both directions keep the two in sync.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CrossLinkConfidenceThresholds {
    /// Scores at or above this value classify as `High`.
    #[serde(default = "default_high_threshold")]
    pub high: f32,
    /// Scores at or above this value (and below `high`) classify as
    /// `ReviewQueue`; anything lower is `BelowThreshold`.
    #[serde(default = "default_review_queue_threshold")]
    pub review_queue: f32,
}

impl Default for CrossLinkConfidenceThresholds {
    fn default() -> Self {
        Self {
            high: default_high_threshold(),
            review_queue: default_review_queue_threshold(),
        }
    }
}

impl From<CrossLinkConfidenceThresholds> for crate::overlay::ConfidenceThresholds {
    fn from(c: CrossLinkConfidenceThresholds) -> Self {
        crate::overlay::ConfidenceThresholds {
            high: c.high,
            review_queue: c.review_queue,
        }
    }
}

impl From<crate::overlay::ConfidenceThresholds> for CrossLinkConfidenceThresholds {
    fn from(c: crate::overlay::ConfidenceThresholds) -> Self {
        CrossLinkConfidenceThresholds {
            high: c.high,
            review_queue: c.review_queue,
        }
    }
}

fn default_high_threshold() -> f32 {
    0.85
}

fn default_review_queue_threshold() -> f32 {
    0.6
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

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Auto => f.write_str("auto"),
            Mode::Curated => f.write_str("curated"),
        }
    }
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
            synthesis: SynthesisConfig::default(),
        }
    }
}

impl Config {
    /// Load config from `repo_root/.synrepo/config.toml`. If the file
    /// doesn't exist, return defaults.
    pub fn load(repo_root: &Path) -> crate::Result<Self> {
        let path = repo_root.join(".synrepo/config.toml");
        if !path.exists() {
            return Err(crate::Error::NotInitialized(repo_root.to_path_buf()));
        }
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).map_err(|e| crate::Error::Config(e.to_string()))
    }

    /// Path to the `.synrepo/` directory for a given repo root.
    pub fn synrepo_dir(repo_root: &Path) -> PathBuf {
        repo_root.join(".synrepo")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_missing_file_returns_error() {
        let dir = tempdir().unwrap();
        let err = Config::load(dir.path()).unwrap_err();
        assert!(matches!(err, crate::Error::NotInitialized(_)));
    }

    #[test]
    fn load_valid_file_overrides_defaults() {
        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();

        let custom_toml = r#"
        mode = "curated"
        roots = ["src"]
        git_commit_depth = 100
        "#;
        fs::write(synrepo_dir.join("config.toml"), custom_toml).unwrap();

        let config = Config::load(dir.path()).unwrap();

        assert_eq!(config.mode, Mode::Curated);
        assert_eq!(config.roots, vec!["src".to_string()]);
        assert_eq!(config.git_commit_depth, 100);

        // Ensure defaults are kept for unmentioned fields
        assert_eq!(config.max_file_size_bytes, 1024 * 1024);
    }

    #[test]
    fn cross_link_fields_round_trip_through_toml() {
        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();

        let custom_toml = r#"
            cross_link_cost_limit = 42
            [cross_link_confidence_thresholds]
            high = 0.9
            review_queue = 0.55
        "#;
        fs::write(synrepo_dir.join("config.toml"), custom_toml).unwrap();

        let config = Config::load(dir.path()).unwrap();
        assert_eq!(config.cross_link_cost_limit, 42);
        assert!((config.cross_link_confidence_thresholds.high - 0.9).abs() < 1e-6);
        assert!((config.cross_link_confidence_thresholds.review_queue - 0.55).abs() < 1e-6);

        // Defaults kick in when the TOML omits the cross-link keys.
        let default = Config::default();
        assert_eq!(default.cross_link_cost_limit, 200);
        assert!((default.cross_link_confidence_thresholds.high - 0.85).abs() < 1e-6);
    }

    #[test]
    fn load_invalid_toml_returns_error() {
        let dir = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(dir.path());
        fs::create_dir_all(&synrepo_dir).unwrap();

        fs::write(synrepo_dir.join("config.toml"), "mode = [").unwrap();

        let err = Config::load(dir.path()).unwrap_err();
        assert!(err.to_string().starts_with("config error:"));
    }
}
