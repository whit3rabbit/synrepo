use schemars::JsonSchema;
use serde::Deserialize;

use crate::surface::graph_view::{
    build_graph_neighborhood_with_compiler, parse_edge_kind_filters, GraphNeighborhoodRequest,
    GraphViewDirection,
};

use super::helpers::with_mcp_compiler;
use super::SynrepoState;

/// Parameters for the `synrepo_graph_neighborhood` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphNeighborhoodParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Optional target: node ID, file path, qualified symbol name, or short symbol name.
    pub target: Option<String>,
    /// Direction: "both" (default), "outbound", or "inbound".
    pub direction: Option<String>,
    /// Optional list of edge type filters (e.g. ["calls", "Defines"]).
    pub edge_types: Option<Vec<String>>,
    /// Traversal depth. Defaults to 1; clamped to 3.
    pub depth: Option<usize>,
    /// Node and edge limit. Defaults to 100; clamped to 500.
    pub limit: Option<usize>,
}

/// Return a bounded graph-neighborhood model for MCP clients.
pub fn handle_graph_neighborhood(state: &SynrepoState, params: GraphNeighborhoodParams) -> String {
    with_mcp_compiler(state, |compiler| {
        let direction = params
            .direction
            .as_deref()
            .map(GraphViewDirection::parse)
            .transpose()?
            .unwrap_or_default();
        let edge_types = params
            .edge_types
            .as_deref()
            .map(parse_edge_kind_filters)
            .transpose()?
            .unwrap_or_default();
        let request = GraphNeighborhoodRequest {
            target: params.target,
            direction,
            edge_types,
            depth: params.depth.unwrap_or(1),
            limit: params.limit.unwrap_or(100),
        };
        Ok(build_graph_neighborhood_with_compiler(compiler, request)?)
    })
}
