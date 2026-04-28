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

    let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir, false);
    match outcome {
        ReconcileOutcome::Completed(summary) => {
            assert!(summary.files_discovered >= 1);
            assert!(summary.symbols_extracted >= 1);
        }
        other => panic!("expected Completed, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn reconcile_pass_returns_lock_conflict_when_lock_is_held() {
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();

    // Hold the kernel flock on a separate open file description (simulates a
    // foreign writer) and stamp a live foreign PID into the ownership
    // metadata so error reporting shows a recognizable holder PID.
    let (mut child, foreign_pid) = super::live_foreign_pid();
    let lock_path = crate::pipeline::writer::writer_lock_path(&synrepo_dir);
    fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let owner = crate::pipeline::writer::WriterOwnership {
        pid: foreign_pid,
        acquired_at: crate::pipeline::writer::now_rfc3339(),
    };
    let _flock = crate::pipeline::writer::hold_writer_flock_with_ownership(&lock_path, &owner);

    let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir, false);
    assert!(matches!(
        outcome,
        ReconcileOutcome::LockConflict { holder_pid } if holder_pid == foreign_pid
    ));

    child.kill().unwrap();
}

#[test]
fn reconcile_pass_corrects_stale_graph_state() {
    let _guard = crate::test_support::global_test_lock("watch-reconcile-refresh");
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let first = run_reconcile_pass(&repo, &config, &synrepo_dir, false);
    if !matches!(first, ReconcileOutcome::Completed(_)) {
        panic!("expected first pass Completed, got {first:?}");
    }

    fs::write(repo.join("src/new.rs"), "pub fn new_fn() {}\n").unwrap();
    let second = run_reconcile_pass(&repo, &config, &synrepo_dir, false);
    if let ReconcileOutcome::Completed(summary) = second {
        assert!(summary.files_discovered >= 2);
    } else {
        panic!("expected Completed after adding new file, got {second:?}");
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

    let state = load_reconcile_state(&synrepo_dir).expect("must load reconcile state");
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

    let state = load_reconcile_state(&synrepo_dir).expect("must load reconcile state");
    assert_eq!(state.last_outcome, "failed");
    assert_eq!(state.last_error.as_deref(), Some("disk full"));
}

#[test]
fn reconcile_prunes_cross_link_orphans() {
    use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};

    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let first = run_reconcile_pass(&repo, &config, &synrepo_dir, false);
    if !matches!(first, ReconcileOutcome::Completed(_)) {
        panic!("expected first pass Completed, got {first:?}");
    }

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

    let second = run_reconcile_pass(&repo, &config, &synrepo_dir, false);
    if !matches!(second, ReconcileOutcome::Completed(_)) {
        panic!("expected second pass Completed after cross-link insert, got {second:?}");
    }

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

    let state = load_reconcile_state(&synrepo_dir).expect("must load reconcile state");
    assert_eq!(state.last_outcome, "lock-conflict");
    assert_eq!(state.triggering_events, 1);
}

#[test]
fn reconcile_emits_cochange_edges_on_repo_with_multi_file_commits() {
    use crate::core::ids::NodeId;
    use crate::store::sqlite::SqliteGraphStore;
    use crate::structure::graph::EdgeKind;

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();

    // Create a git repo with multi-file commits to produce co-change data.
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .expect("git command")
    };
    git(&["init"]);
    git(&["config", "user.name", "test"]);
    git(&["config", "user.email", "test@test.com"]);

    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/a.rs"), "pub fn one() {}\n").unwrap();
    fs::write(repo.join("src/b.rs"), "pub fn two() {}\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "initial"]);

    // Make two more commits touching both files to exceed the threshold of 2.
    fs::write(repo.join("src/a.rs"), "pub fn one() { /* v2 */ }\n").unwrap();
    fs::write(repo.join("src/b.rs"), "pub fn two() { /* v2 */ }\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "touch both v2"]);

    fs::write(repo.join("src/a.rs"), "pub fn one() { /* v3 */ }\n").unwrap();
    fs::write(repo.join("src/b.rs"), "pub fn two() { /* v3 */ }\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "touch both v3"]);

    let synrepo_dir = repo.join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    crate::store::compatibility::write_runtime_snapshot(
        &synrepo_dir,
        &crate::config::Config::default(),
    )
    .unwrap();

    let config = crate::config::Config::default();
    let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir, false);
    if !matches!(outcome, ReconcileOutcome::Completed(_)) {
        panic!("expected Completed on multi-file-commit repo, got {outcome:?}");
    }

    // Verify CoChangesWith edges exist in the graph.
    let graph = SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap();
    let files = graph.all_file_paths().unwrap();
    let file_a = files.iter().find(|(p, _)| p.contains("a.rs")).unwrap().1;
    let cochange_edges = graph
        .outbound(NodeId::File(file_a), Some(EdgeKind::CoChangesWith))
        .unwrap();
    assert!(
        !cochange_edges.is_empty(),
        "expected CoChangesWith edges after reconcile on repo with multi-file commits"
    );
}

#[test]
fn touched_worktree_reconcile_does_not_delete_sibling_worktree_files() {
    use super::super::reconcile::run_reconcile_pass_with_touched_paths;
    use crate::{
        store::{compatibility::write_runtime_snapshot, sqlite::SqliteGraphStore},
        substrate::{discover_roots, DiscoveryRootKind},
    };
    use std::{path::Path, process::Command};

    let main = tempfile::tempdir().unwrap();
    let wt_a = tempfile::tempdir().unwrap();
    let wt_b = tempfile::tempdir().unwrap();
    fs::create_dir_all(main.path().join("src")).unwrap();
    fs::write(main.path().join("src/lib.rs"), "pub fn main_root() {}\n").unwrap();
    git(main.path(), &["init"]);
    git(main.path(), &["config", "user.email", "test@example.com"]);
    git(main.path(), &["config", "user.name", "Test User"]);
    git(main.path(), &["add", "."]);
    git(main.path(), &["commit", "-m", "initial"]);
    fs::remove_dir(wt_a.path()).unwrap();
    fs::remove_dir(wt_b.path()).unwrap();
    git(
        main.path(),
        &["worktree", "add", "-b", "wt-a", path_str(wt_a.path())],
    );
    git(
        main.path(),
        &["worktree", "add", "-b", "wt-b", path_str(wt_b.path())],
    );
    fs::write(wt_a.path().join("src/a_only.rs"), "pub fn a_only() {}\n").unwrap();
    fs::write(wt_b.path().join("src/b_only.rs"), "pub fn b_only() {}\n").unwrap();

    let config = crate::config::Config::default();
    let synrepo_dir = main.path().join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    write_runtime_snapshot(&synrepo_dir, &config).unwrap();

    let first = run_reconcile_pass(main.path(), &config, &synrepo_dir, true);
    assert!(matches!(first, ReconcileOutcome::Completed(_)));

    let roots = discover_roots(main.path(), &config);
    let wt_b_root = roots
        .iter()
        .find(|root| {
            root.kind == DiscoveryRootKind::Worktree
                && root.absolute_path == wt_b.path().canonicalize().unwrap()
        })
        .unwrap()
        .discriminant
        .clone();

    fs::remove_file(wt_b.path().join("src/b_only.rs")).unwrap();
    fs::write(wt_a.path().join("src/a_only.rs"), "pub fn a_only_v2() {}\n").unwrap();
    let touched = [wt_a.path().join("src/a_only.rs")];
    let second = run_reconcile_pass_with_touched_paths(
        main.path(),
        &config,
        &synrepo_dir,
        Some(&touched),
        true,
    );
    assert!(matches!(second, ReconcileOutcome::Completed(_)));

    let graph = SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap();
    assert!(
        graph
            .file_by_root_path(&wt_b_root, "src/b_only.rs")
            .unwrap()
            .is_some(),
        "scoped reconcile for worktree A must not delete missing files from worktree B"
    );

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn path_str(path: &Path) -> &str {
        path.to_str().unwrap()
    }
}
