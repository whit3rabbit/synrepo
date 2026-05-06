use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// High-level request accepted by the task-context front door.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq)]
pub struct ContextAskRequest {
    /// Optional repository root for global/defaultless MCP sessions.
    pub repo_root: Option<PathBuf>,
    /// Plain-language task or question to compile into a context packet.
    pub ask: String,
    /// Optional file, symbol, or change-set scope.
    #[serde(default)]
    pub scope: ContextScope,
    /// Requested sections or output hints.
    #[serde(default)]
    pub shape: ContextShape,
    /// Grounding and overlay inclusion policy.
    #[serde(default)]
    pub ground: GroundingOptions,
    /// Token, file, symbol, freshness, and tier limits.
    #[serde(default)]
    pub budget: ContextBudget,
}

/// Optional explicit scope for the context packet.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq)]
pub struct ContextScope {
    /// Repo-relative paths to include in the task context.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Symbol names or IDs to include in the task context.
    #[serde(default)]
    pub symbols: Vec<String>,
    /// Advisory change-set label such as `working_tree`.
    #[serde(default)]
    pub change_set: Option<String>,
}

/// Requested answer sections. Unknown sections are preserved as intent hints.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq)]
pub struct ContextShape {
    /// Requested high-level answer sections, preserved as intent hints.
    #[serde(default)]
    pub sections: Vec<String>,
}

/// Grounding policy for compiled task contexts.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq)]
pub struct GroundingOptions {
    /// `citations` is accepted as an alias for Nexus-like callers.
    #[serde(default, alias = "citations")]
    pub mode: GroundingMode,
    /// Include line spans when the underlying artifact exposes them.
    #[serde(default)]
    pub include_spans: bool,
    /// Allow advisory overlay notes or commentary in the packet.
    #[serde(default)]
    pub allow_overlay: bool,
}

impl Default for GroundingOptions {
    fn default() -> Self {
        Self {
            mode: GroundingMode::Required,
            include_spans: true,
            allow_overlay: false,
        }
    }
}

/// Citation requirement for a task-context response.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundingMode {
    /// Evidence is required for the packet to be considered grounded.
    #[default]
    Required,
    /// Evidence should be returned when available.
    Preferred,
    /// Grounding is explicitly disabled.
    Off,
}

/// Coarse confidence labels used by high-level context evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Directly observed from graph, substrate, or filesystem-backed facts.
    Observed,
    /// Inferred with high confidence by deterministic code.
    InferredHigh,
    /// Inferred with low confidence by deterministic code.
    InferredLow,
    /// Machine-authored overlay with high confidence.
    MachineOverlayHigh,
    /// Machine-authored overlay with low confidence.
    MachineOverlayLow,
    /// Human-authored or human-declared fact.
    HumanDeclared,
}

impl From<crate::structure::graph::Epistemic> for Confidence {
    fn from(epistemic: crate::structure::graph::Epistemic) -> Self {
        match epistemic {
            crate::structure::graph::Epistemic::ParserObserved
            | crate::structure::graph::Epistemic::GitObserved => Self::Observed,
            crate::structure::graph::Epistemic::HumanDeclared => Self::HumanDeclared,
        }
    }
}

impl From<crate::overlay::OverlayEpistemic> for Confidence {
    fn from(epistemic: crate::overlay::OverlayEpistemic) -> Self {
        match epistemic {
            crate::overlay::OverlayEpistemic::MachineAuthoredHighConf => Self::MachineOverlayHigh,
            crate::overlay::OverlayEpistemic::MachineAuthoredLowConf => Self::MachineOverlayLow,
        }
    }
}

impl From<crate::overlay::ConfidenceTier> for Confidence {
    fn from(tier: crate::overlay::ConfidenceTier) -> Self {
        match tier {
            crate::overlay::ConfidenceTier::High => Self::MachineOverlayHigh,
            crate::overlay::ConfidenceTier::ReviewQueue
            | crate::overlay::ConfidenceTier::BelowThreshold => Self::MachineOverlayLow,
        }
    }
}

/// Line-span evidence exposed by high-level task-context responses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CitedLineSpan {
    /// One-based start line.
    pub start_line: u64,
    /// One-based end line.
    pub end_line: u64,
}

/// Surface-level source reference for context evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextSourceRef {
    /// Repo-relative source path or stable target label.
    pub path: String,
    /// Store that produced the evidence, such as `graph`, `overlay`, or `substrate_index`.
    pub source_store: String,
    /// Content hash when known from the underlying artifact.
    pub content_hash: Option<String>,
}

/// Generic cited value used by task-context response fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Cited<T> {
    /// The answer value.
    pub value: T,
    /// Source references that ground the value.
    pub provenance: Vec<ContextSourceRef>,
    /// Line spans that ground the value when available and requested.
    pub spans: Vec<CitedLineSpan>,
    /// Coarse confidence label for the value.
    pub confidence: Confidence,
}

/// Evidence row returned by `synrepo_ask`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContextEvidence {
    /// Human-readable claim grounded by the source reference.
    pub claim: String,
    /// Backward-compatible primary source field.
    pub source: String,
    /// Backward-compatible primary span field. `null` means unknown or withheld.
    pub span: Option<CitedLineSpan>,
    /// All line spans used to ground the claim.
    pub spans: Vec<CitedLineSpan>,
    /// Store that produced the evidence.
    pub source_store: String,
    /// Coarse confidence label for the claim.
    pub confidence: Confidence,
    /// Source references that ground the claim.
    pub provenance: Vec<ContextSourceRef>,
}

/// Budget controls for a compiled task context.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq)]
pub struct ContextBudget {
    /// Maximum estimated tokens for the rendered packet.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Maximum scoped file/path targets to consider.
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    /// Maximum scoped symbol targets to consider.
    #[serde(default = "default_max_symbols")]
    pub max_symbols: usize,
    /// Freshness preference, currently advisory.
    #[serde(default)]
    pub freshness: Option<String>,
    /// Optional card budget tier: `tiny`, `normal`, or `deep`.
    #[serde(default)]
    pub tier: Option<String>,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            max_files: default_max_files(),
            max_symbols: default_max_symbols(),
            freshness: None,
            tier: None,
        }
    }
}

/// Existing context-pack target expressed without depending on the MCP module.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContextTarget {
    /// Existing context-pack target kind.
    pub kind: String,
    /// File path, symbol name, directory path, or search query.
    pub target: String,
    /// Optional per-target budget tier.
    pub budget: Option<String>,
}

/// Default token cap for `synrepo_ask` packets.
pub fn default_max_tokens() -> usize {
    6_000
}

/// Default maximum path/file scopes for `synrepo_ask`.
pub fn default_max_files() -> usize {
    12
}

/// Default maximum symbol scopes for `synrepo_ask`.
pub fn default_max_symbols() -> usize {
    40
}
