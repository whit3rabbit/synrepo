use schemars::JsonSchema;
use serde::Deserialize;

use crate::{core::ids::NodeId, surface::card::CardCompiler};

use super::helpers::{attach_decision_cards, lift_commentary_text, parse_budget, with_mcp_compiler};
use super::SynrepoState;

/// Parameters for the `synrepo_card` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CardParams {
    /// Target to look up. Accepts a file path, a qualified symbol name, or a
    /// short symbol name (display name). First match wins.
    pub target: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
}

pub fn default_budget() -> String {
    "tiny".to_string()
}

/// Parameters for the `synrepo_entrypoints` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntrypointsParams {
    /// Optional path prefix to scope the search (e.g. `"src/bin/"`, `"src/surface/"`).
    pub scope: Option<String>,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
}

/// Parameters for the `synrepo_module_card` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ModuleCardParams {
    /// Directory path of the module to summarize (e.g. `"src/auth"`).
    pub path: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
}

/// Parameters for the `synrepo_public_api` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PublicAPICardParams {
    /// Directory path to inspect (e.g. `"src/auth"` or `"src/surface/card"`).
    pub path: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
}

/// Parameters for the `synrepo_minimum_context` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MinimumContextParams {
    /// Target: node ID (e.g. "symbol_0000000000000024") or qualified path.
    pub target: String,
    /// Budget tier: "tiny", "normal", or "deep". Defaults to "normal".
    #[serde(default = "default_budget")]
    pub budget: String,
}

/// Parameters for the `synrepo_call_path` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CallPathParams {
    /// Target symbol: node ID (e.g. "symbol_0000000000000024") or qualified name.
    pub target: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
}

/// Parameters for the `synrepo_test_surface` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TestSurfaceParams {
    /// Scope: file path or directory to find tests for.
    pub scope: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
}

pub fn handle_card(state: &SynrepoState, target: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let node_id = compiler
            .resolve_target(&target)?
            .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

        match node_id {
            NodeId::Symbol(sym_id) => {
                let card = compiler.symbol_card(sym_id, budget)?;
                let mut json_val = serde_json::to_value(&card)?;
                lift_commentary_text(&mut json_val);
                attach_decision_cards(&mut json_val, NodeId::Symbol(sym_id), compiler.graph(), budget)?;
                Ok(json_val)
            }
            NodeId::File(file_id) => {
                let card = compiler.file_card(file_id, budget)?;
                let mut json_val = serde_json::to_value(&card)?;
                attach_decision_cards(&mut json_val, NodeId::File(file_id), compiler.graph(), budget)?;
                Ok(json_val)
            }
            NodeId::Concept(concept_id) => {
                let concept = compiler
                    .graph()
                    .get_concept(concept_id)?
                    .ok_or_else(|| anyhow::anyhow!("concept not found"))?;
                Ok(serde_json::to_value(&concept)?)
            }
        }
    })
}

pub fn handle_entrypoints(state: &SynrepoState, scope: Option<String>, budget: String) -> String {
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let card = compiler.entry_point_card(scope.as_deref(), budget)?;
        Ok(serde_json::to_value(&card)?)
    })
}

pub fn handle_module_card(state: &SynrepoState, path: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let card = compiler.module_card(&path, budget)?;
        Ok(serde_json::to_value(&card)?)
    })
}

pub fn handle_public_api(state: &SynrepoState, path: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let card = compiler.public_api_card(&path, budget)?;
        Ok(serde_json::to_value(&card)?)
    })
}

pub fn handle_minimum_context(state: &SynrepoState, target: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let response = crate::surface::card::neighborhood::resolve_neighborhood(
            compiler,
            &target,
            budget,
        )?;
        Ok(serde_json::to_value(&response)?)
    })
}

pub fn handle_call_path(state: &SynrepoState, target: String, budget: String) -> String {
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
        Ok(serde_json::to_value(&card)?)
    })
}

pub fn handle_test_surface(state: &SynrepoState, scope: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    with_mcp_compiler(state, |compiler| {
        let card = compiler.test_surface_card(&scope, budget)?;
        Ok(serde_json::to_value(&card)?)
    })
}
