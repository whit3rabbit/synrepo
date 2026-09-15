//! Single-flight workers for dashboard actions that may scan or mutate a repo.

#[cfg(not(test))]
use std::thread;

#[cfg(not(test))]
use crossbeam_channel::bounded;
use crossbeam_channel::TryRecvError;

use crate::pipeline::explain::docs::{
    clean_commentary_docs, export_commentary_docs, CommentaryDocsExportOptions,
};
use crate::store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore};
use crate::structure::graph::with_graph_read_snapshot;
use crate::tui::actions::{
    apply_compatibility_now, reconcile_now, set_mcp_sentry_telemetry, start_watch_daemon,
    stop_watch, sync_now, ActionContext, ActionOutcome,
};

use super::AppState;

#[derive(Clone, Copy, Debug)]
pub(super) enum BackgroundActionKind {
    Reconcile,
    Sync,
    Compatibility,
    WatchToggle { stop: bool },
    DocsExport { force: bool },
    DocsClean { apply: bool },
    SentryTelemetry { desired: bool },
}

impl BackgroundActionKind {
    fn label(self) -> &'static str {
        match self {
            Self::Reconcile => "reconcile",
            Self::Sync => "sync",
            Self::Compatibility => "compatibility",
            Self::WatchToggle { .. } => "watch",
            Self::DocsExport { .. } | Self::DocsClean { .. } => "docs",
            Self::SentryTelemetry { .. } => "sentry telemetry",
        }
    }

    fn log_tag(self) -> &'static str {
        match self {
            Self::SentryTelemetry { .. } => "sentry",
            _ => self.label(),
        }
    }

    fn invalidates_repo_views(self) -> bool {
        matches!(self, Self::Reconcile | Self::Sync | Self::Compatibility)
    }

    fn run(self, ctx: &ActionContext) -> ActionOutcome {
        match self {
            Self::Reconcile => reconcile_now(ctx),
            Self::Sync => sync_now(ctx),
            Self::Compatibility => apply_compatibility_now(ctx),
            Self::WatchToggle { stop: true } => stop_watch(ctx),
            Self::WatchToggle { stop: false } => start_watch_daemon(ctx),
            Self::DocsExport { force } => export_docs(ctx, force),
            Self::DocsClean { apply } => clean_docs(ctx, apply),
            Self::SentryTelemetry { desired } => set_mcp_sentry_telemetry(ctx, desired),
        }
    }
}

pub(super) struct BackgroundActionResult {
    kind: BackgroundActionKind,
    outcome: ActionOutcome,
}

impl AppState {
    pub(super) fn start_background_action(&mut self, kind: BackgroundActionKind) -> bool {
        if self.background_action_rx.is_some() {
            self.set_toast("another dashboard action is still running");
            return true;
        }
        let ctx = self.action_context();

        #[cfg(test)]
        self.finish_background_action(BackgroundActionResult {
            kind,
            outcome: kind.run(&ctx),
        });

        #[cfg(not(test))]
        {
            let (tx, rx) = bounded(1);
            thread::spawn(move || {
                let outcome = kind.run(&ctx);
                let _ = tx.try_send(BackgroundActionResult { kind, outcome });
            });
            self.background_action_rx = Some(rx);
            self.reconcile_active = true;
            self.set_toast(format!("{} started in background", kind.label()));
        }
        true
    }

    pub(super) fn drain_background_action(&mut self) {
        let Some(rx) = self.background_action_rx.as_ref() else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.background_action_rx = None;
                self.reconcile_active = false;
                self.set_toast("dashboard action worker stopped unexpectedly");
                return;
            }
        };
        self.background_action_rx = None;
        self.finish_background_action(result);
    }

    fn finish_background_action(&mut self, result: BackgroundActionResult) {
        let label = result.kind.label();
        let invalidates_repo_views = result.kind.invalidates_repo_views()
            && matches!(&result.outcome, ActionOutcome::Completed { .. });
        self.reconcile_active = false;
        self.set_toast(self.action_toast(label, &result.outcome));
        self.log_action_outcome(result.kind.log_tag(), &result.outcome);
        if invalidates_repo_views {
            self.invalidate_suggestions();
            self.invalidate_explain_preview();
        }
        self.refresh_after_action();
    }
}

fn export_docs(ctx: &ActionContext, force: bool) -> ActionOutcome {
    let result = (|| -> anyhow::Result<String> {
        let graph = SqliteGraphStore::open_existing(&ctx.synrepo_dir.join("graph"))?;
        let overlay = SqliteOverlayStore::open_existing(&ctx.synrepo_dir.join("overlay")).ok();
        let summary = with_graph_read_snapshot(&graph, |graph| {
            export_commentary_docs(
                &ctx.synrepo_dir,
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
    action_result(result, "docs export failed")
}

fn clean_docs(ctx: &ActionContext, apply: bool) -> ActionOutcome {
    let result = clean_commentary_docs(&ctx.synrepo_dir, apply)
        .map(|summary| {
            let verb = if apply { "removed" } else { "would remove" };
            let suffix = if apply { "" } else { " (preview only)" };
            format!(
                "{verb} {} doc file(s) and {} index file(s){suffix}",
                summary.doc_files, summary.index_files
            )
        })
        .map_err(anyhow::Error::from);
    action_result(result, "docs clean failed")
}

fn action_result(result: anyhow::Result<String>, error_prefix: &str) -> ActionOutcome {
    match result {
        Ok(message) => ActionOutcome::Completed { message },
        Err(error) => ActionOutcome::Error {
            message: format!("{error_prefix}: {error:#}"),
        },
    }
}
