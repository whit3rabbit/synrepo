//! Neighborhood resolution for the `synrepo_minimum_context` MCP tool.
//!
//! Assembles a budget-bounded 1-hop neighborhood around a focal node,
//! combining structural edges with git co-change signals into one response.

use anyhow::anyhow;
use serde::Serialize;

use crate::core::ids::NodeId;
use crate::structure::graph::{EdgeKind, GraphStore};
use crate::surface::card::compiler::resolve::resolve_target;
use crate::surface::card::compiler::GraphCardCompiler;
use crate::surface::card::decision::DecisionCard;
use crate::surface::card::git::FileGitIntelligence;
use crate::surface::card::types::Freshness;
use crate::surface::card::{Budget, CardCompiler};

/// Whether co-change data was available for the focal node's file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoChangeState {
    /// Co-change data was available.
    Available,
    /// No co-change data found.
    Missing,
}

/// A lightweight summary of a structural neighbor (used at `normal` budget).
#[derive(Clone, Debug, Serialize)]
pub struct NeighborSummary {
    /// Node ID of the neighbor.
    pub node_id: String,
    /// Qualified name or file path.
    pub qualified_name: String,
    /// Symbol kind (e.g. "function", "file").
    pub kind: String,
    /// Edge type to this neighbor (e.g. "calls", "imports").
    pub edge_type: String,
}

/// A co-change partner entry sourced from the git-intelligence cache.
#[derive(Clone, Debug, Serialize)]
pub struct CoChangePartner {
    /// Repository-relative file path of the co-change partner.
    pub path: String,
    /// Number of sampled commits changing both paths.
    pub co_change_count: usize,
    /// Data source label (always "git_intelligence").
    pub source: &'static str,
    /// Precision of the co-change signal (always "file").
    pub granularity: &'static str,
}

/// Edge counts returned at every budget tier (even `tiny`).
#[derive(Clone, Debug, Serialize)]
pub struct EdgeCounts {
    /// Number of outbound Calls edges.
    pub outbound_calls_count: usize,
    /// Number of outbound Imports edges.
    pub outbound_imports_count: usize,
    /// Number of incoming Governs edges.
    pub governs_count: usize,
    /// Number of co-change partners in git intelligence.
    pub co_change_count: usize,
}

/// The full response payload for `synrepo_minimum_context`.
#[derive(Clone, Debug, Serialize)]
pub struct MinimumContextResponse {
    /// The focal node's card (SymbolCard or FileCard as JSON).
    pub focal_card: serde_json::Value,
    /// Full neighbor cards (deep budget only).
    pub neighbors: Option<Vec<serde_json::Value>>,
    /// Neighbor summaries (normal budget only).
    pub neighbor_summaries: Option<Vec<NeighborSummary>>,
    /// Governing DecisionCards.
    pub decision_cards: Option<Vec<serde_json::Value>>,
    /// Co-change partners from git intelligence.
    pub co_change_partners: Option<Vec<CoChangePartner>>,
    /// Whether co-change data was available.
    pub co_change_state: CoChangeState,
    /// Edge counts for the focal node.
    pub edge_counts: EdgeCounts,
    /// Budget tier used for this response.
    pub budget: &'static str,
}

/// Resolve a 1-hop neighborhood around `target` at the given `budget`.
///
/// Returns an explicit error when `target` does not resolve to a graph node.
/// The entire resolution runs under a single graph read snapshot so the
/// response reflects a consistent epoch.
pub fn resolve_neighborhood(
    compiler: &GraphCardCompiler,
    target: &str,
    budget: Budget,
) -> crate::Result<MinimumContextResponse> {
    let graph = compiler.graph();

    graph.begin_read_snapshot()?;
    let result = resolve_neighborhood_inner(compiler, graph, target, budget);
    let _ = graph.end_read_snapshot();
    result
}

fn resolve_neighborhood_inner(
    compiler: &GraphCardCompiler,
    graph: &dyn GraphStore,
    target: &str,
    budget: Budget,
) -> crate::Result<MinimumContextResponse> {
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
fn strip_overlay_fields(json: &mut serde_json::Value) {
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

/// Hard cap on neighbor count across all edge kinds combined.
const NEIGHBOR_CAP: usize = 20;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::pipeline::structural::run_structural_compile;
    use crate::store::sqlite::SqliteGraphStore;
    use crate::surface::card::compiler::test_support::bootstrap;
    use crate::surface::card::compiler::GraphCardCompiler;
    use crate::surface::card::Budget;
    use insta::assert_snapshot;
    use std::fs;
    use tempfile::tempdir;

    fn make_compiler(repo: &tempfile::TempDir) -> GraphCardCompiler {
        let graph = bootstrap(repo);
        GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
    }

    fn make_compiler_with_config(repo: &tempfile::TempDir, config: Config) -> GraphCardCompiler {
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
        run_structural_compile(repo.path(), &config, &mut graph).unwrap();
        GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
    }

    // 4.1: tiny budget returns focal card + counts, no neighbor details
    #[test]
    fn tiny_budget_returns_focal_card_with_counts_and_no_details() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

        let compiler = make_compiler(&repo);
        let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Tiny).unwrap();

        assert_eq!(resp.budget, "tiny");
        assert!(resp.focal_card.is_object(), "focal_card must be present");
        assert!(resp.neighbors.is_none(), "no full cards at tiny budget");
        assert!(
            resp.neighbor_summaries.is_none(),
            "no summaries at tiny budget"
        );
        assert!(resp.decision_cards.is_none(), "no decisions at tiny budget");
        assert!(
            resp.co_change_partners.is_none(),
            "no partners at tiny budget"
        );
        // Overlay-only fields must be stripped from focal card.
        let fc = resp.focal_card.as_object().unwrap();
        assert!(!fc.contains_key("overlay_commentary"));
        assert!(!fc.contains_key("proposed_links"));
        assert!(!fc.contains_key("commentary_state"));
        assert!(!fc.contains_key("links_state"));
    }

    // 4.2: normal budget returns summaries (even if empty), not full cards; co-change missing without git
    #[test]
    fn normal_budget_returns_summaries_not_full_cards() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

        let compiler = make_compiler(&repo);
        let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Normal).unwrap();

        assert_eq!(resp.budget, "normal");
        assert!(
            resp.neighbor_summaries.is_some(),
            "summaries must be Some at normal budget"
        );
        assert!(
            resp.neighbors.is_none(),
            "full cards must be None at normal budget"
        );
        // No git init in tempdir → co-change state is missing.
        assert_eq!(resp.co_change_state, CoChangeState::Missing);
        assert_eq!(
            resp.co_change_partners.as_deref().unwrap_or(&[]).len(),
            0,
            "no co-change partners without git"
        );
    }

    // 4.3: deep budget returns full neighbor cards, not summaries
    #[test]
    fn deep_budget_returns_full_cards_not_summaries() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

        let compiler = make_compiler(&repo);
        let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Deep).unwrap();

        assert_eq!(resp.budget, "deep");
        assert!(
            resp.neighbors.is_some(),
            "full cards must be Some at deep budget"
        );
        assert!(
            resp.neighbor_summaries.is_none(),
            "summaries must be None at deep budget"
        );
    }

    // 4.4: unresolved target returns explicit error containing the target string
    #[test]
    fn unresolved_target_returns_error_with_target_string() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

        let compiler = make_compiler(&repo);
        let err =
            resolve_neighborhood(&compiler, "nonexistent_xyz_target", Budget::Tiny).unwrap_err();

        assert!(
            err.to_string().contains("nonexistent_xyz_target"),
            "error must include target string; got: {err}"
        );
    }

    // 4.5: missing git intelligence yields empty co-change list with state "missing"
    #[test]
    fn missing_git_intelligence_returns_empty_co_change_list() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
        // No git init → git cache returns None.

        let compiler = make_compiler(&repo);
        let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Normal).unwrap();

        assert_eq!(
            resp.co_change_state,
            CoChangeState::Missing,
            "state must be missing without git"
        );
        assert!(
            resp.co_change_partners
                .as_deref()
                .is_none_or(|p| p.is_empty()),
            "co_change_partners must be empty without git"
        );
    }

    // 4.6: governing decisions surface as DecisionCards at normal and deep budgets
    #[test]
    fn governing_decisions_included_in_normal_and_deep() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::create_dir_all(repo.path().join("docs/adr")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
        fs::write(
            repo.path().join("docs/adr/0001.md"),
            "---\ntitle: Use modular design\ngoverns: [src/lib.rs]\n---\n\nThis governs the library root.\n",
        )
        .unwrap();

        let config = Config {
            concept_directories: vec!["docs/adr".to_string()],
            ..Config::default()
        };
        let compiler = make_compiler_with_config(&repo, config);

        let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Normal).unwrap();
        let cards = resp
            .decision_cards
            .expect("decision_cards must be Some when a concept governs the target");
        assert!(
            !cards.is_empty(),
            "at least one decision card must be returned"
        );

        // Deep budget: open a second connection to the already-populated graph.
        let graph_dir = repo.path().join(".synrepo/graph");
        let graph2 = SqliteGraphStore::open(&graph_dir).unwrap();
        let compiler2 = GraphCardCompiler::new(Box::new(graph2), Some(repo.path()));
        let resp2 = resolve_neighborhood(&compiler2, "src/lib.rs", Budget::Deep).unwrap();
        assert!(
            resp2
                .decision_cards
                .as_deref()
                .is_some_and(|c| !c.is_empty()),
            "decision cards must appear at deep budget too"
        );
    }

    // 4.7: snapshot the full MinimumContextResponse at each budget tier
    #[test]
    fn minimum_context_response_snapshots() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/lib.rs"),
            "/// Add two integers.\npub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .unwrap();

        let compiler = make_compiler(&repo);

        let tiny = serde_json::to_string_pretty(
            &resolve_neighborhood(&compiler, "src/lib.rs", Budget::Tiny).unwrap(),
        )
        .unwrap();
        assert_snapshot!("neighborhood_response_tiny", tiny);

        let normal = serde_json::to_string_pretty(
            &resolve_neighborhood(&compiler, "src/lib.rs", Budget::Normal).unwrap(),
        )
        .unwrap();
        assert_snapshot!("neighborhood_response_normal", normal);

        let deep = serde_json::to_string_pretty(
            &resolve_neighborhood(&compiler, "src/lib.rs", Budget::Deep).unwrap(),
        )
        .unwrap();
        assert_snapshot!("neighborhood_response_deep", deep);
    }

    #[test]
    fn co_change_state_serializes_as_snake_case() {
        let available = serde_json::to_string(&CoChangeState::Available).unwrap();
        assert_eq!(available, "\"available\"");
        let missing = serde_json::to_string(&CoChangeState::Missing).unwrap();
        assert_eq!(missing, "\"missing\"");
    }

    #[test]
    fn strip_overlay_fields_removes_expected_keys() {
        let mut json = serde_json::json!({
            "name": "test",
            "overlay_commentary": "should be removed",
            "proposed_links": [],
            "commentary_state": "should be removed",
            "links_state": "should be removed",
            "commentary_text": "should be removed",
        });
        strip_overlay_fields(&mut json);
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(!obj.contains_key("overlay_commentary"));
        assert!(!obj.contains_key("proposed_links"));
        assert!(!obj.contains_key("commentary_state"));
        assert!(!obj.contains_key("links_state"));
        assert!(!obj.contains_key("commentary_text"));
    }

    #[test]
    fn co_change_partner_has_correct_labels() {
        let partner = CoChangePartner {
            path: "src/main.rs".to_string(),
            co_change_count: 5,
            source: "git_intelligence",
            granularity: "file",
        };
        let json = serde_json::to_value(&partner).unwrap();
        assert_eq!(json["source"], "git_intelligence");
        assert_eq!(json["granularity"], "file");
    }

    #[test]
    fn edge_counts_serializes_with_expected_keys() {
        let counts = EdgeCounts {
            outbound_calls_count: 3,
            outbound_imports_count: 1,
            governs_count: 2,
            co_change_count: 4,
        };
        let json = serde_json::to_value(&counts).unwrap();
        assert_eq!(json["outbound_calls_count"], 3);
        assert_eq!(json["outbound_imports_count"], 1);
        assert_eq!(json["governs_count"], 2);
        assert_eq!(json["co_change_count"], 4);
    }
}
