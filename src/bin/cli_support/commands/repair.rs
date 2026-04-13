use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::{
        repair::{build_repair_report, execute_sync, SyncOptions},
        watch::{
            load_reconcile_state, request_watch_control, WatchControlRequest, WatchControlResponse,
        },
    },
};

use super::watch::ensure_watch_not_running;

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
) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|error| {
        anyhow::anyhow!("sync: not initialized — run `synrepo init` first ({error})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "sync")?;

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
            WatchControlResponse::Status { .. } => {
                println!("Delegated reconcile to active watch service (pid {pid}).");
                print_reconcile_summary(&synrepo_dir)
            }
            WatchControlResponse::Ack { message } => Err(anyhow::anyhow!(
                "reconcile delegation did not return a status snapshot: {message}"
            )),
            WatchControlResponse::Error { message } => {
                Err(anyhow::anyhow!("reconcile delegation failed: {message}"))
            }
        }
    } else {
        let outcome =
            synrepo::pipeline::watch::run_reconcile_pass(repo_root, &config, &synrepo_dir);
        synrepo::pipeline::watch::persist_reconcile_state(&synrepo_dir, &outcome, 0);
        match &outcome {
            synrepo::pipeline::watch::ReconcileOutcome::Completed(_) => print_reconcile_summary(&synrepo_dir),
            synrepo::pipeline::watch::ReconcileOutcome::LockConflict { holder_pid } => Err(anyhow::anyhow!(
                "Reconcile skipped: writer lock held by pid {holder_pid}. Wait for that process to finish, then retry."
            )),
            synrepo::pipeline::watch::ReconcileOutcome::Failed(message) => {
                Err(anyhow::anyhow!("Reconcile failed: {message}"))
            }
        }
    }
}

fn print_reconcile_summary(synrepo_dir: &Path) -> anyhow::Result<()> {
    let Some(state) = load_reconcile_state(synrepo_dir) else {
        anyhow::bail!("reconcile completed, but no reconcile state was written");
    };

    match state.last_outcome.as_str() {
        "completed" => {
            println!(
                "Reconcile outcome: completed\n  files discovered: {}\n  symbols extracted: {}\n  triggering events: {}",
                state.files_discovered.unwrap_or(0),
                state.symbols_extracted.unwrap_or(0),
                state.triggering_events
            );
            Ok(())
        }
        "lock-conflict" => Err(anyhow::anyhow!(
            "Reconcile skipped: writer lock held by another process."
        )),
        "failed" => Err(anyhow::anyhow!(
            "Reconcile failed: {}",
            state
                .last_error
                .unwrap_or_else(|| "unknown error".to_string())
        )),
        other => Err(anyhow::anyhow!("unexpected reconcile outcome: {other}")),
    }
}
