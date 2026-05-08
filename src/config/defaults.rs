use super::semantic::{
    default_embedding_dim, default_semantic_embedding_batch_size,
    default_semantic_embedding_provider, default_semantic_model, default_semantic_ollama_endpoint,
    default_semantic_similarity_threshold, SemanticProviderSource,
};
use super::{Config, CrossLinkConfidenceThresholds, ExplainConfig, Mode};

pub(super) fn default_roots() -> Vec<String> {
    vec![".".to_string()]
}

pub(super) fn default_concept_dirs() -> Vec<String> {
    vec![
        "docs/concepts".to_string(),
        "docs/adr".to_string(),
        "docs/decisions".to_string(),
    ]
}

pub(super) fn default_include_worktrees() -> bool {
    true
}

pub(super) fn default_include_submodules() -> bool {
    false
}

pub(super) fn default_git_commit_depth() -> u32 {
    500
}

pub(super) fn default_max_file_size() -> u64 {
    1024 * 1024 // 1 MB
}

pub(super) fn default_max_graph_snapshot_bytes() -> usize {
    128 * 1024 * 1024
}

pub(super) fn default_redact_globs() -> Vec<String> {
    vec![
        "**/secrets/**".to_string(),
        "**/*.env*".to_string(),
        "**/*-private.md".to_string(),
    ]
}

pub(super) fn default_commentary_cost_limit() -> u32 {
    5000
}

pub(super) fn default_cross_link_cost_limit() -> u32 {
    200
}

pub(super) fn default_export_dir() -> String {
    "synrepo-context".to_string()
}

pub(super) fn default_retain_retired_revisions() -> u64 {
    10
}

pub(super) fn default_auto_sync_enabled() -> bool {
    true
}

pub(super) fn default_reconcile_keepalive_seconds() -> u32 {
    1800
}

pub(super) fn default_watch_sync_timeout_seconds() -> u32 {
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
            semantic_embedding_provider_source: SemanticProviderSource::Defaulted,
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
