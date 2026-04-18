use std::path::Path;

use synrepo::{
    config::{Config, Mode},
    core::ids::{EdgeId, NodeId},
    overlay::{OverlayEdgeKind, OverlayStore},
    pipeline::writer::{acquire_write_admission, map_lock_error},
    store::overlay::{candidate_pass_suffix, parse_overlay_edge_kind, SqliteOverlayStore},
    store::sqlite::SqliteGraphStore,
    structure::graph::{Edge, Epistemic, GraphStore},
};

/// Narrow surface so fault-injection tests can inject failures with a
/// 4-method wrapper instead of a full `GraphStore` / `OverlayStore` mock.
pub(crate) trait LinksCommitStore {
    fn mark_pending(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> anyhow::Result<()>;

    fn insert_edge(&mut self, edge: Edge) -> anyhow::Result<()>;

    fn mark_promoted(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
        edge_id: &str,
    ) -> anyhow::Result<()>;

    fn delete_edge(&mut self, edge_id: EdgeId) -> anyhow::Result<()>;
}

pub(crate) struct RealLinksStore<'a> {
    pub graph: &'a mut SqliteGraphStore,
    pub overlay: &'a mut SqliteOverlayStore,
}

impl LinksCommitStore for RealLinksStore<'_> {
    fn mark_pending(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> anyhow::Result<()> {
        self.overlay
            .mark_candidate_pending(from, to, kind, reviewer)
            .map_err(Into::into)
    }

    fn insert_edge(&mut self, edge: Edge) -> anyhow::Result<()> {
        self.graph.insert_edge(edge).map_err(Into::into)
    }

    fn mark_promoted(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
        edge_id: &str,
    ) -> anyhow::Result<()> {
        self.overlay
            .mark_candidate_promoted(from, to, kind, reviewer, edge_id)
            .map_err(Into::into)
    }

    fn delete_edge(&mut self, edge_id: EdgeId) -> anyhow::Result<()> {
        self.graph.delete_edge(edge_id).map_err(Into::into)
    }
}

pub(crate) struct CommitArgs<'a> {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: OverlayEdgeKind,
    pub edge_kind: synrepo::structure::graph::EdgeKind,
    pub edge_id: EdgeId,
    pub reviewer: &'a str,
}

/// A candidate ID parsed into its endpoint triple plus optional revision suffix.
/// `pass_suffix` is `None` for the legacy 3-part form (`from::to::kind`) emitted
/// before revision binding landed; the 4-part form is required for new scripts.
pub(super) struct ParsedCandidateId<'a> {
    pub(super) raw: &'a str,
    pub(super) from: NodeId,
    pub(super) to: NodeId,
    pub(super) kind: OverlayEdgeKind,
    pub(super) pass_suffix: Option<String>,
}

pub(super) fn parse_candidate_id(id: &str) -> anyhow::Result<ParsedCandidateId<'_>> {
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

/// Phase 1 (overlay pending) → Phase 2 (graph edge) → Phase 3 (overlay
/// promoted). On Phase 3 failure, compensate by deleting the graph edge;
/// surface the original overlay error verbatim so callers see the root cause,
/// not the compensation path.
pub(crate) fn links_accept_commit(
    store: &mut dyn LinksCommitStore,
    args: &CommitArgs<'_>,
) -> anyhow::Result<()> {
    store.mark_pending(args.from, args.to, args.kind, args.reviewer)?;
    maybe_abort_test_crash("links_accept:after_pending");

    store.insert_edge(build_curated_edge(args))?;
    maybe_abort_test_crash("links_accept:after_graph_insert");

    if let Err(overlay_err) = store.mark_promoted(
        args.from,
        args.to,
        args.kind,
        args.reviewer,
        &args.edge_id.to_string(),
    ) {
        if let Err(compensation_err) = store.delete_edge(args.edge_id) {
            tracing::error!(
                overlay_err = %overlay_err,
                compensation_err = %compensation_err,
                "links accept: overlay finalize failed AND graph compensation failed; overlay and graph are inconsistent"
            );
        }
        return Err(anyhow::anyhow!(
            "overlay finalize failed after graph insert: {overlay_err}"
        ));
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

    let _writer_lock = acquire_write_admission(&synrepo_dir, "links accept")
        .map_err(|err| map_lock_error("links accept", err))?;

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
    let edge_id = synrepo::pipeline::structural::derive_edge_id(from, to, edge_kind);

    let matched = overlay
        .links_for(from)?
        .into_iter()
        .find(|candidate| candidate.to == to && candidate.kind == kind);

    if matched.is_some() {
        let conn = rusqlite::Connection::open(SqliteOverlayStore::db_path(&overlay_dir))?;
        let state_str: String = conn.query_row(
            "SELECT state FROM cross_links WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
            [
                from.to_string(),
                to.to_string(),
                overlay_edge_kind_as_str(kind).to_string(),
            ],
            |row| row.get(0),
        )?;
        if state_str == "promoted" {
            println!("Candidate {candidate_id} is already promoted.");
            return Ok(());
        }
        if state_str == "pending_promotion" {
            let edge_exists = graph
                .outbound(from, Some(edge_kind))
                .map_err(|e| anyhow::anyhow!("graph read failed during promotion recovery: {e}"))?
                .iter()
                .any(|e| e.to == to);
            if edge_exists {
                overlay.mark_candidate_promoted(from, to, kind, reviewer, &edge_id.to_string())?;
                println!("Candidate {candidate_id} promotion completed (crash recovery).");
                return Ok(());
            }
        }
    }

    let mut store = RealLinksStore {
        graph: &mut graph,
        overlay: &mut overlay,
    };
    links_accept_commit(
        &mut store,
        &CommitArgs {
            from,
            to,
            kind,
            edge_kind,
            edge_id,
            reviewer,
        },
    )?;

    println!("Candidate {candidate_id} accepted and written to graph.");
    Ok(())
}

fn build_curated_edge(args: &CommitArgs<'_>) -> Edge {
    Edge {
        id: args.edge_id,
        from: args.from,
        to: args.to,
        kind: args.edge_kind,
        owner_file_id: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::HumanDeclared,
        drift_score: 0.0,
        provenance: synrepo::core::provenance::Provenance {
            created_at: time::OffsetDateTime::now_utc(),
            source_revision: "curated_workflow".to_string(),
            created_by: synrepo::core::provenance::CreatedBy::Human,
            pass: format!("links_accept:{}", args.reviewer),
            source_artifacts: vec![],
        },
    }
}

fn overlay_edge_kind_as_str(kind: OverlayEdgeKind) -> &'static str {
    match kind {
        OverlayEdgeKind::References => "references",
        OverlayEdgeKind::Governs => "governs",
        OverlayEdgeKind::DerivedFrom => "derived_from",
        OverlayEdgeKind::Mentions => "mentions",
    }
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

fn maybe_abort_test_crash(point: &str) {
    #[cfg(debug_assertions)]
    {
        if std::env::var("SYNREPO_TEST_CRASH_AT")
            .ok()
            .as_deref()
            .is_some_and(|value| value == point)
        {
            std::process::abort();
        }
    }
}
