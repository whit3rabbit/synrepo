//! Dashboard action handlers for mutating keys and docs operations.

use crate::config::Config;
use crate::pipeline::explain::docs::{
    clean_commentary_docs, export_commentary_docs, CommentaryDocsExportOptions,
};
use crate::pipeline::watch::WatchServiceStatus;
use crate::store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore};
use crate::structure::graph::with_graph_read_snapshot;
use crate::tui::actions::{
    apply_compatibility_now, materialize_now, outcome_to_log, outcome_to_project_log,
    reconcile_now, semantic_feature_compiled, set_auto_sync, set_semantic_triage,
    start_watch_daemon, stop_watch, sync_now, ActionContext, ActionOutcome, ProjectActionContext,
};

use super::{
    AppMode, AppState, ConfirmStopWatchState, PendingEmbeddingBuild, PendingStopWatchAction,
};

impl AppState {
    pub(super) fn handle_docs_export(&mut self, force: bool) -> bool {
        let synrepo_dir = Config::synrepo_dir(&self.repo_root);
        let result = (|| -> anyhow::Result<String> {
            let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;
            let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).ok();
            let summary = with_graph_read_snapshot(&graph, |graph| {
                export_commentary_docs(
                    &synrepo_dir,
                    graph,
                    overlay.as_ref(),
                    CommentaryDocsExportOptions { force },
                )
            })?;
            Ok(format!(
                "{} docs exported, {} changed{}",
                summary.total_docs,
                summary.changed_paths,
                if force { " (forced rebuild)" } else { "" }
            ))
        })();
        self.record_docs_action("docs", result);
        true
    }

    pub(super) fn handle_docs_clean(&mut self, apply: bool) -> bool {
        let synrepo_dir = Config::synrepo_dir(&self.repo_root);
        let result = clean_commentary_docs(&synrepo_dir, apply).map(|summary| {
            let verb = if apply { "removed" } else { "would remove" };
            let suffix = if apply { "" } else { " (preview only)" };
            format!(
                "{verb} {} doc file(s) and {} index file(s){suffix}",
                summary.doc_files, summary.index_files
            )
        });
        self.record_docs_action("docs", result.map_err(anyhow::Error::from));
        true
    }

    pub(super) fn handle_reconcile_now(&mut self) -> bool {
        let ctx = self.action_context();
        let outcome = reconcile_now(&ctx);
        self.set_toast(self.action_toast("reconcile", &outcome));
        self.log_action_outcome("reconcile", &outcome);
        self.refresh_now();
        true
    }

    pub(super) fn handle_materialize_now(&mut self) -> bool {
        let ctx = self.action_context();
        let outcome = materialize_now(&ctx, &mut self.materializer);
        self.set_toast(self.action_toast("materialize", &outcome));
        self.log_action_outcome("materialize", &outcome);
        true
    }

    pub(super) fn handle_sync_now(&mut self) -> bool {
        let ctx = self.action_context();
        let outcome = sync_now(&ctx);
        self.set_toast(self.action_toast("sync", &outcome));
        self.log_action_outcome("sync", &outcome);
        self.refresh_now();
        true
    }

    pub(super) fn handle_apply_compatibility_now(&mut self) -> bool {
        let ctx = self.action_context();
        let outcome = apply_compatibility_now(&ctx);
        self.set_toast(self.action_toast("compatibility", &outcome));
        self.log_action_outcome("compatibility", &outcome);
        self.refresh_now();
        true
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
            self.refresh_now();
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

        let ctx = self.action_context();
        match crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir) {
            WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => {
                self.confirm_stop_watch = Some(ConfirmStopWatchState {
                    pending: PendingStopWatchAction::BuildEmbeddings,
                });
            }
            _ => self.launch_embedding_build(PendingEmbeddingBuild {
                stopped_watch: false,
            }),
        }
    }

    pub(super) fn handle_watch_toggle(&mut self) -> bool {
        if !matches!(self.mode, AppMode::DashboardPoll) {
            return false;
        }

        let ctx = self.action_context();
        let outcome = if self.watch_is_running() {
            stop_watch(&ctx)
        } else {
            start_watch_daemon(&ctx)
        };
        self.set_toast(self.action_toast("watch", &outcome));
        self.log_action_outcome("watch", &outcome);
        self.refresh_now();
        true
    }

    /// Watch label for the footer hint row, when a toggle is available.
    pub fn watch_toggle_label(&self) -> Option<&'static str> {
        watch_toggle_label_for(&self.mode, &self.snapshot)
    }

    fn record_docs_action(&mut self, tag: &str, result: anyhow::Result<String>) {
        match result {
            Ok(message) => {
                let message = self.project_message(message);
                self.set_toast(message.clone());
                self.log.push(crate::tui::widgets::LogEntry {
                    timestamp: crate::tui::actions::now_rfc3339(),
                    tag: tag.to_string(),
                    message,
                    severity: crate::tui::probe::Severity::Healthy,
                });
                self.refresh_now();
            }
            Err(error) => {
                let message = self.project_message(format!("{error:#}"));
                self.set_toast(format!("docs action failed: {message}"));
                self.log.push(crate::tui::widgets::LogEntry {
                    timestamp: crate::tui::actions::now_rfc3339(),
                    tag: tag.to_string(),
                    message,
                    severity: crate::tui::probe::Severity::Stale,
                });
            }
        }
    }

    fn action_context(&self) -> ActionContext {
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

    fn log_action_outcome(&mut self, tag: &str, outcome: &ActionOutcome) {
        let entry = self
            .project_action_context()
            .map(|ctx| outcome_to_project_log(&ctx, tag, outcome))
            .unwrap_or_else(|| outcome_to_log(tag, outcome));
        self.log.push(entry);
    }

    fn action_toast(&self, verb: &str, outcome: &ActionOutcome) -> String {
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
