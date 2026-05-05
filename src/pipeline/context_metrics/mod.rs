//! Best-effort operational metrics for context-serving behavior.
//!
//! `ContextMetrics` distinguishes **observed** counters (direct counts of
//! calls or responses synrepo served) from **estimated** counters (values
//! derived from card-accounting comparisons). Callers that persist or render
//! these metrics MUST preserve that separation — see
//! [`ContextMetrics`] field docs and the `prometheus` module for the wire
//! format.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod persistence;
mod prometheus;
mod recording;
pub use persistence::{
    load, load_optional, record_anchored_edit_outcomes_best_effort, record_card_best_effort,
    record_cards_best_effort, record_changed_files_best_effort,
    record_commentary_refresh_best_effort, record_compact_output_best_effort,
    record_context_pack_tokens_best_effort, record_cross_link_generation_best_effort,
    record_cross_link_promoted_best_effort, record_hook_route_emission_best_effort,
    record_mcp_resource_read_best_effort, record_mcp_response_budget_best_effort,
    record_mcp_tool_result_best_effort, record_task_route_classification_best_effort,
    record_workflow_call_best_effort, save,
};

/// Aggregated context-serving metrics stored under `.synrepo/state/`.
///
/// Fields split into two categories. Fields marked **observed** are counts
/// of calls or responses synrepo directly handled. Fields marked **estimated**
/// are derived from card-accounting comparisons (raw-file token estimates
/// vs. card token estimates) and are not proof that an external agent did
/// not read files outside synrepo.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContextMetrics {
    /// **Observed**: number of card-shaped responses served.
    pub cards_served_total: u64,
    /// **Estimated**: sum of estimated card tokens.
    pub card_tokens_total: u64,
    /// **Estimated**: sum of estimated raw-file tokens that cards replaced.
    pub raw_file_tokens_total: u64,
    /// **Estimated**: cold-file-read avoidance derived from raw-file vs. card
    /// token comparisons. Not a direct observation of external agent reads.
    pub estimated_tokens_saved_total: u64,
    /// **Observed**: count of responses by budget tier.
    pub budget_tier_usage: BTreeMap<String, u64>,
    /// **Observed**: number of responses that report truncation.
    pub truncation_applied_total: u64,
    /// **Observed**: number of responses that surfaced stale advisory content.
    pub stale_responses_total: u64,
    /// **Observed**: number of test-surface responses with at least one
    /// discovered test.
    pub test_surface_hits_total: u64,
    /// **Observed**: number of changed files observed by `synrepo_changed`.
    pub changed_files_total: u64,
    /// **Observed**: total request latency recorded by card handlers.
    pub context_query_latency_ms_total: u64,
    /// **Observed**: number of request latency samples.
    pub context_query_latency_samples: u64,
    /// **Observed**: workflow tool-call counts keyed by workflow alias name
    /// (`orient`, `find`, `explain`, `impact`, `risks`, `tests`, `changed`,
    /// `minimum_context`). Populated when an agent invokes one of these
    /// aliases through the MCP surface.
    #[serde(default)]
    pub workflow_calls_total: BTreeMap<String, u64>,
    /// **Observed**: total repository-scoped MCP requests that reached a
    /// prepared synrepo runtime. Missing or unregistered `repo_root` failures
    /// are intentionally not recorded because they have no trusted repo bucket.
    #[serde(default)]
    pub mcp_requests_total: u64,
    /// **Observed**: MCP tool calls keyed by MCP tool name. Does not store
    /// prompts, queries, claims, caller identity, or response bodies.
    #[serde(default)]
    pub mcp_tool_calls_total: BTreeMap<String, u64>,
    /// **Observed**: MCP tool responses that returned a top-level `error` field,
    /// keyed by MCP tool name.
    #[serde(default)]
    pub mcp_tool_errors_total: BTreeMap<String, u64>,
    /// **Observed**: MCP tool errors keyed by tool name and stable error code.
    /// Does not store targets, queries, prompts, or response bodies.
    #[serde(default)]
    pub mcp_tool_error_codes_total: BTreeMap<String, BTreeMap<String, u64>>,
    /// **Observed**: MCP resource reads that reached a prepared repository.
    #[serde(default)]
    pub mcp_resource_reads_total: u64,
    /// **Observed**: explicit advisory saved-context mutations, keyed by stable
    /// operation (`note_add`, `note_link`, etc.). This is a count only, never
    /// note text or evidence content.
    #[serde(default)]
    pub saved_context_writes_total: BTreeMap<String, u64>,
    /// **Observed**: number of compact MCP read outputs served.
    #[serde(default)]
    pub compact_outputs_total: u64,
    /// **Estimated**: sum of estimated tokens in compact outputs.
    #[serde(default)]
    pub compact_returned_tokens_total: u64,
    /// **Estimated**: sum of estimated tokens in the uncompact output shape.
    #[serde(default)]
    pub compact_original_tokens_total: u64,
    /// **Estimated**: token savings from compact output comparisons.
    #[serde(default)]
    pub compact_estimated_tokens_saved_total: u64,
    /// **Observed**: total omitted search rows or compactable items.
    #[serde(default)]
    pub compact_omitted_items_total: u64,
    /// **Observed**: compact outputs that omitted content.
    #[serde(default)]
    pub compact_truncation_applied_total: u64,
    /// **Observed**: MCP responses whose estimated size exceeded the soft cap.
    #[serde(default)]
    pub responses_over_soft_cap_total: u64,
    /// **Observed**: MCP responses trimmed by the final response clamp.
    #[serde(default)]
    pub responses_truncated_total: u64,
    /// **Observed**: deep card-shaped responses served.
    #[serde(default)]
    pub deep_cards_served_total: u64,
    /// **Estimated**: sum of estimated tokens in served context packs.
    #[serde(default)]
    pub context_pack_tokens_total: u64,
    /// **Estimated**: largest MCP response token estimate observed.
    #[serde(default)]
    pub largest_response_tokens: u64,
    /// **Estimated**: MCP response token estimates keyed by tool name.
    #[serde(default)]
    pub tool_token_totals: BTreeMap<String, u64>,
    /// **Observed**: number of task-route classifications served by CLI, MCP,
    /// or nudge hooks. Task text is never stored.
    #[serde(default)]
    pub route_classifications_total: u64,
    /// **Observed**: hook emissions containing the context fast-path signal.
    #[serde(default)]
    pub context_fast_path_signals_total: u64,
    /// **Observed**: route classifications that returned a deterministic edit
    /// candidate.
    #[serde(default)]
    pub deterministic_edit_candidates_total: u64,
    /// **Observed**: hook emissions containing the deterministic edit candidate
    /// signal.
    #[serde(default)]
    pub deterministic_edit_candidate_signals_total: u64,
    /// **Observed**: hook emissions containing the LLM-not-required signal.
    #[serde(default)]
    pub llm_not_required_signals_total: u64,
    /// **Observed**: anchored edit operations accepted by the gated edit
    /// surface.
    #[serde(default)]
    pub anchored_edit_accepted_total: u64,
    /// **Observed**: anchored edit operations rejected by the gated edit
    /// surface.
    #[serde(default)]
    pub anchored_edit_rejected_total: u64,
    /// **Observed**: cross-link candidate pairs sent to the configured
    /// generator.
    #[serde(default)]
    pub cross_link_generation_total: u64,
    /// **Observed**: proposed cross-links promoted into the graph.
    #[serde(default)]
    pub cross_link_promoted_total: u64,
    /// **Observed**: commentary refresh attempts.
    #[serde(default)]
    pub commentary_refresh_total: u64,
    /// **Observed**: commentary refresh attempts that returned an error.
    #[serde(default)]
    pub commentary_refresh_errors_total: u64,
    /// **Estimated**: count of route or hook recommendations where synrepo
    /// structural context was sufficient and an LLM call was likely avoidable.
    #[serde(default)]
    pub estimated_llm_calls_avoided_total: u64,
}

impl ContextMetrics {
    /// Average estimated card tokens per served card.
    pub fn card_tokens_avg(&self) -> f64 {
        if self.cards_served_total == 0 {
            0.0
        } else {
            self.card_tokens_total as f64 / self.cards_served_total as f64
        }
    }

    /// Average recorded context query latency.
    pub fn context_query_latency_ms_avg(&self) -> f64 {
        if self.context_query_latency_samples == 0 {
            0.0
        } else {
            self.context_query_latency_ms_total as f64 / self.context_query_latency_samples as f64
        }
    }

    pub(super) fn merge_from(&mut self, delta: &Self) {
        self.cards_served_total += delta.cards_served_total;
        self.card_tokens_total += delta.card_tokens_total;
        self.raw_file_tokens_total += delta.raw_file_tokens_total;
        self.estimated_tokens_saved_total += delta.estimated_tokens_saved_total;
        merge_map(&mut self.budget_tier_usage, &delta.budget_tier_usage);
        self.truncation_applied_total += delta.truncation_applied_total;
        self.stale_responses_total += delta.stale_responses_total;
        self.test_surface_hits_total += delta.test_surface_hits_total;
        self.changed_files_total += delta.changed_files_total;
        self.context_query_latency_ms_total += delta.context_query_latency_ms_total;
        self.context_query_latency_samples += delta.context_query_latency_samples;
        merge_map(&mut self.workflow_calls_total, &delta.workflow_calls_total);
        self.mcp_requests_total += delta.mcp_requests_total;
        merge_map(&mut self.mcp_tool_calls_total, &delta.mcp_tool_calls_total);
        merge_map(
            &mut self.mcp_tool_errors_total,
            &delta.mcp_tool_errors_total,
        );
        merge_nested_map(
            &mut self.mcp_tool_error_codes_total,
            &delta.mcp_tool_error_codes_total,
        );
        self.mcp_resource_reads_total += delta.mcp_resource_reads_total;
        merge_map(
            &mut self.saved_context_writes_total,
            &delta.saved_context_writes_total,
        );
        self.compact_outputs_total += delta.compact_outputs_total;
        self.compact_returned_tokens_total += delta.compact_returned_tokens_total;
        self.compact_original_tokens_total += delta.compact_original_tokens_total;
        self.compact_estimated_tokens_saved_total += delta.compact_estimated_tokens_saved_total;
        self.compact_omitted_items_total += delta.compact_omitted_items_total;
        self.compact_truncation_applied_total += delta.compact_truncation_applied_total;
        self.responses_over_soft_cap_total += delta.responses_over_soft_cap_total;
        self.responses_truncated_total += delta.responses_truncated_total;
        self.deep_cards_served_total += delta.deep_cards_served_total;
        self.context_pack_tokens_total += delta.context_pack_tokens_total;
        self.largest_response_tokens = self
            .largest_response_tokens
            .max(delta.largest_response_tokens);
        merge_map(&mut self.tool_token_totals, &delta.tool_token_totals);
        self.route_classifications_total += delta.route_classifications_total;
        self.context_fast_path_signals_total += delta.context_fast_path_signals_total;
        self.deterministic_edit_candidates_total += delta.deterministic_edit_candidates_total;
        self.deterministic_edit_candidate_signals_total +=
            delta.deterministic_edit_candidate_signals_total;
        self.llm_not_required_signals_total += delta.llm_not_required_signals_total;
        self.anchored_edit_accepted_total += delta.anchored_edit_accepted_total;
        self.anchored_edit_rejected_total += delta.anchored_edit_rejected_total;
        self.cross_link_generation_total += delta.cross_link_generation_total;
        self.cross_link_promoted_total += delta.cross_link_promoted_total;
        self.commentary_refresh_total += delta.commentary_refresh_total;
        self.commentary_refresh_errors_total += delta.commentary_refresh_errors_total;
        self.estimated_llm_calls_avoided_total += delta.estimated_llm_calls_avoided_total;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.cards_served_total == 0
            && self.card_tokens_total == 0
            && self.raw_file_tokens_total == 0
            && self.estimated_tokens_saved_total == 0
            && self.budget_tier_usage.is_empty()
            && self.truncation_applied_total == 0
            && self.stale_responses_total == 0
            && self.test_surface_hits_total == 0
            && self.changed_files_total == 0
            && self.context_query_latency_ms_total == 0
            && self.context_query_latency_samples == 0
            && self.workflow_calls_total.is_empty()
            && self.mcp_requests_total == 0
            && self.mcp_tool_calls_total.is_empty()
            && self.mcp_tool_errors_total.is_empty()
            && self.mcp_tool_error_codes_total.is_empty()
            && self.mcp_resource_reads_total == 0
            && self.saved_context_writes_total.is_empty()
            && self.compact_outputs_total == 0
            && self.compact_returned_tokens_total == 0
            && self.compact_original_tokens_total == 0
            && self.compact_estimated_tokens_saved_total == 0
            && self.compact_omitted_items_total == 0
            && self.compact_truncation_applied_total == 0
            && self.responses_over_soft_cap_total == 0
            && self.responses_truncated_total == 0
            && self.deep_cards_served_total == 0
            && self.context_pack_tokens_total == 0
            && self.largest_response_tokens == 0
            && self.tool_token_totals.is_empty()
            && self.route_classifications_total == 0
            && self.context_fast_path_signals_total == 0
            && self.deterministic_edit_candidates_total == 0
            && self.deterministic_edit_candidate_signals_total == 0
            && self.llm_not_required_signals_total == 0
            && self.anchored_edit_accepted_total == 0
            && self.anchored_edit_rejected_total == 0
            && self.cross_link_generation_total == 0
            && self.cross_link_promoted_total == 0
            && self.commentary_refresh_total == 0
            && self.commentary_refresh_errors_total == 0
            && self.estimated_llm_calls_avoided_total == 0
    }
}

fn merge_map(target: &mut BTreeMap<String, u64>, delta: &BTreeMap<String, u64>) {
    for (key, value) in delta {
        *target.entry(key.clone()).or_default() += value;
    }
}

fn merge_nested_map(
    target: &mut BTreeMap<String, BTreeMap<String, u64>>,
    delta: &BTreeMap<String, BTreeMap<String, u64>>,
) {
    for (outer, inner) in delta {
        merge_map(target.entry(outer.clone()).or_default(), inner);
    }
}

#[cfg(test)]
mod tests;
