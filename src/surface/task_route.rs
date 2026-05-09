//! Deterministic task routing recommendations for agent fast paths.

use serde::{Deserialize, Serialize};

mod classifier;
mod routes;
#[path = "task_route/semantic.rs"]
mod semantic;
mod typescript;
pub use classifier::{classify_task_route, classify_task_route_with_config};
pub use typescript::typescript_var_to_const_eligibility;

/// Stable hook signal for context-first routing.
pub const SIGNAL_CONTEXT_FAST_PATH: &str = "[SYNREPO_CONTEXT_FAST_PATH]";
/// Stable hook signal for deterministic edit candidates.
pub const SIGNAL_DETERMINISTIC_EDIT_CANDIDATE: &str = "[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE]";
/// Stable hook signal for work that does not need LLM output.
pub const SIGNAL_LLM_NOT_REQUIRED: &str = "[SYNREPO_LLM_NOT_REQUIRED]";

/// Result returned by the task-route classifier.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskRoute {
    /// Stable intent name, for example `context-search` or `var-to-const`.
    pub intent: String,
    /// Deterministic confidence score in the range 0.0..=1.0.
    pub confidence: f32,
    /// Recommended synrepo tools in the order an agent should try them.
    pub recommended_tools: Vec<String>,
    /// Recommended card budget tier for the first context read.
    pub budget_tier: String,
    /// True when the task needs semantic generation beyond structural context.
    pub llm_required: bool,
    /// Optional deterministic edit candidate. Advisory only.
    pub edit_candidate: Option<EditCandidate>,
    /// Stable signals suitable for hook output.
    pub signals: Vec<String>,
    /// Short human-readable explanation.
    pub reason: String,
    /// Classifier strategy used to produce the route.
    #[serde(default = "routes::default_routing_strategy")]
    pub routing_strategy: String,
    /// Semantic similarity score when semantic routing participated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_score: Option<f32>,
}

/// Advisory deterministic edit candidate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EditCandidate {
    /// Candidate intent.
    pub intent: String,
    /// Whether eligibility was proven from supplied source text.
    pub eligible: bool,
    /// Why the candidate is or is not eligible.
    pub reason: String,
}

/// TypeScript `var`/`let` to `const` eligibility result.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VarToConstEligibility {
    /// True when a single var/let binding was found and no reassignment was observed.
    pub eligible: bool,
    /// Binding name, when a single simple binding was found.
    pub binding: Option<String>,
    /// Explanation of the decision.
    pub reason: String,
}

#[cfg(test)]
mod tests;
