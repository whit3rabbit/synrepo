//! Tests for the capability readiness matrix. Drive the matrix off carefully
//! shaped status snapshots so we exercise each row's degraded branch without
//! needing a real `.synrepo/` on disk.

use std::path::PathBuf;

use super::*;
use crate::{
    bootstrap::runtime_probe::{
        AgentIntegration, AgentTargetKind, Missing, ProbeReport, RuntimeClassification,
    },
    config::{Config, Mode},
    pipeline::{
        context_metrics::ContextMetrics,
        diagnostics::{
            EmbeddingHealth, ReconcileHealth, ReconcileStaleness, RuntimeDiagnostics, WriterStatus,
        },
        watch::{ReconcileState, WatchServiceStatus},
    },
    surface::status_snapshot::{
        CommentaryCoverage, ExportState, ExportStatus, GraphSnapshotStatus, RepairAuditState,
        StatusSnapshot,
    },
};

fn repo_root() -> PathBuf {
    PathBuf::from("/tmp/readiness-fixture")
}

fn base_diagnostics() -> RuntimeDiagnostics {
    RuntimeDiagnostics {
        reconcile_health: ReconcileHealth::Current,
        watch_status: WatchServiceStatus::Inactive,
        writer_status: WriterStatus::Free,
        store_guidance: vec![],
        last_reconcile: Some(ReconcileState {
            last_reconcile_at: "2026-04-20T00:00:00Z".to_string(),
            last_outcome: "completed".to_string(),
            last_error: None,
            triggering_events: 0,
            files_discovered: Some(42),
            symbols_extracted: Some(300),
        }),
        embedding_health: EmbeddingHealth::Disabled,
    }
}

fn base_snapshot(diag: RuntimeDiagnostics) -> StatusSnapshot {
    StatusSnapshot {
        initialized: true,
        config: Some(Config::default()),
        diagnostics: Some(diag),
        graph_stats: None,
        graph_snapshot: GraphSnapshotStatus {
            epoch: 1,
            age_ms: 0,
            size_bytes: 0,
            file_count: 42,
            symbol_count: 300,
            edge_count: 0,
        },
        export_freshness: "current".to_string(),
        export_status: ExportStatus {
            state: ExportState::Current,
            display: "current".to_string(),
            export_dir: "synrepo-context".to_string(),
            format: Some("markdown".to_string()),
            budget: Some("normal".to_string()),
        },
        overlay_cost_summary: "0 LLM calls".to_string(),
        commentary_coverage: CommentaryCoverage {
            total: Some(0),
            fresh: None,
            estimated_fresh: None,
            estimated_stale_ratio: None,
            estimate_confidence: None,
            display: "0 entries".to_string(),
        },
        agent_note_counts: None,
        explain_provider: None,
        explain_totals: None,
        context_metrics: Some(ContextMetrics::default()),
        last_compaction: None,
        repair_audit: RepairAuditState::Ok,
        recent_activity: None,
        synrepo_dir: PathBuf::from("/tmp/readiness-fixture/.synrepo"),
    }
}

fn ready_probe() -> ProbeReport {
    ProbeReport {
        classification: RuntimeClassification::Ready,
        agent_integration: AgentIntegration::Complete {
            target: AgentTargetKind::Claude,
        },
        detected_agent_targets: vec![],
        synrepo_dir: PathBuf::from("/tmp/readiness-fixture/.synrepo"),
    }
}

fn find_row(matrix: &ReadinessMatrix, cap: Capability) -> &ReadinessRow {
    matrix
        .rows
        .iter()
        .find(|r| r.capability == cap)
        .unwrap_or_else(|| panic!("expected row for {}", cap.as_str()))
}

#[test]
fn severity_mapping_preserves_disabled_vs_unavailable() {
    assert_eq!(ReadinessState::Disabled.severity(), Severity::Healthy);
    assert_eq!(ReadinessState::Supported.severity(), Severity::Healthy);
    assert_eq!(ReadinessState::Unavailable.severity(), Severity::Stale);
    assert_eq!(ReadinessState::Degraded.severity(), Severity::Stale);
    assert_eq!(ReadinessState::Stale.severity(), Severity::Stale);
    assert_eq!(ReadinessState::Blocked.severity(), Severity::Blocked);
}

#[test]
fn all_capabilities_have_stable_labels() {
    let caps = [
        (Capability::Parser, "parser"),
        (Capability::GitIntelligence, "git-intelligence"),
        (Capability::Embeddings, "embeddings"),
        (Capability::Watch, "watch"),
        (Capability::IndexFreshness, "index-freshness"),
        (Capability::Overlay, "overlay"),
        (Capability::Compatibility, "compatibility"),
    ];
    for (cap, expected) in caps {
        assert_eq!(cap.as_str(), expected);
    }
}

#[test]
fn matrix_contains_all_seven_rows_in_stable_order() {
    let snapshot = base_snapshot(base_diagnostics());
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let order: Vec<&'static str> = matrix.rows.iter().map(|r| r.capability.as_str()).collect();
    assert_eq!(
        order,
        vec![
            "parser",
            "git-intelligence",
            "embeddings",
            "watch",
            "index-freshness",
            "overlay",
            "compatibility",
        ]
    );
}

#[test]
fn parser_row_reports_degraded_when_last_outcome_failed() {
    let mut diag = base_diagnostics();
    diag.reconcile_health =
        ReconcileHealth::Stale(ReconcileStaleness::Outcome("failed".to_string()));
    diag.last_reconcile = None;
    let snapshot = base_snapshot(diag);
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Parser);
    assert_eq!(row.state, ReadinessState::Degraded);
    assert!(
        row.next_action.is_some(),
        "degraded parser must carry a next action"
    );
}

#[test]
fn git_row_reports_unavailable_when_no_git_repository() {
    // /tmp is not a git repository.
    let snapshot = base_snapshot(base_diagnostics());
    let matrix = ReadinessMatrix::build(
        &PathBuf::from("/tmp"),
        &ready_probe(),
        &snapshot,
        &Config::default(),
    );
    let row = find_row(&matrix, Capability::GitIntelligence);
    assert_eq!(row.state, ReadinessState::Unavailable);
    assert_eq!(row.state.severity(), Severity::Stale);
}

#[test]
fn index_freshness_row_reports_stale_for_old_reconcile() {
    let mut diag = base_diagnostics();
    diag.reconcile_health = ReconcileHealth::Stale(ReconcileStaleness::Age {
        last_reconcile_at: "2024-01-01T00:00:00Z".to_string(),
    });
    let snapshot = base_snapshot(diag);
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::IndexFreshness);
    assert_eq!(row.state, ReadinessState::Stale);
    assert_eq!(row.next_action.as_deref(), Some("run `synrepo reconcile`"));
}

#[test]
fn overlay_row_reports_unavailable_when_commentary_reports_unavailable() {
    let mut snapshot = base_snapshot(base_diagnostics());
    snapshot.commentary_coverage = CommentaryCoverage {
        total: None,
        fresh: None,
        estimated_fresh: None,
        estimated_stale_ratio: None,
        estimate_confidence: None,
        display: "unavailable (open failed)".to_string(),
    };
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Overlay);
    assert_eq!(row.state, ReadinessState::Unavailable);
}

#[test]
fn watch_row_reports_disabled_when_inactive() {
    let snapshot = base_snapshot(base_diagnostics());
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Watch);
    assert_eq!(
        row.state,
        ReadinessState::Disabled,
        "watch inactive is intentional not broken"
    );
    assert_eq!(row.state.severity(), Severity::Healthy);
}

#[test]
fn compatibility_row_reports_blocked_when_probe_is_blocked() {
    let snapshot = base_snapshot(base_diagnostics());
    let probe = ProbeReport {
        classification: RuntimeClassification::Partial {
            missing: vec![Missing::CompatBlocked {
                guidance: vec!["graph store schema too new".to_string()],
            }],
        },
        agent_integration: AgentIntegration::Absent,
        detected_agent_targets: vec![],
        synrepo_dir: PathBuf::from("/tmp/readiness-fixture/.synrepo"),
    };
    let matrix = ReadinessMatrix::build(&repo_root(), &probe, &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Compatibility);
    assert_eq!(row.state, ReadinessState::Blocked);
    assert_eq!(row.state.severity(), Severity::Blocked);
    assert_eq!(row.next_action.as_deref(), Some("run `synrepo upgrade`"));
}

#[test]
fn disabled_embeddings_do_not_block_core_operation() {
    // Scenario 3.2: optional disabled features do not block core graph-backed
    // operation. With embeddings disabled but parser + index + compatibility
    // all healthy, the matrix's degraded-rows iterator must not include a
    // graph-blocking row.
    let diag = base_diagnostics();
    let snapshot = base_snapshot(diag);
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let embeddings = find_row(&matrix, Capability::Embeddings);
    assert_eq!(embeddings.state, ReadinessState::Disabled);
    assert_eq!(
        embeddings.detail,
        "optional; semantic routing uses lexical fallback"
    );

    // The core-graph capabilities must stay non-blocked.
    for cap in [
        Capability::Parser,
        Capability::IndexFreshness,
        Capability::Compatibility,
    ] {
        let row = find_row(&matrix, cap);
        assert_ne!(
            row.state,
            ReadinessState::Blocked,
            "{} must not be Blocked when only embeddings are disabled",
            cap.as_str()
        );
    }
}

#[test]
fn unknown_reconcile_marks_parser_as_stale_not_blocked() {
    let mut diag = base_diagnostics();
    diag.reconcile_health = ReconcileHealth::Unknown;
    diag.last_reconcile = None;
    let snapshot = base_snapshot(diag);
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Parser);
    // Unknown reconcile is "never run" - stale, not blocked. Recovery is a
    // first reconcile, not a repair.
    assert_eq!(row.state, ReadinessState::Stale);
    assert_eq!(row.next_action.as_deref(), Some("run `synrepo reconcile`"));
}

#[test]
fn corrupt_reconcile_marks_index_freshness_as_blocked() {
    let mut diag = base_diagnostics();
    diag.reconcile_health = ReconcileHealth::Corrupt("parse error".to_string());
    let snapshot = base_snapshot(diag);
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::IndexFreshness);
    assert_eq!(row.state, ReadinessState::Blocked);
    assert_eq!(row.state.severity(), Severity::Blocked);
}

#[test]
fn watch_stale_artifacts_is_stale_not_disabled() {
    // Stale(None) matches the no-live-owner-but-artifacts-on-disk case.
    // Production uses Stale(Some(_)) too, but the state field set depends on
    // internal lease types that are not all `pub`-accessible here.
    let mut diag = base_diagnostics();
    diag.watch_status = WatchServiceStatus::Stale(None);
    let snapshot = base_snapshot(diag);
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Watch);
    assert_eq!(row.state, ReadinessState::Stale);
    assert_eq!(row.state.severity(), Severity::Stale);
    assert!(row.next_action.is_some());
}

#[cfg(feature = "semantic-triage")]
#[test]
fn degraded_embeddings_report_degraded_with_next_action() {
    let mut diag = base_diagnostics();
    diag.embedding_health = EmbeddingHealth::Degraded("embedding index missing".to_string());
    let snapshot = base_snapshot(diag);
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Embeddings);
    assert_eq!(row.state, ReadinessState::Degraded);
    assert!(
        row.next_action
            .as_deref()
            .is_some_and(|s| s.contains("reconcile")),
        "degraded embeddings must point at reconcile"
    );
}

#[test]
fn uninitialized_snapshot_reports_parser_unavailable() {
    // Scenario 3.3: degraded workflow output labels unavailable data sources.
    // When a snapshot has no diagnostics, parser must report Unavailable rather
    // than claim health it does not have.
    let mut snapshot = base_snapshot(base_diagnostics());
    snapshot.diagnostics = None;
    snapshot.initialized = false;
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Parser);
    assert_eq!(row.state, ReadinessState::Unavailable);
    assert_eq!(row.next_action.as_deref(), Some("run `synrepo init`"));
}

#[test]
fn degraded_rows_iterator_filters_healthy_rows() {
    let mut diag = base_diagnostics();
    diag.reconcile_health = ReconcileHealth::Stale(ReconcileStaleness::Age {
        last_reconcile_at: "2024-01-01T00:00:00Z".to_string(),
    });
    let snapshot = base_snapshot(diag);
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let degraded: Vec<&ReadinessRow> = matrix.degraded_rows().collect();
    assert!(
        degraded
            .iter()
            .any(|r| r.capability == Capability::IndexFreshness),
        "stale index must surface in degraded_rows()"
    );
    assert!(
        !degraded
            .iter()
            .any(|r| r.capability == Capability::Embeddings),
        "disabled embeddings must not surface in degraded_rows()"
    );
}

// Silence unused-import warnings when only some branches are exercised here.
#[allow(dead_code)]
fn _retain_mode_import() {
    let _ = Mode::Auto;
}
