use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// High-level request accepted by the task-context front door.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq)]
pub struct ContextAskRequest {
    pub repo_root: Option<PathBuf>,
    /// Plain-language task or question to compile into a context packet.
    pub ask: String,
    #[serde(default)]
    pub scope: ContextScope,
    #[serde(default)]
    pub shape: ContextShape,
    #[serde(default)]
    pub ground: GroundingOptions,
    #[serde(default)]
    pub budget: ContextBudget,
}

/// Optional explicit scope for the context packet.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq)]
pub struct ContextScope {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub change_set: Option<String>,
}

/// Requested answer sections. Unknown sections are preserved as intent hints.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq)]
pub struct ContextShape {
    #[serde(default)]
    pub sections: Vec<String>,
}

/// Grounding policy for compiled task contexts.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq)]
pub struct GroundingOptions {
    /// `citations` is accepted as an alias for Nexus-like callers.
    #[serde(default, alias = "citations")]
    pub mode: GroundingMode,
    #[serde(default)]
    pub include_spans: bool,
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
    #[default]
    Required,
    Preferred,
    Off,
}

/// Coarse confidence labels used by high-level context evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Observed,
    InferredHigh,
    InferredLow,
    MachineOverlayHigh,
    MachineOverlayLow,
    HumanDeclared,
}

/// Budget controls for a compiled task context.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq)]
pub struct ContextBudget {
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    #[serde(default = "default_max_symbols")]
    pub max_symbols: usize,
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
    pub kind: String,
    pub target: String,
    pub budget: Option<String>,
}

pub fn default_max_tokens() -> usize {
    6_000
}

pub fn default_max_files() -> usize {
    12
}

pub fn default_max_symbols() -> usize {
    40
}
