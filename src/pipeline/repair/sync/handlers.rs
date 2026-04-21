//! Action handlers extracted from sync.rs.
//!
//! These handle the various repair actions for auto-fixable findings.

use anyhow::anyhow;
use std::path::Path;

use crate::{
    config::Config,
    pipeline::{
        maintenance::execute_maintenance,
        structural::run_structural_compile,
        watch::{
            emit_cochange_edges_pass, emit_symbol_revisions_pass, finish_runtime_surfaces,
            persist_reconcile_state, ReconcileOutcome, RepoIndexStrategy,
        },
    },
    store::sqlite::SqliteGraphStore,
    structure::graph::GraphStore,
};

use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

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
pub fn handle_actionable_finding(
    finding: &RepairFinding,
    context: &ActionContext<'_>,
    repaired: &mut Vec<RepairFinding>,
    report_only: &mut Vec<RepairFinding>,
    blocked: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    match finding.recommended_action {
        RepairAction::None => {}
        RepairAction::RunMaintenance => {
            run_maintenance_if_needed(context.synrepo_dir, context.maint_plan, actions_taken)?;
            repaired.push(finding.clone());
        }
        RepairAction::RunMaintenanceThenReconcile => {
            run_maintenance_if_needed(context.synrepo_dir, context.maint_plan, actions_taken)?;
            record_reconcile_attempt(
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
                prune_dead_edges(finding, context.synrepo_dir, repaired, actions_taken)?;
            } else {
                record_reconcile_attempt(
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
            // Revalidation deferred — the fuzzy-LCS verifier is not yet wired.
            // Stale candidates remain on disk with their tier intact.
            actions_taken.push(format!(
                "cross-link revalidation deferred for {}: verifier not yet wired",
                finding.surface.as_str()
            ));
            report_only.push(finding.clone());
        }
        RepairAction::RegenerateExports => match regenerate_exports(context, actions_taken) {
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
        },
        RepairAction::CompactRetired => {
            match compact_retired_observations(context, actions_taken) {
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
            match super::commentary::refresh_commentary(context, actions_taken, None) {
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

/// Prune edges with drift score of 1.0 (dead edges).
pub fn prune_dead_edges(
    finding: &RepairFinding,
    synrepo_dir: &Path,
    repaired: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let graph_dir = synrepo_dir.join("graph");
    let Ok(mut graph) = SqliteGraphStore::open_existing(&graph_dir) else {
        actions_taken.push("edge drift pruning skipped: graph store not found".to_string());
        return Ok(());
    };

    // Use the latest revision recorded in edge_drift.
    let Some(revision) = graph.latest_drift_revision()? else {
        return Ok(());
    };

    let scores = graph.read_drift_scores(&revision)?;
    let dead: Vec<_> = scores
        .iter()
        .filter(|(_, score)| (*score - 1.0).abs() < f32::EPSILON)
        .collect();

    if dead.is_empty() {
        return Ok(());
    }

    let mut pruned = 0;
    for (edge_id, _) in &dead {
        if graph.delete_edge(*edge_id).is_ok() {
            pruned += 1;
        }
    }

    actions_taken.push(format!("pruned {pruned} dead edges (drift 1.0)"));
    repaired.push(finding.clone());
    Ok(())
}

/// Record a reconcile attempt and persist state.
pub fn record_reconcile_attempt(
    finding: &RepairFinding,
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    repaired: &mut Vec<RepairFinding>,
    blocked: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) {
    let graph_dir = synrepo_dir.join("graph");
    let mut graph = match SqliteGraphStore::open(&graph_dir) {
        Ok(g) => g,
        Err(err) => {
            let message = err.to_string();
            actions_taken.push(format!(
                "structural reconcile for {} failed to open graph: {}",
                finding.surface.as_str(),
                message
            ));
            blocked.push(blocked_reconcile_finding(
                finding,
                format!("Reconcile failed: could not open graph store: {message}"),
            ));
            return;
        }
    };

    let outcome = match run_structural_compile(repo_root, config, &mut graph) {
        Ok(summary) => {
            if let Err(err) = emit_cochange_edges_pass(repo_root, config, &mut graph) {
                tracing::warn!(error = %err, "co-change edge emission failed; continuing");
            }
            if let Err(err) = emit_symbol_revisions_pass(repo_root, config, &mut graph) {
                tracing::warn!(error = %err, "symbol revision derivation failed; continuing");
            }
            if let Err(err) = finish_runtime_surfaces(
                repo_root,
                config,
                synrepo_dir,
                &graph,
                RepoIndexStrategy::FullRebuild,
            ) {
                ReconcileOutcome::Failed(format!("surface maintenance failed: {err}"))
            } else {
                ReconcileOutcome::Completed(summary)
            }
        }
        Err(err) => ReconcileOutcome::Failed(err.to_string()),
    };

    persist_reconcile_state(synrepo_dir, &outcome, 0);
    match outcome {
        ReconcileOutcome::Completed(_) => {
            actions_taken.push(format!(
                "ran structural reconcile for {}",
                finding.surface.as_str()
            ));
            repaired.push(finding.clone());
        }
        ReconcileOutcome::LockConflict { holder_pid } => {
            let message =
                format!("unexpected lock conflict with PID {holder_pid} while holding writer lock");
            tracing::error!(%message);
            blocked.push(blocked_reconcile_finding(finding, message));
        }
        ReconcileOutcome::Failed(message) => {
            actions_taken.push(format!(
                "structural reconcile for {} failed: {}",
                finding.surface.as_str(),
                message
            ));
            blocked.push(blocked_reconcile_finding(
                finding,
                format!(
                    "Reconcile failed while repairing {}: {message}",
                    finding.surface.as_str()
                ),
            ));
        }
    }
}

/// Re-run export generation.
pub fn regenerate_exports(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    use crate::pipeline::export::{load_manifest, write_exports, ExportFormat};
    use crate::surface::card::Budget;

    let existing = load_manifest(context.repo_root, context.config);
    let format = existing
        .as_ref()
        .map(|m| m.format)
        .unwrap_or(ExportFormat::Markdown);
    let budget = existing
        .as_ref()
        .and_then(|m| match m.budget.as_str() {
            "deep" => Some(Budget::Deep),
            "normal" => Some(Budget::Normal),
            _ => None,
        })
        .unwrap_or(Budget::Normal);

    write_exports(
        context.repo_root,
        context.synrepo_dir,
        context.config,
        format,
        budget,
        false,
    )
    .map_err(|e| anyhow!("{e}"))?;

    actions_taken.push(format!(
        "regenerated export directory (format={}, budget={})",
        format.as_str(),
        match budget {
            Budget::Tiny => "tiny",
            Budget::Normal => "normal",
            Budget::Deep => "deep",
        }
    ));
    Ok(())
}

/// Run compaction on retired observations.
pub fn compact_retired_observations(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let graph_dir = context.synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open_existing(&graph_dir)?;

    let current_rev = graph.next_compile_revision()?;
    let retain = context.config.retain_retired_revisions;
    if current_rev <= retain {
        actions_taken.push("compaction skipped: not enough revisions yet".to_string());
        return Ok(());
    }
    let threshold = current_rev - retain;
    let summary = graph.compact_retired(threshold)?;

    actions_taken.push(format!(
        "compaction: removed {} retired symbols, {} retired edges, {} old revisions",
        summary.symbols_removed, summary.edges_removed, summary.revisions_removed
    ));
    Ok(())
}

fn blocked_reconcile_finding(finding: &RepairFinding, notes: String) -> RepairFinding {
    let mut blocked = finding.clone();
    blocked.drift_class = DriftClass::Blocked;
    blocked.severity = Severity::Blocked;
    blocked.recommended_action = RepairAction::ManualReview;
    blocked.notes = Some(notes);
    blocked
}
