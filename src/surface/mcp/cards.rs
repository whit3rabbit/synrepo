use schemars::JsonSchema;
use serde::Deserialize;
use std::time::Instant;

use crate::{core::ids::NodeId, surface::card::CardCompiler};

use super::card_accounting::{finalize_card_json, record_embedded_card_metrics};
use super::card_render::render_card_target;
pub use super::commentary::{handle_refresh_commentary, RefreshCommentaryParams};
use super::helpers::{parse_budget, with_mcp_compiler};
use super::SynrepoState;

/// Parameters for the `synrepo_card` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CardParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Target to look up. Accepts a file path, a qualified symbol name, or a
    /// short symbol name (display name). First match wins.
    #[serde(default)]
    pub target: Option<String>,
    /// Batch targets to look up. Capped at 10 entries.
    #[serde(default)]
    pub targets: Vec<String>,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
    /// Optional numeric token cap. Single-card responses report truncation
    /// when the estimate exceeds the cap; card-set tools may drop lower-ranked
    /// cards before returning.
    #[serde(default)]
    pub budget_tokens: Option<usize>,
    /// Include bounded advisory agent notes under `advisory_notes`.
    #[serde(default)]
    pub include_notes: bool,
}

pub fn default_budget() -> String {
    "tiny".to_string()
}

/// Parameters for the `synrepo_entrypoints` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntrypointsParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Optional path prefix to scope the search (e.g. `"src/bin/"`, `"src/surface/"`).
    pub scope: Option<String>,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

/// Parameters for the `synrepo_module_card` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ModuleCardParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Directory path of the module to summarize (e.g. `"src/auth"`).
    pub path: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

/// Parameters for the `synrepo_public_api` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PublicAPICardParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Directory path to inspect (e.g. `"src/auth"` or `"src/surface/card"`).
    pub path: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

/// Parameters for the `synrepo_minimum_context` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MinimumContextParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Target: node ID (e.g. "sym_0000000000000024") or qualified path.
    pub target: String,
    /// Budget tier: "tiny", "normal", or "deep". Defaults to "normal".
    #[serde(default = "default_budget")]
    pub budget: String,
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

/// Parameters for the `synrepo_call_path` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CallPathParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Target symbol: node ID (e.g. "sym_0000000000000024") or qualified name.
    pub target: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

/// Parameters for the `synrepo_test_surface` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TestSurfaceParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Scope: file path or directory to find tests for.
    pub scope: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

/// Parameters for the `synrepo_change_risk` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangeRiskParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Target: file path or qualified symbol name.
    pub target: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

pub fn handle_card(
    state: &SynrepoState,
    target: String,
    budget: String,
    budget_tokens: Option<usize>,
    include_notes: bool,
) -> String {
    let start = Instant::now();
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        render_card_target(
            state,
            compiler,
            &target,
            budget,
            budget_tokens,
            include_notes,
            start,
        )
    })
}

pub fn handle_entrypoints(
    state: &SynrepoState,
    scope: Option<String>,
    budget: String,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let card = compiler.entry_point_card(scope.as_deref(), budget)?;
        Ok(finalize_card_json(
            state,
            serde_json::to_value(&card)?,
            budget_tokens,
            start,
            false,
        ))
    })
}

pub fn handle_module_card(
    state: &SynrepoState,
    path: String,
    budget: String,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let card = compiler.module_card(&path, budget)?;
        Ok(finalize_card_json(
            state,
            serde_json::to_value(&card)?,
            budget_tokens,
            start,
            false,
        ))
    })
}

pub fn handle_public_api(
    state: &SynrepoState,
    path: String,
    budget: String,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let card = compiler.public_api_card(&path, budget)?;
        Ok(finalize_card_json(
            state,
            serde_json::to_value(&card)?,
            budget_tokens,
            start,
            false,
        ))
    })
}

pub fn handle_minimum_context(
    state: &SynrepoState,
    target: String,
    budget: String,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let mut response =
            crate::surface::card::neighborhood::resolve_neighborhood(compiler, &target, budget)?;
        if let Some(cap) = budget_tokens {
            response.apply_numeric_cap(cap);
        }
        let json = serde_json::to_value(&response)?;
        record_embedded_card_metrics(state, &json, start, false);
        Ok(json)
    })
}

pub fn handle_call_path(
    state: &SynrepoState,
    target: String,
    budget: String,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        // Resolve target to a symbol node ID.
        let node_id = compiler
            .resolve_target(&target)?
            .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

        let sym_id = match node_id {
            NodeId::Symbol(sym_id) => sym_id,
            _ => {
                return Err(anyhow::anyhow!(
                    "target must be a symbol, got: {:?}",
                    node_id
                ))
            }
        };

        let card = compiler.call_path_card(sym_id, budget)?;
        Ok(finalize_card_json(
            state,
            serde_json::to_value(&card)?,
            budget_tokens,
            start,
            false,
        ))
    })
}

pub fn handle_test_surface(
    state: &SynrepoState,
    scope: String,
    budget: String,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let card = compiler.test_surface_card(&scope, budget)?;
        let test_hit = card.test_symbol_count > 0 || card.test_file_count > 0;
        Ok(finalize_card_json(
            state,
            serde_json::to_value(&card)?,
            budget_tokens,
            start,
            test_hit,
        ))
    })
}

pub fn handle_change_risk(
    state: &SynrepoState,
    target: String,
    budget: String,
    budget_tokens: Option<usize>,
) -> String {
    let start = Instant::now();
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let node_id = compiler
            .resolve_target(&target)?
            .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

        let card = compiler.change_risk_card(node_id, budget)?;
        Ok(finalize_card_json(
            state,
            serde_json::to_value(&card)?,
            budget_tokens,
            start,
            false,
        ))
    })
}
