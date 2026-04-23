use serde::{Deserialize, Serialize};

use crate::core::ids::NodeId;

use super::{ContextAccounting, SourceStore};

/// Risk level classification for change risk assessment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Low risk: score < 0.4
    Low,
    /// Medium risk: score >= 0.4
    Medium,
    /// High risk: score >= 0.6
    High,
    /// Critical risk: score >= 0.8
    Critical,
}

impl RiskLevel {
    /// Derive risk level from a composite score (0-1).
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            Self::Critical
        } else if score >= 0.6 {
            Self::High
        } else if score >= 0.4 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

/// A single contributing factor to the risk score.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Signal type identifier.
    pub signal: String,
    /// Raw value before normalization.
    pub raw_value: f64,
    /// Normalized value (0-1 scale).
    pub normalized_value: f64,
    /// Human-readable description of this factor.
    pub description: String,
}

/// ChangeRiskCard — answers "what is the risk of changing this symbol or file?"
///
/// Aggregates drift score, co-change relationships, and git hotspot data
/// into a risk assessment computed on-demand from graph signals.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeRiskCard {
    /// Target this card assesses (symbol or file).
    pub target: NodeId,
    /// Display name of the target.
    pub target_name: String,
    /// Target kind ("symbol" or "file").
    pub target_kind: String,
    /// Overall risk level.
    pub risk_level: RiskLevel,
    /// Composite risk score (0-1 weighted sum).
    pub risk_score: f64,
    /// Drift score from structural fingerprint changes (0-1).
    /// Only populated at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_score: Option<f64>,
    /// Count of co-change partners, normalized to 0-1.
    /// Only populated at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_change_partner_count: Option<f64>,
    /// Recent touch frequency score from git intelligence (0-1).
    /// Only populated at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotspot_score: Option<f64>,
    /// Contributing risk factors. Populated at `Normal` and `Deep` budgets.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub risk_factors: Vec<RiskFactor>,
    /// Count of outgoing edges with drift scores at the current revision.
    /// Only populated at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_edge_count: Option<usize>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Context-accounting metadata for this card.
    pub context_accounting: ContextAccounting,
    /// Source store (always `Graph` — ChangeRiskCard uses graph signals only).
    pub source_store: SourceStore,
}
