use tempfile::tempdir;
use time::OffsetDateTime;

use super::super::support::init_synrepo;
use crate::config::Config;
use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
use crate::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
    OverlayStore,
};
use crate::pipeline::repair::{
    build_repair_report, DriftClass, RepairAction, RepairSurface, Severity,
};
use crate::store::overlay::SqliteOverlayStore;
use crate::store::sqlite::SqliteGraphStore;

mod revalidation;

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
