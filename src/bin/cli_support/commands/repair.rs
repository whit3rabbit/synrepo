use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::{
        explain::{accounting, telemetry},
        repair::{build_repair_report, execute_sync, SyncOptions},
        watch::{request_watch_control, WatchControlRequest, WatchControlResponse},
    },
};

/// Report drift across all repair surfaces. Read-only; never mutates state.
pub(crate) fn check(repo_root: &Path, json_output: bool) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|error| {
        anyhow::anyhow!("check: not initialized — run `synrepo init` first ({error})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let report = build_repair_report(&synrepo_dir, &config);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.render());
    }

    if report.has_blocked() {
        return Err(anyhow::anyhow!(
            "check: blocked findings require manual intervention"
        ));
    }
    if report.has_actionable() {
        return Err(anyhow::anyhow!(
            "check: actionable findings detected — run `synrepo sync` to repair"
        ));
    }
    Ok(())
}

/// Repair auto-fixable drift surfaces and record the outcome.
pub(crate) fn sync(
    repo_root: &Path,
    json_output: bool,
    generate_cross_links: bool,
    regenerate_cross_links: bool,
    reset_explain_totals: bool,
) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|error| {
        anyhow::anyhow!("sync: not initialized — run `synrepo init` first ({error})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    telemetry::set_synrepo_dir(&synrepo_dir);

    if reset_explain_totals {
        accounting::reset(&synrepo_dir)
            .map_err(|error| anyhow::anyhow!("sync: failed to reset explain totals ({error})"))?;
        if json_output {
            println!("{}", serde_json::json!({ "reset_explain_totals": true }));
        } else {
            println!("Explain totals reset. Call log rotated to `.bak` with timestamp.");
        }
        return Ok(());
    }

    let options = SyncOptions {
        generate_cross_links,
        regenerate_cross_links,
    };
    let summary = execute_sync(repo_root, &synrepo_dir, &config, options)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        print!("{}", summary.render());
    }

    if !summary.blocked.is_empty() {
        return Err(anyhow::anyhow!(
            "sync: {} finding(s) could not be repaired (blocked)",
            summary.blocked.len()
        ));
    }
    Ok(())
}

/// Reconcile the structural graph and update the index.
pub(crate) fn reconcile(repo_root: &Path) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

    if let Some(pid) = super::watch::active_watch_pid(&synrepo_dir)? {
        match request_watch_control(&synrepo_dir, WatchControlRequest::ReconcileNow)? {
            WatchControlResponse::Reconcile {
                outcome,
                triggering_events,
            } => {
                println!("Delegated reconcile to active watch service (pid {pid}).");
                report_reconcile_outcome(&outcome, triggering_events)
            }
            WatchControlResponse::Error { message } => {
                Err(anyhow::anyhow!("reconcile delegation failed: {message}"))
            }
            other => Err(anyhow::anyhow!(
                "reconcile delegation returned unexpected response: {:?}",
                other
            )),
        }
    } else {
        let outcome =
            synrepo::pipeline::watch::run_reconcile_pass(repo_root, &config, &synrepo_dir);
        synrepo::pipeline::watch::persist_reconcile_state(&synrepo_dir, &outcome, 0);
        report_reconcile_outcome(&outcome, 0)
    }
}

use synrepo::pipeline::watch::ReconcileOutcome;

fn report_reconcile_outcome(
    outcome: &ReconcileOutcome,
    triggering_events: usize,
) -> anyhow::Result<()> {
    match outcome {
        ReconcileOutcome::Completed(summary) => {
            println!(
                "Reconcile outcome: completed\n  files discovered: {}\n  symbols extracted: {}\n  triggering events: {}",
                summary.files_discovered, summary.symbols_extracted, triggering_events
            );
            Ok(())
        }
        ReconcileOutcome::LockConflict { holder_pid } => Err(anyhow::anyhow!(
            "Reconcile skipped: writer lock held by pid {holder_pid}. Wait for that process to finish, then retry."
        )),
        ReconcileOutcome::Failed(message) => Err(anyhow::anyhow!("Reconcile failed: {message}")),
    }
}
