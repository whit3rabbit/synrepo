use std::fs;

use crate::{
    overlay::{
        CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic,
        OverlayLink, OverlayStore,
    },
    pipeline::{
        structural::CompileSummary,
        watch::{
            load_reconcile_state, persist_reconcile_state, run_reconcile_pass, ReconcileOutcome,
        },
    },
    store::overlay::SqliteOverlayStore,
};
use time::OffsetDateTime;

use super::setup_test_repo;

#[test]
fn reconcile_pass_completes_on_valid_repo() {
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();

    let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(matches!(outcome, ReconcileOutcome::Completed(_)));
    if let ReconcileOutcome::Completed(summary) = outcome {
        assert!(summary.files_discovered >= 1);
        assert!(summary.symbols_extracted >= 1);
    }
}

#[test]
fn reconcile_pass_returns_lock_conflict_when_lock_is_held() {
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let _lock = crate::pipeline::writer::acquire_writer_lock(&synrepo_dir).unwrap();

    let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(matches!(outcome, ReconcileOutcome::LockConflict { .. }));
}

#[test]
fn reconcile_pass_corrects_stale_graph_state() {
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let first = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(matches!(first, ReconcileOutcome::Completed(_)));

    fs::write(repo.join("src/new.rs"), "pub fn new_fn() {}\n").unwrap();
    let second = run_reconcile_pass(&repo, &config, &synrepo_dir);
    if let ReconcileOutcome::Completed(summary) = second {
        assert!(summary.files_discovered >= 2);
    } else {
        panic!("expected Completed after adding new file");
    }
}

#[test]
fn persist_and_load_reconcile_state_roundtrip() {
    let synrepo_dir = tempfile::tempdir().unwrap().path().join(".synrepo");
    let summary = CompileSummary {
        files_discovered: 5,
        symbols_extracted: 12,
        ..CompileSummary::default()
    };
    persist_reconcile_state(&synrepo_dir, &ReconcileOutcome::Completed(summary), 3);

    let state = load_reconcile_state(&synrepo_dir).unwrap();
    assert_eq!(state.last_outcome, "completed");
    assert_eq!(state.files_discovered, Some(5));
    assert_eq!(state.symbols_extracted, Some(12));
    assert_eq!(state.triggering_events, 3);
}

#[test]
fn persist_reconcile_state_records_failure_message() {
    let synrepo_dir = tempfile::tempdir().unwrap().path().join(".synrepo");
    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("disk full".to_string()),
        0,
    );

    let state = load_reconcile_state(&synrepo_dir).unwrap();
    assert_eq!(state.last_outcome, "failed");
    assert_eq!(state.last_error.as_deref(), Some("disk full"));
}

#[test]
fn reconcile_prunes_cross_link_orphans() {
    use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};

    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let first = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(matches!(first, ReconcileOutcome::Completed(_)));

    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ConceptNodeId(9_999));
    let to = NodeId::Symbol(SymbolNodeId(9_998));
    overlay
        .insert_link(OverlayLink {
            from,
            to,
            kind: OverlayEdgeKind::References,
            epistemic: OverlayEpistemic::MachineAuthoredHighConf,
            source_spans: vec![CitedSpan {
                artifact: from,
                normalized_text: "gone".into(),
                verified_at_offset: 0,
                lcs_ratio: 0.9,
            }],
            target_spans: vec![CitedSpan {
                artifact: to,
                normalized_text: "fn gone".into(),
                verified_at_offset: 0,
                lcs_ratio: 1.0,
            }],
            from_content_hash: "hf".into(),
            to_content_hash: "ht".into(),
            confidence_score: 0.9,
            confidence_tier: ConfidenceTier::High,
            rationale: None,
            provenance: CrossLinkProvenance {
                pass_id: "cross-link-v1".into(),
                model_identity: "claude-sonnet-4-6".into(),
                generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
            },
        })
        .unwrap();
    drop(overlay);

    let second = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(matches!(second, ReconcileOutcome::Completed(_)));

    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    assert_eq!(overlay.cross_link_count().unwrap(), 0);
    let audit = overlay
        .cross_link_audit_events(&from.to_string(), &to.to_string(), "references")
        .unwrap();
    assert!(audit.iter().any(|event| event.event_kind == "pruned"));
}

#[test]
fn persist_reconcile_state_records_lock_conflict() {
    let synrepo_dir = tempfile::tempdir().unwrap().path().join(".synrepo");
    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::LockConflict { holder_pid: 42 },
        1,
    );

    let state = load_reconcile_state(&synrepo_dir).unwrap();
    assert_eq!(state.last_outcome, "lock-conflict");
    assert_eq!(state.triggering_events, 1);
}
