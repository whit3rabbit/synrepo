//! Progress rendering and telemetry tracking for `synrepo synthesize`.

use std::io::Write;
use std::path::Path;

use synrepo::pipeline::repair::{CommentaryProgressEvent, CommentaryWorkItem, CommentaryWorkPhase};

use super::synthesize_tracker::TelemetryTracker;

pub(super) fn render_progress_event(
    stderr: &mut dyn Write,
    repo_root: &Path,
    tracker: &mut TelemetryTracker,
    api_label: Option<&str>,
    event: CommentaryProgressEvent,
) -> anyhow::Result<()> {
    match event {
        CommentaryProgressEvent::PlanReady {
            refresh,
            file_seeds,
            symbol_seed_candidates,
            max_targets,
        } => {
            tracker.note_plan(max_targets);
            writeln!(
                stderr,
                "plan: {refresh} stale item(s), {file_seeds} file(s) missing commentary, {symbol_seed_candidates} symbol candidate(s) missing commentary, up to {max_targets} target(s)",
            )?
        }
        CommentaryProgressEvent::TargetStarted { item, current } => writeln!(
            stderr,
            "{}{} {}: {}",
            tracker.render_counter(current),
            render_api_label(api_label),
            start_label(item.phase),
            render_target(&item)
        )?,
        CommentaryProgressEvent::TargetFinished {
            item,
            current,
            generated,
        } => {
            let status = tracker.take_status(item.node_id, generated);
            writeln!(
                stderr,
                "{}{} {}: {}",
                tracker.render_counter(current),
                render_api_label(api_label),
                status.headline(success_label(item.phase)),
                render_target(&item),
            )?;
            writeln!(stderr, "      {}", status.detail())?;
        }
        CommentaryProgressEvent::DocsDirCreated { path } => {
            writeln!(stderr, "mkdir {}", repo_relative(repo_root, &path))?
        }
        CommentaryProgressEvent::DocWritten { path } => {
            writeln!(stderr, "output file: {}", repo_relative(repo_root, &path))?
        }
        CommentaryProgressEvent::DocDeleted { path } => {
            writeln!(stderr, "delete {}", repo_relative(repo_root, &path))?
        }
        CommentaryProgressEvent::IndexDirCreated { path } => {
            writeln!(stderr, "mkdir {}", repo_relative(repo_root, &path))?
        }
        CommentaryProgressEvent::IndexUpdated {
            path,
            touched_paths,
        } => writeln!(
            stderr,
            "output index: updated {} ({touched_paths} path(s))",
            repo_relative(repo_root, &path)
        )?,
        CommentaryProgressEvent::IndexRebuilt {
            path,
            touched_paths,
        } => writeln!(
            stderr,
            "output index: rebuilt {} ({touched_paths} path(s))",
            repo_relative(repo_root, &path)
        )?,
        CommentaryProgressEvent::PhaseSummary {
            phase,
            attempted,
            generated,
            not_generated,
        } => writeln!(
            stderr,
            "{}: attempted={attempted} generated={generated} not_generated={not_generated}",
            phase_summary_label(phase),
        )?,
        CommentaryProgressEvent::RunSummary {
            refreshed,
            seeded,
            not_generated,
            attempted,
            stopped,
        } => writeln!(
            stderr,
            "summary: attempted={attempted} refreshed={refreshed} generated={seeded} not_generated={not_generated} stopped={stopped}"
        )?,
    }
    Ok(())
}

pub(super) fn render_telemetry_summary(
    stderr: &mut dyn Write,
    tracker: &TelemetryTracker,
) -> anyhow::Result<()> {
    if tracker.total_calls() == 0 {
        return Ok(());
    }
    write!(
        stderr,
        "usage: calls={} ok={} failed={} budget_blocked={} in={} out={}",
        tracker.total_calls(),
        tracker.calls(),
        tracker.failures(),
        tracker.budget_blocked(),
        tracker.input_tokens(),
        tracker.output_tokens()
    )?;
    if tracker.any_estimated() {
        write!(stderr, " estimated_tokens=yes")?;
    }
    if tracker.unpriced_calls() > 0 {
        write!(stderr, " unpriced_calls={}", tracker.unpriced_calls())?;
    } else {
        write!(stderr, " cost=${:.4}", tracker.usd_cost())?;
    }
    writeln!(stderr)?;
    Ok(())
}

fn repo_relative(repo_root: &Path, path: &Path) -> String {
    let rendered = path
        .strip_prefix(repo_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string());
    if cfg!(windows) {
        rendered.replace('\\', "/")
    } else {
        rendered
    }
}

fn start_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "refresh stale commentary",
        CommentaryWorkPhase::Seed => "generate missing commentary",
    }
}

fn render_target(item: &CommentaryWorkItem) -> String {
    match &item.qualified_name {
        Some(name) => format!("{} :: {}", item.path, name),
        None => item.path.clone(),
    }
}

fn success_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "refreshed",
        CommentaryWorkPhase::Seed => "generated",
    }
}

fn phase_summary_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "refresh phase summary",
        CommentaryWorkPhase::Seed => "missing commentary summary",
    }
}

fn render_api_label(api_label: Option<&str>) -> String {
    match api_label {
        Some(label) => format!(" {label}"),
        None => String::new(),
    }
}
