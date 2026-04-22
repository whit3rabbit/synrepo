//! `synrepo synthesize` — refresh commentary synthesis against stale rows.
//!
//! Mirrors the `RepairAction::RefreshCommentary` code path executed by
//! `synrepo sync`, but lets the operator scope the run to a list of repo-root
//! path prefixes or to hotspots from recent git history. `--dry-run` prints the
//! intersected target set without loading a provider.

use std::io::Write;
use std::path::{Path, PathBuf};

use synrepo::{
    config::Config,
    pipeline::{
        git::GitIntelligenceContext,
        git_intelligence::analyze_recent_history,
        maintenance::plan_maintenance,
        repair::{
            load_commentary_work_plan, refresh_commentary, ActionContext,
            CommentaryProgressEvent, CommentaryWorkItem,
        },
        synthesis::telemetry::{self},
        writer::{acquire_write_admission, map_lock_error},
    },
};

use super::synthesize_progress::{
    render_progress_event, render_telemetry_summary, TelemetryTracker,
};

/// Refresh commentary synthesis. Optional `paths`/`changed`/`dry_run` scope the run.
pub(crate) fn synthesize(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    synthesize_with_writers(repo_root, paths, changed, dry_run, &mut stdout, &mut stderr)
}

#[cfg(test)]
pub(crate) fn synthesize_output(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
    dry_run: bool,
) -> anyhow::Result<(String, String)> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    synthesize_with_writers(repo_root, paths, changed, dry_run, &mut stdout, &mut stderr)?;
    Ok((
        String::from_utf8(stdout).expect("stdout should be valid UTF-8"),
        String::from_utf8(stderr).expect("stderr should be valid UTF-8"),
    ))
}

fn synthesize_with_writers(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
    dry_run: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|e| {
        anyhow::anyhow!("synthesize: not initialized — run `synrepo init` first ({e})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

    let scope = compute_scope(repo_root, &config, paths, changed)?;
    if changed && matches!(&scope, Some(s) if s.is_empty()) {
        writeln!(stdout, "No changed files found in last 50 commits; nothing to refresh.")?;
        return Ok(());
    }

    if dry_run {
        return print_dry_run(&synrepo_dir, scope.as_deref(), stdout);
    }

    let maint_plan = plan_maintenance(&synrepo_dir, &config);
    let _writer_lock = acquire_write_admission(&synrepo_dir, "synthesize")
        .map_err(|err| map_lock_error("synthesize", err))?;
    telemetry::set_synrepo_dir(&synrepo_dir);
    let rx = telemetry::subscribe();
    let mut tracker = TelemetryTracker::new(rx);

    let action_context = ActionContext {
        repo_root,
        synrepo_dir: &synrepo_dir,
        config: &config,
        maint_plan: &maint_plan,
    };

    let mut actions_taken: Vec<String> = Vec::new();
    let repo_root_buf = repo_root.to_path_buf();
    let mut render_progress = |event: CommentaryProgressEvent| {
        tracker.drain();
        let _ = render_progress_event(stderr, &repo_root_buf, &mut tracker, event);
    };
    refresh_commentary(
        &action_context,
        &mut actions_taken,
        scope.as_deref(),
        Some(&mut render_progress),
    )?;
    tracker.drain();
    render_telemetry_summary(stderr, &tracker)?;

    if actions_taken.is_empty() {
        writeln!(stdout, "No actions taken.")?;
    } else {
        for action in &actions_taken {
            writeln!(stdout, "  {action}")?;
        }
    }
    Ok(())
}

fn compute_scope(
    repo_root: &Path,
    config: &Config,
    paths: Vec<String>,
    changed: bool,
) -> anyhow::Result<Option<Vec<PathBuf>>> {
    if changed {
        let context = GitIntelligenceContext::inspect(repo_root, config);
        let insights = analyze_recent_history(&context, 50, 50)
            .map_err(|e| anyhow::anyhow!("synthesize: cannot sample git history ({e})"))?;
        let hotspot_paths: Vec<PathBuf> = insights
            .hotspots
            .iter()
            .map(|h| PathBuf::from(&h.path))
            .collect();
        Ok(Some(hotspot_paths))
    } else if paths.is_empty() {
        Ok(None)
    } else {
        Ok(Some(paths.into_iter().map(PathBuf::from).collect()))
    }
}

fn print_dry_run(
    synrepo_dir: &Path,
    scope: Option<&[PathBuf]>,
    stdout: &mut dyn Write,
) -> anyhow::Result<()> {
    let plan = load_commentary_work_plan(synrepo_dir, scope)
        .map_err(|e| anyhow::anyhow!("synthesize --dry-run: cannot plan commentary work ({e})"))?;
    if plan.is_empty() {
        writeln!(stdout, "No commentary work planned.")?;
        return Ok(());
    }

    writeln!(stdout, "Planned commentary work:")?;
    writeln!(stdout, "  refresh targets: {}", plan.refresh_count())?;
    writeln!(stdout, "  file seed targets: {}", plan.file_seed_count())?;
    writeln!(
        stdout,
        "  symbol seed candidates: {}",
        plan.symbol_seed_candidate_count()
    )?;
    writeln!(stdout, "  max targets this snapshot: {}", plan.max_target_count())?;

    print_plan_group(stdout, "Refresh targets", &plan.refresh)?;
    print_plan_group(stdout, "File seed targets", &plan.file_seeds)?;
    print_plan_group(
        stdout,
        "Symbol seed candidates",
        &plan.symbol_seed_candidates,
    )?;
    Ok(())
}

fn print_plan_group(
    stdout: &mut dyn Write,
    label: &str,
    items: &[CommentaryWorkItem],
) -> anyhow::Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    writeln!(stdout, "{label}:")?;
    for item in items {
        writeln!(stdout, "  {}", render_target(item))?;
    }
    Ok(())
}

fn render_target(item: &CommentaryWorkItem) -> String {
    match &item.qualified_name {
        Some(name) => format!("{} :: {}", item.path, name),
        None => item.path.clone(),
    }
}
