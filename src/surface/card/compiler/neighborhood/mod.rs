//! Neighborhood resolution for the `synrepo_minimum_context` MCP tool.
//!
//! Assembles a budget-bounded 1-hop neighborhood around a focal node,
//! combining structural edges with git co-change signals into one response.

use serde::Serialize;

use crate::surface::card::compiler::GraphCardCompiler;
use crate::surface::card::Budget;

mod resolve;
#[cfg(test)]
mod tests;

/// Whether co-change data was available for the focal node's file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoChangeState {
    /// Co-change data was available.
    Available,
    /// No co-change data found.
    Missing,
}

/// A lightweight summary of a structural neighbor (used at `normal` budget).
#[derive(Clone, Debug, Serialize)]
pub struct NeighborSummary {
    /// Node ID of the neighbor.
    pub node_id: String,
    /// Qualified name or file path.
    pub qualified_name: String,
    /// Symbol kind (e.g. "function", "file").
    pub kind: String,
    /// Edge type to this neighbor (e.g. "calls", "imports").
    pub edge_type: String,
}

/// A co-change partner entry sourced from the git-intelligence cache.
#[derive(Clone, Debug, Serialize)]
pub struct CoChangePartner {
    /// Repository-relative file path of the co-change partner.
    pub path: String,
    /// Number of sampled commits changing both paths.
    pub co_change_count: usize,
    /// Data source label (always "git_intelligence").
    pub source: &'static str,
    /// Precision of the co-change signal (always "file").
    pub granularity: &'static str,
}

/// Edge counts returned at every budget tier (even `tiny`).
#[derive(Clone, Debug, Serialize)]
pub struct EdgeCounts {
    /// Number of outbound Calls edges.
    pub outbound_calls_count: usize,
    /// Number of outbound Imports edges.
    pub outbound_imports_count: usize,
    /// Number of incoming Governs edges.
    pub governs_count: usize,
    /// Number of co-change partners in git intelligence.
    pub co_change_count: usize,
}

/// The full response payload for `synrepo_minimum_context`.
#[derive(Clone, Debug, Serialize)]
pub struct MinimumContextResponse {
    /// The focal node's card (SymbolCard or FileCard as JSON).
    pub focal_card: serde_json::Value,
    /// Full neighbor cards (deep budget only).
    pub neighbors: Option<Vec<serde_json::Value>>,
    /// Neighbor summaries (normal budget only).
    pub neighbor_summaries: Option<Vec<NeighborSummary>>,
    /// Governing DecisionCards.
    pub decision_cards: Option<Vec<serde_json::Value>>,
    /// Co-change partners from git intelligence.
    pub co_change_partners: Option<Vec<CoChangePartner>>,
    /// Whether co-change data was available.
    pub co_change_state: CoChangeState,
    /// Edge counts for the focal node.
    pub edge_counts: EdgeCounts,
    /// Budget tier used for this response.
    pub budget: &'static str,
}

impl MinimumContextResponse {
    /// Apply a numeric cap by trimming optional ranked card sets first.
    pub fn apply_numeric_cap(&mut self, budget_tokens: usize) {
        let token_estimate = self
            .focal_card
            .pointer("/context_accounting/token_estimate")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        if token_estimate <= budget_tokens {
            return;
        }
        self.neighbors = None;
        self.neighbor_summaries = None;
        self.decision_cards = None;
        self.co_change_partners = None;
        if let Some(accounting) = self
            .focal_card
            .pointer_mut("/context_accounting/truncation_applied")
        {
            *accounting = serde_json::Value::Bool(true);
        }
    }
}

/// Hard cap on neighbor count across all edge kinds combined.
const NEIGHBOR_CAP: usize = 20;

/// Resolve a 1-hop neighborhood around `target` at the given `budget`.
///
/// Returns an explicit error when `target` does not resolve to a graph node.
/// The entire resolution runs under a single graph read snapshot so the
/// response reflects a consistent epoch.
pub fn resolve_neighborhood(
    compiler: &GraphCardCompiler,
    target: &str,
    budget: Budget,
) -> crate::Result<MinimumContextResponse> {
    compiler
        .with_reader(|graph| resolve::resolve_neighborhood_inner(compiler, graph, target, budget))
}
