use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{
    core::ids::{EdgeId, NodeId},
    overlay::OverlayStore,
    structure::graph::EdgeKind,
    surface::{
        card::compiler::resolve_target,
        graph_view::{parse_edge_kind_filter, parse_edge_kind_filters},
    },
};

use super::helpers::{render_result, with_mcp_compiler};
use super::SynrepoState;

/// Parameters for the `synrepo_node` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NodeParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Node ID in display form (e.g. "file_0000000000000042",
    /// "sym_0000000000000024", "concept_0000000000000099").
    pub id: String,
}

/// Parameters for the `synrepo_edges` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EdgesParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Node ID in display form.
    pub id: String,
    /// Direction: "outbound" (default) or "inbound".
    #[serde(default = "default_direction")]
    pub direction: String,
    /// Optional list of edge type filters (e.g. ["Defines", "Imports"]).
    pub edge_types: Option<Vec<String>>,
}

fn default_direction() -> String {
    "outbound".to_string()
}

/// Parameters for the `synrepo_query` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Query string: "outbound <target> \[edge_kind]" or "inbound <target> \[edge_kind]".
    pub query: String,
}

/// Parameters for the `synrepo_overlay` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OverlayParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Node ID in display form.
    pub id: String,
}

/// Parameters for the `synrepo_provenance` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProvenanceParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Node ID in display form.
    pub id: String,
}

fn parse_node_id(id: &str) -> anyhow::Result<NodeId> {
    id.parse::<NodeId>().map_err(|e| {
        anyhow::anyhow!(
            "invalid node ID `{id}`: {e}. \
             Valid prefixes: file_, sym_, concept_"
        )
    })
}

/// Inline graph query parser. Accepts: `<direction> <target> [edge_kind]`.
fn parse_graph_query(query: &str) -> anyhow::Result<(QueryDirection, String, Option<EdgeKind>)> {
    let parts: Vec<&str> = query.split_whitespace().collect();
    if parts.len() < 2 || parts.len() > 3 {
        anyhow::bail!(
            "invalid query: expected `outbound|inbound <target> [edge_kind]`, got `{query}`"
        );
    }

    let direction = match parts[0] {
        "outbound" => QueryDirection::Outbound,
        "inbound" => QueryDirection::Inbound,
        other => anyhow::bail!("invalid direction `{other}`: expected `outbound` or `inbound`"),
    };

    let target = parts[1].to_string();
    let edge_kind = parts
        .get(2)
        .map(|kind| parse_edge_kind_filter(kind).map_err(anyhow::Error::from))
        .transpose()?;

    Ok((direction, target, edge_kind))
}

#[derive(Clone, Copy)]
enum QueryDirection {
    Outbound,
    Inbound,
}

pub fn handle_node(state: &SynrepoState, id: String) -> String {
    let node_id = match parse_node_id(&id) {
        Ok(id) => id,
        Err(e) => return render_result(Err(e)),
    };
    let canonical_id = node_id.to_string();

    with_mcp_compiler(state, |compiler| match node_id {
        NodeId::File(file_id) => {
            let node = compiler
                .reader()
                .get_file(file_id)?
                .ok_or_else(|| anyhow::anyhow!("node not found: {id}"))?;
            Ok(json!({
                "node_id": canonical_id,
                "node_type": "file",
                "node": node,
            }))
        }
        NodeId::Symbol(symbol_id) => {
            let node = compiler
                .reader()
                .get_symbol(symbol_id)?
                .ok_or_else(|| anyhow::anyhow!("node not found: {id}"))?;
            Ok(json!({
                "node_id": canonical_id,
                "node_type": "symbol",
                "node": node,
            }))
        }
        NodeId::Concept(concept_id) => {
            let node = compiler
                .reader()
                .get_concept(concept_id)?
                .ok_or_else(|| anyhow::anyhow!("node not found: {id}"))?;
            Ok(json!({
                "node_id": canonical_id,
                "node_type": "concept",
                "node": node,
            }))
        }
    })
}

pub fn handle_edges(
    state: &SynrepoState,
    id: String,
    direction: String,
    edge_types: Option<Vec<String>>,
) -> String {
    let node_id = match parse_node_id(&id) {
        Ok(id) => id,
        Err(e) => return render_result(Err(e)),
    };

    with_mcp_compiler(state, |compiler| {
        node_exists(compiler, node_id)?;

        let parsed_kinds = edge_types
            .as_deref()
            .map(parse_edge_kind_filters)
            .transpose()?;

        let store_filter = parsed_kinds.as_deref().and_then(|k| match k {
            [single] => Some(*single),
            _ => None,
        });

        let edges = match direction.as_str() {
            "inbound" => compiler.reader().inbound(node_id, store_filter)?,
            _ => compiler.reader().outbound(node_id, store_filter)?,
        };

        let drift_scores = current_drift_scores(compiler);
        let filtered: Vec<serde_json::Value> = match parsed_kinds.as_deref() {
            Some(kinds) if kinds.len() > 1 => edges
                .into_iter()
                .filter(|e| kinds.contains(&e.kind))
                .map(|e| serialize_edge(&e, &drift_scores))
                .collect(),
            _ => edges
                .into_iter()
                .map(|e| serialize_edge(&e, &drift_scores))
                .collect(),
        };

        Ok(json!({
            "node_id": id,
            "direction": direction,
            "edges": filtered,
        }))
    })
}

pub fn handle_query(state: &SynrepoState, query: String) -> String {
    let (direction, target, edge_kind) = match parse_graph_query(&query) {
        Ok(res) => res,
        Err(e) => return render_result(Err(e)),
    };

    with_mcp_compiler(state, |compiler| {
        let node_id = resolve_target(compiler.reader(), &target)?
            .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;
        let edges = match direction {
            QueryDirection::Outbound => compiler.reader().outbound(node_id, edge_kind)?,
            QueryDirection::Inbound => compiler.reader().inbound(node_id, edge_kind)?,
        };

        let drift_scores = current_drift_scores(compiler);
        let rendered: Vec<serde_json::Value> = edges
            .into_iter()
            .map(|e| serialize_edge(&e, &drift_scores))
            .collect();

        let dir_str = match direction {
            QueryDirection::Outbound => "outbound",
            QueryDirection::Inbound => "inbound",
        };

        Ok(json!({
            "direction": dir_str,
            "node_id": node_id.to_string(),
            "edge_kind": edge_kind.map(|k| k.as_str().to_string()),
            "edges": rendered,
        }))
    })
}

pub fn handle_overlay(state: &SynrepoState, id: String) -> String {
    let node_id = match parse_node_id(&id) {
        Ok(id) => id,
        Err(e) => return render_result(Err(e)),
    };

    with_mcp_compiler(state, |compiler| {
        node_exists(compiler, node_id)?;

        let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
        let overlay_dir = synrepo_dir.join("overlay");
        state.require_overlay_materialized()?;
        let overlay = crate::store::overlay::SqliteOverlayStore::open_existing(&overlay_dir)?;

        let commentary = overlay.commentary_for(node_id)?;
        let links = overlay.links_for(node_id)?;

        if commentary.is_none() && links.is_empty() {
            return Ok(json!({ "overlay": null }));
        }

        Ok(json!({
            "overlay": {
                "commentary": commentary,
                "links": links,
            }
        }))
    })
}

pub fn handle_provenance(state: &SynrepoState, id: String) -> String {
    let node_id = match parse_node_id(&id) {
        Ok(id) => id,
        Err(e) => return render_result(Err(e)),
    };

    with_mcp_compiler(state, |compiler| {
        let provenance = match node_id {
            NodeId::File(file_id) => compiler
                .reader()
                .get_file(file_id)?
                .ok_or_else(|| anyhow::anyhow!("node not found: {id}"))
                .map(|n| n.provenance),
            NodeId::Symbol(symbol_id) => compiler
                .reader()
                .get_symbol(symbol_id)?
                .ok_or_else(|| anyhow::anyhow!("node not found: {id}"))
                .map(|n| n.provenance),
            NodeId::Concept(concept_id) => compiler
                .reader()
                .get_concept(concept_id)?
                .ok_or_else(|| anyhow::anyhow!("node not found: {id}"))
                .map(|n| n.provenance),
        }?;

        let outbound = compiler.reader().outbound(node_id, None)?;
        let inbound = compiler.reader().inbound(node_id, None)?;

        let mut all_edges: Vec<serde_json::Value> = Vec::new();
        for e in outbound {
            all_edges.push(json!({
                "direction": "outbound",
                "peer": e.to.to_string(),
                "edge_kind": e.kind.as_str(),
                "provenance": e.provenance,
            }));
        }
        for e in inbound {
            all_edges.push(json!({
                "direction": "inbound",
                "peer": e.from.to_string(),
                "edge_kind": e.kind.as_str(),
                "provenance": e.provenance,
            }));
        }

        Ok(json!({
            "node_id": id,
            "provenance": provenance,
            "edges": all_edges,
        }))
    })
}

fn node_exists(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    node_id: NodeId,
) -> anyhow::Result<()> {
    let exists = match node_id {
        NodeId::File(id) => compiler.reader().get_file(id)?.is_some(),
        NodeId::Symbol(id) => compiler.reader().get_symbol(id)?.is_some(),
        NodeId::Concept(id) => compiler.reader().get_concept(id)?.is_some(),
    };
    if !exists {
        anyhow::bail!("node not found: {node_id}");
    }
    Ok(())
}

fn serialize_edge(
    edge: &crate::structure::graph::Edge,
    drift_scores: &HashMap<EdgeId, f32>,
) -> serde_json::Value {
    json!({
        "id": edge.id.to_string(),
        "from": edge.from.to_string(),
        "to": edge.to.to_string(),
        "kind": edge.kind.as_str(),
        "epistemic": edge.epistemic,
        "drift_score": drift_scores.get(&edge.id).copied().unwrap_or(0.0),
        "provenance": edge.provenance,
    })
}

/// Build a `(EdgeId -> drift_score)` map for the latest drift revision, or
/// an empty map when no drift scores are present yet.
fn current_drift_scores(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
) -> HashMap<EdgeId, f32> {
    let revision = match compiler.latest_drift_revision() {
        Ok(Some(rev)) => rev,
        _ => return HashMap::new(),
    };
    compiler
        .read_drift_scores(&revision)
        .map(|pairs| pairs.into_iter().collect())
        .unwrap_or_default()
}
