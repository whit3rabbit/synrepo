use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::writer::{acquire_writer_lock, LockError},
    store::compatibility::{apply_runtime_actions, evaluate_runtime, CompatAction, StoreId},
};

use super::watch::ensure_watch_not_running;

/// Print a dry-run compatibility plan or execute it with `--apply`.
pub(crate) fn upgrade(repo_root: &Path, apply: bool) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|e| {
        anyhow::anyhow!("upgrade: not initialized — run `synrepo init` first ({e})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "upgrade")?;

    let report = evaluate_runtime(&synrepo_dir, synrepo_dir.exists(), &config)
        .map_err(|e| anyhow::anyhow!("upgrade: compatibility evaluation failed: {e}"))?;

    // Print plan table
    println!("{:<22} {:<16} Reason", "Store", "Action");
    println!("{}", "-".repeat(72));
    for entry in &report.entries {
        println!(
            "{:<22} {:<16} {}",
            entry.store_id.as_str(),
            entry.action.as_str(),
            entry.reason
        );
    }
    for warning in &report.warnings {
        println!("  advisory: {warning}");
    }

    if !apply {
        let has_work = report
            .entries
            .iter()
            .any(|e| e.action != CompatAction::Continue);
        if has_work {
            println!("\nDry run. Run `synrepo upgrade --apply` to execute these actions.");
        } else {
            println!("\nAll stores are compatible. No upgrade needed.");
        }
        return Ok(());
    }

    // Check for blocking actions first before mutating anything.
    for entry in &report.entries {
        match entry.action {
            CompatAction::Block | CompatAction::MigrateRequired => {
                anyhow::bail!(
                    "upgrade blocked: {} requires manual intervention ({}: {})\n\
                     Recovery: remove `.synrepo/` and run `synrepo init` to rebuild from scratch.\n\
                     If this is a graph store with a newer format version, downgrade the binary \
                     or delete `.synrepo/graph/` and re-run `synrepo init`.",
                    entry.store_id.as_str(),
                    entry.action.as_str(),
                    entry.reason
                );
            }
            _ => {}
        }
    }

    // Execute non-blocking actions in dependency order: index → graph → overlay → others.
    let ordered: Vec<StoreId> = [
        StoreId::Index,
        StoreId::Graph,
        StoreId::Overlay,
        StoreId::Embeddings,
        StoreId::LlmResponsesCache,
        StoreId::State,
    ]
    .into_iter()
    .filter(|id| {
        report
            .entries
            .iter()
            .any(|e| e.store_id == *id && e.action != CompatAction::Continue)
    })
    .collect();

    if ordered.is_empty() {
        println!("\nAll stores are compatible. No upgrade needed.");
        return Ok(());
    }

    // Acquire the writer lock for the duration of the upgrade.
    let _lock = acquire_writer_lock(&synrepo_dir).map_err(|err| match err {
        LockError::HeldByOther { pid, .. } => anyhow::anyhow!(
            "upgrade: writer lock held by pid {pid}; wait for it to finish or stop the watch daemon"
        ),
        LockError::Io { path, source } => anyhow::anyhow!(
            "upgrade: could not acquire writer lock at {}: {source}",
            path.display()
        ),
    })?;

    apply_runtime_actions(&synrepo_dir, &report)
        .map_err(|e| anyhow::anyhow!("upgrade: failed to apply actions: {e}"))?;

    println!();
    for id in &ordered {
        let entry = report.entries.iter().find(|e| e.store_id == *id).unwrap();
        println!("  {} {}: cleared", entry.action.as_str(), id.as_str());
    }

    // If any Rebuild action was applied, run a reconcile pass to repopulate.
    let needs_reconcile = report
        .entries
        .iter()
        .any(|e| e.action == CompatAction::Rebuild);
    if needs_reconcile {
        println!("  Running structural reconcile to repopulate rebuilt stores...");
        use synrepo::pipeline::structural::run_structural_compile;
        use synrepo::pipeline::watch::{
            emit_cochange_edges_pass, emit_symbol_revisions_pass, persist_reconcile_state,
            ReconcileOutcome,
        };
        use synrepo::store::sqlite::SqliteGraphStore;

        let graph_dir = synrepo_dir.join("graph");
        let mut graph = SqliteGraphStore::open(&graph_dir)?;

        let outcome = match run_structural_compile(repo_root, &config, &mut graph) {
            Ok(summary) => {
                if let Err(err) = emit_cochange_edges_pass(repo_root, &config, &mut graph) {
                    tracing::warn!(error = %err, "co-change edge emission failed; continuing");
                }
                if let Err(err) = emit_symbol_revisions_pass(repo_root, &config, &mut graph) {
                    tracing::warn!(error = %err, "symbol revision derivation failed; continuing");
                }
                if let Err(err) = synrepo::substrate::build_index(&config, repo_root) {
                    ReconcileOutcome::Failed(format!("index rebuild failed: {err}"))
                } else {
                    ReconcileOutcome::Completed(summary)
                }
            }
            Err(err) => ReconcileOutcome::Failed(err.to_string()),
        };

        persist_reconcile_state(&synrepo_dir, &outcome, 0);
        report_reconcile_outcome(outcome)?;
    }

    println!("Upgrade complete.");
    Ok(())
}

pub(crate) fn report_reconcile_outcome(
    outcome: synrepo::pipeline::watch::ReconcileOutcome,
) -> anyhow::Result<()> {
    match outcome {
        synrepo::pipeline::watch::ReconcileOutcome::Completed(_) => {
            println!("  Reconcile completed.");
            Ok(())
        }
        synrepo::pipeline::watch::ReconcileOutcome::Failed(msg) => {
            anyhow::bail!("upgrade: reconcile after rebuild failed: {msg}");
        }
        synrepo::pipeline::watch::ReconcileOutcome::LockConflict { holder_pid } => {
            anyhow::bail!(
                "upgrade: reconcile skipped because writer lock is held by pid {holder_pid}"
            );
        }
    }
}
