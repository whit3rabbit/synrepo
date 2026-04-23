//! Progress rendering and telemetry tracking for `synrepo synthesize`.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

use synrepo::pipeline::repair::{CommentaryProgressEvent, CommentaryWorkItem, CommentaryWorkPhase};

use super::synthesize_tracker::TelemetryTracker;

pub(super) struct PlainProgressRenderer {
    interactive: bool,
    live_len: usize,
    last_scan_bucket: Option<usize>,
}

impl PlainProgressRenderer {
    pub(super) fn new(interactive: bool) -> Self {
        Self {
            interactive,
            live_len: 0,
            last_scan_bucket: None,
        }
    }

    pub(super) fn render_event(
        &mut self,
        stderr: &mut dyn Write,
        repo_root: &Path,
        tracker: &mut TelemetryTracker,
        api_label: Option<&str>,
        event: CommentaryProgressEvent,
    ) -> anyhow::Result<()> {
        if let CommentaryProgressEvent::ScanProgress {
            files_scanned,
            files_total,
            symbols_scanned,
            symbols_total,
        } = event
        {
            if self.interactive {
                let line = format!(
                    "scan: checked {files_scanned}/{files_total} file(s), {symbols_scanned}/{symbols_total} symbol(s)"
                );
                self.write_live_line(stderr, &line)?;
                return Ok(());
            }
            let bucket =
                scan_progress_bucket(files_scanned, files_total, symbols_scanned, symbols_total);
            if self.last_scan_bucket == Some(bucket) {
                return Ok(());
            }
            self.last_scan_bucket = Some(bucket);
            return render_progress_event(stderr, repo_root, tracker, api_label, event);
        }

        self.finish_live_line(stderr)?;
        render_progress_event(stderr, repo_root, tracker, api_label, event)
    }

    pub(super) fn finish_live_line(&mut self, stderr: &mut dyn Write) -> anyhow::Result<()> {
        if self.live_len == 0 {
            return Ok(());
        }
        self.clear_live_line(stderr)?;
        self.live_len = 0;
        Ok(())
    }

    fn write_live_line(&mut self, stderr: &mut dyn Write, line: &str) -> anyhow::Result<()> {
        self.clear_live_line(stderr)?;
        write!(stderr, "{line}")?;
        stderr.flush()?;
        self.live_len = line.len();
        Ok(())
    }

    fn clear_live_line(&self, stderr: &mut dyn Write) -> anyhow::Result<()> {
        if self.live_len == 0 {
            return Ok(());
        }
        let mut clear = String::new();
        write!(&mut clear, "\r{:width$}\r", "", width = self.live_len)?;
        write!(stderr, "{clear}")?;
        Ok(())
    }
}

pub(super) fn render_progress_event(
    stderr: &mut dyn Write,
    repo_root: &Path,
    tracker: &mut TelemetryTracker,
    api_label: Option<&str>,
    event: CommentaryProgressEvent,
) -> anyhow::Result<()> {
    match event {
        CommentaryProgressEvent::ScanProgress {
            files_scanned,
            files_total,
            symbols_scanned,
            symbols_total,
        } => writeln!(
            stderr,
            "scan: checked {files_scanned}/{files_total} file(s), {symbols_scanned}/{symbols_total} symbol(s)"
        )?,
        CommentaryProgressEvent::PlanReady {
            refresh,
            file_seeds,
            symbol_seed_candidates,
            scoped_files,
            scoped_symbols,
            max_targets,
        } => {
            tracker.note_plan(max_targets);
            if max_targets == 0 {
                writeln!(
                    stderr,
                    "queued: checked {scoped_files} file(s) and {scoped_symbols} symbol(s) in scope; nothing currently needs commentary"
                )?
            } else {
                writeln!(
                    stderr,
                    "queued: checked {scoped_files} file(s) and {scoped_symbols} symbol(s) in scope; {max_targets} item(s) need commentary ({refresh} outdated, {file_seeds} files missing commentary, {symbol_seed_candidates} symbol candidate(s))",
                )?
            }
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
        } => {
            if attempted == 0 && generated == 0 && not_generated == 0 {
                return Ok(());
            }
            writeln!(
                stderr,
                "{}: attempted={attempted} generated={generated} not_generated={not_generated}",
                phase_summary_label(phase),
            )?
        }
        CommentaryProgressEvent::RunSummary {
            refreshed,
            seeded,
            not_generated,
            attempted,
            stopped,
        } => {
            if attempted == 0 && refreshed == 0 && seeded == 0 && not_generated == 0 && !stopped {
                writeln!(stderr, "summary: no commentary changes were needed")?
            } else {
                writeln!(
                    stderr,
                    "summary: attempted={attempted} refreshed={refreshed} generated={seeded} not_generated={not_generated} stopped={stopped}"
                )?
            }
        }
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
        "provider activity: calls={} ok={} failed={} budget_blocked={} in={} out={}",
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
        CommentaryWorkPhase::Refresh => "update commentary for",
        CommentaryWorkPhase::Seed => "generate commentary for",
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
        CommentaryWorkPhase::Refresh => "updated",
        CommentaryWorkPhase::Seed => "generated",
    }
}

fn phase_summary_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "outdated items",
        CommentaryWorkPhase::Seed => "missing commentary",
    }
}

fn render_api_label(api_label: Option<&str>) -> String {
    match api_label {
        Some(label) => format!(" {label}"),
        None => String::new(),
    }
}

fn scan_progress_bucket(
    files_scanned: usize,
    files_total: usize,
    symbols_scanned: usize,
    symbols_total: usize,
) -> usize {
    let scanned = files_scanned.saturating_add(symbols_scanned);
    let total = files_total.saturating_add(symbols_total).max(1);
    (scanned.saturating_mul(10)).min(total.saturating_mul(10)) / total
}
