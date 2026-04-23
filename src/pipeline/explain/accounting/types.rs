use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Per-provider rollup inside the totals snapshot.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProviderTotals {
    /// Number of completed calls for this provider.
    pub calls: u64,
    /// Total input tokens across all calls (may mix reported + estimated).
    pub input_tokens: u64,
    /// Total output tokens across all calls (may mix reported + estimated).
    pub output_tokens: u64,
    /// Computed USD cost. `None` when at least one call used an unknown
    /// `(provider, model)` pair; the user sees this as "cost unknown".
    #[serde(default)]
    pub usd_cost: Option<f64>,
}

/// Aggregates snapshot. Rewritten atomically on each call.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExplainTotals {
    /// RFC-3339 timestamp of the first recorded call (or the most recent
    /// reset, if `synrepo sync --reset-explain-totals` was used).
    #[serde(default)]
    pub since: Option<String>,
    /// RFC-3339 timestamp of the most recent event.
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Total successful calls.
    #[serde(default)]
    pub calls: u64,
    /// Total input tokens across all successful calls.
    #[serde(default)]
    pub input_tokens: u64,
    /// Total output tokens across all successful calls.
    #[serde(default)]
    pub output_tokens: u64,
    /// Total failed calls (HTTP / parse / transport errors).
    #[serde(default)]
    pub failures: u64,
    /// Total budget-blocked calls (refused before the network).
    #[serde(default)]
    pub budget_blocked: u64,
    /// Sum of usd_cost across calls with a known `(provider, model)`.
    #[serde(default)]
    pub usd_cost: f64,
    /// `true` once at least one call's tokens came from an estimate rather
    /// than a provider-reported count.
    #[serde(default)]
    pub any_estimated: bool,
    /// `true` once at least one call had an unknown `(provider, model)`
    /// and could not be priced.
    #[serde(default)]
    pub any_unpriced: bool,
    /// Per-provider rollups.
    #[serde(default)]
    pub per_provider: HashMap<String, ProviderTotals>,
}

/// One JSONL record in `explain-log.jsonl`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExplainCallRecord {
    /// RFC-3339 timestamp the record was emitted.
    pub timestamp: String,
    /// Correlation id.
    pub call_id: u64,
    /// Provider label (stable, lowercase).
    pub provider: String,
    /// Model identifier used for the call.
    pub model: String,
    /// Kind of target: "commentary" | "cross_link".
    pub target_kind: String,
    /// Node id in display form (commentary path: one id; cross-link path:
    /// `"from → to"`).
    pub target_label: String,
    /// Outcome label.
    pub outcome: String,
    /// Wall-clock duration when applicable (zero on budget-blocked).
    #[serde(default)]
    pub duration_ms: u64,
    /// Input / prompt tokens. `0` on budget-blocked.
    #[serde(default)]
    pub input_tokens: u32,
    /// Output / completion tokens. `0` on budget-blocked or failure.
    #[serde(default)]
    pub output_tokens: u32,
    /// Source of the counts ("reported" | "estimated"). Empty on budget
    /// blocks or failures.
    #[serde(default)]
    pub usage_source: String,
    /// USD cost or `null` if the model is not in the rate table.
    #[serde(default)]
    pub usd_cost: Option<f64>,
    /// Short truncated error on failure. Empty otherwise.
    #[serde(default)]
    pub error_tail: String,
}
