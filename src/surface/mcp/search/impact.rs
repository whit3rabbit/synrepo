use std::collections::HashSet;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::graph::{Edge, EdgeKind, GraphReader},
    surface::card::CardCompiler,
};

use crate::surface::mcp::{helpers::render_result, SynrepoState};

/// Parameters for the `synrepo_change_impact` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangeImpactParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Target file path or symbol name to assess change impact for.
    pub target: String,
    /// Direction to inspect: "inbound" (default), "outbound", or "both".
    #[serde(default)]
    pub direction: ImpactDirection,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ImpactDirection {
    #[default]
    Inbound,
    Outbound,
    Both,
}

pub fn handle_change_impact(state: &SynrepoState, target: String) -> String {
    handle_change_impact_with_direction(state, target, ImpactDirection::Inbound)
}

pub fn handle_change_impact_with_direction(
    state: &SynrepoState,
    target: String,
    direction: ImpactDirection,
) -> String {
    let result: anyhow::Result<serde_json::Value> = state
        .with_read_compiler(|compiler| {
            let node_id = compiler.resolve_target(&target)?.ok_or_else(|| {
                crate::Error::Other(
                    crate::surface::mcp::error::McpError::not_found(format!(
                        "target not found: {target}"
                    ))
                    .into(),
                )
            })?;
            let inbound = if matches!(direction, ImpactDirection::Inbound | ImpactDirection::Both) {
                let imports = compiler
                    .reader()
                    .inbound(node_id, Some(EdgeKind::Imports))?;
                let calls = compiler.reader().inbound(node_id, Some(EdgeKind::Calls))?;
                collect_edge_files(compiler.reader(), imports.iter().chain(calls.iter()), true)?
            } else {
                Vec::new()
            };
            let outbound = if matches!(direction, ImpactDirection::Outbound | ImpactDirection::Both)
            {
                let imports = compiler
                    .reader()
                    .outbound(node_id, Some(EdgeKind::Imports))?;
                let calls = compiler.reader().outbound(node_id, Some(EdgeKind::Calls))?;
                collect_edge_files(compiler.reader(), imports.iter().chain(calls.iter()), false)?
            } else {
                Vec::new()
            };
            let impacted_files = match direction {
                ImpactDirection::Inbound => inbound.clone(),
                ImpactDirection::Outbound => outbound.clone(),
                ImpactDirection::Both => {
                    let mut combined = inbound.clone();
                    combined.extend(outbound.clone());
                    combined
                }
            };
            let total = impacted_files.len();
            Ok(json!({
                "target": target,
                "direction": direction.as_str(),
                "impacted_files": impacted_files,
                "inbound_files": inbound,
                "outbound_files": outbound,
                "total": total,
            }))
        })
        .map_err(|err| anyhow::anyhow!(err));
    render_result(result)
}

impl ImpactDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
            Self::Both => "both",
        }
    }
}

fn collect_edge_files<'a>(
    graph: &dyn GraphReader,
    edges: impl Iterator<Item = &'a Edge>,
    inbound: bool,
) -> crate::Result<Vec<serde_json::Value>> {
    let mut files = Vec::new();
    let mut seen = HashSet::<FileNodeId>::new();
    for edge in edges {
        let endpoint = if inbound { edge.from } else { edge.to };
        let file_id = match endpoint {
            NodeId::File(id) => id,
            NodeId::Symbol(sym_id) => match graph.get_symbol(sym_id)? {
                Some(sym) => sym.file_id,
                None => continue,
            },
            NodeId::Concept(_) => continue,
        };
        if seen.insert(file_id) {
            if let Some(file) = graph.get_file(file_id)? {
                files.push(json!({
                    "path": file.path,
                    "edge_kind": edge.kind.as_str(),
                }));
            }
        }
    }
    Ok(files)
}
