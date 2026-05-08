use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::{
        maintenance::apply_compatibility_report,
        writer::{acquire_write_admission, map_lock_error},
    },
    store::compatibility::{evaluate_runtime, CompatAction},
};

/// Print a dry-run compatibility plan or execute it with `--apply`.
pub(crate) fn upgrade(repo_root: &Path, apply: bool) -> anyhow::Result<()> {
    let config = Config::load(repo_root)
        .map_err(|e| anyhow::anyhow!("upgrade: not initialized: run `synrepo init` first ({e})"))?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

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
        if entry.action == CompatAction::Block {
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
    }

    let has_work = report
        .entries
        .iter()
        .any(|entry| entry.action != CompatAction::Continue);
    if !has_work {
        println!("\nAll stores are compatible. No upgrade needed.");
        return Ok(());
    }

    let _lock = acquire_write_admission(&synrepo_dir, "upgrade")
        .map_err(|err| map_lock_error("upgrade", err))?;

    let needs_reconcile = report
        .entries
        .iter()
        .any(|e| e.action == CompatAction::Rebuild);
    if needs_reconcile {
        println!("  Running structural reconcile to repopulate rebuilt stores...");
    }

    let summary = apply_compatibility_report(repo_root, &config, &synrepo_dir, &report, &_lock)
        .map_err(|e| anyhow::anyhow!("upgrade: failed to apply actions: {e}"))?;

    println!();
    for applied in &summary.applied {
        println!(
            "  {} {}: cleared",
            applied.action.as_str(),
            applied.store_id.as_str()
        );
    }

    if let Some(outcome) = summary.reconcile_outcome {
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
