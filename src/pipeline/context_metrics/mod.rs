//! Best-effort operational metrics for context-serving behavior.
//!
//! `ContextMetrics` distinguishes **observed** counters (direct counts of
//! calls or responses synrepo served) from **estimated** counters (values
//! derived from card-accounting comparisons). Callers that persist or render
//! these metrics MUST preserve that separation — see
//! [`ContextMetrics`] field docs and the `prometheus` module for the wire
//! format.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::surface::card::{Budget, ContextAccounting};

mod prometheus;

const METRICS_FILE: &str = "context-metrics.json";

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
}

/// Load context metrics. Missing files return empty metrics.
pub fn load(synrepo_dir: &Path) -> anyhow::Result<ContextMetrics> {
    let path = metrics_path(synrepo_dir);
    if !path.exists() {
        return Ok(ContextMetrics::default());
    }
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Load context metrics only when the metrics file exists.
pub fn load_optional(synrepo_dir: &Path) -> anyhow::Result<Option<ContextMetrics>> {
    let path = metrics_path(synrepo_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

/// Save context metrics.
pub fn save(synrepo_dir: &Path, metrics: &ContextMetrics) -> anyhow::Result<()> {
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir)?;
    let bytes = serde_json::to_vec_pretty(metrics)?;
    crate::util::atomic_write::atomic_write(&metrics_path(synrepo_dir), &bytes)?;
    Ok(())
}

/// Best-effort card metric recording. Failures are debug-only.
pub fn record_card_best_effort(
    synrepo_dir: &Path,
    accounting: &ContextAccounting,
    latency_ms: u64,
    test_surface_hit: bool,
) {
    record_cards_best_effort(
        synrepo_dir,
        std::slice::from_ref(accounting),
        latency_ms,
        test_surface_hit,
    );
}

/// Batched variant that loads and saves the metrics file once for a whole
/// response. The same latency is attributed to every card in the batch.
pub fn record_cards_best_effort(
    synrepo_dir: &Path,
    accountings: &[ContextAccounting],
    latency_ms: u64,
    test_surface_hit: bool,
) {
    if accountings.is_empty() {
        return;
    }
    if let Err(error) = (|| -> anyhow::Result<()> {
        let mut metrics = load(synrepo_dir)?;
        for accounting in accountings {
            metrics.record_card(accounting, latency_ms);
        }
        if test_surface_hit {
            metrics.record_test_surface_hit();
        }
        save(synrepo_dir, &metrics)
    })() {
        tracing::debug!(%error, "context metrics record failed");
    }
}

/// Best-effort changed-file metric recording.
pub fn record_changed_files_best_effort(synrepo_dir: &Path, count: usize) {
    if count == 0 {
        return;
    }
    if let Err(error) = (|| -> anyhow::Result<()> {
        let mut metrics = load(synrepo_dir)?;
        metrics.record_changed_files(count);
        save(synrepo_dir, &metrics)
    })() {
        tracing::debug!(%error, "context changed-file metrics record failed");
    }
}

/// Best-effort recording of a workflow alias call (e.g. `"orient"`,
/// `"find"`, `"minimum_context"`). Canonical tool names are lowercase and
/// use underscore-separated form so they remain stable across client
/// surfaces. Failures are debug-only.
pub fn record_workflow_call_best_effort(synrepo_dir: &Path, tool: &str) {
    if let Err(error) = (|| -> anyhow::Result<()> {
        let mut metrics = load(synrepo_dir)?;
        metrics.record_workflow_call(tool);
        save(synrepo_dir, &metrics)
    })() {
        tracing::debug!(%error, tool, "context workflow-call metrics record failed");
    }
}

/// Best-effort recording of a repository-scoped MCP tool request.
pub fn record_mcp_tool_result_best_effort(
    synrepo_dir: &Path,
    tool: &str,
    errored: bool,
    saved_context_write: Option<&str>,
) {
    if let Err(error) = (|| -> anyhow::Result<()> {
        let mut metrics = load(synrepo_dir)?;
        metrics.record_mcp_tool_result(tool, errored, saved_context_write);
        save(synrepo_dir, &metrics)
    })() {
        tracing::debug!(%error, tool, "context MCP tool metrics record failed");
    }
}

/// Best-effort recording of a repository-scoped MCP resource read.
pub fn record_mcp_resource_read_best_effort(synrepo_dir: &Path) {
    if let Err(error) = (|| -> anyhow::Result<()> {
        let mut metrics = load(synrepo_dir)?;
        metrics.record_mcp_resource_read();
        save(synrepo_dir, &metrics)
    })() {
        tracing::debug!(%error, "context MCP resource metrics record failed");
    }
}

fn metrics_path(synrepo_dir: &Path) -> std::path::PathBuf {
    synrepo_dir.join("state").join(METRICS_FILE)
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
