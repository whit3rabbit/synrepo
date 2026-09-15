use super::defaults::*;
use super::semantic::{
    default_embedding_dim, default_semantic_embedding_batch_size,
    default_semantic_embedding_provider, default_semantic_model, default_semantic_ollama_endpoint,
    default_semantic_similarity_threshold, default_semantic_vector_precision,
};
use super::{BranchRootsConfig, Config, ConfigPresence, SemanticProviderSource};

impl Config {
    /// Merge another config into this one. `other` wins on all fields.
    pub fn merge(&mut self, other: Self) {
        // This is a manual merge for now since we want explicit control over
        // which fields are project-scoped.
        self.mode = other.mode;
        self.merge_common_overrides(&other);
        if other.semantic_embedding_provider != default_semantic_embedding_provider() {
            self.semantic_embedding_provider = other.semantic_embedding_provider;
            self.semantic_embedding_provider_source = SemanticProviderSource::Explicit;
        }
        if other.semantic_model != default_semantic_model() {
            self.semantic_model.clone_from(&other.semantic_model);
        }
        if other.embedding_dim != default_embedding_dim() {
            self.embedding_dim = other.embedding_dim;
        }
        if other.semantic_ollama_endpoint != default_semantic_ollama_endpoint() {
            self.semantic_ollama_endpoint
                .clone_from(&other.semantic_ollama_endpoint);
        }
        if other.semantic_embedding_batch_size != default_semantic_embedding_batch_size() {
            self.semantic_embedding_batch_size = other.semantic_embedding_batch_size;
        }
        if other.semantic_vector_precision != default_semantic_vector_precision() {
            self.semantic_vector_precision = other.semantic_vector_precision;
        }
        let keepalive_present =
            other.reconcile_keepalive_seconds != default_reconcile_keepalive_seconds();
        self.merge_nested_and_runtime(other, keepalive_present);
    }

    pub(super) fn apply_config_presence(&mut self, presence: ConfigPresence) {
        self.semantic_embedding_provider_source = if presence.semantic_provider {
            SemanticProviderSource::Explicit
        } else {
            SemanticProviderSource::Defaulted
        };
    }

    pub(super) fn merge_with_config_presence(&mut self, other: Self, presence: ConfigPresence) {
        self.mode = other.mode;
        self.merge_common_overrides(&other);
        if presence.semantic_provider {
            self.semantic_embedding_provider = other.semantic_embedding_provider;
            self.semantic_embedding_provider_source = SemanticProviderSource::Explicit;
        }
        if presence.semantic_model {
            self.semantic_model.clone_from(&other.semantic_model);
        }
        if presence.embedding_dim {
            self.embedding_dim = other.embedding_dim;
        }
        if other.semantic_similarity_threshold != default_semantic_similarity_threshold() {
            self.semantic_similarity_threshold = other.semantic_similarity_threshold;
        }
        if presence.semantic_ollama_endpoint {
            self.semantic_ollama_endpoint
                .clone_from(&other.semantic_ollama_endpoint);
        }
        if presence.semantic_embedding_batch_size {
            self.semantic_embedding_batch_size = other.semantic_embedding_batch_size;
        }
        if presence.semantic_vector_precision {
            self.semantic_vector_precision = other.semantic_vector_precision;
        }
        self.merge_nested_and_runtime(other, presence.reconcile_keepalive_seconds);
    }

    fn merge_common_overrides(&mut self, other: &Self) {
        // Only override roots if it's not the default ["."]
        if other.roots != default_roots() {
            self.roots.clone_from(&other.roots);
        }
        if other.include_worktrees != default_include_worktrees() {
            self.include_worktrees = other.include_worktrees;
        }
        if other.include_submodules != default_include_submodules() {
            self.include_submodules = other.include_submodules;
        }
        if other.branch_roots != BranchRootsConfig::default() {
            self.branch_roots = other.branch_roots.clone();
        }
        if other.concept_directories != default_concept_dirs() {
            self.concept_directories
                .clone_from(&other.concept_directories);
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
            self.redact_globs.clone_from(&other.redact_globs);
        }
        if other.commentary_cost_limit != default_commentary_cost_limit() {
            self.commentary_cost_limit = other.commentary_cost_limit;
        }
        if other.cross_link_cost_limit != default_cross_link_cost_limit() {
            self.cross_link_cost_limit = other.cross_link_cost_limit;
        }
        if other.export_dir != default_export_dir() {
            self.export_dir.clone_from(&other.export_dir);
        }
        if other.retain_retired_revisions != default_retain_retired_revisions() {
            self.retain_retired_revisions = other.retain_retired_revisions;
        }
        if other.enable_semantic_triage {
            self.enable_semantic_triage = true;
        }
        if other.mcp_sentry_telemetry.is_some() {
            self.mcp_sentry_telemetry = other.mcp_sentry_telemetry;
        }
    }

    fn merge_nested_and_runtime(&mut self, other: Self, keepalive_present: bool) {
        self.cross_link_confidence_thresholds = other.cross_link_confidence_thresholds;
        self.explain.merge(other.explain);
        if keepalive_present {
            self.reconcile_keepalive_seconds = other.reconcile_keepalive_seconds;
        }
    }
}
