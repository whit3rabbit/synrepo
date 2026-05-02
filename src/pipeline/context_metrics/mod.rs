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

use crate::surface::card::{Budget, ContextAccounting};

mod persistence;
mod prometheus;
pub use persistence::{
    load, load_optional, record_card_best_effort, record_cards_best_effort,
    record_changed_files_best_effort, record_mcp_resource_read_best_effort,
    record_mcp_tool_result_best_effort, record_workflow_call_best_effort, save,
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
    /// **Observed**: MCP resource reads that reached a prepared repository.
    #[serde(default)]
    pub mcp_resource_reads_total: u64,
    /// **Observed**: explicit advisory saved-context mutations, keyed by stable
    /// operation (`note_add`, `note_link`, etc.). This is a count only, never
    /// note text or evidence content.
    #[serde(default)]
    pub saved_context_writes_total: BTreeMap<String, u64>,
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

    /// Record a workflow alias call by canonical tool name (for example
    /// `"orient"`, `"find"`, `"minimum_context"`). Stored as an **observed**
    /// counter; callers that serve the call are responsible for invoking
    /// this.
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
        self.mcp_resource_reads_total += delta.mcp_resource_reads_total;
        merge_map(
            &mut self.saved_context_writes_total,
            &delta.saved_context_writes_total,
        );
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
            && self.mcp_resource_reads_total == 0
            && self.saved_context_writes_total.is_empty()
    }
}

fn merge_map(target: &mut BTreeMap<String, u64>, delta: &BTreeMap<String, u64>) {
    for (key, value) in delta {
        *target.entry(key.clone()).or_default() += value;
    }
}

fn budget_label(budget: Budget) -> &'static str {
    match budget {
        Budget::Tiny => "tiny",
        Budget::Normal => "normal",
        Budget::Deep => "deep",
    }
}

#[cfg(test)]
mod tests;
