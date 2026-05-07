//! Configuration loading. Reads `.synrepo/config.toml`.

mod explain;
mod io;
mod mode;
mod semantic;
mod thresholds;

pub use explain::ExplainConfig;
pub use io::home_dir;
pub use mode::Mode;
pub use semantic::SemanticEmbeddingProvider;
pub use thresholds::CrossLinkConfidenceThresholds;

use io::reject_legacy_explain_block;
use semantic::{
    default_embedding_dim, default_semantic_embedding_batch_size,
    default_semantic_embedding_provider, default_semantic_model, default_semantic_ollama_endpoint,
    default_semantic_similarity_threshold,
};
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

    /// Include linked git worktrees as additional discovery roots.
    #[serde(default = "default_include_worktrees")]
    pub include_worktrees: bool,

    /// Include initialized git submodules as additional discovery roots.
    #[serde(default = "default_include_submodules")]
    pub include_submodules: bool,

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
    /// generation. When true, `synrepo embeddings build` can build an
    /// index used to prefilter candidate pairs based on cosine similarity.
    #[serde(default)]
    pub enable_semantic_triage: bool,

    /// Embedding backend. `onnx` preserves the original local runtime;
    /// `ollama` calls a local Ollama `/api/embed` endpoint.
    #[serde(default = "default_semantic_embedding_provider")]
    pub semantic_embedding_provider: SemanticEmbeddingProvider,

    /// The embedding model to use for semantic triage. ONNX accepts built-in
    /// model names (all-MiniLM-L6-v2, all-MiniLM-L12-v2, all-mpnet-base-v2);
    /// Ollama accepts a local Ollama model name.
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

    /// Base URL for the local Ollama server when `semantic_embedding_provider`
    /// is `ollama`.
    #[serde(default = "default_semantic_ollama_endpoint")]
    pub semantic_ollama_endpoint: String,

    /// Number of texts sent per embedding request.
    #[serde(default = "default_semantic_embedding_batch_size")]
    pub semantic_embedding_batch_size: usize,

    /// LLM explain configuration. Off by default; opting in is required
    /// even when provider API keys are present in the env. See
    /// [`ExplainConfig`].
    #[serde(default)]
    pub explain: ExplainConfig,

    /// Run cheap auto-sync surfaces (export regeneration, retired-observation
    /// compaction) automatically after every reconcile pass the watch service
    /// completes. Commentary refresh and other token-cost surfaces are NOT
    /// auto-run regardless of this flag. Only honored while `synrepo watch`
    /// is active; standalone CLI sync is not affected. Default is `true`.
    /// The TUI `A` keybinding flips this flag in-memory without persisting;
    /// to change the persistent default, edit this field and restart watch.
    #[serde(default = "default_auto_sync_enabled")]
    pub auto_sync_enabled: bool,

    /// Interval in seconds for the `watch` service to perform a periodic
    /// background reconcile when no filesystem events have been observed.
    /// Default is `1800` (30 minutes); set to `0` to disable. The keepalive
    /// runs in fast mode (skips git-history passes) so its main effect is
    /// refreshing the reconcile timestamp; if `auto_sync_enabled` is also
    /// set, the post-reconcile hook then runs the cheap auto-sync surfaces.
    #[serde(default = "default_reconcile_keepalive_seconds")]
    pub reconcile_keepalive_seconds: u32,

    /// Maximum time in seconds the watch control bridge waits for a sync
    /// request to return a response from the watch loop before reporting
    /// failure. Commentary refresh on large repos can legitimately exceed
    /// the default; raise this if you see "watch loop did not answer the
    /// sync request in time" without a real wedge.
    #[serde(default = "default_watch_sync_timeout_seconds")]
    pub watch_sync_timeout_seconds: u32,
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

fn default_include_worktrees() -> bool {
    true
}

fn default_include_submodules() -> bool {
    false
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

fn default_auto_sync_enabled() -> bool {
    true
}

fn default_reconcile_keepalive_seconds() -> u32 {
    1800
}

fn default_watch_sync_timeout_seconds() -> u32 {
    600
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            roots: default_roots(),
            include_worktrees: default_include_worktrees(),
            include_submodules: default_include_submodules(),
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
            semantic_embedding_provider: default_semantic_embedding_provider(),
            semantic_model: default_semantic_model(),
            embedding_dim: default_embedding_dim(),
            semantic_similarity_threshold: default_semantic_similarity_threshold(),
            semantic_ollama_endpoint: default_semantic_ollama_endpoint(),
            semantic_embedding_batch_size: default_semantic_embedding_batch_size(),
            explain: ExplainConfig::default(),
            auto_sync_enabled: default_auto_sync_enabled(),
            reconcile_keepalive_seconds: default_reconcile_keepalive_seconds(),
            watch_sync_timeout_seconds: default_watch_sync_timeout_seconds(),
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
        if other.include_worktrees != default_include_worktrees() {
            self.include_worktrees = other.include_worktrees;
        }
        if other.include_submodules != default_include_submodules() {
            self.include_submodules = other.include_submodules;
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
        if other.semantic_embedding_provider != default_semantic_embedding_provider() {
            self.semantic_embedding_provider = other.semantic_embedding_provider;
        }
        if other.semantic_model != default_semantic_model() {
            self.semantic_model = other.semantic_model;
        }
        if other.embedding_dim != default_embedding_dim() {
            self.embedding_dim = other.embedding_dim;
        }
        if other.semantic_ollama_endpoint != default_semantic_ollama_endpoint() {
            self.semantic_ollama_endpoint = other.semantic_ollama_endpoint;
        }
        if other.semantic_embedding_batch_size != default_semantic_embedding_batch_size() {
            self.semantic_embedding_batch_size = other.semantic_embedding_batch_size;
        }
        self.cross_link_confidence_thresholds = other.cross_link_confidence_thresholds;

        // Explain merge is more complex (nested)
        self.explain.merge(other.explain);

        if other.reconcile_keepalive_seconds != default_reconcile_keepalive_seconds() {
            self.reconcile_keepalive_seconds = other.reconcile_keepalive_seconds;
        }
    }
}

#[doc(hidden)]
pub mod test_home;

#[cfg(test)]
mod tests;
