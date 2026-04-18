use std::path::Path;

#[path = "links/accept.rs"]
mod accept;

pub(crate) use accept::links_accept;
use accept::parse_candidate_id;
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use accept::{links_accept_commit, CommitArgs, LinksCommitStore, RealLinksStore};
use synrepo::{
    config::{Config, Mode},
    pipeline::writer::{acquire_write_admission, map_lock_error},
    store::overlay::{
        format_candidate_id, parse_cross_link_freshness, parse_overlay_edge_kind, FindingsFilter,
        SqliteOverlayStore,
    },
    store::sqlite::SqliteGraphStore,
};

/// Default row cap applied to `links list` when `--limit` is not supplied.
/// Pass `--limit 0` on the CLI to disable the cap and load every candidate.
const LINKS_LIST_DEFAULT_LIMIT: usize = 50;

pub(crate) fn links_list(
    repo_root: &Path,
    tier: Option<&str>,
    limit: Option<usize>,
    json_output: bool,
) -> anyhow::Result<()> {
    print!(
        "{}",
        links_list_output(repo_root, tier, limit, json_output)?
    );
    Ok(())
}

pub(crate) fn links_list_output(
    repo_root: &Path,
    tier: Option<&str>,
    limit: Option<usize>,
    json_output: bool,
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let synrepo_dir = Config::synrepo_dir(repo_root);
    let overlay_dir = synrepo_dir.join("overlay");
    let overlay = SqliteOverlayStore::open_existing(&overlay_dir)
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))?;

    // limit resolution:
    //   None    → cap at LINKS_LIST_DEFAULT_LIMIT (push LIMIT into SQL)
    //   Some(0) → opt out of the cap, load every active candidate
    //   Some(n) → cap at n
    let (candidates, applied_cap) = match limit {
        None => (
            overlay.candidates_limited(tier, LINKS_LIST_DEFAULT_LIMIT)?,
            Some(LINKS_LIST_DEFAULT_LIMIT),
        ),
        Some(0) => (overlay.all_candidates(tier)?, None),
        Some(n) => (overlay.candidates_limited(tier, n)?, Some(n)),
    };
    let mut out = String::new();
    if json_output {
        writeln!(out, "{}", serde_json::to_string_pretty(&candidates)?).unwrap();
        return Ok(out);
    }

    match applied_cap {
        Some(cap) if candidates.len() == cap => writeln!(
            out,
            "Showing {} candidates (capped at {cap}; pass --limit 0 for all).",
            candidates.len()
        )
        .unwrap(),
        _ => writeln!(out, "Found {} candidates.", candidates.len()).unwrap(),
    }
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

    let candidates = match limit {
        Some(limit) => overlay.candidates_limited(Some("review_queue"), limit)?,
        None => overlay.all_candidates(Some("review_queue"))?,
    };

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

    let _writer_lock = acquire_write_admission(&synrepo_dir, "links reject")
        .map_err(|err| map_lock_error("links reject", err))?;

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
