//! Structural reconcile repair handler.

use std::path::Path;

use crate::{
    config::Config,
    pipeline::{
        repair::{DriftClass, RepairAction, RepairFinding, Severity},
        structural::run_structural_compile,
        watch::{
            emit_cochange_edges_pass, emit_symbol_revisions_pass, finish_runtime_surfaces,
            persist_reconcile_state, ReconcileOutcome, RepoIndexStrategy,
        },
    },
    store::sqlite::SqliteGraphStore,
};

/// Record a reconcile attempt and persist state.
pub(super) fn record_reconcile_attempt(
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
            // Why: these derived layers may fail without invalidating the
            // structural graph refresh.
            if let Err(err) = emit_cochange_edges_pass(repo_root, config, &mut graph) {
                tracing::warn!(error = %err, "co-change edge emission failed; continuing");
                actions_taken.push(format!("co-change edge emission failed: {err}"));
            }
            if let Err(err) = emit_symbol_revisions_pass(repo_root, config, &mut graph) {
                tracing::warn!(error = %err, "symbol revision derivation failed; continuing");
                actions_taken.push(format!("symbol revision derivation failed: {err}"));
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

fn blocked_reconcile_finding(finding: &RepairFinding, notes: String) -> RepairFinding {
    let mut blocked = finding.clone();
    blocked.drift_class = DriftClass::Blocked;
    blocked.severity = Severity::Blocked;
    blocked.recommended_action = RepairAction::ManualReview;
    blocked.notes = Some(notes);
    blocked
}
