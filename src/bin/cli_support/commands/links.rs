use std::path::Path;

use synrepo::{
    config::{Config, Mode},
    core::ids::NodeId,
    overlay::{OverlayEdgeKind, OverlayStore},
    pipeline::writer::{acquire_writer_lock, LockError},
    store::overlay::{
        candidate_pass_suffix, compare_score_desc, format_candidate_id, parse_cross_link_freshness,
        parse_overlay_edge_kind, FindingsFilter, SqliteOverlayStore,
    },
    store::sqlite::SqliteGraphStore,
    structure::graph::Epistemic,
};

use super::watch::ensure_watch_not_running;

/// A candidate ID parsed into its endpoint triple plus optional revision suffix.
/// `pass_suffix` is `None` for the legacy 3-part form (`from::to::kind`) emitted
/// before revision binding landed; the 4-part form is required for new scripts.
struct ParsedCandidateId<'a> {
    raw: &'a str,
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    pass_suffix: Option<String>,
}

fn parse_candidate_id(id: &str) -> anyhow::Result<ParsedCandidateId<'_>> {
    let parts: Vec<&str> = id.split("::").collect();
    if !matches!(parts.len(), 3 | 4) {
        anyhow::bail!("Invalid candidate ID format. Expected <from>::<to>::<kind>::<pass_suffix>");
    }
    let from = std::str::FromStr::from_str(parts[0])
        .map_err(|error| anyhow::anyhow!("Invalid from_node: {error}"))?;
    let to = std::str::FromStr::from_str(parts[1])
        .map_err(|error| anyhow::anyhow!("Invalid to_node: {error}"))?;
    let kind = parse_overlay_edge_kind(parts[2])
        .map_err(|error| anyhow::anyhow!("Invalid edge kind: {error}"))?;
    let pass_suffix = parts.get(3).map(|s| (*s).to_string());
    Ok(ParsedCandidateId {
        raw: id,
        from,
        to,
        kind,
        pass_suffix,
    })
}

pub(crate) fn links_list(
    repo_root: &Path,
    tier: Option<&str>,
    json_output: bool,
) -> anyhow::Result<()> {
    print!("{}", links_list_output(repo_root, tier, json_output)?);
    Ok(())
}

pub(crate) fn links_list_output(
    repo_root: &Path,
    tier: Option<&str>,
    json_output: bool,
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let synrepo_dir = Config::synrepo_dir(repo_root);
    let overlay_dir = synrepo_dir.join("overlay");
    let overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    let candidates = overlay.all_candidates(tier)?;
    let mut out = String::new();
    if json_output {
        writeln!(out, "{}", serde_json::to_string_pretty(&candidates)?).unwrap();
        return Ok(out);
    }

    writeln!(out, "Found {} candidates.", candidates.len()).unwrap();
    for candidate in candidates {
        writeln!(
            out,
            "{}  tier: {}  score: {:.3}",
            format_candidate_id(
                candidate.from,
                candidate.to,
                candidate.kind,
                &candidate.provenance.pass_id,
            ),
            candidate.confidence_tier.as_str(),
            candidate.confidence_score
        )
        .unwrap();
    }
    Ok(out)
}

pub(crate) fn links_review(
    repo_root: &Path,
    limit: Option<usize>,
    json_output: bool,
) -> anyhow::Result<()> {
    print!("{}", links_review_output(repo_root, limit, json_output)?);
    Ok(())
}

pub(crate) fn links_review_output(
    repo_root: &Path,
    limit: Option<usize>,
    json_output: bool,
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let synrepo_dir = Config::synrepo_dir(repo_root);
    let overlay_dir = synrepo_dir.join("overlay");
    let overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    let mut candidates = overlay.all_candidates(Some("review_queue"))?;
    candidates
        .sort_by(|left, right| compare_score_desc(left.confidence_score, right.confidence_score));

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    let mut out = String::new();
    if json_output {
        writeln!(out, "{}", serde_json::to_string_pretty(&candidates)?).unwrap();
        return Ok(out);
    }

    writeln!(out, "Review queue: {} candidates.", candidates.len()).unwrap();
    for candidate in candidates {
        writeln!(
            out,
            "Candidate: {}",
            format_candidate_id(
                candidate.from,
                candidate.to,
                candidate.kind,
                &candidate.provenance.pass_id,
            )
        )
        .unwrap();
        writeln!(out, "  Score: {:.3}", candidate.confidence_score).unwrap();
        if let Some(rationale) = &candidate.rationale {
            writeln!(out, "  Rationale: {rationale}").unwrap();
        }
    }
    Ok(out)
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

    let _writer_lock = acquire_writer_lock(&synrepo_dir).map_err(|err| match err {
        LockError::HeldByOther { pid, .. } => anyhow::anyhow!(
            "links accept: writer lock held by pid {pid}; wait for it to finish or stop the watch daemon"
        ),
        LockError::Io { path, source } => anyhow::anyhow!(
            "links accept: could not acquire writer lock at {}: {source}",
            path.display()
        ),
    })?;

    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    let parsed = parse_candidate_id(candidate_id)?;
    let reviewer = reviewer.unwrap_or("cli-user");
    ensure_candidate_exists(&overlay, &parsed)?;
    let ParsedCandidateId { from, to, kind, .. } = parsed;

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
        owner_file_id: None,
        last_observed_rev: None,
        retired_at_rev: None,
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

    // Compensation only: a crash between `insert_edge` and
    // `mark_candidate_promoted` still leaves the overlay candidate `active`.
    // Full two-phase safety requires a `pending_promotion` state, deferred.
    if let Err(overlay_err) =
        overlay.mark_candidate_promoted(from, to, kind, reviewer, &edge_id.to_string())
    {
        match graph.delete_edge(edge_id) {
            Ok(()) => anyhow::bail!(
                "links accept failed: overlay write failed ({overlay_err}); graph edge was rolled back"
            ),
            Err(delete_err) => anyhow::bail!(
                "links accept failed and stores may be inconsistent: overlay write failed ({overlay_err}); graph edge {edge_id} could not be rolled back ({delete_err}). Inspect `.synrepo/` manually."
            ),
        }
    }
    println!("Candidate {candidate_id} accepted and written to graph.");
    Ok(())
}

fn ensure_candidate_exists(
    overlay: &SqliteOverlayStore,
    parsed: &ParsedCandidateId<'_>,
) -> anyhow::Result<()> {
    let matched = overlay
        .links_for(parsed.from)?
        .into_iter()
        .find(|candidate| candidate.to == parsed.to && candidate.kind == parsed.kind);
    let Some(candidate) = matched else {
        anyhow::bail!("Candidate not found: {}", parsed.raw);
    };

    match parsed.pass_suffix.as_deref() {
        Some(expected) => {
            let actual = candidate_pass_suffix(&candidate.provenance.pass_id);
            if actual != expected {
                anyhow::bail!(
                    "Stale review: candidate {} was regenerated (stored pass suffix `{actual}`, reviewed `{expected}`). Re-run `synrepo links review` and accept the current revision.",
                    parsed.raw
                );
            }
        }
        None => {
            eprintln!(
                "warning: candidate ID `{}` is in the legacy 3-part form. Future versions will require the 4-part form (`from::to::kind::pass_suffix`). Re-run `synrepo links review` for the current ID.",
                parsed.raw
            );
        }
    }
    Ok(())
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

    let _writer_lock = acquire_writer_lock(&synrepo_dir).map_err(|err| match err {
        LockError::HeldByOther { pid, .. } => anyhow::anyhow!(
            "links reject: writer lock held by pid {pid}; wait for it to finish or stop the watch daemon"
        ),
        LockError::Io { path, source } => anyhow::anyhow!(
            "links reject: could not acquire writer lock at {}: {source}",
            path.display()
        ),
    })?;

    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    let parsed = parse_candidate_id(candidate_id)?;
    let reviewer = reviewer.unwrap_or("cli-user");
    overlay.mark_candidate_rejected(parsed.from, parsed.to, parsed.kind, reviewer)?;

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
    print!(
        "{}",
        findings_output(repo_root, node, kind, freshness, limit, json_output)?
    );
    Ok(())
}

pub(crate) fn findings_output(
    repo_root: &Path,
    node: Option<&str>,
    kind: Option<&str>,
    freshness: Option<&str>,
    limit: Option<usize>,
    json_output: bool,
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

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

    let mut out = String::new();
    if json_output {
        writeln!(out, "{}", serde_json::to_string_pretty(&findings)?).unwrap();
        return Ok(out);
    }

    writeln!(out, "Found {} findings.", findings.len()).unwrap();
    for finding in findings {
        writeln!(
            out,
            "{} [Tier: {}] [Freshness: {}] [Score: {:.3}]",
            finding.candidate_id,
            finding.tier.as_str(),
            finding.freshness.as_str(),
            finding.score
        )
        .unwrap();
    }
    Ok(out)
}
