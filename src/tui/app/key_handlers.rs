//! Key-event handling and quick-action helpers for [`AppState`]. Split out of
//! `app/mod.rs` so the core state machine stays under the 400-line cap.

use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use super::{ActiveTab, AppMode, AppState, ExplainMode};
use crate::config::Config;
use crate::pipeline::explain::docs::{
    clean_commentary_docs, export_commentary_docs, CommentaryDocsExportOptions,
};
use crate::pipeline::watch::WatchServiceStatus;
use crate::store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore};
use crate::structure::graph::with_graph_read_snapshot;
use crate::surface::status_snapshot::StatusSnapshot;
use crate::tui::actions::{
    materialize_now, outcome_to_log, reconcile_now, set_auto_sync, start_watch_daemon, stop_watch,
    sync_now, ActionContext, ActionOutcome,
};
use crate::tui::widgets::QuickAction;

impl AppState {
    /// Handle a key event. Returns true when the event was consumed.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        // Confirm-stop-watch modal takes precedence: it is a blocking decision
        // point ("stop watch and run explain?") that must be answered before
        // any other key can have effect. Tab switches and global quit fall
        // through below so the operator is never trapped.
        if self.confirm_stop_watch.is_some() {
            if let Some(consumed) = self.handle_confirm_stop_watch_key(code, modifiers) {
                return consumed;
            }
        }
        // Folder-picker modal: consumes navigation/toggle/commit/cancel keys
        // before anything else. Global quit (q/Ctrl-C) and tab switches still
        // fall through below so the operator is never trapped.
        if self.picker.is_some() {
            if let Some(consumed) = self.handle_picker_key(code, modifiers) {
                return consumed;
            }
        }
        // Global quit bindings.
        if matches!(code, KeyCode::Char('q') | KeyCode::Esc) {
            self.should_exit = true;
            return true;
        }
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
            return true;
        }
        // Tab switching.
        match code {
            KeyCode::Tab => {
                self.cycle_tab();
                return true;
            }
            KeyCode::Char('1') => {
                self.set_tab(ActiveTab::Live);
                return true;
            }
            KeyCode::Char('2') => {
                self.set_tab(ActiveTab::Health);
                return true;
            }
            KeyCode::Char('3') => {
                self.set_tab(ActiveTab::Trust);
                return true;
            }
            KeyCode::Char('4') => {
                self.set_tab(ActiveTab::Explain);
                return true;
            }
            KeyCode::Char('5') => {
                self.set_tab(ActiveTab::Actions);
                return true;
            }
            _ => {}
        }
        // Explain-tab key dispatch. Plan-specified bindings:
        //   s — launch explain setup sub-wizard
        //   r — run `synrepo explain` against all stale entries
        //   c — run with `--changed` (recent hotspots)
        //   f — open folder picker sub-view (in-tab, no dashboard exit)
        //   d — export docs from overlay without model calls
        //   D — force rebuild docs/index from overlay
        //   x — preview clean of materialized docs/index
        //   X — clean materialized docs/index, overlay untouched
        if matches!(self.active_tab, ActiveTab::Explain) {
            match code {
                KeyCode::Char('s') => {
                    self.launch_explain_setup = true;
                    self.should_exit = true;
                    return true;
                }
                KeyCode::Char('r') => {
                    self.queue_explain(ExplainMode::AllStale);
                    return true;
                }
                KeyCode::Char('c') => {
                    self.queue_explain(ExplainMode::Changed);
                    return true;
                }
                KeyCode::Char('f') => {
                    self.open_folder_picker();
                    return true;
                }
                KeyCode::Char('d') => {
                    return self.handle_docs_export(false);
                }
                KeyCode::Char('D') => {
                    return self.handle_docs_export(true);
                }
                KeyCode::Char('x') => {
                    return self.handle_docs_clean(false);
                }
                KeyCode::Char('X') => {
                    return self.handle_docs_clean(true);
                }
                _ => {}
            }
        }
        // Live-tab scroll bindings. Disabled on the other tabs so `j`/`k`
        // remain free for future per-tab navigation.
        if matches!(self.active_tab, ActiveTab::Live) {
            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.scroll_up(1);
                    return true;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.scroll_down(1);
                    return true;
                }
                KeyCode::PageUp => {
                    self.page_up();
                    return true;
                }
                KeyCode::PageDown => {
                    self.page_down();
                    return true;
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    self.scroll_home();
                    return true;
                }
                KeyCode::End | KeyCode::Char('G') => {
                    self.scroll_end();
                    return true;
                }
                KeyCode::Char('f') => {
                    self.toggle_follow();
                    return true;
                }
                _ => {}
            }
        }
        match code {
            KeyCode::Char('r') => {
                self.refresh_now();
                let counts = match self.snapshot.graph_stats.as_ref() {
                    Some(g) => format!("{} files, {} symbols", g.file_nodes, g.symbol_nodes),
                    None => "no graph data".to_string(),
                };
                self.set_toast(format!("Refreshed: {counts}"));
                true
            }
            KeyCode::Char('R') => self.handle_reconcile_now(),
            KeyCode::Char('S') => self.handle_sync_now(),
            KeyCode::Char('A') => self.handle_toggle_auto_sync(),
            KeyCode::Char('M') => self.handle_materialize_now(),
            KeyCode::Char('w') => self.handle_watch_toggle(),
            KeyCode::Char('i') => {
                self.launch_integration = true;
                self.should_exit = true;
                true
            }
            _ => false,
        }
    }

    fn handle_docs_export(&mut self, force: bool) -> bool {
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

    fn handle_docs_clean(&mut self, apply: bool) -> bool {
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

    fn record_docs_action(&mut self, tag: &str, result: anyhow::Result<String>) {
        match result {
            Ok(message) => {
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
                let message = format!("{error:#}");
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

    fn handle_reconcile_now(&mut self) -> bool {
        let ctx = ActionContext::new(&self.repo_root);
        let outcome = reconcile_now(&ctx);
        self.set_toast(action_outcome_toast("reconcile", &outcome));
        self.log.push(outcome_to_log("reconcile", &outcome));
        self.refresh_now();
        true
    }

    fn handle_materialize_now(&mut self) -> bool {
        let ctx = ActionContext::new(&self.repo_root);
        let outcome = materialize_now(&ctx, &mut self.materializer);
        self.set_toast(action_outcome_toast("materialize", &outcome));
        self.log.push(outcome_to_log("materialize", &outcome));
        true
    }

    fn handle_sync_now(&mut self) -> bool {
        let ctx = ActionContext::new(&self.repo_root);
        let outcome = sync_now(&ctx);
        self.set_toast(action_outcome_toast("sync", &outcome));
        self.log.push(outcome_to_log("sync", &outcome));
        self.refresh_now();
        true
    }

    fn handle_toggle_auto_sync(&mut self) -> bool {
        let ctx = ActionContext::new(&self.repo_root);
        // Track the flag on AppState so each press toggles from the last value
        // the service acked. Default is `true` (matches config default).
        let desired = !self.auto_sync_enabled;
        let outcome = set_auto_sync(&ctx, desired);
        match &outcome {
            ActionOutcome::Ack { .. } => {
                self.auto_sync_enabled = desired;
            }
            _ => {
                // Leave the local flag untouched; log the failure so the
                // operator knows the service did not accept the flip.
            }
        }
        self.set_toast(action_outcome_toast("auto-sync", &outcome));
        self.log.push(outcome_to_log("auto-sync", &outcome));
        true
    }

    /// Poll-mode dashboards toggle the detached watch daemon with `w`.
    fn handle_watch_toggle(&mut self) -> bool {
        if !matches!(self.mode, AppMode::DashboardPoll) {
            return false;
        }

        let ctx = ActionContext::new(&self.repo_root);
        let outcome = if self.watch_is_running() {
            stop_watch(&ctx)
        } else {
            start_watch_daemon(&ctx)
        };
        self.set_toast(watch_toast_message(&outcome));
        self.log.push(outcome_to_log("watch", &outcome));
        self.refresh_now();
        true
    }

    /// Watch label for the footer hint row, when a toggle is available.
    pub fn watch_toggle_label(&self) -> Option<&'static str> {
        watch_toggle_label_for(&self.mode, &self.snapshot)
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

pub(super) fn quick_actions_for(mode: &AppMode, snapshot: &StatusSnapshot) -> Vec<QuickAction> {
    let mut actions = vec![QuickAction {
        key: "r".to_string(),
        label: "refresh snapshot".to_string(),
        disabled: false,
    }];
    if snapshot.graph_stats.is_none() && snapshot.initialized {
        actions.push(QuickAction {
            key: "M".to_string(),
            label: "generate graph".to_string(),
            disabled: false,
        });
    }
    if let Some(watch_label) = watch_toggle_label_for(mode, snapshot) {
        actions.push(QuickAction {
            key: "w".to_string(),
            label: format!("{watch_label} watch"),
            disabled: false,
        });
    }
    actions.extend([
        QuickAction {
            key: "i".to_string(),
            label: "agent integration".to_string(),
            disabled: false,
        },
        QuickAction {
            key: "q".to_string(),
            label: "quit".to_string(),
            disabled: false,
        },
    ]);
    actions
}

pub(super) fn watch_toggle_label_for(
    mode: &AppMode,
    snapshot: &StatusSnapshot,
) -> Option<&'static str> {
    if !matches!(mode, AppMode::DashboardPoll) {
        return None;
    }
    match snapshot.diagnostics.as_ref().map(|diag| &diag.watch_status) {
        Some(WatchServiceStatus::Running(_) | WatchServiceStatus::Starting) => Some("stop"),
        _ => Some("start"),
    }
}

fn watch_toast_message(outcome: &ActionOutcome) -> String {
    action_outcome_toast("watch", outcome)
}

fn action_outcome_toast(verb: &str, outcome: &ActionOutcome) -> String {
    match outcome {
        ActionOutcome::Ack { message } | ActionOutcome::Completed { message } => message.clone(),
        ActionOutcome::Conflict { guidance, .. } => format!("{verb}: {guidance}"),
        ActionOutcome::Error { message } => format!("{verb}: {message}"),
    }
}

/// Poll the terminal for a key event, honoring a budget tied to the refresh
/// interval. Returns `None` when no key arrived within the budget.
pub fn poll_key(timeout: Duration) -> anyhow::Result<Option<(KeyCode, KeyModifiers)>> {
    if !crossterm::event::poll(timeout)? {
        return Ok(None);
    }
    match crossterm::event::read()? {
        Event::Key(k) if k.kind == KeyEventKind::Press => Ok(Some((k.code, k.modifiers))),
        _ => Ok(None),
    }
}
