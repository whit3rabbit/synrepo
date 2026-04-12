use super::*;
use crate::store::compatibility::write_runtime_snapshot;
use std::fs;
use tempfile::tempdir;

fn setup_test_repo(dir: &tempfile::TempDir) -> (PathBuf, Config, PathBuf) {
    let repo = dir.path().to_path_buf();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
    let synrepo_dir = repo.join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    write_runtime_snapshot(&synrepo_dir, &Config::default()).unwrap();
    (repo, Config::default(), synrepo_dir)
}

#[test]
fn reconcile_pass_completes_on_valid_repo() {
    let dir = tempdir().unwrap();
    let (repo, config, synrepo_dir) = setup_test_repo(&dir);

    let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(
        matches!(outcome, ReconcileOutcome::Completed(_)),
        "expected Completed, got {}",
        outcome.as_str(),
    );

    if let ReconcileOutcome::Completed(ref summary) = outcome {
        assert!(summary.files_discovered >= 1, "must discover src/lib.rs");
        assert!(summary.symbols_extracted >= 1, "must extract hello()");
    }
}

#[test]
fn reconcile_pass_returns_lock_conflict_when_lock_is_held() {
    let dir = tempdir().unwrap();
    let (repo, config, synrepo_dir) = setup_test_repo(&dir);

    let _lock = crate::pipeline::writer::acquire_writer_lock(&synrepo_dir).unwrap();

    let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(
        matches!(outcome, ReconcileOutcome::LockConflict { .. }),
        "expected LockConflict, got {}",
        outcome.as_str(),
    );
}

#[test]
fn reconcile_pass_corrects_stale_graph_state() {
    let dir = tempdir().unwrap();
    let (repo, config, synrepo_dir) = setup_test_repo(&dir);

    let first = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(matches!(first, ReconcileOutcome::Completed(_)));

    fs::write(repo.join("src/new.rs"), "pub fn new_fn() {}\n").unwrap();

    let second = run_reconcile_pass(&repo, &config, &synrepo_dir);
    if let ReconcileOutcome::Completed(summary) = second {
        assert!(
            summary.files_discovered >= 2,
            "new file must be discovered on reconcile fallback"
        );
    } else {
        panic!("expected Completed after adding new file");
    }
}

#[test]
fn persist_and_load_reconcile_state_roundtrip() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let summary = CompileSummary {
        files_discovered: 5,
        symbols_extracted: 12,
        ..CompileSummary::default()
    };
    let outcome = ReconcileOutcome::Completed(summary);
    persist_reconcile_state(&synrepo_dir, &outcome, 3);

    let state = load_reconcile_state(&synrepo_dir).unwrap();
    assert_eq!(state.last_outcome, "completed");
    assert_eq!(state.files_discovered, Some(5));
    assert_eq!(state.symbols_extracted, Some(12));
    assert_eq!(state.triggering_events, 3);
    assert!(state.last_error.is_none());
}

#[test]
fn persist_reconcile_state_records_failure_message() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let outcome = ReconcileOutcome::Failed("disk full".to_string());
    persist_reconcile_state(&synrepo_dir, &outcome, 0);

    let state = load_reconcile_state(&synrepo_dir).unwrap();
    assert_eq!(state.last_outcome, "failed");
    assert_eq!(state.last_error.as_deref(), Some("disk full"));
    assert!(state.files_discovered.is_none());
}

#[test]
fn reconcile_prunes_cross_link_orphans() {
    use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
    use crate::overlay::{
        CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic,
        OverlayLink, OverlayStore,
    };
    use crate::store::overlay::SqliteOverlayStore;
    use time::OffsetDateTime;

    let dir = tempdir().unwrap();
    let (repo, config, synrepo_dir) = setup_test_repo(&dir);

    // First reconcile populates the graph with src/lib.rs and its hello() symbol.
    let first = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(matches!(first, ReconcileOutcome::Completed(_)));

    // Seed an overlay cross-link whose endpoints do not exist in the graph.
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
    assert_eq!(overlay.cross_link_count().unwrap(), 1);
    drop(overlay);

    // Second reconcile should prune the orphan via prune_overlay_orphans.
    let second = run_reconcile_pass(&repo, &config, &synrepo_dir);
    assert!(matches!(second, ReconcileOutcome::Completed(_)));

    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    assert_eq!(overlay.cross_link_count().unwrap(), 0);
    let audit = overlay
        .cross_link_audit_events(&from.to_string(), &to.to_string(), "references")
        .unwrap();
    assert!(
        audit.iter().any(|e| e.event_kind == "pruned"),
        "reconcile must append a `pruned` audit row"
    );
}

#[test]
fn persist_reconcile_state_records_lock_conflict() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let outcome = ReconcileOutcome::LockConflict { holder_pid: 42 };
    persist_reconcile_state(&synrepo_dir, &outcome, 1);

    let state = load_reconcile_state(&synrepo_dir).unwrap();
    assert_eq!(state.last_outcome, "lock-conflict");
    assert!(state.last_error.is_none());
    assert_eq!(state.triggering_events, 1);
}
