use crate::core::ids::NodeId;
use crate::structure::graph::{EdgeKind, GraphStore};
use crate::surface::card::compiler::GraphCardCompiler;
use crate::surface::card::git::FileGitIntelligence;
use crate::surface::card::types::Freshness;
use crate::surface::card::{Budget, CardCompiler};

use super::{
    CoChangePartner, CoChangeState, EdgeCounts, MinimumContextResponse, NeighborSummary,
    NEIGHBOR_CAP,
};

use anyhow::anyhow;

/// Implementation of neighborhood resolution.
/// All functions are pub(super) since they are only called from mod.rs.
pub(super) fn resolve_neighborhood_inner(
    compiler: &GraphCardCompiler,
    graph: &dyn GraphStore,
    target: &str,
    budget: Budget,
) -> crate::Result<MinimumContextResponse> {
    use crate::surface::card::compiler::resolve::resolve_target;

    let node_id =
        resolve_target(graph, target)?.ok_or_else(|| anyhow!("target not found: {target}"))?;

    // Focal card (produced at the requested budget, without overlay content).
    let focal_card = node_card_json(compiler, node_id, budget)?;

    // For symbols, resolve the containing file to access git-intelligence.
    let file_path = file_path_for_node(graph, node_id)?;

    // Edge counts (always computed).
    let edge_counts = compute_edge_counts(graph, node_id, &file_path, compiler)?;

    // At tiny budget: focal card + counts only.
    if budget == Budget::Tiny {
        return Ok(MinimumContextResponse {
            focal_card,
            neighbors: None,
            neighbor_summaries: None,
            decision_cards: None,
            co_change_partners: None,
            co_change_state: if edge_counts.co_change_count > 0 {
                CoChangeState::Available
            } else {
                CoChangeState::Missing
            },
            edge_counts,
            budget: "tiny",
        });
    }

    // Structural neighbor resolution.
    let (neighbor_summaries, neighbors) =
        resolve_structural_neighbors(compiler, graph, node_id, budget)?;

    // Governing decisions.
    let decision_cards = resolve_governing_decisions(graph, node_id, budget)?;

    // Co-change partners.
    let (co_change_partners, co_change_state) =
        resolve_co_change_partners(compiler, &file_path, budget)?;

    Ok(MinimumContextResponse {
        focal_card,
        neighbors,
        neighbor_summaries,
        decision_cards,
        co_change_partners: Some(co_change_partners),
        co_change_state,
        edge_counts,
        budget: match budget {
            Budget::Normal => "normal",
            Budget::Deep => "deep",
            Budget::Tiny => "tiny",
        },
    })
}

/// Render a graph node as a JSON card at `budget`, stripping overlay-only fields.
///
/// Symbols and files produce their respective cards; concepts fall back to the
/// raw `ConceptNode` payload because there is no ConceptCard surface today.
fn node_card_json(
    compiler: &GraphCardCompiler,
    node_id: NodeId,
    budget: Budget,
) -> crate::Result<serde_json::Value> {
    let mut json = match node_id {
        NodeId::Symbol(sym_id) => {
            let card = compiler.symbol_card(sym_id, budget)?;
            serde_json::to_value(&card).map_err(|e| anyhow!(e))?
        }
        NodeId::File(file_id) => {
            let card = compiler.file_card(file_id, budget)?;
            serde_json::to_value(&card).map_err(|e| anyhow!(e))?
        }
        NodeId::Concept(concept_id) => {
            let concept = compiler
                .graph()
                .get_concept(concept_id)?
                .ok_or_else(|| anyhow!("concept not found"))?;
            serde_json::to_value(&concept).map_err(|e| anyhow!(e))?
        }
    };
    strip_overlay_fields(&mut json);
    Ok(json)
}

/// Remove overlay-only keys that minimum-context must not expose.
pub(super) fn strip_overlay_fields(json: &mut serde_json::Value) {
    if let serde_json::Value::Object(ref mut map) = json {
        map.remove("overlay_commentary");
        map.remove("proposed_links");
        map.remove("commentary_state");
        map.remove("links_state");
        map.remove("commentary_text");
    }
}

/// Get the file path for a node (needed for git-intelligence lookup).
/// For symbols, resolves through the containing file. For files, returns
/// the path directly. For concepts, returns empty (no git intelligence).
fn file_path_for_node(graph: &dyn GraphStore, node_id: NodeId) -> crate::Result<String> {
    match node_id {
        NodeId::File(file_id) => {
            let file = graph
                .get_file(file_id)?
                .ok_or_else(|| anyhow!("file not found"))?;
            Ok(file.path)
        }
        NodeId::Symbol(sym_id) => {
            let sym = graph
                .get_symbol(sym_id)?
                .ok_or_else(|| anyhow!("symbol not found"))?;
            let file = graph
                .get_file(sym.file_id)?
                .ok_or_else(|| anyhow!("symbol's file not found"))?;
            Ok(file.path)
        }
        NodeId::Concept(_) => Ok(String::new()),
    }
}

/// Count edges for the `edge_counts` payload.
fn compute_edge_counts(
    graph: &dyn GraphStore,
    node_id: NodeId,
    file_path: &str,
    compiler: &GraphCardCompiler,
) -> crate::Result<EdgeCounts> {
    let outbound_calls = graph.outbound(node_id, Some(EdgeKind::Calls))?;
    let outbound_imports = graph
        .outbound(node_id, Some(EdgeKind::Imports))
        .unwrap_or_default();
    let governs = graph.find_governing_concepts(node_id)?;

    let co_change_count = compiler
        .resolve_file_git_intelligence(file_path)
        .map(|insights| insights.co_change_partners.len())
        .unwrap_or(0);

    Ok(EdgeCounts {
        outbound_calls_count: outbound_calls.len(),
        outbound_imports_count: outbound_imports.len(),
        governs_count: governs.len(),
        co_change_count,
    })
}

/// Resolve structural neighbors. At `normal`: summaries. At `deep`: full cards.
#[allow(clippy::type_complexity)]
fn resolve_structural_neighbors(
    compiler: &GraphCardCompiler,
    graph: &dyn GraphStore,
    node_id: NodeId,
    budget: Budget,
) -> crate::Result<(Option<Vec<NeighborSummary>>, Option<Vec<serde_json::Value>>)> {
    let calls_edges = graph.outbound(node_id, Some(EdgeKind::Calls))?;
    let imports_edges = graph
        .outbound(node_id, Some(EdgeKind::Imports))
        .unwrap_or_default();

    let all_edges: Vec<_> = calls_edges
        .iter()
        .chain(imports_edges.iter())
        .take(NEIGHBOR_CAP)
        .collect();

    match budget {
        Budget::Normal => {
            let mut summaries = Vec::new();
            for edge in &all_edges {
                if let Some(summary) = neighbor_summary_for_node(graph, &edge.to, &edge.kind)? {
                    summaries.push(summary);
                }
            }
            Ok((Some(summaries), None))
        }
        Budget::Deep => {
            let mut cards = Vec::new();
            for edge in &all_edges {
                // Concept nodes don't render as neighbor cards; skip them.
                if matches!(edge.to, NodeId::Concept(_)) {
                    continue;
                }
                let card = node_card_json(compiler, edge.to, Budget::Deep)?;
                cards.push(card);
            }
            Ok((None, Some(cards)))
        }
        Budget::Tiny => Ok((None, None)),
    }
}

/// Build a lightweight summary for a neighbor node.
fn neighbor_summary_for_node(
    graph: &dyn GraphStore,
    node_id: &NodeId,
    edge_kind: &EdgeKind,
) -> crate::Result<Option<NeighborSummary>> {
    match node_id {
        NodeId::Symbol(sym_id) => {
            let sym = match graph.get_symbol(*sym_id)? {
                Some(s) => s,
                None => return Ok(None),
            };
            Ok(Some(NeighborSummary {
                node_id: sym_id.to_string(),
                qualified_name: sym.qualified_name,
                kind: sym.kind.as_str().to_string(),
                edge_type: edge_kind.as_str().to_string(),
            }))
        }
        NodeId::File(file_id) => {
            let file = match graph.get_file(*file_id)? {
                Some(f) => f,
                None => return Ok(None),
            };
            Ok(Some(NeighborSummary {
                node_id: file_id.to_string(),
                qualified_name: file.path,
                kind: "file".to_string(),
                edge_type: edge_kind.as_str().to_string(),
            }))
        }
        NodeId::Concept(_) => Ok(None),
    }
}

/// Resolve governing decisions as DecisionCards (summary at normal, full at deep).
fn resolve_governing_decisions(
    graph: &dyn GraphStore,
    node_id: NodeId,
    budget: Budget,
) -> crate::Result<Option<Vec<serde_json::Value>>> {
    use crate::surface::card::decision::DecisionCard;

    let concepts = graph.find_governing_concepts(node_id)?;
    if concepts.is_empty() {
        return Ok(None);
    }

    let mut cards = Vec::new();
    for concept in &concepts {
        let governs_edges = graph.outbound(NodeId::Concept(concept.id), Some(EdgeKind::Governs))?;
        let governed_node_ids: Vec<NodeId> = governs_edges.iter().map(|e| e.to).collect();

        let dc = DecisionCard {
            title: concept.title.clone(),
            status: concept.status.clone(),
            decision_body: concept.decision_body.clone(),
            governed_node_ids,
            source_path: concept.path.clone(),
            freshness: Freshness::Fresh,
        };
        cards.push(dc.render(budget));
    }
    Ok(Some(cards))
}

/// Resolve co-change partners from the git-intelligence cache.
fn resolve_co_change_partners(
    compiler: &GraphCardCompiler,
    file_path: &str,
    budget: Budget,
) -> crate::Result<(Vec<CoChangePartner>, CoChangeState)> {
    let insights = compiler.resolve_file_git_intelligence(file_path);

    match insights {
        Some(insights) => {
            let file_git = FileGitIntelligence::from(&*insights);
            let cap = match budget {
                Budget::Normal => 3,
                Budget::Deep => 5,
                Budget::Tiny => 0,
            };

            // Already ranked by co_change_count descending in the analysis.
            let partners: Vec<CoChangePartner> = file_git
                .co_change_partners
                .into_iter()
                .take(cap)
                .map(|cc| CoChangePartner {
                    path: cc.path,
                    co_change_count: cc.co_change_count,
                    source: "git_intelligence",
                    granularity: "file",
                })
                .collect();

            let state = if partners.is_empty() {
                CoChangeState::Missing
            } else {
                CoChangeState::Available
            };

            Ok((partners, state))
        }
        None => Ok((vec![], CoChangeState::Missing)),
    }
}
