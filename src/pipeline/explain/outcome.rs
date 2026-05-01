//! Reason-aware commentary generation outcomes.

use std::time::Duration;

use crate::overlay::CommentaryEntry;

/// Result of attempting to generate commentary for one graph node.
#[derive(Clone, Debug)]
pub enum CommentaryGeneration {
    /// The provider produced valid commentary.
    Generated(CommentaryEntry),
    /// Generation did not produce commentary, with an operator-visible reason.
    Skipped(CommentarySkip),
}

impl CommentaryGeneration {
    /// Convert an optional legacy entry into a reason-aware outcome.
    pub fn from_optional(entry: Option<CommentaryEntry>, fallback: CommentarySkipReason) -> Self {
        match entry {
            Some(entry) => Self::Generated(entry),
            None => Self::Skipped(CommentarySkip::new(fallback)),
        }
    }
}

/// Stable reason a commentary target was not generated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentarySkipReason {
    /// Explain is not opted in.
    ProviderDisabled,
    /// The selected cloud provider lacks its API key.
    MissingApiKey,
    /// The input context exceeded the configured per-call budget.
    BudgetBlocked,
    /// The provider returned a rate-limit response.
    RateLimited,
    /// The provider call failed for a non-rate-limit reason.
    ProviderFailed,
    /// The provider responded, but the body was empty or failed template validation.
    InvalidOutput,
    /// The target no longer resolves in the graph.
    GraphNodeMissing,
    /// Legacy or custom generator returned no entry without a richer reason.
    Unknown,
}

impl CommentarySkipReason {
    /// Stable machine-readable label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderDisabled => "provider_disabled",
            Self::MissingApiKey => "missing_api_key",
            Self::BudgetBlocked => "budget_blocked",
            Self::RateLimited => "rate_limited",
            Self::ProviderFailed => "provider_failed",
            Self::InvalidOutput => "invalid_output",
            Self::GraphNodeMissing => "graph_node_missing",
            Self::Unknown => "unknown",
        }
    }
}

/// Details for a skipped commentary target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentarySkip {
    /// Stable reason code.
    pub reason: CommentarySkipReason,
    /// Human-readable detail for progress surfaces.
    pub detail: Option<String>,
    /// Estimated input tokens, when the budget blocked the call.
    pub estimated_tokens: Option<u32>,
    /// Configured input-token budget, when the budget blocked the call.
    pub budget: Option<u32>,
    /// Provider-advised or locally chosen retry delay.
    pub retry_after: Option<Duration>,
}

impl CommentarySkip {
    /// Build a skip with only a stable reason.
    pub fn new(reason: CommentarySkipReason) -> Self {
        Self {
            reason,
            detail: None,
            estimated_tokens: None,
            budget: None,
            retry_after: None,
        }
    }

    /// Attach detail text.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Build a budget-blocked skip.
    pub fn budget_blocked(estimated_tokens: u32, budget: u32) -> Self {
        Self {
            reason: CommentarySkipReason::BudgetBlocked,
            detail: Some(format!("{estimated_tokens} est. tokens > {budget} budget")),
            estimated_tokens: Some(estimated_tokens),
            budget: Some(budget),
            retry_after: None,
        }
    }

    /// Build a rate-limit skip.
    pub fn rate_limited(detail: impl Into<String>, retry_after: Option<Duration>) -> Self {
        Self {
            reason: CommentarySkipReason::RateLimited,
            detail: Some(detail.into()),
            estimated_tokens: None,
            budget: None,
            retry_after,
        }
    }

    /// User-facing reason string.
    pub fn display(&self) -> String {
        match &self.detail {
            Some(detail) if !detail.is_empty() => detail.clone(),
            _ => self.reason.as_str().replace('_', " "),
        }
    }
}
