//! Action handlers extracted from sync.rs.
//!
//! These handle the various repair actions for auto-fixable findings.

use anyhow::anyhow;
use std::path::Path;

use crate::{config::Config, pipeline::maintenance::execute_maintenance};

use crate::pipeline::repair::{
    DriftClass, RepairAction, RepairFinding, RepairSurface, Severity, SyncProgress,
};

use super::commentary_plan::{CommentaryProgressEvent, CommentaryWorkPhase};

/// Context for action handlers.
pub struct ActionContext<'a> {
    /// Repository root (absolute path).
    pub repo_root: &'a Path,
    /// `.synrepo/` directory for the repo.
    pub synrepo_dir: &'a Path,
    /// Loaded runtime config.
    pub config: &'a Config,
    /// Pre-computed maintenance plan shared across repair actions.
    pub maint_plan: &'a crate::Result<crate::pipeline::maintenance::MaintenancePlan>,
}

/// Run maintenance if the plan has work.
pub fn run_maintenance_if_needed(
    synrepo_dir: &Path,
    maint_plan: &crate::Result<crate::pipeline::maintenance::MaintenancePlan>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let plan = maint_plan.as_ref().map_err(|e| anyhow!("{e}"))?;
    if plan.has_work() {
        execute_maintenance(synrepo_dir, plan)?;
        actions_taken.push("ran maintenance".to_string());
    }
    Ok(())
}

/// Handle actionable finding based on recommended action.
#[allow(clippy::too_many_arguments)]
pub fn handle_actionable_finding(
    finding: &RepairFinding,
    context: &ActionContext<'_>,
    repaired: &mut Vec<RepairFinding>,
    report_only: &mut Vec<RepairFinding>,
    blocked: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
    progress: &mut Option<&mut dyn FnMut(SyncProgress)>,
) -> crate::Result<()> {
    let _span = tracing::info_span!(
        "sync_surface",
        surface = finding.surface.as_str(),
        action = finding.recommended_action.as_str()
    )
    .entered();

    match finding.recommended_action {
        RepairAction::None => {}
        RepairAction::RunMaintenance => {
            run_maintenance_if_needed(context.synrepo_dir, context.maint_plan, actions_taken)?;
            repaired.push(finding.clone());
        }
        RepairAction::RunMaintenanceThenReconcile => {
            run_maintenance_if_needed(context.synrepo_dir, context.maint_plan, actions_taken)?;
            super::reconcile_handler::record_reconcile_attempt(
                finding,
                context.repo_root,
                context.synrepo_dir,
                context.config,
                repaired,
                blocked,
                actions_taken,
            );
        }
        RepairAction::RunReconcile => {
            if finding.surface == RepairSurface::EdgeDrift {
                super::graph_maintenance::prune_dead_edges(
                    finding,
                    context.synrepo_dir,
                    repaired,
                    actions_taken,
                )?;
            } else {
                super::reconcile_handler::record_reconcile_attempt(
                    finding,
                    context.repo_root,
                    context.synrepo_dir,
                    context.config,
                    repaired,
                    blocked,
                    actions_taken,
                );
            }
        }
        RepairAction::ManualReview | RepairAction::NotSupported => {
            report_only.push(finding.clone());
        }
        RepairAction::RevalidateLinks => {
            super::revalidate_links::handle_revalidate_links(
                finding,
                context,
                repaired,
                report_only,
                blocked,
                actions_taken,
            );
        }
        RepairAction::RegenerateExports => {
            match super::export_regen::regenerate_exports(context, actions_taken) {
                Ok(()) => repaired.push(finding.clone()),
                Err(err) => {
                    actions_taken.push(format!(
                        "export regeneration failed for {}: {err}",
                        finding.surface.as_str()
                    ));
                    let mut blocked_finding = finding.clone();
                    blocked_finding.drift_class = DriftClass::Blocked;
                    blocked_finding.severity = Severity::Blocked;
                    blocked_finding.recommended_action = RepairAction::ManualReview;
                    blocked_finding.notes = Some(format!("Export regeneration failed: {err}"));
                    blocked.push(blocked_finding);
                }
            }
        }
        RepairAction::RevalidateAgentNotes => {
            match mark_agent_note_stale(finding, context.synrepo_dir, actions_taken) {
                Ok(()) => repaired.push(finding.clone()),
                Err(err) => {
                    actions_taken.push(format!("agent-note revalidation failed: {err}"));
                    let mut blocked_finding = finding.clone();
                    blocked_finding.drift_class = DriftClass::Blocked;
                    blocked_finding.severity = Severity::Blocked;
                    blocked_finding.recommended_action = RepairAction::ManualReview;
                    blocked_finding.notes = Some(format!("Agent-note revalidation failed: {err}"));
                    blocked.push(blocked_finding);
                }
            }
        }
        RepairAction::CompactRetired => {
            match super::graph_maintenance::compact_retired_observations(context, actions_taken) {
                Ok(()) => repaired.push(finding.clone()),
                Err(err) => {
                    actions_taken.push(format!("compaction failed: {err}"));
                    let mut blocked_finding = finding.clone();
                    blocked_finding.drift_class = DriftClass::Blocked;
                    blocked_finding.severity = Severity::Blocked;
                    blocked_finding.recommended_action = RepairAction::ManualReview;
                    blocked_finding.notes = Some(format!("Compaction failed: {err}"));
                    blocked.push(blocked_finding);
                }
            }
        }
        RepairAction::RefreshCommentary => {
            let result = if let Some(sink) = progress.as_deref_mut() {
                let mut adapter = |event: CommentaryProgressEvent| {
                    if let Some(mapped) = commentary_event_to_sync_progress(&event) {
                        sink(mapped);
                    }
                };
                super::commentary::refresh_commentary(
                    context,
                    actions_taken,
                    None,
                    Some(&mut adapter),
                    None,
                )
            } else {
                super::commentary::refresh_commentary(context, actions_taken, None, None, None)
            };
            crate::pipeline::context_metrics::record_commentary_refresh_best_effort(
                context.synrepo_dir,
                result.is_err(),
            );
            match result {
                Ok(()) => repaired.push(finding.clone()),
                Err(err) => {
                    actions_taken.push(format!(
                        "commentary refresh failed for {}: {err}",
                        finding.surface.as_str()
                    ));
                    let mut blocked_finding = finding.clone();
                    blocked_finding.drift_class = DriftClass::Blocked;
                    blocked_finding.severity = Severity::Blocked;
                    blocked_finding.recommended_action = RepairAction::ManualReview;
                    blocked_finding.notes = Some(format!("Commentary refresh failed: {err}"));
                    blocked.push(blocked_finding);
                }
            }
        }
    }
    Ok(())
}

/// Adapt the internal commentary progress stream to the wire-serializable
/// [`SyncProgress`] stream. Low-signal events (e.g. docs dir creation) are
/// dropped because they do not meaningfully help operators watching sync.
fn commentary_event_to_sync_progress(event: &CommentaryProgressEvent) -> Option<SyncProgress> {
    match event {
        CommentaryProgressEvent::PlanReady {
            refresh,
            file_seeds,
            symbol_seed_candidates,
            ..
        } => Some(SyncProgress::CommentaryPlan {
            refresh: *refresh,
            file_seeds: *file_seeds,
            symbol_seed_candidates: *symbol_seed_candidates,
        }),
        CommentaryProgressEvent::TargetFinished {
            item,
            current,
            generated,
            skip_reason,
            skip_message,
            retry_attempts,
            queued_for_next_run,
        } => Some(SyncProgress::CommentaryItem {
            target: Some(item.path.clone()),
            current: *current,
            generated: *generated,
            reason: skip_reason.map(|reason| reason.as_str().to_string()),
            message: skip_message.clone(),
            retry_attempts: *retry_attempts,
            queued_for_next_run: *queued_for_next_run,
        }),
        CommentaryProgressEvent::RunSummary {
            refreshed,
            seeded,
            not_generated,
            attempted,
            stopped,
            queued_for_next_run,
            skip_reasons,
        } => Some(SyncProgress::CommentarySummary {
            refreshed: *refreshed,
            seeded: *seeded,
            not_generated: *not_generated,
            attempted: *attempted,
            stopped: *stopped,
            queued_for_next_run: *queued_for_next_run,
            skip_reasons: skip_reasons.clone(),
        }),
        CommentaryProgressEvent::ScanProgress { .. }
        | CommentaryProgressEvent::TargetStarted { .. }
        | CommentaryProgressEvent::DocsDirCreated { .. }
        | CommentaryProgressEvent::DocWritten { .. }
        | CommentaryProgressEvent::DocDeleted { .. }
        | CommentaryProgressEvent::IndexDirCreated { .. }
        | CommentaryProgressEvent::IndexUpdated { .. }
        | CommentaryProgressEvent::IndexRebuilt { .. }
        | CommentaryProgressEvent::PhaseSummary { .. } => {
            let _ = CommentaryWorkPhase::Refresh;
            None
        }
    }
}

fn mark_agent_note_stale(
    finding: &RepairFinding,
    synrepo_dir: &Path,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    use crate::overlay::OverlayStore;
    use crate::store::overlay::SqliteOverlayStore;

    let Some(note_id) = finding.target_id.as_ref() else {
        return Ok(());
    };
    let mut overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let changed = overlay.mark_stale_notes(std::slice::from_ref(note_id), "synrepo-sync")?;
    actions_taken.push(format!("marked {changed} agent note(s) stale"));
    Ok(())
}
