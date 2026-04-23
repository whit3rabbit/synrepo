//! `synrepo synthesize` — refresh commentary synthesis against stale rows.
//!
//! Mirrors the `RepairAction::RefreshCommentary` code path executed by
//! `synrepo sync`, but lets the operator scope the run to a list of repo-root
//! path prefixes or to hotspots from recent git history. `--dry-run` prints the
//! intersected target set without loading a provider.

use std::io::IsTerminal;
use std::io::Write;
use std::path::{Path, PathBuf};

use synrepo::{
    config::Config,
    pipeline::{
        git::GitIntelligenceContext,
        git_intelligence::analyze_recent_history,
        maintenance::plan_maintenance,
        repair::{refresh_commentary, ActionContext, CommentaryProgressEvent},
        synthesis::{
            describe_active_provider,
            telemetry::{self},
            SynthesisStatus,
        },
        writer::{acquire_write_admission, map_lock_error},
    },
};

use super::synthesize_progress::{render_telemetry_summary, PlainProgressRenderer};
use super::synthesize_status::synthesize_status_output_with_heading;
use super::synthesize_tracker::TelemetryTracker;
use super::synthesize_ui::run_synthesize_tui;

#[derive(Clone, Debug)]
pub(super) struct SynthesizeRunContext {
    pub config: Config,
    pub synrepo_dir: PathBuf,
    pub scope: Option<Vec<PathBuf>>,
    pub changed: bool,
    pub requested_paths: Vec<String>,
}

/// Refresh commentary synthesis. Optional `paths`/`changed`/`dry_run` scope the run.
pub(crate) fn synthesize(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    synthesize_with_mode(repo_root, paths, changed, dry_run, true)
}

/// Refresh commentary synthesis without re-entering the progress alt-screen.
/// Used by the dashboard handoff, which has already exited its own TUI and
/// should run the plain command output directly in the calling terminal.
pub(crate) fn synthesize_without_tui(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    synthesize_with_mode(repo_root, paths, changed, dry_run, false)
}

fn synthesize_with_mode(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
    dry_run: bool,
    allow_tui: bool,
) -> anyhow::Result<()> {
    let context = load_run_context(repo_root, paths, changed)?;
    if context.changed_scope_is_empty() {
        println!("No changed files found in last 50 commits, nothing to refresh.");
        return Ok(());
    }
    if allow_tui && !dry_run && std::io::stdout().is_terminal() && std::io::stderr().is_terminal() {
        match run_synthesize_tui(repo_root, context.clone()) {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!(
                    "synthesis UI unavailable: {err}. Falling back to plain progress output."
                );
            }
        }
    }
    let stderr_is_terminal = std::io::stderr().is_terminal();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    synthesize_with_writers(
        repo_root,
        context,
        dry_run,
        stderr_is_terminal,
        &mut stdout,
        &mut stderr,
    )
}

#[cfg(test)]
pub(crate) fn synthesize_output(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
    dry_run: bool,
) -> anyhow::Result<(String, String)> {
    let context = load_run_context(repo_root, paths, changed)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    synthesize_with_writers(repo_root, context, dry_run, false, &mut stdout, &mut stderr)?;
    Ok((
        String::from_utf8(stdout).expect("stdout should be valid UTF-8"),
        String::from_utf8(stderr).expect("stderr should be valid UTF-8"),
    ))
}

fn synthesize_with_writers(
    repo_root: &Path,
    context: SynthesizeRunContext,
    dry_run: bool,
    stderr_is_terminal: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> anyhow::Result<()> {
    if dry_run {
        return print_dry_run(repo_root, &context, stdout);
    }

    render_run_intro(stderr, &context)?;
    let mut progress_renderer = PlainProgressRenderer::new(stderr_is_terminal);
    let (actions_taken, tracker) = execute_synthesize_run(
        repo_root,
        &context,
        |repo_root, tracker, event| {
            let _ = progress_renderer.render_event(
                stderr,
                repo_root,
                tracker,
                context.progress_api_label().as_deref(),
                event,
            );
        },
        None,
    )?;
    progress_renderer.finish_live_line(stderr)?;
    render_telemetry_summary(stderr, &tracker)?;
    write_actions_taken(stdout, &actions_taken)
}

pub(super) fn execute_synthesize_run(
    repo_root: &Path,
    context: &SynthesizeRunContext,
    mut on_progress: impl FnMut(&Path, &mut TelemetryTracker, CommentaryProgressEvent),
    should_stop: Option<&mut dyn FnMut() -> bool>,
) -> anyhow::Result<(Vec<String>, TelemetryTracker)> {
    let maint_plan = plan_maintenance(&context.synrepo_dir, &context.config);
    let _writer_lock = acquire_write_admission(&context.synrepo_dir, "synthesize")
        .map_err(|err| map_lock_error("synthesize", err))?;
    telemetry::set_synrepo_dir(&context.synrepo_dir);
    let rx = telemetry::subscribe();
    let mut tracker = TelemetryTracker::new(rx);

    let action_context = ActionContext {
        repo_root,
        synrepo_dir: &context.synrepo_dir,
        config: &context.config,
        maint_plan: &maint_plan,
    };

    let mut actions_taken: Vec<String> = Vec::new();
    let repo_root_buf = repo_root.to_path_buf();
    let mut render_progress = |event: CommentaryProgressEvent| {
        tracker.drain();
        on_progress(&repo_root_buf, &mut tracker, event);
    };
    refresh_commentary(
        &action_context,
        &mut actions_taken,
        context.scope.as_deref(),
        Some(&mut render_progress),
        should_stop,
    )?;
    tracker.drain();
    Ok((actions_taken, tracker))
}

pub(super) fn render_run_intro(
    stderr: &mut dyn Write,
    context: &SynthesizeRunContext,
) -> anyhow::Result<()> {
    writeln!(
        stderr,
        "synthesis: refresh stale commentary and generate missing commentary"
    )?;
    writeln!(stderr, "  scope: {}", context.scope_label())?;
    writeln!(stderr, "  provider: {}", context.provider_label())?;
    match context.provider_status() {
        SynthesisStatus::Enabled => writeln!(
            stderr,
            "  api calls: yes, synrepo will send commentary requests to [{}], and those requests may cost money depending on your provider billing",
            context.provider_name()
        )?,
        SynthesisStatus::Disabled => writeln!(
            stderr,
            "  api calls: no, synthesis is disabled so no provider requests will be made"
        )?,
        SynthesisStatus::DisabledKeyDetected { env_var } => writeln!(
            stderr,
            "  api calls: no, synthesis is disabled even though ${env_var} is set"
        )?,
    }
    writeln!(
        stderr,
        "  write flow: completed commentary rows write into .synrepo/overlay/overlay.db as items finish; docs and index reconcile at the end"
    )?;
    writeln!(
        stderr,
        "  output files: symbol commentary docs under .synrepo/synthesis-docs/ plus the searchable index under .synrepo/synthesis-index/"
    )?;
    Ok(())
}

pub(super) fn write_actions_taken(
    stdout: &mut dyn Write,
    actions_taken: &[String],
) -> anyhow::Result<()> {
    if actions_taken.is_empty() {
        writeln!(stdout, "No actions taken.")?;
    } else {
        for action in actions_taken {
            writeln!(stdout, "  {action}")?;
        }
    }
    Ok(())
}

fn load_run_context(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
) -> anyhow::Result<SynthesizeRunContext> {
    let config = Config::load(repo_root).map_err(|e| {
        anyhow::anyhow!("synthesize: not initialized — run `synrepo init` first ({e})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let scope = compute_scope(repo_root, &config, paths.clone(), changed)?;
    Ok(SynthesizeRunContext {
        config,
        synrepo_dir,
        scope,
        changed,
        requested_paths: paths,
    })
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
    repo_root: &Path,
    context: &SynthesizeRunContext,
    stdout: &mut dyn Write,
) -> anyhow::Result<()> {
    let output = synthesize_status_output_with_heading(
        repo_root,
        context.requested_paths.clone(),
        context.changed,
        "Synthesis dry run:",
    )?;
    write!(stdout, "{output}")?;
    Ok(())
}

impl SynthesizeRunContext {
    fn changed_scope_is_empty(&self) -> bool {
        self.changed && matches!(&self.scope, Some(scope) if scope.is_empty())
    }

    pub(super) fn scope_label(&self) -> String {
        if self.changed {
            "files changed in the last 50 commits".to_string()
        } else {
            match &self.scope {
                None => "the whole repository".to_string(),
                Some(scope) if scope.is_empty() => "no matching files".to_string(),
                Some(scope) => format!(
                    "selected paths: {}",
                    scope
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
    }

    pub(super) fn provider_name(&self) -> &'static str {
        describe_active_provider(&self.config).provider
    }

    pub(super) fn provider_label(&self) -> String {
        let active = describe_active_provider(&self.config);
        match active.model {
            Some(model) => format!("{} / {model}", active.provider),
            None => active.provider.to_string(),
        }
    }

    pub(super) fn provider_status(&self) -> SynthesisStatus {
        describe_active_provider(&self.config).status
    }

    pub(super) fn progress_api_label(&self) -> Option<String> {
        match self.provider_status() {
            SynthesisStatus::Enabled => Some(format!("[{} API]", self.provider_name())),
            SynthesisStatus::Disabled | SynthesisStatus::DisabledKeyDetected { .. } => None,
        }
    }
}
