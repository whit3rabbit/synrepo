use std::path::Path;

use synrepo::{
    config::{Config, Mode},
    core::ids::NodeId,
    overlay::{OverlayEdgeKind, OverlayStore},
    store::overlay::{
        format_candidate_id, parse_cross_link_freshness, parse_overlay_edge_kind, FindingsFilter,
        SqliteOverlayStore,
    },
    store::sqlite::SqliteGraphStore,
    structure::graph::Epistemic,
};

use super::watch::ensure_watch_not_running;

fn parse_candidate_id(id: &str) -> anyhow::Result<(NodeId, NodeId, OverlayEdgeKind)> {
    let parts: Vec<&str> = id.split("::").collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid candidate ID format. Expected <from>::<to>::<kind>");
    }
    let from = std::str::FromStr::from_str(parts[0])
        .map_err(|error| anyhow::anyhow!("Invalid from_node: {error}"))?;
    let to = std::str::FromStr::from_str(parts[1])
        .map_err(|error| anyhow::anyhow!("Invalid to_node: {error}"))?;
    let kind = parse_overlay_edge_kind(parts[2])
        .map_err(|error| anyhow::anyhow!("Invalid edge kind: {error}"))?;
    Ok((from, to, kind))
}

pub(crate) fn links_list(
    repo_root: &Path,
    tier: Option<&str>,
    json_output: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let overlay_dir = synrepo_dir.join("overlay");
    let overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    let candidates = overlay.all_candidates(tier)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&candidates)?);
        return Ok(());
    }

    println!("Found {} candidates.", candidates.len());
    for candidate in candidates {
        println!(
            "{}  tier: {}  score: {:.3}",
            format_candidate_id(candidate.from, candidate.to, candidate.kind),
            candidate.confidence_tier.as_str(),
            candidate.confidence_score
        );
    }
    Ok(())
}

pub(crate) fn links_review(
    repo_root: &Path,
    limit: Option<usize>,
    json_output: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let overlay_dir = synrepo_dir.join("overlay");
    let overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    let mut candidates = overlay.all_candidates(Some("review_queue"))?;
    candidates.sort_by(|left, right| {
        right
            .confidence_score
            .partial_cmp(&left.confidence_score)
            .unwrap()
    });

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&candidates)?);
        return Ok(());
    }

    println!("Review queue: {} candidates.", candidates.len());
    for candidate in candidates {
        println!(
            "Candidate: {}",
            format_candidate_id(candidate.from, candidate.to, candidate.kind)
        );
        println!("  Score: {:.3}", candidate.confidence_score);
        if let Some(rationale) = &candidate.rationale {
            println!("  Rationale: {rationale}");
        }
    }
    Ok(())
}

pub(crate) fn links_accept(
    repo_root: &Path,
    candidate_id: &str,
    reviewer: Option<&str>,
) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    if config.mode != Mode::Curated {
        anyhow::bail!("Rejecting: `links accept` is only available in `curated` mode.");
    }
    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "links accept")?;

    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    let (from, to, kind) = parse_candidate_id(candidate_id)?;
    let reviewer = reviewer.unwrap_or("cli-user");
    ensure_candidate_exists(&overlay, from, to, kind, candidate_id)?;

    let graph_dir = synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open_existing(&graph_dir)?;

    let edge_kind = match kind {
        OverlayEdgeKind::References => synrepo::structure::graph::EdgeKind::References,
        OverlayEdgeKind::Governs => synrepo::structure::graph::EdgeKind::Governs,
        OverlayEdgeKind::DerivedFrom => synrepo::structure::graph::EdgeKind::References,
        OverlayEdgeKind::Mentions => synrepo::structure::graph::EdgeKind::Mentions,
    };

    use synrepo::structure::graph::{Edge, GraphStore};
    let edge_id = synrepo::pipeline::structural::derive_edge_id(from, to, edge_kind);
    graph.insert_edge(Edge {
        id: edge_id,
        from,
        to,
        kind: edge_kind,
        epistemic: Epistemic::HumanDeclared,
        drift_score: 0.0,
        provenance: synrepo::core::provenance::Provenance {
            created_at: time::OffsetDateTime::now_utc(),
            source_revision: "curated_workflow".to_string(),
            created_by: synrepo::core::provenance::CreatedBy::Human,
            pass: format!("links_accept:{reviewer}"),
            source_artifacts: vec![],
        },
    })?;

    overlay.mark_candidate_promoted(from, to, kind, reviewer, &edge_id.to_string())?;
    println!("Candidate {candidate_id} accepted and written to graph.");
    Ok(())
}

fn ensure_candidate_exists(
    overlay: &SqliteOverlayStore,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    candidate_id: &str,
) -> anyhow::Result<()> {
    let exists = overlay
        .links_for(from)?
        .into_iter()
        .any(|candidate| candidate.to == to && candidate.kind == kind);
    if exists {
        return Ok(());
    }

    anyhow::bail!("Candidate not found: {candidate_id}");
}

pub(crate) fn links_reject(
    repo_root: &Path,
    candidate_id: &str,
    reviewer: Option<&str>,
) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    if config.mode != Mode::Curated {
        anyhow::bail!("Rejecting: `links reject` is only available in `curated` mode.");
    }
    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "links reject")?;

    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    let (from, to, kind) = parse_candidate_id(candidate_id)?;
    let reviewer = reviewer.unwrap_or("cli-user");
    overlay.mark_candidate_rejected(from, to, kind, reviewer)?;

    println!("Candidate {candidate_id} rejected.");
    Ok(())
}

pub(crate) fn findings(
    repo_root: &Path,
    node: Option<&str>,
    kind: Option<&str>,
    freshness: Option<&str>,
    limit: Option<usize>,
    json_output: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let overlay_dir = synrepo_dir.join("overlay");
    let graph_dir = synrepo_dir.join("graph");
    let overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;
    let graph = SqliteGraphStore::open_existing(&graph_dir)
        .map_err(|error| anyhow::anyhow!("Could not open graph store: {error}"))?;

    let node_id = node
        .map(std::str::FromStr::from_str)
        .transpose()
        .map_err(|error| anyhow::anyhow!("Invalid node id: {error}"))?;
    let kind = kind
        .map(parse_overlay_edge_kind)
        .transpose()
        .map_err(|error| anyhow::anyhow!("Invalid edge kind: {error}"))?;
    let freshness = freshness
        .map(parse_cross_link_freshness)
        .transpose()
        .map_err(|error| anyhow::anyhow!("Invalid freshness state: {error}"))?;

    let findings = overlay.findings(
        &graph,
        &FindingsFilter {
            node_id,
            kind,
            freshness,
            limit,
        },
    )?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&findings)?);
        return Ok(());
    }

    println!("Found {} findings.", findings.len());
    for finding in findings {
        println!(
            "{} [Tier: {}] [Freshness: {}] [Score: {:.3}]",
            finding.candidate_id,
            finding.tier.as_str(),
            finding.freshness.as_str(),
            finding.score
        );
    }
    Ok(())
}
