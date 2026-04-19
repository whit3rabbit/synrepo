//! ChangeRiskCard compilation.

use crate::core::ids::NodeId;
use crate::structure::graph::{Edge, EdgeKind, GraphReader};

use super::{
    super::Budget, super::ChangeRiskCard, super::RiskFactor, super::RiskLevel, GraphCardCompiler,
};

/// Compile a ChangeRiskCard for a symbol or file target.
pub fn change_risk_card(
    compiler: &GraphCardCompiler,
    graph: &dyn GraphReader,
    target: NodeId,
    budget: Budget,
) -> crate::Result<ChangeRiskCard> {
    let (target_name, target_kind) = match &target {
        NodeId::File(fid) => {
            let node = graph
                .get_file(*fid)?
                .ok_or_else(|| anyhow::anyhow!("file node not found"))?;
            (node.path.clone(), "file")
        }
        NodeId::Symbol(sid) => {
            let node = graph
                .get_symbol(*sid)?
                .ok_or_else(|| anyhow::anyhow!("symbol node not found"))?;
            (node.qualified_name.clone(), "symbol")
        }
        NodeId::Concept(_) => {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "ChangeRiskCard does not support concept targets"
            )));
        }
    };

    let revision = "current";

    let outgoing_edges = graph.outbound(target, None)?;
    let incoming_edges = graph.inbound(target, None)?;

    let (drift_score, affected_edge_count) =
        compute_drift_info(compiler, &outgoing_edges, revision);
    let co_change_partners = compute_co_change_partners(&incoming_edges);
    let hotspot_score = compute_hotspot_score(compiler, &target).unwrap_or(0.0);

    let risk_score = 0.4 * drift_score + 0.3 * co_change_partners.normalized + 0.3 * hotspot_score;
    let risk_level = RiskLevel::from_score(risk_score);

    let mut risk_factors = vec![];
    if matches!(budget, Budget::Normal | Budget::Deep) {
        if drift_score > 0.0 {
            risk_factors.push(RiskFactor {
                signal: "drift".to_string(),
                raw_value: drift_score,
                normalized_value: drift_score,
                description: format!(
                    "Average drift score {:.2} from {} structural edges",
                    drift_score,
                    outgoing_edges.len()
                ),
            });
        }
        if co_change_partners.raw > 0 {
            risk_factors.push(RiskFactor {
                signal: "co_change".to_string(),
                raw_value: co_change_partners.raw as f64,
                normalized_value: co_change_partners.normalized,
                description: format!(
                    "{} co-change partners in recent history",
                    co_change_partners.raw
                ),
            });
        }
        if hotspot_score > 0.0 {
            risk_factors.push(RiskFactor {
                signal: "hotspot".to_string(),
                raw_value: hotspot_score,
                normalized_value: hotspot_score,
                description: "Recent changes in git history".to_string(),
            });
        }
    }

    let deep = matches!(budget, Budget::Deep);
    let affected_edge_count_field = if deep && affected_edge_count > 0 {
        Some(affected_edge_count)
    } else {
        None
    };

    let approx_tokens = estimate_tokens(&budget, &risk_factors, affected_edge_count);

    Ok(ChangeRiskCard {
        target,
        target_name,
        target_kind: target_kind.to_string(),
        risk_level,
        risk_score,
        drift_score: if deep { Some(drift_score) } else { None },
        co_change_partner_count: if deep {
            Some(co_change_partners.normalized)
        } else {
            None
        },
        hotspot_score: if deep { Some(hotspot_score) } else { None },
        risk_factors: if matches!(budget, Budget::Normal | Budget::Deep) {
            risk_factors
        } else {
            vec![]
        },
        affected_edge_count: affected_edge_count_field,
        approx_tokens,
        source_store: super::SourceStore::Graph,
    })
}

/// Compute average drift score and count of edges with drift scores.
/// Single read of the drift score table; returns (average, matched_count).
fn compute_drift_info(
    compiler: &GraphCardCompiler,
    edges: &[Edge],
    revision: &str,
) -> (f64, usize) {
    if edges.is_empty() {
        return (0.0, 0);
    }

    let all_scores = match compiler.read_drift_scores(revision) {
        Ok(scores) => scores,
        Err(_) => return (0.0, 0),
    };

    let score_map: std::collections::HashMap<_, f64> = all_scores
        .into_iter()
        .map(|(id, score)| (id, score as f64))
        .collect();

    let matched = edges
        .iter()
        .filter(|e| score_map.contains_key(&e.id))
        .count();
    if matched == 0 {
        return (0.0, 0);
    }

    let total: f64 = edges
        .iter()
        .filter_map(|e| score_map.get(&e.id).copied())
        .sum();

    (total / matched as f64, matched)
}

/// Count of co-change partners.
#[derive(Debug)]
struct CoChangeCount {
    raw: usize,
    normalized: f64,
}

impl CoChangeCount {
    fn new(raw: usize) -> Self {
        let max_threshold = 10;
        let normalized = (raw as f64 / max_threshold as f64).min(1.0);
        Self { raw, normalized }
    }
}

/// Compute co-change partner count from incoming CoChangesWith edges.
fn compute_co_change_partners(incoming: &[crate::structure::graph::Edge]) -> CoChangeCount {
    let co_changes: usize = incoming
        .iter()
        .filter(|e| e.kind == EdgeKind::CoChangesWith)
        .count();

    CoChangeCount::new(co_changes)
}

/// Compute hotspot score from git intelligence.
fn compute_hotspot_score(compiler: &GraphCardCompiler, target: &NodeId) -> crate::Result<f64> {
    let path = match target {
        NodeId::File(fid) => {
            compiler
                .reader()
                .get_file(*fid)?
                .ok_or_else(|| anyhow::anyhow!("file node not found"))?
                .path
        }
        _ => return Ok(0.0),
    };

    let insights = match compiler.resolve_file_git_intelligence(&path) {
        Some(arc) => arc,
        None => return Ok(0.0),
    };

    let hotspot = match &insights.hotspot {
        Some(h) => h,
        None => return Ok(0.0),
    };

    let touches = hotspot.touches as f64;
    Ok((touches / 10.0).min(1.0))
}

/// Estimate token count based on budget and content.
fn estimate_tokens(
    budget: &Budget,
    risk_factors: &[RiskFactor],
    affected_edge_count: usize,
) -> usize {
    match budget {
        Budget::Tiny => 50,
        Budget::Normal => 150 + risk_factors.len() * 50,
        Budget::Deep => 300 + risk_factors.len() * 50 + affected_edge_count * 20,
    }
}
