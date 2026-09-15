//! Dashboard action handlers for mutating keys and docs operations.

use crate::config::Config;
use crate::pipeline::watch::WatchServiceStatus;
use crate::tui::actions::{
    materialize_now, outcome_to_log, outcome_to_project_log, semantic_feature_compiled,
    set_auto_sync, set_semantic_triage, set_worktrees_enabled, ActionContext, ActionOutcome,
    ProjectActionContext,
};

use super::background_actions::BackgroundActionKind;
use super::{AppMode, AppState, PendingEmbeddingBuild};

impl AppState {
    pub(super) fn handle_docs_export(&mut self, force: bool) -> bool {
        self.start_background_action(BackgroundActionKind::DocsExport { force })
    }

    pub(super) fn handle_docs_clean(&mut self, apply: bool) -> bool {
        self.start_background_action(BackgroundActionKind::DocsClean { apply })
    }

    pub(super) fn handle_reconcile_now(&mut self) -> bool {
        self.start_background_action(BackgroundActionKind::Reconcile)
    }

    pub(super) fn handle_materialize_now(&mut self) -> bool {
        let ctx = self.action_context();
        let outcome = materialize_now(&ctx, &mut self.materializer);
        self.set_toast(self.action_toast("materialize", &outcome));
        self.log_action_outcome("materialize", &outcome);
        true
    }

    pub(super) fn handle_sync_now(&mut self) -> bool {
        self.start_background_action(BackgroundActionKind::Sync)
    }

    pub(super) fn handle_apply_compatibility_now(&mut self) -> bool {
        self.start_background_action(BackgroundActionKind::Compatibility)
    }

    pub(super) fn handle_toggle_auto_sync(&mut self) -> bool {
        let ctx = self.action_context();
        let desired = !self.auto_sync_enabled;
        let outcome = set_auto_sync(&ctx, desired);
        if matches!(outcome, ActionOutcome::Ack { .. }) {
            self.auto_sync_enabled = desired;
            self.rebuild_header_vm();
        }
        self.set_toast(self.action_toast("auto-sync", &outcome));
        self.log_action_outcome("auto-sync", &outcome);
        true
    }

    pub(super) fn handle_toggle_semantic_triage(&mut self) -> bool {
        let ctx = self.action_context();
        let enabled = self
            .snapshot
            .config
            .as_ref()
            .map(|config| config.enable_semantic_triage)
            .unwrap_or(false);
        if enabled {
            let outcome = set_semantic_triage(&ctx, false);
            self.set_toast(self.action_toast("embeddings", &outcome));
            self.log_action_outcome("embeddings", &outcome);
            self.refresh_after_action();
            return true;
        }

        if !semantic_feature_compiled() {
            self.set_toast("embeddings unavailable: rebuild with `--features semantic-triage`");
            return true;
        }

        self.launch_embeddings_setup = true;
        self.should_exit = true;
        true
    }

    pub(super) fn handle_toggle_worktrees(&mut self) -> bool {
        let ctx = self.action_context();
        let enabled = self
            .snapshot
            .config
            .as_ref()
            .map(|config| config.include_worktrees)
            .unwrap_or_else(|| Config::default().include_worktrees);
        let outcome = set_worktrees_enabled(&ctx, !enabled);
        self.set_toast(self.action_toast("worktrees", &outcome));
        self.log_action_outcome("worktrees", &outcome);
        if matches!(outcome, ActionOutcome::Completed { .. }) {
            self.refresh_after_action();
        }
        true
    }

    pub(super) fn handle_toggle_mcp_sentry_telemetry(&mut self) -> bool {
        self.start_background_action(BackgroundActionKind::SentryTelemetry {
            desired: !self.mcp_sentry_telemetry_enabled(),
        })
    }

    pub(super) fn mcp_sentry_telemetry_enabled(&self) -> bool {
        self.snapshot
            .config
            .as_ref()
            .is_some_and(|config| config.mcp_sentry_telemetry_enabled())
    }

    pub(super) fn queue_embedding_build(&mut self) {
        if !semantic_feature_compiled() {
            self.set_toast("embeddings unavailable: rebuild with `--features semantic-triage`");
            return;
        }

        let enabled = self
            .snapshot
            .config
            .as_ref()
            .map(|config| config.enable_semantic_triage)
            .unwrap_or(false);
        if !enabled {
            self.set_toast("enable embeddings first with T");
            return;
        }

        self.launch_embedding_build(PendingEmbeddingBuild {
            stopped_watch: false,
        });
    }

    pub(super) fn handle_watch_toggle(&mut self) -> bool {
        if !matches!(self.mode, AppMode::DashboardPoll) {
            return false;
        }

        self.start_background_action(BackgroundActionKind::WatchToggle {
            stop: self.watch_is_running(),
        })
    }

    /// Watch label for the footer hint row, when a toggle is available.
    pub fn watch_toggle_label(&self) -> Option<&'static str> {
        watch_toggle_label_for(&self.mode, &self.snapshot)
    }

    pub(super) fn action_context(&self) -> ActionContext {
        self.project_action_context()
            .map(|ctx| ctx.action_context())
            .unwrap_or_else(|| ActionContext::new(&self.repo_root))
    }

    fn project_action_context(&self) -> Option<ProjectActionContext> {
        let project_id = self.project_id.as_ref()?;
        let project_name = self.project_name.as_deref().unwrap_or(project_id);
        Some(ProjectActionContext::new(
            project_id,
            project_name,
            &self.repo_root,
        ))
    }

    pub(super) fn log_action_outcome(&mut self, tag: &str, outcome: &ActionOutcome) {
        let entry = self
            .project_action_context()
            .map(|ctx| outcome_to_project_log(&ctx, tag, outcome))
            .unwrap_or_else(|| outcome_to_log(tag, outcome));
        self.log.push(entry);
    }

    pub(super) fn action_toast(&self, verb: &str, outcome: &ActionOutcome) -> String {
        self.project_message(action_outcome_toast(verb, outcome))
    }

    fn project_message(&self, message: String) -> String {
        match self.project_name.as_ref() {
            Some(name) => format!("[{name}] {message}"),
            None => message,
        }
    }

    fn watch_is_running(&self) -> bool {
        matches!(
            self.snapshot
                .diagnostics
                .as_ref()
                .map(|diag| &diag.watch_status),
            Some(WatchServiceStatus::Running(_) | WatchServiceStatus::Starting)
        )
    }
}

pub(super) fn watch_toggle_label_for(
    mode: &AppMode,
    snapshot: &crate::surface::status_snapshot::StatusSnapshot,
) -> Option<&'static str> {
    if !matches!(mode, AppMode::DashboardPoll) {
        return None;
    }
    match snapshot.diagnostics.as_ref().map(|diag| &diag.watch_status) {
        Some(WatchServiceStatus::Running(_) | WatchServiceStatus::Starting) => Some("stop"),
        _ => Some("start"),
    }
}

fn action_outcome_toast(verb: &str, outcome: &ActionOutcome) -> String {
    match outcome {
        ActionOutcome::Ack { message } | ActionOutcome::Completed { message } => message.clone(),
        ActionOutcome::Conflict { guidance, .. } => format!("{verb}: {guidance}"),
        ActionOutcome::Error { message } => format!("{verb}: {message}"),
    }
}
