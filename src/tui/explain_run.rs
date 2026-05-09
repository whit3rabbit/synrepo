//! In-dashboard Explain execution.
//!
//! This keeps commentary refresh inside the dashboard alt-screen instead of
//! handing off to a standalone command.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::config::Config;
use crate::pipeline::explain::{describe_active_provider, telemetry, ExplainStatus};
use crate::pipeline::git::GitIntelligenceContext;
use crate::pipeline::git_intelligence::analyze_recent_history;
use crate::pipeline::maintenance::plan_maintenance;
use crate::pipeline::repair::{refresh_commentary, ActionContext, CommentaryProgressEvent};
use crate::pipeline::writer::{acquire_write_admission, map_lock_error};
use crate::tui::actions::now_rfc3339;
use crate::tui::app::{poll_key, AppState, ExplainMode, PendingExplainRun};
use crate::tui::dashboard::DashboardTerminal;
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

mod nodes;
mod view;

use view::{draw_progress, ExplainRunUi};

const EXPLAIN_PROGRESS_CHANNEL_CAPACITY: usize = 128;

pub(crate) fn run_explain_in_dashboard(
    terminal: &mut DashboardTerminal,
    state: &mut AppState,
    pending: PendingExplainRun,
) -> anyhow::Result<()> {
    let repo_root = state.repo_root.clone();
    let mut ui = match ExplainRunContext::load(&repo_root, pending.mode) {
        Ok(context) if context.changed_scope_is_empty() => {
            ExplainRunUi::message("Explain", "No changed files found in the last 50 commits.")
        }
        Ok(context) if context.generate_scope_is_empty() => {
            ExplainRunUi::message("Explain", "No commentary targets matched that scope.")
        }
        Ok(context) => run_context(terminal, state, &repo_root, context, pending.stopped_watch)?,
        Err(error) => ExplainRunUi::error(format!("{error:#}")),
    };

    ui.finished_prompt = true;
    draw_progress(terminal, &ui)?;
    state.refresh_now();
    state.set_tab(crate::tui::app::ActiveTab::Explain);
    let _ = crossterm::event::read();
    Ok(())
}

fn run_context(
    terminal: &mut DashboardTerminal,
    state: &mut AppState,
    repo_root: &Path,
    context: ExplainRunContext,
    stopped_watch: bool,
) -> anyhow::Result<ExplainRunUi> {
    let mut ui = ExplainRunUi::new(&context);
    if stopped_watch {
        ui.push_recent("Watch was stopped to free the writer lock.".to_string());
    }
    draw_progress(terminal, &ui)?;

    let maint_plan = plan_maintenance(&context.synrepo_dir, &context.config);
    let _writer_lock = match acquire_write_admission(&context.synrepo_dir, "explain") {
        Ok(lock) => lock,
        Err(error) => {
            let message = map_lock_error("explain", error).to_string();
            state
                .log
                .push(log_entry("explain", message.clone(), Severity::Stale));
            ui.mark_error(message);
            return Ok(ui);
        }
    };

    telemetry::set_synrepo_dir(&context.synrepo_dir);

    // Run refresh_commentary on a worker thread so the main thread stays free
    // to pump key events and redraw. The provider HTTP client is blocking with
    // a 30s default timeout, so without this the dashboard would freeze (and
    // q / Esc / Ctrl-C would queue silently in the crossterm input buffer)
    // for the duration of each provider call.
    let cancel = Arc::new(AtomicBool::new(false));
    let (event_tx, event_rx) =
        crossbeam_channel::bounded::<CommentaryProgressEvent>(EXPLAIN_PROGRESS_CHANNEL_CAPACITY);
    let mut actions_taken: Vec<String> = Vec::new();

    let result: crate::Result<()> = std::thread::scope(|scope_handle| {
        let action_context = ActionContext {
            repo_root,
            synrepo_dir: &context.synrepo_dir,
            config: &context.config,
            maint_plan: &maint_plan,
        };
        let task = context.task.clone();
        let config_for_nodes = context.config.clone();
        let synrepo_dir_for_nodes = context.synrepo_dir.clone();
        let cancel_for_worker = Arc::clone(&cancel);
        let event_tx_for_worker = event_tx;
        let actions_ref = &mut actions_taken;

        let worker = scope_handle.spawn(move || -> crate::Result<()> {
            let mut should_stop = || cancel_for_worker.load(Ordering::Relaxed);
            let mut progress = |event: CommentaryProgressEvent| {
                report_progress(&event_tx_for_worker, event);
            };
            match &task {
                ExplainRunTask::WorkPlan { scope, .. } => refresh_commentary(
                    &action_context,
                    actions_ref,
                    scope.as_deref(),
                    Some(&mut progress),
                    Some(&mut should_stop),
                ),
                ExplainRunTask::Nodes {
                    scope,
                    target,
                    nodes,
                } => {
                    nodes::run_scoped_nodes(
                        repo_root,
                        &config_for_nodes,
                        &synrepo_dir_for_nodes,
                        nodes,
                        &event_tx_for_worker,
                        &cancel_for_worker,
                    )?;
                    actions_ref.push(format!(
                        "generated/refreshed {} commentary for {target}",
                        scope.as_str()
                    ));
                    Ok(())
                }
            }
        });

        let mut last_frame = Instant::now();
        loop {
            let mut had_event = false;
            while let Ok(event) = event_rx.try_recv() {
                ui.apply_event(event);
                had_event = true;
            }
            let animation_due = last_frame.elapsed() >= Duration::from_millis(200);
            if animation_due {
                ui.tick();
                last_frame = Instant::now();
            }
            if had_event || animation_due {
                state.drain_events();
                let _ = draw_progress(terminal, &ui);
            }

            // 50ms paces the loop so a queued keypress lands in at most 50ms
            // even if the worker is parked in a long provider call.
            if let Ok(Some((code, mods))) = poll_key(Duration::from_millis(50)) {
                let cancel_now = matches!(code, KeyCode::Esc | KeyCode::Char('q'))
                    || (matches!(code, KeyCode::Char('c')) && mods.contains(KeyModifiers::CONTROL));
                if cancel_now && !cancel.swap(true, Ordering::Relaxed) {
                    ui.mark_stop_requested();
                    let _ = draw_progress(terminal, &ui);
                }
            }

            if worker.is_finished() {
                break;
            }
        }

        // Drain any events the worker sent after the last try_recv pass.
        while let Ok(event) = event_rx.try_recv() {
            ui.apply_event(event);
        }

        match worker.join() {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    });

    state.drain_events();
    match result {
        Ok(()) => {
            ui.mark_finished();
            for action in actions_taken {
                state
                    .log
                    .push(log_entry("explain", action, Severity::Healthy));
            }
        }
        Err(error) => {
            let message = format!("{error:#}");
            ui.mark_error(message.clone());
            state
                .log
                .push(log_entry("explain", message, Severity::Stale));
        }
    }
    Ok(ui)
}

fn log_entry(tag: &str, message: String, severity: Severity) -> LogEntry {
    LogEntry {
        timestamp: now_rfc3339(),
        tag: tag.to_string(),
        message,
        severity,
    }
}

fn report_progress(
    sender: &crossbeam_channel::Sender<CommentaryProgressEvent>,
    event: CommentaryProgressEvent,
) {
    let _ = sender.try_send(event);
}

#[derive(Clone, Debug)]
struct ExplainRunContext {
    config: Config,
    synrepo_dir: PathBuf,
    task: ExplainRunTask,
}

#[derive(Clone, Debug)]
enum ExplainRunTask {
    WorkPlan {
        scope: Option<Vec<PathBuf>>,
        changed: bool,
    },
    Nodes {
        scope: crate::tui::app::GenerateCommentaryScope,
        target: String,
        nodes: Vec<crate::core::ids::NodeId>,
    },
}

impl ExplainRunContext {
    fn load(repo_root: &Path, mode: ExplainMode) -> anyhow::Result<Self> {
        let config = Config::load(repo_root)
            .map_err(|error| anyhow::anyhow!("explain: not initialized ({error})"))?;
        let synrepo_dir = Config::synrepo_dir(repo_root);
        let task = match mode {
            ExplainMode::AllStale => ExplainRunTask::WorkPlan {
                scope: None,
                changed: false,
            },
            ExplainMode::Changed => ExplainRunTask::WorkPlan {
                scope: compute_scope(repo_root, &config, Vec::new(), true)?,
                changed: true,
            },
            ExplainMode::Paths(paths) => ExplainRunTask::WorkPlan {
                scope: compute_scope(repo_root, &config, paths, false)?,
                changed: false,
            },
            ExplainMode::Generate { scope, target } => {
                let nodes =
                    nodes::resolve_scoped_nodes(repo_root, &config, &synrepo_dir, scope, &target)?;
                ExplainRunTask::Nodes {
                    scope,
                    target,
                    nodes,
                }
            }
        };
        Ok(Self {
            config,
            synrepo_dir,
            task,
        })
    }

    fn changed_scope_is_empty(&self) -> bool {
        matches!(
            &self.task,
            ExplainRunTask::WorkPlan {
                scope: Some(scope),
                changed: true
            } if scope.is_empty()
        )
    }

    fn generate_scope_is_empty(&self) -> bool {
        matches!(&self.task, ExplainRunTask::Nodes { nodes, .. } if nodes.is_empty())
    }

    fn scope_label(&self) -> String {
        match &self.task {
            ExplainRunTask::WorkPlan {
                scope: _,
                changed: true,
            } => "files changed in the last 50 commits".to_string(),
            ExplainRunTask::WorkPlan {
                scope,
                changed: false,
            } => match scope {
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
            },
            ExplainRunTask::Nodes { scope, target, .. } => {
                format!("{} scope: {target}", scope.as_str())
            }
        }
    }

    fn provider_label(&self) -> String {
        let active = describe_active_provider(&self.config);
        match active.model {
            Some(model) => format!("{} / {model}", active.provider),
            None => active.provider.to_string(),
        }
    }

    fn api_line(&self) -> String {
        let active = describe_active_provider(&self.config);
        match active.status {
            ExplainStatus::Enabled => {
                format!(
                    "[{}] is called only for items needing commentary",
                    active.provider
                )
            }
            ExplainStatus::Disabled => "provider calls are off".to_string(),
            ExplainStatus::DisabledKeyDetected { env_var } => {
                format!("provider calls are off, ${env_var} is detected but explain is disabled")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_progress_channel_drops_events_when_full() {
        let (sender, receiver) = crossbeam_channel::bounded(EXPLAIN_PROGRESS_CHANNEL_CAPACITY);

        for index in 0..(EXPLAIN_PROGRESS_CHANNEL_CAPACITY + 10) {
            report_progress(&sender, scan_event(index));
        }

        assert_eq!(
            receiver.try_iter().count(),
            EXPLAIN_PROGRESS_CHANNEL_CAPACITY
        );
    }

    fn scan_event(index: usize) -> CommentaryProgressEvent {
        CommentaryProgressEvent::ScanProgress {
            files_scanned: index,
            files_total: EXPLAIN_PROGRESS_CHANNEL_CAPACITY + 10,
            symbols_scanned: 0,
            symbols_total: 0,
        }
    }
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
            .map_err(|error| anyhow::anyhow!("explain: cannot sample git history ({error})"))?;
        Ok(Some(
            insights
                .hotspots
                .iter()
                .map(|hotspot| PathBuf::from(&hotspot.path))
                .collect(),
        ))
    } else if paths.is_empty() {
        Ok(None)
    } else {
        Ok(Some(paths.into_iter().map(PathBuf::from).collect()))
    }
}
