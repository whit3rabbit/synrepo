use synrepo::bootstrap::bootstrap;
use synrepo::config::{Config, Mode};
use synrepo::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
use synrepo::core::provenance::CreatedBy;
use synrepo::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
    OverlayStore,
};
use synrepo::store::overlay::{format_candidate_id, FindingsFilter, SqliteOverlayStore};
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{EdgeKind, Epistemic, GraphStore};
use tempfile::tempdir;
use time::OffsetDateTime;

use super::support::seed_graph;

fn sample_link(from: NodeId, to: NodeId) -> OverlayLink {
    OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: "source".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: "target".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        from_content_hash: "h1".into(),
        to_content_hash: "h2".into(),
        confidence_score: 0.95,
        confidence_tier: ConfidenceTier::High,
        rationale: Some("Test rationale".into()),
        provenance: CrossLinkProvenance {
            pass_id: "test-pass".into(),
            model_identity: "test-model".into(),
            generated_at: OffsetDateTime::now_utc(),
        },
    }
}

#[test]
fn links_list_outputs_candidates() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(2));
    overlay.insert_link(sample_link(from, to)).unwrap();

    // Just verify it doesn't crash for now, as we don't have easy stdout capture in these unit tests
    // without more boilerplate (usually handled by integration tests or macros).
    super::super::commands::links_list(repo.path(), None, false).unwrap();
}

#[test]
fn links_accept_blocked_in_auto_mode() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();

    let err = super::super::commands::links_accept(
        repo.path(),
        "concept_0000000000000001::sym_0000000000000002::references",
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("only available in `curated` mode"));
}

#[test]
fn links_reject_blocked_in_auto_mode() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();

    let err = super::super::commands::links_reject(
        repo.path(),
        "concept_0000000000000001::sym_0000000000000002::references",
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("only available in `curated` mode"));
}

#[test]
fn findings_obays_limit() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to1 = NodeId::Symbol(SymbolNodeId(2));
    let to2 = NodeId::Symbol(SymbolNodeId(3));

    overlay.insert_link(sample_link(from, to1)).unwrap();
    overlay.insert_link(sample_link(from, to2)).unwrap();

    // Verify limit works (implicitly by not crashing and covering the branches)
    super::super::commands::findings(repo.path(), None, None, None, Some(1), false).unwrap();
}

#[test]
fn links_accept_writes_human_declared_edge() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        "mode = \"curated\"\n",
    )
    .unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    overlay.insert_link(sample_link(from, to)).unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References);
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edges = graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .into_iter()
        .filter(|edge| edge.to == to)
        .collect::<Vec<_>>();

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].epistemic, Epistemic::HumanDeclared);
    assert_eq!(edges[0].provenance.created_by, CreatedBy::Human);
    assert_eq!(edges[0].provenance.source_revision, "curated_workflow");
    assert_eq!(edges[0].provenance.pass, "links_accept:reviewer-a");

    let audit = overlay
        .cross_link_audit_events(&from.to_string(), &to.to_string(), "references")
        .unwrap();
    assert!(audit.iter().any(|row| row.event_kind == "promoted"));
}

#[test]
fn links_reject_updates_candidate_state() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        "mode = \"curated\"\n",
    )
    .unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    overlay.insert_link(sample_link(from, to)).unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References);
    super::super::commands::links_reject(repo.path(), &candidate_id, Some("reviewer-b")).unwrap();

    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    let (state, reviewer): (String, Option<String>) = conn
        .query_row(
            "SELECT state, reviewer FROM cross_links WHERE from_node = ?1 AND to_node = ?2 AND kind = 'references'",
            [from.to_string(), to.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "rejected");
    assert_eq!(reviewer.as_deref(), Some("reviewer-b"));
}

#[test]
fn findings_returns_below_threshold_candidates() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();

    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    let mut link = sample_link(from, to);
    link.confidence_tier = ConfidenceTier::BelowThreshold;
    link.confidence_score = 0.42;
    link.from_content_hash = "hash".into();
    link.to_content_hash = "abc123".into();
    overlay.insert_link(link).unwrap();

    super::super::commands::findings(repo.path(), None, None, None, Some(10), false).unwrap();

    let findings = overlay
        .findings(&graph, &FindingsFilter::default())
        .unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].tier, ConfidenceTier::BelowThreshold);
}
