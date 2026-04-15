use schemars::JsonSchema;
use serde::Deserialize;

use crate::{core::ids::NodeId, surface::card::CardCompiler};

use super::helpers::{
    attach_decision_cards, lift_commentary_text, parse_budget, render_result, with_graph_snapshot,
};
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

pub fn handle_card(state: &SynrepoState, target: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    let result = with_graph_snapshot(state.compiler.graph(), || {
        let node_id = state
            .compiler
            .resolve_target(&target)?
            .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

        match node_id {
            NodeId::Symbol(sym_id) => {
                let card = state.compiler.symbol_card(sym_id, budget)?;
                let mut json_val = serde_json::to_value(&card)?;
                lift_commentary_text(&mut json_val);
                attach_decision_cards(
                    &mut json_val,
                    NodeId::Symbol(sym_id),
                    state.compiler.graph(),
                    budget,
                )?;
                Ok(json_val)
            }
            NodeId::File(file_id) => {
                let card = state.compiler.file_card(file_id, budget)?;
                let mut json_val = serde_json::to_value(&card)?;
                attach_decision_cards(
                    &mut json_val,
                    NodeId::File(file_id),
                    state.compiler.graph(),
                    budget,
                )?;
                Ok(json_val)
            }
            NodeId::Concept(concept_id) => {
                let concept = state
                    .compiler
                    .graph()
                    .get_concept(concept_id)?
                    .ok_or_else(|| anyhow::anyhow!("concept not found"))?;
                Ok(serde_json::to_value(&concept)?)
            }
        }
    });
    render_result(result)
}

pub fn handle_entrypoints(state: &SynrepoState, scope: Option<String>, budget: String) -> String {
    let budget = parse_budget(&budget);
    let result: anyhow::Result<serde_json::Value> =
        with_graph_snapshot(state.compiler.graph(), || {
            let card = state.compiler.entry_point_card(scope.as_deref(), budget)?;
            Ok(serde_json::to_value(&card)?)
        });
    render_result(result)
}

pub fn handle_module_card(state: &SynrepoState, path: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    let result: anyhow::Result<serde_json::Value> =
        with_graph_snapshot(state.compiler.graph(), || {
            let card = state.compiler.module_card(&path, budget)?;
            Ok(serde_json::to_value(&card)?)
        });
    render_result(result)
}

pub fn handle_public_api(state: &SynrepoState, path: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    let result: anyhow::Result<serde_json::Value> =
        with_graph_snapshot(state.compiler.graph(), || {
            let card = state.compiler.public_api_card(&path, budget)?;
            Ok(serde_json::to_value(&card)?)
        });
    render_result(result)
}

pub fn handle_minimum_context(state: &SynrepoState, target: String, budget: String) -> String {
    let budget = parse_budget(&budget);
    let result = with_graph_snapshot(state.compiler.graph(), || {
        let response = crate::surface::card::neighborhood::resolve_neighborhood(
            &state.compiler,
            &target,
            budget,
        )?;
        Ok(serde_json::to_value(&response)?)
    });
    render_result(result)
}
