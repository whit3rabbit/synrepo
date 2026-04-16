use std::path::Path;

use anyhow::anyhow;

use crate::{
    config::Config,
    pipeline::{
        maintenance::{execute_maintenance, plan_maintenance},
        watch::{persist_reconcile_state, ReconcileOutcome},
        writer::{acquire_writer_lock, now_rfc3339, LockError},
    },
};

use super::{
    append_resolution_log, cross_links::run_cross_link_generation, report::assemble_repair_report,
    DriftClass, RepairAction, RepairFinding, RepairSurface, ResolutionLogEntry, Severity,
    SyncOptions, SyncOutcome, SyncSummary,
};

/// Execute a targeted sync: repair auto-fixable findings from `build_repair_report`.
///
/// Acquires the writer lock for the duration of the sync so that concurrent
/// CLI invocations and watch-daemon reconcile passes cannot race against the
/// maintenance, edge-pruning, commentary-refresh, and structural-compile
/// steps. `record_reconcile_attempt` runs the structural compile directly
/// (bypassing the lock) because the lock is already held by this function.
///
/// Routes storage repairs through `plan_maintenance` / `execute_maintenance`
/// and structural refreshes through `run_structural_compile`. Report-only,
/// unsupported, and blocked findings are collected but left untouched.
/// Appends a resolution log entry to `.synrepo/state/repair-log.jsonl`.
pub fn execute_sync(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    options: SyncOptions,
) -> crate::Result<SyncSummary> {
    // Acquire the writer lock for the full sync duration. Any mutating
    // sub-step (maintenance, edge prune, commentary, structural compile)
    // is now covered without each step needing its own independent lock
    // acquisition — which would create re-entrant deadlocks for steps that
    // internally call run_reconcile_pass.
    let _writer_lock = acquire_writer_lock(synrepo_dir).map_err(|err| match err {
        LockError::HeldByOther { pid, .. } => anyhow::anyhow!(
            "sync: writer lock held by pid {pid}; wait for it to finish or stop the watch daemon"
        ),
        LockError::Io { path, source } => anyhow::anyhow!(
            "sync: could not acquire writer lock at {}: {source}",
            path.display()
        ),
    })?;

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

    // 2. Resolve stuck pending_promotion rows before any generation pass.
    if let Ok(count) = resolve_pending_promotions(synrepo_dir) {
        if count > 0 {
            actions_taken.push(format!("resolved {count} stuck pending_promotion row(s)"));
        }
    }

    // 3. Optional cross-link generation pass
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
            if finding.surface == super::RepairSurface::EdgeDrift {
                prune_dead_edges(finding, context, repaired, actions_taken)?;
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
        RepairAction::CompactRetired => {
            match compact_retired_observations(context, actions_taken) {
                Ok(()) => repaired.push(finding.clone()),
                Err(err) => {
                    actions_taken.push(format!("compaction failed: {err}"));
                    let mut blocked_finding = finding.clone();
                    blocked_finding.drift_class = super::DriftClass::Blocked;
                    blocked_finding.severity = Severity::Blocked;
                    blocked_finding.recommended_action = RepairAction::ManualReview;
                    blocked_finding.notes = Some(format!("Compaction failed: {err}"));
                    blocked.push(blocked_finding);
                }
            }
        }
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

fn prune_dead_edges(
    finding: &RepairFinding,
    context: &ActionContext<'_>,
    repaired: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    use crate::store::sqlite::SqliteGraphStore;
    use crate::structure::graph::GraphStore;

    let graph_dir = context.synrepo_dir.join("graph");
    let Ok(mut graph) = SqliteGraphStore::open_existing(&graph_dir) else {
        actions_taken.push("edge drift pruning skipped: graph store not found".to_string());
        return Ok(());
    };

    // Use the latest revision actually recorded in edge_drift. Inferring the
    // revision from file provenance is unreliable: files skipped by the
    // incremental compile retain their original (older) revision, so find_map
    // over file nodes returns a stale revision that truncate_drift_scores has
    // already removed \u2014 leaving read_drift_scores empty and edges un-pruned.
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

/// Run the structural compile directly, under the writer lock already held by
/// `execute_sync`. This avoids a double-acquisition of the writer lock that
/// would otherwise deadlock when `run_reconcile_pass` tries to acquire it.
fn record_reconcile_attempt(
    finding: &RepairFinding,
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    repaired: &mut Vec<RepairFinding>,
    blocked: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) {
    use crate::pipeline::structural::run_structural_compile;
    use crate::pipeline::watch::{emit_cochange_edges_pass, emit_symbol_revisions_pass};
    use crate::store::sqlite::SqliteGraphStore;

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
            if let Err(err) = crate::substrate::build_index(config, repo_root) {
                ReconcileOutcome::Failed(format!("index rebuild failed: {err}"))
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
        #[cfg(test)]
        ReconcileOutcome::LockConflict { holder_pid: _ } => {
            // This arm is only reachable in tests; production calls hold the lock.
            unreachable!("LockConflict cannot occur when lock is already held")
        }
        #[cfg(not(test))]
        ReconcileOutcome::LockConflict { .. } => {
            // Unreachable in production because execute_sync holds the writer lock.
            // This arm exists for exhaustiveness.
            unreachable!("LockConflict cannot occur when lock is already held")
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

/// Run compaction on retired graph observations older than the configured
/// retention window.
fn compact_retired_observations(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    use crate::store::sqlite::SqliteGraphStore;
    use crate::structure::graph::GraphStore;

    let graph_dir = context.synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open_existing(&graph_dir)?;

    // Determine the compaction threshold: current_rev - retain_retired_revisions.
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
    blocked.drift_class = super::DriftClass::Blocked;
    blocked.severity = Severity::Blocked;
    blocked.recommended_action = RepairAction::ManualReview;
    blocked.notes = Some(notes);
    blocked
}

/// Resolve cross-link rows stuck in `pending_promotion` state after a crash
/// during the `links_accept` three-phase commit. For each stuck row:
/// - If the corresponding graph edge exists, complete Phase 3 (mark promoted).
/// - If no graph edge exists, roll back Phase 1 (reset to active).
fn resolve_pending_promotions(synrepo_dir: &Path) -> crate::Result<usize> {
    use crate::core::ids::NodeId;
    use crate::overlay::OverlayEdgeKind;
    use crate::pipeline::structural::derive_edge_id;
    use crate::store::overlay::{parse_overlay_edge_kind, SqliteOverlayStore};
    use crate::store::sqlite::SqliteGraphStore;
    use crate::structure::graph::{EdgeKind, GraphStore};
    use std::str::FromStr;

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
