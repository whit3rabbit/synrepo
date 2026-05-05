use tempfile::tempdir;
use time::OffsetDateTime;

use super::super::support::init_synrepo;
use crate::config::Config;
use crate::core::ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId};
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
    OverlayStore,
};
use crate::pipeline::repair::{
    build_repair_report, execute_sync, DriftClass, RepairAction, RepairSurface, Severity,
    SyncOptions,
};
use crate::store::overlay::SqliteOverlayStore;
use crate::store::sqlite::SqliteGraphStore;
use crate::structure::graph::{
    ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode,
    Visibility,
};

fn seeded_overlay_link() -> OverlayLink {
    let from = NodeId::Concept(ConceptNodeId(777));
    let to = NodeId::Symbol(SymbolNodeId(888));
    OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: "login".into(),
            verified_at_offset: 0,
            lcs_ratio: 0.95,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: "fn login".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        from_content_hash: "stored-from".into(),
        to_content_hash: "stored-to".into(),
        confidence_score: 0.9,
        confidence_tier: ConfidenceTier::High,
        rationale: None,
        provenance: CrossLinkProvenance {
            pass_id: "cross-link-v1".into(),
            model_identity: "claude-sonnet-4-6".into(),
            generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    }
}

#[test]
fn check_reports_source_deleted_when_endpoints_missing_from_graph() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    // Materialize the graph DB (empty) and overlay DB; seed a link whose
    // endpoints do not exist in the graph.
    drop(SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    overlay.insert_link(seeded_overlay_link()).unwrap();
    drop(overlay);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::ProposedLinksOverlay)
        .expect("proposed links overlay finding must be present");

    assert_eq!(finding.drift_class, DriftClass::SourceDeleted);
    assert_eq!(finding.recommended_action, RepairAction::ManualReview);
    assert_eq!(finding.severity, Severity::ReportOnly);
}

/// Seed a repo on disk + matching graph rows so the revalidation handler
/// has a real source/target to read through `load_endpoint_text`.
fn seed_revalidation_fixture(
    repo: &std::path::Path,
    synrepo_dir: &std::path::Path,
    doc_text: &str,
    code_text: &str,
    graph_from_hash: &str,
    graph_to_hash: &str,
) {
    init_synrepo(synrepo_dir);
    std::fs::create_dir_all(repo.join("docs/adr")).unwrap();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("docs/adr/0001-auth.md"), doc_text).unwrap();
    std::fs::write(repo.join("src/lib.rs"), code_text).unwrap();

    let graph_dir = synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    graph.begin().unwrap();
    let doc_prov = Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "rev".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: "parse".to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: "docs/adr/0001-auth.md".to_string(),
            content_hash: graph_from_hash.to_string(),
        }],
    };
    let code_prov = Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "rev".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: "parse".to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: "src/lib.rs".to_string(),
            content_hash: graph_to_hash.to_string(),
        }],
    };
    graph
        .upsert_file(FileNode {
            id: FileNodeId(1),
            root_id: "primary".to_string(),
            path: "docs/adr/0001-auth.md".to_string(),
            path_history: Vec::new(),
            content_hash: graph_from_hash.to_string(),
            content_sample_hashes: Vec::new(),
            size_bytes: doc_text.len() as u64,
            language: Some("markdown".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: doc_prov.clone(),
        })
        .unwrap();
    graph
        .upsert_file(FileNode {
            id: FileNodeId(10),
            root_id: "primary".to_string(),
            path: "src/lib.rs".to_string(),
            path_history: Vec::new(),
            content_hash: graph_to_hash.to_string(),
            content_sample_hashes: Vec::new(),
            size_bytes: code_text.len() as u64,
            language: Some("rust".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: code_prov.clone(),
        })
        .unwrap();
    let body_start = code_text.find("pub fn authenticate() {}").unwrap() as u32;
    let body_end = body_start + "pub fn authenticate() {}".len() as u32;
    graph
        .upsert_symbol(SymbolNode {
            id: SymbolNodeId(1),
            file_id: FileNodeId(10),
            qualified_name: "crate::authenticate".to_string(),
            display_name: "authenticate".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            body_byte_range: (body_start, body_end),
            body_hash: "body-auth".to_string(),
            signature: Some("pub fn authenticate()".to_string()),
            doc_comment: None,
            first_seen_rev: None,
            last_modified_rev: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: code_prov,
        })
        .unwrap();
    graph
        .upsert_concept(ConceptNode {
            id: ConceptNodeId(1),
            path: "docs/adr/0001-auth.md".to_string(),
            title: "Authenticate Flow".to_string(),
            aliases: vec!["authenticate".to_string()],
            summary: Some("authenticate works".to_string()),
            status: None,
            decision_body: None,
            last_observed_rev: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: doc_prov,
        })
        .unwrap();
    graph
        .insert_edge(Edge {
            id: EdgeId(1),
            from: NodeId::File(FileNodeId(10)),
            to: NodeId::Symbol(SymbolNodeId(1)),
            kind: EdgeKind::Defines,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: Provenance {
                created_at: OffsetDateTime::UNIX_EPOCH,
                source_revision: "rev".to_string(),
                created_by: CreatedBy::StructuralPipeline,
                pass: "parse".to_string(),
                source_artifacts: vec![SourceRef {
                    file_id: None,
                    path: "src/lib.rs".to_string(),
                    content_hash: graph_to_hash.to_string(),
                }],
            },
        })
        .unwrap();
    graph.commit().unwrap();
}

fn revalidation_link(from_hash: &str, to_hash: &str, source_needle: &str) -> OverlayLink {
    let from = NodeId::Concept(ConceptNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(1));
    OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: source_needle.to_string(),
            verified_at_offset: 0,
            lcs_ratio: 0.95,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: "pub fn authenticate() {}".to_string(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        from_content_hash: from_hash.to_string(),
        to_content_hash: to_hash.to_string(),
        confidence_score: 0.9,
        confidence_tier: ConfidenceTier::High,
        rationale: None,
        provenance: CrossLinkProvenance {
            pass_id: "cross-link-v1".to_string(),
            model_identity: "fake-generator".to_string(),
            generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    }
}

#[test]
fn sync_revalidates_cross_link_when_spans_still_match() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    let synrepo_dir = repo.join(".synrepo");
    let doc_text = "Authenticate flow documents how authenticate works.\n";
    let code_text = "pub fn authenticate() {}\n";
    seed_revalidation_fixture(
        repo,
        &synrepo_dir,
        doc_text,
        code_text,
        "current-doc",
        "current-code",
    );

    // Seed the candidate with STALE hashes that don't match the graph's
    // current values. The drift surface will flag this as Stale and emit a
    // RevalidateLinks finding.
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    overlay
        .insert_link(revalidation_link(
            "stale-doc-hash",
            "stale-code-hash",
            "authenticate flow documents how authenticate works",
        ))
        .unwrap();
    drop(overlay);

    let summary = execute_sync(
        repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();
    let repaired_surfaces: Vec<_> = summary.repaired.iter().map(|f| f.surface).collect();
    assert!(
        repaired_surfaces.contains(&RepairSurface::ProposedLinksOverlay),
        "revalidate must land in repaired. repaired={:?} blocked={:?} report_only={:?}",
        repaired_surfaces,
        summary
            .blocked
            .iter()
            .map(|f| (f.surface, f.notes.clone()))
            .collect::<Vec<_>>(),
        summary
            .report_only
            .iter()
            .map(|f| (f.surface, f.notes.clone()))
            .collect::<Vec<_>>(),
    );

    // Stored hashes must now be the current graph values.
    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    let hashes = overlay.cross_link_hashes().unwrap();
    let row = hashes
        .iter()
        .find(|r| r.kind == "references")
        .expect("one candidate row");
    assert_eq!(row.from_content_hash, "current-doc");
    assert_eq!(row.to_content_hash, "current-code");
}

#[test]
fn sync_reports_only_when_cited_text_no_longer_in_source() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    let synrepo_dir = repo.join(".synrepo");
    // Target file still has the authenticate symbol so seed_revalidation_fixture
    // can locate it; doc intentionally mentions different subject matter.
    let doc_text = "Unrelated note about something else entirely.\n";
    let code_text = "pub fn authenticate() {}\n";
    seed_revalidation_fixture(
        repo,
        &synrepo_dir,
        doc_text,
        code_text,
        "current-doc",
        "current-code",
    );

    // Candidate cites text that is NOT present in the current doc on disk.
    // LCS verifier should return None → finding stays in report_only.
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    overlay
        .insert_link(revalidation_link(
            "stale-doc-hash",
            "stale-code-hash",
            "this exact phrase never appears in the doc file",
        ))
        .unwrap();
    drop(overlay);

    let summary = execute_sync(
        repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();

    let report_only_notes: Vec<_> = summary
        .report_only
        .iter()
        .filter(|f| f.surface == RepairSurface::ProposedLinksOverlay)
        .filter_map(|f| f.notes.clone())
        .collect();
    assert!(
        report_only_notes
            .iter()
            .any(|n| n.contains("could not re-locate cited spans")),
        "verifier-rejection note expected in report_only, got notes={report_only_notes:?}; repaired={:?} blocked={:?}",
        summary.repaired.iter().map(|f| f.surface).collect::<Vec<_>>(),
        summary.blocked.iter().map(|f| (f.surface, f.notes.clone())).collect::<Vec<_>>(),
    );

    // Stored hashes must be unchanged.
    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    let hashes = overlay.cross_link_hashes().unwrap();
    let row = hashes
        .iter()
        .find(|r| r.kind == "references")
        .expect("one candidate row");
    assert_eq!(row.from_content_hash, "stale-doc-hash");
    assert_eq!(row.to_content_hash, "stale-code-hash");
}

#[test]
fn check_surfaces_pending_promotion_as_actionable() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    // Materialize the graph DB (empty) and overlay DB, then seed a link and
    // regress its state to pending_promotion to simulate a crash.
    drop(SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    overlay.insert_link(seeded_overlay_link()).unwrap();
    drop(overlay);

    // Simulate crash: set state to pending_promotion.
    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    conn.execute("UPDATE cross_links SET state = 'pending_promotion'", [])
        .unwrap();
    drop(conn);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| {
            f.surface == RepairSurface::ProposedLinksOverlay
                && f.notes
                    .as_ref()
                    .is_some_and(|n| n.contains("pending_promotion"))
        })
        .expect("pending_promotion finding must be present");

    assert_eq!(finding.drift_class, DriftClass::Stale);
    assert_eq!(finding.severity, Severity::Actionable);
    assert!(
        finding
            .notes
            .as_ref()
            .unwrap()
            .contains("pending_promotion"),
        "finding must mention pending_promotion: {:?}",
        finding.notes
    );
}
