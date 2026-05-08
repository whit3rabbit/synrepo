use std::path::Path;

use crate::{
    config::Config,
    pipeline::{
        structural::run_structural_compile,
        watch::{
            emit_cochange_edges_pass, emit_symbol_revisions_pass, persist_reconcile_state,
            ReconcileOutcome,
        },
        writer::WriterLock,
    },
    store::{
        compatibility::{self, CompatAction, CompatibilityReport, StoreId},
        sqlite::SqliteGraphStore,
    },
};

/// One compatibility action applied to a runtime store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityAppliedAction {
    /// Store that was cleared for rebuild/invalidation.
    pub store_id: StoreId,
    /// Compatibility action that required clearing the store.
    pub action: CompatAction,
}

/// Result of applying non-blocking compatibility actions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityApplySummary {
    /// Store actions applied in dependency order.
    pub applied: Vec<CompatibilityAppliedAction>,
    /// Reconcile outcome when rebuild actions required repopulation.
    pub reconcile_outcome: Option<ReconcileOutcome>,
    /// True when the runtime snapshot was updated to the current config.
    pub snapshot_written: bool,
}

impl CompatibilityApplySummary {
    /// Return true when any store was changed.
    pub fn has_work(&self) -> bool {
        !self.applied.is_empty()
    }
}

/// Apply a precomputed compatibility report using the caller's writer lock.
///
/// This is the shared implementation behind `synrepo upgrade --apply` and the
/// dashboard's compatibility action. It clears non-blocking stale stores,
/// repopulates graph/index surfaces for rebuild actions, and writes a fresh
/// compatibility snapshot only after successful rebuild work.
pub fn apply_compatibility_report(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    report: &CompatibilityReport,
    lock: &WriterLock,
) -> crate::Result<CompatibilityApplySummary> {
    if let Some(entry) = report
        .entries
        .iter()
        .find(|entry| entry.action == CompatAction::Block)
    {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "{} requires manual intervention ({}: {})",
            entry.store_id.as_str(),
            entry.action.as_str(),
            entry.reason
        )));
    }

    let applied = ordered_applied_actions(report);
    if applied.is_empty() {
        return Ok(CompatibilityApplySummary {
            applied,
            reconcile_outcome: None,
            snapshot_written: false,
        });
    }

    compatibility::apply_runtime_actions(lock, synrepo_dir, report)?;

    let needs_reconcile = report
        .entries
        .iter()
        .any(|entry| entry.action == CompatAction::Rebuild);
    let reconcile_outcome = needs_reconcile.then(|| {
        let outcome = rebuild_runtime_surfaces(repo_root, config, synrepo_dir);
        persist_reconcile_state(synrepo_dir, &outcome, 0);
        outcome
    });

    let rebuild_failed = matches!(
        reconcile_outcome,
        Some(ReconcileOutcome::Failed(_)) | Some(ReconcileOutcome::LockConflict { .. })
    );
    let snapshot_written = if rebuild_failed {
        false
    } else {
        compatibility::write_runtime_snapshot(synrepo_dir, config)?;
        true
    };

    Ok(CompatibilityApplySummary {
        applied,
        reconcile_outcome,
        snapshot_written,
    })
}

/// Return non-blocking compatibility actions in the order they should run.
pub fn ordered_applied_actions(report: &CompatibilityReport) -> Vec<CompatibilityAppliedAction> {
    [
        StoreId::Index,
        StoreId::Graph,
        StoreId::Overlay,
        StoreId::Embeddings,
        StoreId::LlmResponsesCache,
        StoreId::State,
    ]
    .into_iter()
    .filter_map(|store_id| {
        report
            .entries
            .iter()
            .find(|entry| entry.store_id == store_id && entry.action != CompatAction::Continue)
            .filter(|entry| entry.action != CompatAction::Block)
            .map(|entry| CompatibilityAppliedAction {
                store_id: entry.store_id,
                action: entry.action,
            })
    })
    .collect()
}

fn rebuild_runtime_surfaces(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
) -> ReconcileOutcome {
    let graph_dir = synrepo_dir.join("graph");
    let mut graph = match SqliteGraphStore::open(&graph_dir) {
        Ok(graph) => graph,
        Err(err) => return ReconcileOutcome::Failed(err.to_string()),
    };

    match run_structural_compile(repo_root, config, &mut graph) {
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
    }
}
