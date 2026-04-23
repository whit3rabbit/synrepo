//! Event types for explain telemetry.
//!
//! Providers emit these at call boundaries; the telemetry fan-out publishes
//! them to subscribers and the accounting writer.

use crate::core::ids::NodeId;
use crate::overlay::OverlayEdgeKind;

/// Source of a reported token count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageSource {
    /// Tokens came directly from the provider's response body (exact).
    Reported,
    /// Tokens were estimated (the response did not carry usage, or an
    /// upstream server omitted it). Surfaces must label these as approximate.
    Estimated,
}

impl UsageSource {
    /// Stable machine-readable label (used by JSONL + JSON snapshots).
    pub fn as_str(&self) -> &'static str {
        match self {
            UsageSource::Reported => "reported",
            UsageSource::Estimated => "estimated",
        }
    }
}

/// Input + output token counts for a single API call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenUsage {
    /// Input / prompt tokens.
    pub input_tokens: u32,
    /// Output / completion tokens.
    pub output_tokens: u32,
    /// Whether these counts came from the provider (Reported) or from a
    /// chars-per-token heuristic (Estimated).
    pub source: UsageSource,
}

impl TokenUsage {
    /// Construct a `Reported` usage record.
    pub fn reported(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            source: UsageSource::Reported,
        }
    }

    /// Construct an `Estimated` usage record. Use only when the provider
    /// response did not include token counts.
    pub fn estimated(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            source: UsageSource::Estimated,
        }
    }

    /// Total tokens (input + output, saturating).
    pub fn total(&self) -> u32 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// What kind of work the call was doing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplainTarget {
    /// Per-node commentary generation.
    Commentary {
        /// Target node.
        node: NodeId,
    },
    /// Cross-link candidate extraction over a single prefiltered pair.
    CrossLink {
        /// Source endpoint (typically a concept or file).
        from: NodeId,
        /// Target endpoint (typically a symbol or file).
        to: NodeId,
        /// Proposed overlay edge kind.
        kind: OverlayEdgeKind,
    },
}

impl ExplainTarget {
    /// Short display label used in live-feed log lines ("file_…", "sym_… ↔ sym_…").
    pub fn display_label(&self) -> String {
        match self {
            ExplainTarget::Commentary { node } => format!("{node}"),
            ExplainTarget::CrossLink { from, to, .. } => format!("{from} → {to}"),
        }
    }
}

/// Kinds of outcomes a explain call can have. Used as a stable JSON field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Call completed and produced bytes of output text.
    Success,
    /// Call was refused by the provider's chars-per-token budget check.
    BudgetBlocked,
    /// Call failed (HTTP error, non-success status, parse failure).
    Failed,
}

impl Outcome {
    /// Stable machine-readable label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Success => "success",
            Outcome::BudgetBlocked => "budget_blocked",
            Outcome::Failed => "failed",
        }
    }
}

/// Lifecycle event for one explain API call.
#[derive(Clone, Debug)]
pub enum ExplainEvent {
    /// A call is about to hit the network.
    CallStarted {
        /// Unique, monotonically increasing call id.
        call_id: u64,
        /// Provider name (stable, lowercase).
        provider: &'static str,
        /// Model identifier used for this call.
        model: String,
        /// What the call is working on.
        target: ExplainTarget,
        /// Unix millis when the call started.
        started_at_ms: u128,
    },
    /// A call completed with a response body.
    CallCompleted {
        /// Correlation id matching the prior `CallStarted`.
        call_id: u64,
        /// Provider name (stable, lowercase).
        provider: &'static str,
        /// Model identifier used for this call.
        model: String,
        /// What the call was working on.
        target: ExplainTarget,
        /// Wall-clock duration of the HTTP round-trip.
        duration_ms: u64,
        /// Token counts (reported by provider or explicitly estimated).
        usage: TokenUsage,
        /// Exact billed USD cost when the provider returned it directly.
        billed_usd_cost: Option<f64>,
        /// Size of the accepted response text in bytes.
        output_bytes: u32,
    },
    /// A call was refused by the pre-flight budget check and never hit the
    /// network.
    BudgetBlocked {
        /// Unique call id (matches no prior `CallStarted` because nothing was
        /// sent).
        call_id: u64,
        /// Provider name (stable, lowercase).
        provider: &'static str,
        /// Model identifier that would have been used.
        model: String,
        /// What the call was working on.
        target: ExplainTarget,
        /// Estimated prompt tokens that tripped the budget.
        estimated_tokens: u32,
        /// Configured budget that rejected them.
        budget: u32,
    },
    /// A call failed after being sent.
    CallFailed {
        /// Correlation id matching the prior `CallStarted`.
        call_id: u64,
        /// Provider name (stable, lowercase).
        provider: &'static str,
        /// Model identifier used for this call.
        model: String,
        /// What the call was working on.
        target: ExplainTarget,
        /// Wall-clock duration until the error surfaced.
        duration_ms: u64,
        /// Short error tail; no PII, no raw response body.
        error: String,
    },
}

impl ExplainEvent {
    /// Correlation id.
    pub fn call_id(&self) -> u64 {
        match self {
            ExplainEvent::CallStarted { call_id, .. }
            | ExplainEvent::CallCompleted { call_id, .. }
            | ExplainEvent::BudgetBlocked { call_id, .. }
            | ExplainEvent::CallFailed { call_id, .. } => *call_id,
        }
    }

    /// Provider label.
    pub fn provider(&self) -> &'static str {
        match self {
            ExplainEvent::CallStarted { provider, .. }
            | ExplainEvent::CallCompleted { provider, .. }
            | ExplainEvent::BudgetBlocked { provider, .. }
            | ExplainEvent::CallFailed { provider, .. } => provider,
        }
    }

    /// Target the call was working on.
    pub fn target(&self) -> &ExplainTarget {
        match self {
            ExplainEvent::CallStarted { target, .. }
            | ExplainEvent::CallCompleted { target, .. }
            | ExplainEvent::BudgetBlocked { target, .. }
            | ExplainEvent::CallFailed { target, .. } => target,
        }
    }
}
