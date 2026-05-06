use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Built-in deterministic context recipes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRecipe {
    ExplainSymbol,
    TraceCall,
    ReviewModule,
    SecurityReview,
    ReleaseReadiness,
    FixTest,
    General,
}

impl ContextRecipe {
    pub fn infer(ask: &str) -> Self {
        let text = ask.to_ascii_lowercase();
        if has_any(
            &text,
            &["security", "vulnerability", "injection", "auth", "crypto"],
        ) {
            Self::SecurityReview
        } else if has_any(&text, &["release", "readiness", "ship", "blocker"]) {
            Self::ReleaseReadiness
        } else if has_any(
            &text,
            &["fix test", "failing test", "test failure", "coverage"],
        ) {
            Self::FixTest
        } else if has_any(&text, &["trace", "call path", "call chain", "flow"]) {
            Self::TraceCall
        } else if has_any(&text, &["symbol", "function", "struct", "explain"]) {
            Self::ExplainSymbol
        } else if has_any(&text, &["review", "module", "folder", "directory", "audit"]) {
            Self::ReviewModule
        } else {
            Self::General
        }
    }

    pub fn default_budget_tier(self) -> &'static str {
        match self {
            Self::ExplainSymbol | Self::General => "tiny",
            Self::TraceCall
            | Self::ReviewModule
            | Self::SecurityReview
            | Self::ReleaseReadiness
            | Self::FixTest => "normal",
        }
    }

    pub fn next_tools(self) -> Vec<String> {
        match self {
            Self::TraceCall => vec![
                "synrepo_call_path".into(),
                "synrepo_minimum_context".into(),
                "synrepo_card".into(),
            ],
            Self::SecurityReview => vec![
                "synrepo_change_risk".into(),
                "synrepo_search(output_mode=\"compact\")".into(),
                "synrepo_card".into(),
            ],
            Self::ReleaseReadiness => vec![
                "synrepo_tests".into(),
                "synrepo_change_risk".into(),
                "synrepo_findings".into(),
            ],
            Self::FixTest => vec![
                "synrepo_tests".into(),
                "synrepo_minimum_context".into(),
                "synrepo_card".into(),
            ],
            _ => vec![
                "synrepo_card".into(),
                "synrepo_minimum_context".into(),
                "synrepo_context_pack".into(),
            ],
        }
    }
}

fn has_any(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}
