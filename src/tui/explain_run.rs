//! In-dashboard Explain execution.
//!
//! This keeps commentary refresh inside the dashboard alt-screen instead of
//! handing off to a standalone command.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::event::KeyCode;

use crate::config::Config;
use crate::pipeline::explain::{describe_active_provider, telemetry, ExplainStatus};
use crate::pipeline::git::GitIntelligenceContext;
use crate::pipeline::git_intelligence::analyze_recent_history;
use crate::pipeline::maintenance::plan_maintenance;
use crate::pipeline::repair::{refresh_commentary, ActionContext};
use crate::pipeline::writer::{acquire_write_admission, map_lock_error};
use crate::tui::actions::now_rfc3339;
use crate::tui::app::{AppState, ExplainMode, PendingExplainRun};
use crate::tui::dashboard::DashboardTerminal;
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

mod view;

use view::{draw_progress, ExplainRunUi};

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
    let action_context = ActionContext {
        repo_root,
        synrepo_dir: &context.synrepo_dir,
        config: &context.config,
        maint_plan: &maint_plan,
    };

    let mut actions_taken = Vec::new();
    let mut should_stop = || stop_requested();
    let result = refresh_commentary(
        &action_context,
        &mut actions_taken,
        context.scope.as_deref(),
        Some(&mut |event| {
            ui.apply_event(event);
            state.drain_events();
            let _ = draw_progress(terminal, &ui);
        }),
        Some(&mut should_stop),
    );

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

fn stop_requested() -> bool {
    match crate::tui::app::poll_key(Duration::from_millis(1)) {
        Ok(Some((KeyCode::Esc | KeyCode::Char('q'), _))) => true,
        Ok(Some((KeyCode::Char('c'), modifiers)))
            if modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            true
        }
        _ => false,
    }
}

fn log_entry(tag: &str, message: String, severity: Severity) -> LogEntry {
    LogEntry {
        timestamp: now_rfc3339(),
        tag: tag.to_string(),
        message,
        severity,
    }
}

#[derive(Clone, Debug)]
struct ExplainRunContext {
    config: Config,
    synrepo_dir: PathBuf,
    scope: Option<Vec<PathBuf>>,
    changed: bool,
}

impl ExplainRunContext {
    fn load(repo_root: &Path, mode: ExplainMode) -> anyhow::Result<Self> {
        let config = Config::load(repo_root)
            .map_err(|error| anyhow::anyhow!("explain: not initialized ({error})"))?;
        let synrepo_dir = Config::synrepo_dir(repo_root);
        let (paths, changed) = match mode {
            ExplainMode::AllStale => (Vec::new(), false),
            ExplainMode::Changed => (Vec::new(), true),
            ExplainMode::Paths(paths) => (paths, false),
        };
        let scope = compute_scope(repo_root, &config, paths, changed)?;
        Ok(Self {
            config,
            synrepo_dir,
            scope,
            changed,
        })
    }

    fn changed_scope_is_empty(&self) -> bool {
        self.changed && matches!(&self.scope, Some(scope) if scope.is_empty())
    }

    fn scope_label(&self) -> String {
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
