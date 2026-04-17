//! Sync action handlers submodules.
//!
//! - `handlers.rs` — action handlers extracted from sync.rs

pub mod handlers;

pub use handlers::ActionContext;

use std::path::Path;

use crate::{
    config::Config,
    pipeline::{
        maintenance::plan_maintenance,
        writer::{acquire_write_admission, map_lock_error, now_rfc3339},
    },
};

use crate::pipeline::repair::{
    append_resolution_log, cross_links::run_cross_link_generation, report::assemble_repair_report,
    DriftClass, RepairAction, RepairFinding, RepairSurface, ResolutionLogEntry, Severity,
    SyncOptions, SyncOutcome, SyncSummary,
};

/// Execute a targeted sync: repair auto-fixable findings from `build_repair_report`.
pub fn execute_sync(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    options: SyncOptions,
) -> crate::Result<SyncSummary> {
    let maint_plan = plan_maintenance(synrepo_dir, config);
    let report = assemble_repair_report(synrepo_dir, config, &maint_plan);

    let _writer_lock =
        acquire_write_admission(synrepo_dir, "sync").map_err(|err| map_lock_error("sync", err))?;

    let now = now_rfc3339();

    let mut repaired: Vec<RepairFinding> = Vec::new();
    let mut report_only: Vec<RepairFinding> = Vec::new();
    let mut blocked: Vec<RepairFinding> = Vec::new();
    let mut actions_taken: Vec<String> = Vec::new();

    let action_context = ActionContext {
        repo_root,
        synrepo_dir,
        config,
        maint_plan: &maint_plan,
    };

    for finding in &report.findings {
        match finding.severity {
            Severity::Blocked => blocked.push(finding.clone()),
            Severity::ReportOnly | Severity::Unsupported => report_only.push(finding.clone()),
            Severity::Actionable => {
                handlers::handle_actionable_finding(
                    finding,
                    &action_context,
                    &mut repaired,
                    &mut report_only,
                    &mut blocked,
                    &mut actions_taken,
                )?;
            }
        }
    }

    if let Ok(count) = resolve_pending_promotions(synrepo_dir) {
        if count > 0 {
            actions_taken.push(format!("resolved {count} stuck pending_promotion row(s)"));
        }
    }

    if options.generate_cross_links || options.regenerate_cross_links {
        match run_cross_link_generation(
            action_context.repo_root,
            action_context.synrepo_dir,
            action_context.config,
            options.generate_cross_links,
            options.regenerate_cross_links,
        ) {
            Ok(outcome) => {
                actions_taken.push(format!(
                    "cross-link generation pass: {} new candidates",
                    outcome.inserted
                ));
                if outcome.blocked_pairs > 0 {
                    blocked.push(RepairFinding {
                        surface: RepairSurface::ProposedLinksOverlay,
                        drift_class: DriftClass::Blocked,
                        severity: Severity::Blocked,
                        target_id: Some(outcome.blocked_pairs.to_string()),
                        recommended_action: RepairAction::ManualReview,
                        notes: Some(format!(
                            "Cross-link generation hit the per-run cost limit ({}); {} candidate pair(s) were left blocked.",
                            action_context.config.cross_link_cost_limit,
                            outcome.blocked_pairs
                        )),
                    });
                }
            }
            Err(err) => {
                actions_taken.push(format!("cross-link generation pass failed: {err}"));
                blocked.push(RepairFinding {
                    surface: RepairSurface::ProposedLinksOverlay,
                    drift_class: DriftClass::Blocked,
                    severity: Severity::Blocked,
                    target_id: None,
                    recommended_action: RepairAction::ManualReview,
                    notes: Some(format!("Cross-link generation failed: {err}")),
                });
            }
        }
    }

    let outcome = if blocked.is_empty() {
        SyncOutcome::Completed
    } else {
        SyncOutcome::Partial
    };

    let entry = ResolutionLogEntry {
        synced_at: now.clone(),
        source_revision: None,
        requested_scope: report.findings.iter().map(|f| f.surface).collect(),
        findings_considered: report.findings,
        actions_taken,
        outcome,
    };
    append_resolution_log(synrepo_dir, &entry);

    Ok(SyncSummary {
        synced_at: now,
        repaired,
        report_only,
        blocked,
    })
}

/// Resolve cross-link rows stuck in `pending_promotion` state.
pub fn resolve_pending_promotions(synrepo_dir: &Path) -> crate::Result<usize> {
    use std::str::FromStr;

    use crate::{
        core::ids::NodeId,
        overlay::OverlayEdgeKind,
        pipeline::structural::derive_edge_id,
        store::overlay::{parse_overlay_edge_kind, SqliteOverlayStore},
        store::sqlite::SqliteGraphStore,
        structure::graph::{EdgeKind, GraphStore},
    };

    let overlay_dir = synrepo_dir.join("overlay");
    let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
    if !overlay_db.exists() {
        return Ok(0);
    }

    let mut overlay = SqliteOverlayStore::open_existing(&overlay_dir)?;
    let pending = overlay.pending_promotion_rows()?;
    if pending.is_empty() {
        return Ok(0);
    }

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;
    let mut resolved = 0usize;

    for row in pending {
        let Ok(from) = NodeId::from_str(&row.from_node) else {
            tracing::warn!(
                from = %row.from_node,
                "pending_promotion row has unparseable from_node; skipping"
            );
            continue;
        };
        let Ok(to) = NodeId::from_str(&row.to_node) else {
            tracing::warn!(
                to = %row.to_node,
                "pending_promotion row has unparseable to_node; skipping"
            );
            continue;
        };
        let Ok(overlay_kind) = parse_overlay_edge_kind(&row.kind) else {
            tracing::warn!(kind = %row.kind, "pending_promotion row has unknown kind; skipping");
            continue;
        };

        let edge_kind = match overlay_kind {
            OverlayEdgeKind::References => EdgeKind::References,
            OverlayEdgeKind::Governs => EdgeKind::Governs,
            OverlayEdgeKind::DerivedFrom => EdgeKind::References,
            OverlayEdgeKind::Mentions => EdgeKind::Mentions,
        };

        let edge_id = derive_edge_id(from, to, edge_kind);
        let edge_exists = graph
            .outbound(from, Some(edge_kind))
            .unwrap_or_default()
            .iter()
            .any(|e| e.to == to);

        if edge_exists {
            let reviewer = row.reviewer.as_deref().unwrap_or("crash-recovery");
            overlay.mark_candidate_promoted(
                from,
                to,
                overlay_kind,
                reviewer,
                &edge_id.to_string(),
            )?;
        } else {
            overlay.reset_candidate_to_active(from, to, overlay_kind)?;
        }
        resolved += 1;
    }

    Ok(resolved)
}
