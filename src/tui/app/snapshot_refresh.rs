//! Non-blocking dashboard snapshot refreshes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crossbeam_channel::{bounded, Receiver, TryRecvError};

use super::render_cache::{build_initial_header_vm, build_initial_integration_display_rows};
use super::{quick_actions_for, ActiveTab, AppState};
use crate::bootstrap::runtime_probe::{probe, AgentIntegration};
use crate::config::Config;
use crate::store::sqlite::SqliteGraphStore;
use crate::surface::readiness::ReadinessMatrix;
use crate::surface::status_snapshot::{
    build_status_snapshot_reusing_expensive_fields, build_status_snapshot_with_node_stats,
    StatusOptions, StatusSnapshot,
};
use crate::tui::agent_integrations::{build_agent_install_statuses, AgentInstallStatus};
use crate::tui::probe::HealthRow;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum SnapshotRefreshMode {
    Cached,
    Full,
}

static SNAPSHOT_WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);

struct SnapshotWorkerPermit;

impl SnapshotWorkerPermit {
    fn acquire() -> Option<Self> {
        SNAPSHOT_WORKER_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for SnapshotWorkerPermit {
    fn drop(&mut self) {
        SNAPSHOT_WORKER_ACTIVE.store(false, Ordering::Release);
    }
}

pub(super) struct SnapshotRefreshResult {
    mode: SnapshotRefreshMode,
    snapshot: StatusSnapshot,
    integration: AgentIntegration,
    integration_statuses: Vec<AgentInstallStatus>,
    readiness_rows: Vec<HealthRow>,
}

#[derive(Default)]
pub(super) struct SnapshotRefresher {
    rx: Option<Receiver<SnapshotRefreshResult>>,
}

pub(super) enum SnapshotRefreshPoll {
    Pending,
    Ready(Box<SnapshotRefreshResult>),
    Disconnected,
}

impl SnapshotRefresher {
    pub(super) fn is_running(&self) -> bool {
        self.rx.is_some()
    }

    pub(super) fn start(
        &mut self,
        repo_root: PathBuf,
        previous: StatusSnapshot,
        mode: SnapshotRefreshMode,
    ) -> bool {
        if self.is_running() {
            return false;
        }
        let Some(permit) = SnapshotWorkerPermit::acquire() else {
            return false;
        };
        let (tx, rx) = bounded(1);
        let worker = thread::Builder::new()
            .name("synrepo-tui-status".to_string())
            .spawn(move || {
                let _permit = permit;
                let result = build_refresh_result(&repo_root, &previous, mode);
                // Closing the dashboard drops the receiver. Never keep a
                // worker blocked on a UI that no longer exists.
                let _ = tx.try_send(result);
            });
        if worker.is_err() {
            return false;
        }
        self.rx = Some(rx);
        true
    }

    pub(super) fn poll(&mut self) -> SnapshotRefreshPoll {
        let Some(rx) = self.rx.as_ref() else {
            return SnapshotRefreshPoll::Pending;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.rx = None;
                SnapshotRefreshPoll::Ready(Box::new(result))
            }
            Err(TryRecvError::Empty) => SnapshotRefreshPoll::Pending,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                SnapshotRefreshPoll::Disconnected
            }
        }
    }
}

pub(super) fn graph_store_present(repo_root: &Path) -> bool {
    let graph_dir = Config::synrepo_dir(repo_root).join("graph");
    SqliteGraphStore::db_path(&graph_dir).exists()
}

fn build_refresh_result(
    repo_root: &Path,
    previous: &StatusSnapshot,
    mode: SnapshotRefreshMode,
) -> SnapshotRefreshResult {
    let snapshot = match mode {
        SnapshotRefreshMode::Cached => {
            build_status_snapshot_reusing_expensive_fields(repo_root, previous)
        }
        SnapshotRefreshMode::Full => build_status_snapshot_with_node_stats(
            repo_root,
            StatusOptions {
                recent: true,
                full: false,
            },
        ),
    };
    let probe_report = probe(repo_root);
    let integration = probe_report.agent_integration.clone();
    let integration_statuses = build_agent_install_statuses(repo_root);
    let readiness_rows = if snapshot.initialized {
        let config = snapshot.config.clone().unwrap_or_default();
        ReadinessMatrix::build(repo_root, &probe_report, &snapshot, &config)
            .rows
            .into_iter()
            .map(|row| HealthRow {
                label: format!("readiness:{}", row.capability.as_str()),
                value: format!("{}: {}", row.state.as_str(), row.detail),
                severity: row.state.severity(),
            })
            .collect()
    } else {
        Vec::new()
    };
    SnapshotRefreshResult {
        mode,
        snapshot,
        integration,
        integration_statuses,
        readiness_rows,
    }
}

fn reconcile_timestamp(snapshot: &StatusSnapshot) -> Option<&str> {
    snapshot
        .diagnostics
        .as_ref()?
        .last_reconcile
        .as_ref()
        .map(|state| state.last_reconcile_at.as_str())
}

impl AppState {
    pub(super) fn start_initial_snapshot_refresh(&mut self) {
        #[cfg(not(test))]
        self.request_full_snapshot_refresh(false);
        #[cfg(test)]
        let _ = self;
    }

    /// Synchronous refresh retained only for focused state tests.
    #[cfg(test)]
    pub fn refresh_now(&mut self) {
        let result =
            build_refresh_result(&self.repo_root, &self.snapshot, SnapshotRefreshMode::Full);
        self.apply_snapshot_refresh(result);
    }

    pub(in crate::tui) fn refresh_after_action(&mut self) {
        #[cfg(test)]
        self.refresh_now();
        #[cfg(not(test))]
        self.request_full_snapshot_refresh(false);
    }

    pub(super) fn request_snapshot_refresh(&mut self, mode: SnapshotRefreshMode, manual: bool) {
        let mode = if mode == SnapshotRefreshMode::Cached
            && self.snapshot.graph_stats.is_none()
            && self.graph_store_present
        {
            SnapshotRefreshMode::Full
        } else {
            mode
        };
        self.manual_snapshot_refresh |= manual;
        if self.snapshot_refresher.is_running()
            || !self
                .snapshot_refresher
                .start(self.repo_root.clone(), self.snapshot.clone(), mode)
        {
            self.snapshot_refresh_pending = Some(
                self.snapshot_refresh_pending
                    .map_or(mode, |pending| pending.max(mode)),
            );
        }
    }

    pub(super) fn request_full_snapshot_refresh(&mut self, manual: bool) {
        self.status_change_detector.acknowledge(&self.repo_root);
        self.request_snapshot_refresh(SnapshotRefreshMode::Full, manual);
    }

    pub(super) fn drain_snapshot_refresh(&mut self) {
        match self.snapshot_refresher.poll() {
            SnapshotRefreshPoll::Pending => {}
            SnapshotRefreshPoll::Disconnected => {
                self.snapshot_refresh_pending = None;
                if self.manual_snapshot_refresh {
                    self.manual_snapshot_refresh = false;
                    self.set_toast("snapshot refresh worker stopped unexpectedly");
                }
            }
            SnapshotRefreshPoll::Ready(result) => {
                let pending = self.snapshot_refresh_pending.take();
                match (result.mode, pending) {
                    (_, Some(pending @ SnapshotRefreshMode::Full))
                    | (SnapshotRefreshMode::Cached, Some(pending @ SnapshotRefreshMode::Cached)) => {
                        self.request_snapshot_refresh(pending, self.manual_snapshot_refresh);
                    }
                    (SnapshotRefreshMode::Full, Some(SnapshotRefreshMode::Cached)) => {
                        self.apply_snapshot_refresh(*result);
                        self.request_snapshot_refresh(SnapshotRefreshMode::Cached, false);
                    }
                    (_, None) => self.apply_snapshot_refresh(*result),
                }
            }
        }

        // Another project may have owned the process-wide status worker. A
        // coalesced request retries without spawning or queueing more threads.
        if !self.snapshot_refresher.is_running() {
            if let Some(pending) = self.snapshot_refresh_pending.take() {
                self.request_snapshot_refresh(pending, self.manual_snapshot_refresh);
            }
        }
    }

    fn apply_snapshot_refresh(&mut self, result: SnapshotRefreshResult) {
        let reconcile_changed =
            reconcile_timestamp(&self.snapshot) != reconcile_timestamp(&result.snapshot);
        let selected_tool = self
            .integration_display_rows
            .get(self.integration_selected_index())
            .map(|row| row.tool.clone());
        self.snapshot = result.snapshot;
        self.integration = result.integration;
        self.graph_store_present = graph_store_present(&self.repo_root);
        self.header_vm = build_initial_header_vm(
            &self.repo_root,
            self.project_name.as_deref(),
            &self.snapshot,
            &self.integration,
            self.auto_sync_enabled,
            &result.integration_statuses,
        );
        self.integration_display_rows =
            build_initial_integration_display_rows(&result.integration_statuses);
        self.preserve_integration_selection(selected_tool.as_deref());
        self.readiness_rows = result.readiness_rows;
        self.quick_actions =
            quick_actions_for(&self.mode, &self.snapshot, self.graph_store_present);
        if reconcile_changed {
            self.invalidate_suggestions();
            self.invalidate_explain_preview();
        }
        if matches!(self.active_tab, ActiveTab::Explain) {
            self.refresh_explain_preview(false);
        }

        if self.manual_snapshot_refresh && result.mode == SnapshotRefreshMode::Full {
            self.manual_snapshot_refresh = false;
            let counts = self
                .snapshot
                .graph_stats
                .as_ref()
                .map(|stats| format!("{} files, {} symbols", stats.file_nodes, stats.symbol_nodes))
                .unwrap_or_else(|| "no graph data".to_string());
            self.set_toast(format!("refreshed: {counts}"));
        }
        if reconcile_changed && result.mode == SnapshotRefreshMode::Cached {
            self.request_full_snapshot_refresh(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::runtime_probe::AgentIntegration;
    use crate::tui::theme::Theme;

    #[test]
    fn refresh_request_coalesces_while_worker_runs() {
        let repo = tempfile::tempdir().unwrap();
        let mut state = AppState::new_poll(repo.path(), Theme::plain(), AgentIntegration::Absent);
        let (tx, rx) = bounded(1);
        state.snapshot_refresher.rx = Some(rx);
        state.snapshot.graph_stats = None;
        state.graph_store_present = true;

        state.request_snapshot_refresh(SnapshotRefreshMode::Cached, false);

        assert_eq!(
            state.snapshot_refresh_pending,
            Some(SnapshotRefreshMode::Full)
        );
        drop(tx);
    }
}
