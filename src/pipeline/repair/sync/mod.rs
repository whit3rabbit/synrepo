//! Sync action handlers submodules.
//!
//! - `handlers.rs` — action handlers extracted from sync.rs

mod commentary;
mod commentary_context;
mod commentary_generate;
mod commentary_plan;
mod commentary_progress;
pub mod handlers;
mod revalidate_links;

pub use commentary::refresh_commentary;
pub use commentary_plan::{
    load_commentary_work_plan, normalize_scope_prefixes, path_matches_any_prefix,
    CommentaryProgressEvent, CommentaryWorkItem, CommentaryWorkPhase, CommentaryWorkPlan,
};
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
    SurfaceOutcome, SyncOptions, SyncOutcome, SyncProgress, SyncSummary,
};

/// Surfaces the watch auto-sync hook is allowed to repair inline after
/// reconcile. Intentionally hard-coded: promoting a surface is a visible source
/// change, never a silent config tweak. Commentary, proposed cross-links,
/// declared links, and edge drift stay out of this list because they cost
/// tokens or require human review.
pub const CHEAP_AUTO_SYNC_SURFACES: &[RepairSurface] = &[
    RepairSurface::ExportSurface,
    RepairSurface::RetiredObservations,
];

/// Execute a targeted sync: repair auto-fixable findings from `build_repair_report`.
///
/// Acquires the writer lock via `acquire_write_admission`, which rejects the
/// call when a watch service is active for this repo. In-process callers that
/// already hold the writer lock (the watch service itself) SHALL use
/// [`execute_sync_locked`] directly instead of calling this function.
pub fn execute_sync(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    options: SyncOptions,
) -> crate::Result<SyncSummary> {
    let _writer_lock =
        acquire_write_admission(synrepo_dir, "sync").map_err(|err| map_lock_error("sync", err))?;

    execute_sync_locked(repo_root, synrepo_dir, config, options, &mut None, None)
}

/// Execute a targeted sync assuming the caller already holds the writer lock.
///
/// Used by the watch service, which takes the raw writer lock itself before
/// dispatching control-plane sync requests. External callers should prefer
/// [`execute_sync`] unless they explicitly hold the lock.
///
/// - `progress`: optional callback invoked once per surface boundary and once
///   per commentary sub-event. `None` preserves the pre-change silent
///   behavior.
/// - `surface_filter`: when `Some(allow_list)`, only findings whose surface is
///   in the allow list are dispatched to their handlers; other actionable
///   findings are bucketed as `SurfaceOutcome::FilteredOut` and left untouched.
///   `None` processes every actionable finding, matching prior behavior.
pub fn execute_sync_locked(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    options: SyncOptions,
    progress: &mut Option<&mut dyn FnMut(SyncProgress)>,
    surface_filter: Option<&[RepairSurface]>,
) -> crate::Result<SyncSummary> {
    let maint_plan = plan_maintenance(synrepo_dir, config);
    let report = assemble_repair_report(synrepo_dir, config, &maint_plan);

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
                let allowed = surface_filter
                    .map(|list| list.contains(&finding.surface))
                    .unwrap_or(true);
                if !allowed {
                    emit_progress(
                        progress,
                        SyncProgress::SurfaceStarted {
                            surface: finding.surface,
                            action: finding.recommended_action,
                        },
                    );
                    emit_progress(
                        progress,
                        SyncProgress::SurfaceFinished {
                            surface: finding.surface,
                            outcome: SurfaceOutcome::FilteredOut,
                        },
                    );
                    continue;
                }

                emit_progress(
                    progress,
                    SyncProgress::SurfaceStarted {
                        surface: finding.surface,
                        action: finding.recommended_action,
                    },
                );

                let repaired_before = repaired.len();
                let report_only_before = report_only.len();
                let blocked_before = blocked.len();

                handlers::handle_actionable_finding(
                    finding,
                    &action_context,
                    &mut repaired,
                    &mut report_only,
                    &mut blocked,
                    &mut actions_taken,
                    progress,
                )?;

                let outcome = if repaired.len() > repaired_before {
                    SurfaceOutcome::Repaired
                } else if blocked.len() > blocked_before {
                    SurfaceOutcome::Blocked
                } else if report_only.len() > report_only_before {
                    SurfaceOutcome::ReportOnly
                } else {
                    // Handler chose not to bucket this finding (e.g. RunMaintenance
                    // path where the plan had no work). Treat as report-only for
                    // observability without mutating the summary.
                    SurfaceOutcome::ReportOnly
                };
                emit_progress(
                    progress,
                    SyncProgress::SurfaceFinished {
                        surface: finding.surface,
                        outcome,
                    },
                );
            }
        }
    }

    // Why: resolve_pending_promotions touches the overlay; a silent Err here
    // would hide overlay corruption and leave stuck rows invisible to the
    // operator. Surface failure as a Blocked finding so `synrepo status` shows
    // it on the next run.
    match resolve_pending_promotions(synrepo_dir) {
        Ok(count) => {
            if count > 0 {
                actions_taken.push(format!("resolved {count} stuck pending_promotion row(s)"));
            }
        }
        Err(err) => {
            actions_taken.push(format!("resolve_pending_promotions failed: {err}"));
            blocked.push(RepairFinding {
                surface: RepairSurface::ProposedLinksOverlay,
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!(
                    "Could not resolve stuck pending_promotion rows: {err}"
                )),
            });
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

fn emit_progress(sink: &mut Option<&mut dyn FnMut(SyncProgress)>, event: SyncProgress) {
    if let Some(cb) = sink.as_deref_mut() {
        cb(event);
    }
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
        structure::graph::EdgeKind,
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
        // Why: previously `unwrap_or_default()` silently treated graph read
        // failures as "no edge exists", which would route the candidate to
        // reset_candidate_to_active even when the truth was unknown.
        // Propagate so the caller can surface a Blocked finding.
        let edge_exists = graph
            .outbound(from, Some(edge_kind))?
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
