//! Progress rendering and telemetry tracking for `synrepo synthesize`.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use crossbeam_channel::Receiver;
use synrepo::pipeline::{
    repair::{CommentaryProgressEvent, CommentaryWorkItem, CommentaryWorkPhase},
    synthesis::{
        pricing,
        telemetry::{SynthesisEvent, SynthesisTarget, UsageSource},
    },
};

pub(super) fn render_progress_event(
    stderr: &mut dyn Write,
    repo_root: &Path,
    tracker: &mut TelemetryTracker,
    event: CommentaryProgressEvent,
) -> anyhow::Result<()> {
    match event {
        CommentaryProgressEvent::PlanReady {
            refresh,
            file_seeds,
            symbol_seed_candidates,
            max_targets,
        } => writeln!(
            stderr,
            "plan: refresh={refresh} file_seeds={file_seeds} symbol_seed_candidates={symbol_seed_candidates} max_targets={max_targets}",
        )?,
        CommentaryProgressEvent::TargetStarted { item, current } => writeln!(
            stderr,
            "[{current}] {} start: {}",
            phase_label(item.phase),
            render_target(&item)
        )?,
        CommentaryProgressEvent::TargetFinished {
            item,
            current,
            generated,
        } => {
            let status = tracker.render_status(item.node_id, generated);
            writeln!(
                stderr,
                "[{current}] {} {}: {}",
                phase_label(item.phase),
                outcome_label(&status),
                render_target(&item),
            )?;
            writeln!(stderr, "      {status}")?;
        }
        CommentaryProgressEvent::DocsDirCreated { path } => {
            writeln!(stderr, "mkdir {}", repo_relative(repo_root, &path))?
        }
        CommentaryProgressEvent::DocWritten { path } => {
            writeln!(stderr, "write {}", repo_relative(repo_root, &path))?
        }
        CommentaryProgressEvent::DocDeleted { path } => {
            writeln!(stderr, "delete {}", repo_relative(repo_root, &path))?
        }
        CommentaryProgressEvent::IndexDirCreated { path } => {
            writeln!(stderr, "mkdir {}", repo_relative(repo_root, &path))?
        }
        CommentaryProgressEvent::IndexUpdated { path, touched_paths } => writeln!(
            stderr,
            "index: updated {} ({touched_paths} path(s))",
            repo_relative(repo_root, &path)
        )?,
        CommentaryProgressEvent::IndexRebuilt { path, touched_paths } => writeln!(
            stderr,
            "index: rebuilt {} ({touched_paths} path(s))",
            repo_relative(repo_root, &path)
        )?,
        CommentaryProgressEvent::PhaseSummary {
            phase,
            attempted,
            generated,
            not_generated,
        } => writeln!(
            stderr,
            "{} summary: attempted={attempted} generated={generated} not_generated={not_generated}",
            phase_label(phase),
        )?,
        CommentaryProgressEvent::RunSummary {
            refreshed,
            seeded,
            not_generated,
            attempted,
        } => writeln!(
            stderr,
            "summary: refreshed={refreshed} seeded={seeded} not_generated={not_generated} attempted={attempted}"
        )?,
    }
    Ok(())
}

pub(super) fn render_telemetry_summary(
    stderr: &mut dyn Write,
    tracker: &TelemetryTracker,
) -> anyhow::Result<()> {
    if tracker.calls == 0 && tracker.failures == 0 && tracker.budget_blocked == 0 {
        return Ok(());
    }
    let total_calls = tracker.calls + tracker.failures + tracker.budget_blocked;
    write!(
        stderr,
        "usage: calls={total_calls} ok={} failed={} budget_blocked={} in={} out={}",
        tracker.calls,
        tracker.failures,
        tracker.budget_blocked,
        tracker.input_tokens,
        tracker.output_tokens
    )?;
    if tracker.any_estimated {
        write!(stderr, " estimated_tokens=yes")?;
    }
    if tracker.unpriced_calls > 0 {
        write!(stderr, " unpriced_calls={}", tracker.unpriced_calls)?;
    } else {
        write!(stderr, " cost=${:.4}", tracker.usd_cost)?;
    }
    writeln!(stderr)?;
    Ok(())
}

pub(super) struct TelemetryTracker {
    rx: Receiver<SynthesisEvent>,
    terminal_by_node: HashMap<String, TerminalEvent>,
    calls: u64,
    failures: u64,
    budget_blocked: u64,
    input_tokens: u64,
    output_tokens: u64,
    usd_cost: f64,
    unpriced_calls: u64,
    any_estimated: bool,
}

impl TelemetryTracker {
    pub(super) fn new(rx: Receiver<SynthesisEvent>) -> Self {
        Self {
            rx,
            terminal_by_node: HashMap::new(),
            calls: 0,
            failures: 0,
            budget_blocked: 0,
            input_tokens: 0,
            output_tokens: 0,
            usd_cost: 0.0,
            unpriced_calls: 0,
            any_estimated: false,
        }
    }

    pub(super) fn drain(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                SynthesisEvent::CallStarted { .. } => {}
                SynthesisEvent::CallCompleted {
                    provider,
                    model,
                    target,
                    duration_ms,
                    usage,
                    billed_usd_cost,
                    ..
                } => {
                    self.calls += 1;
                    self.input_tokens += usage.input_tokens as u64;
                    self.output_tokens += usage.output_tokens as u64;
                    if usage.source == UsageSource::Estimated {
                        self.any_estimated = true;
                    }
                    match billed_usd_cost.or_else(|| {
                        pricing::cost_for_call(provider, &model, usage.input_tokens, usage.output_tokens)
                    }) {
                        Some(cost) => self.usd_cost += cost,
                        None => self.unpriced_calls += 1,
                    }
                    if let Some(node_id) = commentary_target_key(&target) {
                        self.terminal_by_node.insert(
                            node_id,
                            TerminalEvent::Completed {
                                duration_ms,
                                input_tokens: usage.input_tokens,
                                output_tokens: usage.output_tokens,
                                estimated: usage.source == UsageSource::Estimated,
                            },
                        );
                    }
                }
                SynthesisEvent::BudgetBlocked {
                    target,
                    estimated_tokens,
                    budget,
                    ..
                } => {
                    self.budget_blocked += 1;
                    if let Some(node_id) = commentary_target_key(&target) {
                        self.terminal_by_node.insert(
                            node_id,
                            TerminalEvent::BudgetBlocked {
                                estimated_tokens,
                                budget,
                            },
                        );
                    }
                }
                SynthesisEvent::CallFailed {
                    target,
                    duration_ms,
                    error,
                    ..
                } => {
                    self.failures += 1;
                    if let Some(node_id) = commentary_target_key(&target) {
                        self.terminal_by_node.insert(
                            node_id,
                            TerminalEvent::Failed { duration_ms, error },
                        );
                    }
                }
            }
        }
    }

    fn render_status(&mut self, node_id: synrepo::NodeId, generated: bool) -> String {
        match self.terminal_by_node.remove(&node_id.to_string()) {
            Some(TerminalEvent::Completed {
                duration_ms,
                input_tokens,
                output_tokens,
                estimated,
            }) if generated => {
                let est = if estimated { " est." } else { "" };
                format!("ok ({duration_ms}ms, {input_tokens} in / {output_tokens} out{est})")
            }
            Some(TerminalEvent::Completed { .. }) => "no commentary produced".to_string(),
            Some(TerminalEvent::BudgetBlocked {
                estimated_tokens,
                budget,
            }) => format!("budget blocked ({estimated_tokens} est. > {budget})"),
            Some(TerminalEvent::Failed { duration_ms, error }) => {
                format!("failed ({duration_ms}ms): {error}")
            }
            None if generated => "ok".to_string(),
            None => "no commentary produced".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
enum TerminalEvent {
    Completed {
        duration_ms: u64,
        input_tokens: u32,
        output_tokens: u32,
        estimated: bool,
    },
    BudgetBlocked {
        estimated_tokens: u32,
        budget: u32,
    },
    Failed {
        duration_ms: u64,
        error: String,
    },
}

fn commentary_target_key(target: &SynthesisTarget) -> Option<String> {
    match target {
        SynthesisTarget::Commentary { node } => Some(node.to_string()),
        SynthesisTarget::CrossLink { .. } => None,
    }
}

fn repo_relative(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn phase_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "refresh",
        CommentaryWorkPhase::Seed => "seed",
    }
}

fn render_target(item: &CommentaryWorkItem) -> String {
    match &item.qualified_name {
        Some(name) => format!("{} :: {}", item.path, name),
        None => item.path.clone(),
    }
}

fn outcome_label(status: &str) -> &'static str {
    if status.starts_with("ok") {
        "ok"
    } else if status.starts_with("failed") {
        "failed"
    } else if status.starts_with("budget blocked") {
        "budget"
    } else {
        "skip"
    }
}
