//! In-memory counter update helpers.

use crate::surface::card::{Budget, ContextAccounting};
use crate::surface::task_route::{
    TaskRoute, SIGNAL_CONTEXT_FAST_PATH, SIGNAL_DETERMINISTIC_EDIT_CANDIDATE,
    SIGNAL_LLM_NOT_REQUIRED,
};

use super::ContextMetrics;

impl ContextMetrics {
    /// Record a card-shaped response.
    pub fn record_card(&mut self, accounting: &ContextAccounting, latency_ms: u64) {
        self.cards_served_total += 1;
        self.card_tokens_total += accounting.token_estimate as u64;
        self.raw_file_tokens_total += accounting.raw_file_token_estimate as u64;
        self.estimated_tokens_saved_total += accounting
            .raw_file_token_estimate
            .saturating_sub(accounting.token_estimate)
            as u64;
        *self
            .budget_tier_usage
            .entry(budget_label(accounting.budget_tier).to_string())
            .or_default() += 1;
        if accounting.truncation_applied {
            self.truncation_applied_total += 1;
        }
        if accounting.stale {
            self.stale_responses_total += 1;
        }
        self.context_query_latency_ms_total += latency_ms;
        self.context_query_latency_samples += 1;
    }

    /// Record a test-surface hit.
    pub fn record_test_surface_hit(&mut self) {
        self.test_surface_hits_total += 1;
    }

    /// Record changed files observed by the changed-context surface.
    pub fn record_changed_files(&mut self, count: usize) {
        self.changed_files_total += count as u64;
    }

    /// Record a workflow alias call by canonical tool name.
    pub fn record_workflow_call(&mut self, tool: &str) {
        *self
            .workflow_calls_total
            .entry(tool.to_string())
            .or_default() += 1;
    }

    /// Record an MCP tool request and its coarse outcome.
    pub fn record_mcp_tool_result(
        &mut self,
        tool: &str,
        errored: bool,
        saved_context_write: Option<&str>,
    ) {
        self.mcp_requests_total += 1;
        *self
            .mcp_tool_calls_total
            .entry(tool.to_string())
            .or_default() += 1;
        if errored {
            *self
                .mcp_tool_errors_total
                .entry(tool.to_string())
                .or_default() += 1;
        }
        if let Some(operation) = saved_context_write {
            self.record_saved_context_write(operation);
        }
    }

    /// Record an MCP resource read for a prepared repository.
    pub fn record_mcp_resource_read(&mut self) {
        self.mcp_requests_total += 1;
        self.mcp_resource_reads_total += 1;
    }

    /// Record an explicit saved-context mutation without storing content.
    pub fn record_saved_context_write(&mut self, operation: &str) {
        *self
            .saved_context_writes_total
            .entry(operation.to_string())
            .or_default() += 1;
    }

    /// Record a compact MCP read output without storing query or result text.
    pub fn record_compact_output(
        &mut self,
        returned_token_estimate: usize,
        original_token_estimate: usize,
        estimated_tokens_saved: usize,
        omitted_count: usize,
        truncation_applied: bool,
    ) {
        self.compact_outputs_total += 1;
        self.compact_returned_tokens_total += returned_token_estimate as u64;
        self.compact_original_tokens_total += original_token_estimate as u64;
        self.compact_estimated_tokens_saved_total += estimated_tokens_saved as u64;
        self.compact_omitted_items_total += omitted_count as u64;
        if truncation_applied {
            self.compact_truncation_applied_total += 1;
        }
    }

    /// Record a task-route classification without storing task text.
    pub fn record_task_route_classification(&mut self, route: &TaskRoute) {
        self.route_classifications_total += 1;
        if route.edit_candidate.is_some() {
            self.deterministic_edit_candidates_total += 1;
        }
        if !route.llm_required {
            self.estimated_llm_calls_avoided_total += 1;
        }
    }

    /// Record structured hook signals emitted from a task route.
    pub fn record_hook_route_emission(&mut self, route: &TaskRoute) {
        self.route_classifications_total += 1;
        if route
            .signals
            .iter()
            .any(|signal| signal == SIGNAL_CONTEXT_FAST_PATH)
        {
            self.context_fast_path_signals_total += 1;
        }
        if route
            .signals
            .iter()
            .any(|signal| signal == SIGNAL_DETERMINISTIC_EDIT_CANDIDATE)
        {
            self.deterministic_edit_candidate_signals_total += 1;
        }
        if route
            .signals
            .iter()
            .any(|signal| signal == SIGNAL_LLM_NOT_REQUIRED)
        {
            self.llm_not_required_signals_total += 1;
            self.estimated_llm_calls_avoided_total += 1;
        }
        if route.edit_candidate.is_some() {
            self.deterministic_edit_candidates_total += 1;
        }
    }

    /// Record gated anchored edit outcomes.
    pub fn record_anchored_edit_outcomes(&mut self, accepted: u64, rejected: u64) {
        self.anchored_edit_accepted_total += accepted;
        self.anchored_edit_rejected_total += rejected;
    }

    /// Record cross-link candidate generation attempts.
    pub fn record_cross_link_generation(&mut self, attempts: u64) {
        self.cross_link_generation_total += attempts;
    }

    /// Record a human or recovery promotion of proposed cross-links into the graph.
    pub fn record_cross_link_promoted(&mut self, count: u64) {
        self.cross_link_promoted_total += count;
    }

    /// Record one commentary refresh attempt.
    pub fn record_commentary_refresh(&mut self, errored: bool) {
        self.commentary_refresh_total += 1;
        if errored {
            self.commentary_refresh_errors_total += 1;
        }
    }
}

fn budget_label(budget: Budget) -> &'static str {
    match budget {
        Budget::Tiny => "tiny",
        Budget::Normal => "normal",
        Budget::Deep => "deep",
    }
}
