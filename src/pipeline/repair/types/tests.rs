use super::{
    DriftClass, RepairAction, RepairFinding, RepairReport, RepairSurface, ResolutionLogEntry,
    Severity, SyncOutcome,
};

#[test]
fn repair_surface_stable_identifiers() {
    assert_eq!(
        RepairSurface::StoreMaintenance.as_str(),
        "store_maintenance"
    );
    assert_eq!(
        RepairSurface::StructuralRefresh.as_str(),
        "structural_refresh"
    );
    assert_eq!(RepairSurface::WriterLock.as_str(), "writer_lock");
    assert_eq!(RepairSurface::DeclaredLinks.as_str(), "declared_links");
    assert_eq!(RepairSurface::StaleRationale.as_str(), "stale_rationale");
    assert_eq!(
        RepairSurface::CommentaryOverlayEntries.as_str(),
        "commentary_overlay_entries"
    );
    assert_eq!(
        RepairSurface::ProposedLinksOverlay.as_str(),
        "proposed_links_overlay"
    );
    assert_eq!(RepairSurface::ExportSurface.as_str(), "export_surface");
}

#[test]
fn drift_class_stable_identifiers() {
    assert_eq!(DriftClass::Current.as_str(), "current");
    assert_eq!(DriftClass::Stale.as_str(), "stale");
    assert_eq!(DriftClass::Absent.as_str(), "absent");
    assert_eq!(DriftClass::TrustConflict.as_str(), "trust_conflict");
    assert_eq!(DriftClass::Unsupported.as_str(), "unsupported");
    assert_eq!(DriftClass::Blocked.as_str(), "blocked");
    assert_eq!(DriftClass::SourceDeleted.as_str(), "source_deleted");
}

#[test]
fn severity_stable_identifiers() {
    assert_eq!(Severity::Actionable.as_str(), "actionable");
    assert_eq!(Severity::ReportOnly.as_str(), "report_only");
    assert_eq!(Severity::Blocked.as_str(), "blocked");
    assert_eq!(Severity::Unsupported.as_str(), "unsupported");
}

#[test]
fn repair_action_stable_identifiers() {
    assert_eq!(RepairAction::None.as_str(), "none");
    assert_eq!(RepairAction::RunReconcile.as_str(), "run_reconcile");
    assert_eq!(RepairAction::RunMaintenance.as_str(), "run_maintenance");
    assert_eq!(
        RepairAction::RunMaintenanceThenReconcile.as_str(),
        "run_maintenance_then_reconcile"
    );
    assert_eq!(RepairAction::ManualReview.as_str(), "manual_review");
    assert_eq!(RepairAction::NotSupported.as_str(), "not_supported");
    assert_eq!(
        RepairAction::RefreshCommentary.as_str(),
        "refresh_commentary"
    );
    assert_eq!(RepairAction::RevalidateLinks.as_str(), "revalidate_links");
    assert_eq!(
        RepairAction::RegenerateExports.as_str(),
        "regenerate_exports"
    );
}

#[test]
fn finding_serializes_and_deserializes() {
    let finding = RepairFinding {
        surface: RepairSurface::StoreMaintenance,
        drift_class: DriftClass::Stale,
        severity: Severity::Actionable,
        target_id: Some("graph".to_string()),
        recommended_action: RepairAction::RunMaintenance,
        notes: Some("Graph store is stale.".to_string()),
    };

    let json = serde_json::to_string(&finding).unwrap();
    let decoded: RepairFinding = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.surface, RepairSurface::StoreMaintenance);
    assert_eq!(decoded.drift_class, DriftClass::Stale);
    assert_eq!(decoded.severity, Severity::Actionable);
    assert_eq!(decoded.recommended_action, RepairAction::RunMaintenance);
}

#[test]
fn repair_report_has_actionable_detects_non_none_actions() {
    let report = RepairReport {
        checked_at: "2026-01-01T00:00:00Z".to_string(),
        findings: vec![RepairFinding {
            surface: RepairSurface::StructuralRefresh,
            drift_class: DriftClass::Stale,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::RunReconcile,
            notes: None,
        }],
    };
    assert!(report.has_actionable());
    assert!(!report.has_blocked());
}

#[test]
fn repair_report_has_blocked_detects_blocked_severity() {
    let report = RepairReport {
        checked_at: "2026-01-01T00:00:00Z".to_string(),
        findings: vec![RepairFinding {
            surface: RepairSurface::WriterLock,
            drift_class: DriftClass::Blocked,
            severity: Severity::Blocked,
            target_id: Some("99".to_string()),
            recommended_action: RepairAction::ManualReview,
            notes: Some("Lock held by pid 99.".to_string()),
        }],
    };
    assert!(!report.has_actionable());
    assert!(report.has_blocked());
}

#[test]
fn resolution_log_entry_serializes_to_json() {
    let entry = ResolutionLogEntry {
        synced_at: "2026-01-01T00:00:00Z".to_string(),
        source_revision: Some("abc123".to_string()),
        requested_scope: vec![
            RepairSurface::StoreMaintenance,
            RepairSurface::StructuralRefresh,
        ],
        findings_considered: vec![],
        actions_taken: vec!["ran maintenance".to_string()],
        outcome: SyncOutcome::Completed,
    };

    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("store_maintenance"));
    assert!(json.contains("structural_refresh"));
    assert!(json.contains("\"completed\""));
}
