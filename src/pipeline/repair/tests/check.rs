use tempfile::tempdir;

use super::support::{init_synrepo, init_synrepo_with_completed_reconcile, write_foreign_lock};
use crate::{
    config::Config,
    pipeline::watch::{persist_reconcile_state, ReconcileOutcome},
};

use crate::pipeline::repair::{
    build_repair_report, RepairAction, RepairSurface, Severity,
};

#[test]
fn check_on_fresh_initialized_runtime_has_no_actionable_findings() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    assert!(
        !report.has_actionable(),
        "fresh runtime should have no actionable findings, got: {:?}",
        report
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Actionable
                && f.recommended_action != RepairAction::None)
            .map(|f| f.surface.as_str())
            .collect::<Vec<_>>()
    );
    assert!(!report.has_blocked());
}

#[test]
fn check_detects_stale_reconcile() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("disk error".to_string()),
        0,
    );

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let structural = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::StructuralRefresh)
        .unwrap();

    assert_eq!(structural.drift_class.as_str(), "stale");
    assert_eq!(structural.recommended_action, RepairAction::RunReconcile);
}

#[test]
fn check_reports_blocked_drift_when_writer_lock_held_by_foreign_pid() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);
    write_foreign_lock(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let lock_finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::WriterLock)
        .unwrap();

    assert!(
        matches!(
            lock_finding.severity,
            Severity::Blocked | Severity::Actionable
        ),
        "writer lock finding must be Blocked or Actionable, got {:?}",
        lock_finding.severity,
    );
}

#[test]
fn check_reports_unsupported_surfaces_explicitly() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());

    for surface in [
        RepairSurface::OverlayEntries,
        RepairSurface::ExportViews,
        RepairSurface::StaleRationale,
    ] {
        let finding = report
            .findings
            .iter()
            .find(|f| f.surface == surface)
            .unwrap_or_else(|| panic!("{} must be in report", surface.as_str()));
        assert_eq!(
            finding.severity,
            Severity::Unsupported,
            "{} must be reported as Unsupported",
            surface.as_str()
        );
    }
}

#[test]
fn check_report_render_includes_all_surfaces() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let rendered = report.render();

    for surface in [
        "store_maintenance",
        "structural_refresh",
        "writer_lock",
        "declared_links",
        "overlay_entries",
        "export_views",
    ] {
        assert!(
            rendered.contains(surface),
            "render must mention surface {surface}"
        );
    }
}

#[test]
fn check_report_serializes_to_valid_json() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let json = serde_json::to_string_pretty(&report).unwrap();
    let decoded: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(decoded["checked_at"].is_string());
    assert!(decoded["findings"].is_array());
    assert!(
        decoded["findings"].as_array().unwrap().len() >= 6,
        "report must include at least 6 surfaces"
    );
}
