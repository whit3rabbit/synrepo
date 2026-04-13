use std::path::Path;

use anyhow::anyhow;

use crate::{
    config::Config,
    pipeline::{
        maintenance::{execute_maintenance, plan_maintenance},
        watch::{persist_reconcile_state, run_reconcile_pass, ReconcileOutcome},
        writer::now_rfc3339,
    },
};

use super::{
    append_resolution_log, cross_links::run_cross_link_generation, report::assemble_repair_report,
    DriftClass, RepairAction, RepairFinding, RepairSurface, ResolutionLogEntry, Severity,
    SyncOptions, SyncOutcome, SyncSummary,
};

/// Execute a targeted sync: repair auto-fixable findings from `build_repair_report`.
///
/// Routes storage repairs through `plan_maintenance` / `execute_maintenance`
/// and structural refreshes through `run_reconcile_pass`. Report-only,
/// unsupported, and blocked findings are collected but left untouched.
/// Appends a resolution log entry to `.synrepo/state/repair-log.jsonl`.
pub fn execute_sync(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    options: SyncOptions,
) -> crate::Result<SyncSummary> {
    let now = now_rfc3339();
    let maint_plan = plan_maintenance(synrepo_dir, config);
    let report = assemble_repair_report(synrepo_dir, config, &maint_plan);

    let mut repaired: Vec<RepairFinding> = Vec::new();
    let mut report_only: Vec<RepairFinding> = Vec::new();
    let mut blocked: Vec<RepairFinding> = Vec::new();
    let mut actions_taken: Vec<String> = Vec::new();

    // 1. Storage maintenance and structural repairs
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
                handle_actionable_finding(
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

    // 2. Optional cross-link generation pass
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

struct ActionContext<'a> {
    repo_root: &'a Path,
    synrepo_dir: &'a Path,
    config: &'a Config,
    maint_plan: &'a crate::Result<crate::pipeline::maintenance::MaintenancePlan>,
}

fn run_maintenance_if_needed(
    finding: &RepairFinding,
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let plan = context.maint_plan.as_ref().map_err(|e| anyhow!("{e}"))?;
    if plan.has_work() {
        execute_maintenance(context.synrepo_dir, plan)?;
        actions_taken.push(format!(
            "ran storage maintenance for {}",
            finding.surface.as_str()
        ));
    }
    Ok(())
}

fn handle_actionable_finding(
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
            run_maintenance_if_needed(finding, context, actions_taken)?;
            repaired.push(finding.clone());
        }
        RepairAction::RunMaintenanceThenReconcile => {
            run_maintenance_if_needed(finding, context, actions_taken)?;
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
        RepairAction::ManualReview | RepairAction::NotSupported => {
            report_only.push(finding.clone());
        }
        RepairAction::RevalidateLinks => {
            // PR 1 wires the dispatch path and records the intent on the
            // resolution log. The deterministic fuzzy-LCS verifier ships with
            // PR 2, alongside the cross-link generator and source-text
            // loading. Until then, stale candidates remain on disk with their
            // tier intact so a later revalidation pass can resolve them.
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
                blocked_finding.drift_class = super::DriftClass::Blocked;
                blocked_finding.severity = Severity::Blocked;
                blocked_finding.recommended_action = RepairAction::ManualReview;
                blocked_finding.notes = Some(format!("Export regeneration failed: {err}"));
                blocked.push(blocked_finding);
            }
        },
        RepairAction::RefreshCommentary => match refresh_commentary(context, actions_taken) {
            Ok(()) => repaired.push(finding.clone()),
            Err(err) => {
                actions_taken.push(format!(
                    "commentary refresh failed for {}: {err}",
                    finding.surface.as_str()
                ));
                let mut blocked_finding = finding.clone();
                blocked_finding.drift_class = super::DriftClass::Blocked;
                blocked_finding.severity = Severity::Blocked;
                blocked_finding.recommended_action = RepairAction::ManualReview;
                blocked_finding.notes = Some(format!("Commentary refresh failed: {err}"));
                blocked.push(blocked_finding);
            }
        },
    }
    Ok(())
}

/// Walk every commentary entry flagged as stale against the current graph
/// and re-run the configured `CommentaryGenerator` for it. Persists fresh
/// entries back to the overlay. If no API key is configured (generator is
/// NoOp) the pass completes with zero refreshes — stale entries are left
/// untouched rather than deleted, matching the "generation failure is not
/// drift" principle.
fn refresh_commentary(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    use super::commentary::resolve_commentary_node;
    use crate::core::ids::NodeId;
    use crate::overlay::{CommentaryProvenance, OverlayStore};
    use crate::pipeline::synthesis::{ClaudeCommentaryGenerator, CommentaryGenerator};
    use crate::store::overlay::SqliteOverlayStore;
    use crate::store::sqlite::SqliteGraphStore;
    use std::str::FromStr;

    let overlay_dir = context.synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open_existing(&overlay_dir)?;
    let graph = SqliteGraphStore::open_existing(&context.synrepo_dir.join("graph"))?;
    let generator: Box<dyn CommentaryGenerator> =
        ClaudeCommentaryGenerator::new_or_noop(context.config.commentary_cost_limit);

    let rows = overlay.commentary_hashes()?;
    let mut refreshed = 0usize;
    let mut skipped = 0usize;

    for (node_id_str, stored_hash) in rows {
        let Ok(node_id) = NodeId::from_str(&node_id_str) else {
            skipped += 1;
            continue;
        };
        let Some(snap) = resolve_commentary_node(&graph, node_id)? else {
            skipped += 1;
            continue;
        };
        if snap.content_hash == stored_hash {
            continue; // already fresh
        }

        let ctx_text = match &snap.symbol {
            Some(sym) => format!(
                "Symbol {} in {}\nSignature: {}\nDoc: {}",
                sym.qualified_name,
                snap.file.path,
                sym.signature.clone().unwrap_or_default(),
                sym.doc_comment.clone().unwrap_or_default(),
            ),
            None => format!("File: {}", snap.file.path),
        };

        let Some(mut entry) = generator.generate(node_id, &ctx_text)? else {
            skipped += 1;
            continue;
        };
        // Caller is responsible for stamping the hash; the generator trait
        // has no access to graph state.
        entry.provenance = CommentaryProvenance {
            source_content_hash: snap.content_hash,
            ..entry.provenance
        };
        overlay.insert_commentary(entry)?;
        refreshed += 1;
    }

    actions_taken.push(format!(
        "commentary refresh: {refreshed} regenerated, {skipped} skipped (no hash change or no generator output)"
    ));
    Ok(())
}

fn record_reconcile_attempt(
    finding: &RepairFinding,
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    repaired: &mut Vec<RepairFinding>,
    blocked: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) {
    let outcome = run_reconcile_pass(repo_root, config, synrepo_dir);
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
            actions_taken.push(format!(
                "skipped structural reconcile for {} because writer lock is held by pid {}",
                finding.surface.as_str(),
                holder_pid
            ));
            blocked.push(blocked_reconcile_finding(
                finding,
                format!("Reconcile skipped: writer lock held by pid {holder_pid}."),
            ));
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

/// Re-run export generation using the format/budget recorded in the existing
/// manifest, or markdown/normal if no manifest is present.
fn regenerate_exports(
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

fn blocked_reconcile_finding(finding: &RepairFinding, notes: String) -> RepairFinding {
    let mut blocked = finding.clone();
    blocked.drift_class = super::DriftClass::Blocked;
    blocked.severity = Severity::Blocked;
    blocked.recommended_action = RepairAction::ManualReview;
    blocked.notes = Some(notes);
    blocked
}
