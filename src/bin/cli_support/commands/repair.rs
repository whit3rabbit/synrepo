use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::{
        explain::{accounting, telemetry},
        repair::{
            build_repair_report, execute_sync, execute_sync_locked, SurfaceOutcome, SyncOptions,
            SyncProgress, SyncSummary,
        },
        watch::{request_watch_control, WatchControlRequest, WatchControlResponse},
    },
};

/// Report drift across all repair surfaces. Read-only; never mutates state.
pub(crate) fn check(repo_root: &Path, json_output: bool) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|error| {
        anyhow::anyhow!("check: not initialized, run `synrepo init --mode auto` first ({error})")
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
        anyhow::anyhow!("sync: not initialized, run `synrepo init --mode auto` first ({error})")
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

    let summary = if let Some(pid) = super::watch::active_watch_pid(&synrepo_dir)? {
        run_sync_via_watch(&synrepo_dir, pid, options.clone(), json_output)?
    } else {
        run_sync_local(repo_root, &synrepo_dir, &config, options, json_output)?
    };

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

/// Run sync in the current process, wiring per-surface progress to stderr
/// unless output is requested as JSON.
fn run_sync_local(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    options: SyncOptions,
    json_output: bool,
) -> anyhow::Result<SyncSummary> {
    if json_output {
        Ok(execute_sync(repo_root, synrepo_dir, config, options)?)
    } else {
        use synrepo::pipeline::writer::{acquire_write_admission, map_lock_error};
        let _lock = acquire_write_admission(synrepo_dir, "sync")
            .map_err(|err| map_lock_error("sync", err))?;
        let mut cb = |progress: SyncProgress| print_progress_to_stderr(&progress);
        let mut progress: Option<&mut dyn FnMut(SyncProgress)> = Some(&mut cb);
        Ok(execute_sync_locked(
            repo_root,
            synrepo_dir,
            config,
            options,
            &mut progress,
            None,
        )?)
    }
}

/// Delegate sync to the active watch service.
///
/// Wire protocol is request-response: the CLI blocks until the watch service
/// returns a `Sync { summary }` or an `Error`. Older daemons that do not
/// understand `SyncNow` respond with a deserialization error; we surface the
/// existing "stop watch first" hint so the user is not left staring at a raw
/// serde message.
fn run_sync_via_watch(
    synrepo_dir: &Path,
    pid: u32,
    options: SyncOptions,
    json_output: bool,
) -> anyhow::Result<SyncSummary> {
    if !json_output {
        eprintln!("Delegated sync to active watch service (pid {pid}); progress will stream to the TUI if attached.");
    }

    match request_watch_control(synrepo_dir, WatchControlRequest::SyncNow { options })? {
        WatchControlResponse::Sync { summary } => Ok(summary),
        WatchControlResponse::Error { message } => Err(anyhow::anyhow!(
            "sync: watch delegation failed: {message}; if this is an older watch daemon, run `synrepo watch stop` first and retry"
        )),
        other => Err(anyhow::anyhow!(
            "sync: watch delegation returned unexpected response: {:?}",
            other
        )),
    }
}

fn print_progress_to_stderr(progress: &SyncProgress) {
    match progress {
        SyncProgress::SurfaceStarted { surface, action } => {
            eprintln!("sync: {} → {}", surface.as_str(), action.as_str());
        }
        SyncProgress::SurfaceFinished { surface, outcome } => {
            let label = match outcome {
                SurfaceOutcome::Repaired => "ok",
                SurfaceOutcome::Blocked => "blocked",
                SurfaceOutcome::ReportOnly => "report-only",
                SurfaceOutcome::FilteredOut => "skipped",
            };
            eprintln!("sync: {} [{}]", surface.as_str(), label);
        }
        SyncProgress::CommentaryPlan {
            refresh,
            file_seeds,
            symbol_seed_candidates,
        } => {
            eprintln!(
                "sync: commentary plan ({refresh} refresh, {file_seeds} file seeds, {symbol_seed_candidates} symbol seeds)"
            );
        }
        SyncProgress::CommentaryItem {
            current, generated, ..
        } => {
            let tag = if *generated { "+" } else { "." };
            eprint!("{tag}");
            if current % 40 == 0 {
                eprintln!(" ({current})");
            }
        }
        SyncProgress::CommentarySummary {
            refreshed,
            seeded,
            not_generated,
            attempted,
            ..
        } => {
            eprintln!(
                "\nsync: commentary done ({refreshed} refreshed, {seeded} seeded, {not_generated} not-generated / {attempted})"
            );
        }
    }
}

/// Reconcile the structural graph and update the index.
pub(crate) fn reconcile(repo_root: &Path, fast: bool) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

    if let Some(pid) = super::watch::active_watch_pid(&synrepo_dir)? {
        match request_watch_control(&synrepo_dir, WatchControlRequest::ReconcileNow { fast })? {
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
        let attempt =
            synrepo::pipeline::watch::run_reconcile_attempt(repo_root, &config, &synrepo_dir, fast);
        let outcome = attempt.outcome.clone();
        synrepo::pipeline::watch::persist_reconcile_attempt_state(&synrepo_dir, &attempt, 0);
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
