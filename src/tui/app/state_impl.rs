//! AppState constructors, toast management, event draining, and snapshot
//! refresh. Split out of `app/mod.rs` so the core state machine stays under
//! the 400-line cap.

use std::path::Path;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};

use super::render_cache::{build_initial_header_vm, build_initial_integration_display_rows};
use super::{
    ActiveTab, AppMode, AppState, DashboardExit, EventLog, PendingEmbeddingBuild, PendingExplainRun,
};
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::config::Config;
use crate::pipeline::explain::telemetry;
use crate::pipeline::watch::{watch_service_status, WatchEvent, WatchServiceStatus};
use crate::surface::status_snapshot::{build_status_snapshot, StatusOptions};
use crate::tui::actions::{materialize_now, now_rfc3339, ActionContext};
use crate::tui::agent_integrations::build_agent_install_statuses;
use crate::tui::materializer::{MaterializeOutcome, MaterializeState, MaterializerSupervisor};
use crate::tui::probe::Severity;
use crate::tui::theme::Theme;
use crate::tui::widgets::LogEntry;

use super::quick_actions_for;

const TOAST_TTL: Duration = Duration::from_millis(4000);

impl AppState {
    /// Build a new poll-mode app state for `repo_root`.
    pub fn new_poll(repo_root: &Path, theme: Theme, integration: AgentIntegration) -> Self {
        Self::new(
            repo_root,
            theme,
            integration,
            AppMode::DashboardPoll,
            None,
            Vec::new(),
        )
    }

    /// Build a new live-mode app state bound to a `WatchEvent` receiver. Live
    /// mode still polls status files for the header stats, but the log pane is
    /// driven by the event bus instead of being inferred from state-file diffs.
    pub fn new_live(
        repo_root: &Path,
        theme: Theme,
        integration: AgentIntegration,
        events_rx: Receiver<WatchEvent>,
    ) -> Self {
        Self::new(
            repo_root,
            theme,
            integration,
            AppMode::DashboardLive,
            Some(events_rx),
            Vec::new(),
        )
    }

    /// Build a new poll-mode app state with pre-seeded log entries.
    pub fn new_poll_with_logs(
        repo_root: &Path,
        theme: Theme,
        integration: AgentIntegration,
        startup_logs: Vec<LogEntry>,
    ) -> Self {
        Self::new(
            repo_root,
            theme,
            integration,
            AppMode::DashboardPoll,
            None,
            startup_logs,
        )
    }

    fn new(
        repo_root: &Path,
        theme: Theme,
        integration: AgentIntegration,
        mode: AppMode,
        events_rx: Option<Receiver<WatchEvent>>,
        startup_logs: Vec<LogEntry>,
    ) -> Self {
        let snapshot = build_status_snapshot(
            repo_root,
            StatusOptions {
                recent: true,
                full: false,
            },
        );
        let quick_actions = quick_actions_for(&mode, &snapshot);
        let auto_sync_enabled = Config::load(repo_root)
            .map(|c| c.auto_sync_enabled)
            .unwrap_or(true);
        let integration_status_rows = build_agent_install_statuses(repo_root);
        let header_vm = build_initial_header_vm(
            repo_root,
            None,
            &snapshot,
            &integration,
            auto_sync_enabled,
            &integration_status_rows,
        );
        let integration_display_rows =
            build_initial_integration_display_rows(&integration_status_rows);
        let mut log = EventLog::default();
        for entry in startup_logs {
            log.push(entry);
        }
        Self {
            project_id: None,
            project_name: None,
            repo_root: repo_root.to_path_buf(),
            theme,
            mode,
            integration,
            snapshot,
            header_vm,
            log,
            quick_actions,
            integration_display_rows,
            integration_selected: 0,
            suggestion_report: None,
            explore_projects: Vec::new(),
            explore_projects_loaded_at: None,
            explore_selected: 0,
            switch_project_root: None,
            should_exit: false,
            launch_integration: None,
            launch_project_mcp_install: false,
            launch_explain_setup: false,
            launch_embeddings_setup: false,
            pending_explain: std::collections::VecDeque::new(),
            launch_embedding_build: None,
            confirm_stop_watch: None,
            pending_quick_confirm: None,
            picker: None,
            explain_preview: None,
            active_tab: ActiveTab::Live,
            scroll_offset: 0,
            live_visible_rows: 18,
            follow_mode: true,
            frame: 0,
            reconcile_active: false,
            auto_sync_enabled,
            poll_timeout: Duration::from_millis(125),
            snapshot_refresh_interval: Duration::from_secs(2),
            last_refresh: Instant::now(),
            explain_preview_refresh_interval: Duration::from_secs(10),
            toast: None,
            events_rx,
            explain_rx: telemetry::subscribe(),
            materializer: MaterializerSupervisor::new(repo_root),
            materialize_state: MaterializeState::Idle,
        }
    }

    /// Set a transient footer toast. A rapid re-press resets the visible
    /// window rather than stacking, so the operator always sees the latest.
    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    /// Currently-visible toast text, or `None` if the TTL has elapsed.
    pub fn active_toast(&self) -> Option<&str> {
        self.toast.as_ref().and_then(|(msg, set_at)| {
            if set_at.elapsed() < TOAST_TTL {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }

    /// Compute the post-loop exit intent. Called after the render loop unwinds.
    pub fn exit_intent(&self) -> DashboardExit {
        if let Some(repo_root) = &self.switch_project_root {
            return DashboardExit::SwitchProject(repo_root.clone());
        }
        if self.launch_explain_setup {
            return DashboardExit::LaunchExplainSetup;
        }
        if self.launch_embeddings_setup {
            return DashboardExit::LaunchEmbeddingsSetup;
        }
        if let Some(pending) = &self.launch_embedding_build {
            return DashboardExit::LaunchEmbeddingBuild(pending.clone());
        }
        if self.launch_project_mcp_install {
            return DashboardExit::LaunchProjectMcpInstall;
        }
        if let Some(request) = &self.launch_integration {
            return DashboardExit::LaunchIntegration(request.clone());
        }
        DashboardExit::Quit
    }

    /// Take a queued explain run so the dashboard loop can execute it without
    /// leaving the alternate screen.
    pub fn take_pending_explain(&mut self) -> Option<PendingExplainRun> {
        self.pending_explain.pop_front()
    }

    /// Queue a dashboard explain run, deduping by mode so rapid repeat
    /// keypresses do not silently replace already scheduled work.
    pub(super) fn enqueue_pending_explain(&mut self, run: PendingExplainRun) {
        if self
            .pending_explain
            .iter()
            .any(|pending| pending.mode == run.mode)
        {
            self.set_toast("explain run already queued");
            return;
        }
        self.pending_explain.push_back(run);
    }

    /// Request an embedding build outside the alternate screen.
    pub(super) fn launch_embedding_build(&mut self, run: PendingEmbeddingBuild) {
        self.launch_embedding_build = Some(run);
        self.should_exit = true;
    }

    /// Refresh the snapshot if the snapshot-refresh interval has elapsed. In
    /// live mode, this also drains any pending `WatchEvent`s from the bus into
    /// the log pane before the file-based snapshot refresh runs. Advances the
    /// spinner frame counter every tick so the animation runs independently of
    /// the snapshot cadence.
    pub fn tick(&mut self) {
        self.drain_events();
        self.frame = self.frame.wrapping_add(1);
        if self.last_refresh.elapsed() >= self.snapshot_refresh_interval {
            self.refresh_now();
        }
        self.drain_materializer();
        self.maybe_auto_materialize();
        self.materialize_state = self.materializer.state().clone();
    }

    /// Reap a finished bootstrap thread, if any. Pushes a log entry + toast
    /// describing the outcome and forces a snapshot refresh on success so
    /// the health row flips on the same frame.
    fn drain_materializer(&mut self) {
        let Some(outcome) = self.materializer.try_drain() else {
            return;
        };
        match outcome {
            MaterializeOutcome::Completed { files, symbols } => {
                let message = format!("graph materialized: {files} files, {symbols} symbols");
                self.set_toast(message.clone());
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message,
                    severity: Severity::Healthy,
                });
                // Force a refresh so the snapshot picks up the new graph
                // store immediately rather than waiting for the next tick.
                self.refresh_now();
            }
            MaterializeOutcome::Failed { error } => {
                let toast = format!("materialize failed: {error}");
                self.set_toast(toast);
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message: format!("bootstrap failed: {error}"),
                    severity: Severity::Blocked,
                });
            }
        }
    }

    /// One-shot auto-fire: when the dashboard sees `graph_stats.is_none()`
    /// and the supervisor is idle and we have not auto-attempted yet this
    /// session, dispatch `materialize_now`. The watch precheck is delegated
    /// to the action so a watch-active repo gets the same `Conflict`
    /// guidance as a manual `M` press.
    fn maybe_auto_materialize(&mut self) {
        if self.snapshot.graph_stats.is_some() {
            return;
        }
        if self.materializer.is_running() || self.materializer.auto_was_attempted() {
            return;
        }
        // Guarded against the .synrepo/-not-initialized case (snapshot
        // surfaces that as `repo: not initialized`, a different row): only
        // attempt when bootstrap has at least a chance of succeeding without
        // user-visible setup. Bootstrap itself is idempotent for both fresh
        // and partial states, so we let it run regardless and rely on the
        // `Failed` arm above to surface any blocker.
        let ctx = ActionContext::new(&self.repo_root);
        // Skip when a foreign watch service or starting daemon owns the
        // repo: the action would just return Conflict, but we also do not
        // want to push a noisy auto-attempt log entry in that case.
        if matches!(
            watch_service_status(&ctx.synrepo_dir),
            WatchServiceStatus::Running(_) | WatchServiceStatus::Starting
        ) {
            self.materializer.mark_auto_attempted();
            return;
        }
        let outcome = materialize_now(&ctx, &mut self.materializer);
        self.materializer.mark_auto_attempted();
        // The action returns Ack on success and Conflict if the watch
        // status flipped between our check and dispatch. Both translate to
        // a single info-level log entry so the operator knows we tried.
        match outcome {
            crate::tui::actions::ActionOutcome::Ack { message } => {
                self.set_toast(format!("auto: {message}"));
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message: format!("auto: {message}"),
                    severity: Severity::Healthy,
                });
            }
            crate::tui::actions::ActionOutcome::Conflict { guidance, .. } => {
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message: format!("auto skipped: {guidance}"),
                    severity: Severity::Stale,
                });
            }
            crate::tui::actions::ActionOutcome::Completed { message }
            | crate::tui::actions::ActionOutcome::Error { message } => {
                self.log.push(LogEntry {
                    timestamp: now_rfc3339(),
                    tag: "materialize".to_string(),
                    message,
                    severity: Severity::Stale,
                });
            }
        }
    }

    /// Drain all pending events from the watch bus into the log pane. Called
    /// from `tick()`. A disconnected sender clears the receiver so subsequent
    /// calls are no-ops. Best-effort: a full log just drops oldest entries.
    /// Also flips `reconcile_active` so the header spinner tracks in-flight
    /// reconcile passes without requiring a separate event channel.
    pub fn drain_events(&mut self) {
        self.drain_watch_events();
        self.drain_explain_events();
    }

    fn drain_watch_events(&mut self) {
        let Some(rx) = self.events_rx.as_ref() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    match &event {
                        WatchEvent::ReconcileStarted { .. } | WatchEvent::SyncStarted { .. } => {
                            self.reconcile_active = true
                        }
                        WatchEvent::ReconcileFinished { .. }
                        | WatchEvent::SyncFinished { .. }
                        | WatchEvent::Error { .. } => self.reconcile_active = false,
                        WatchEvent::SyncProgress { .. } => {}
                    }
                    self.log.push(super::watch_event_to_log_entry(event));
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.events_rx = None;
                    self.reconcile_active = false;
                    return;
                }
            }
        }
    }

    fn drain_explain_events(&mut self) {
        loop {
            match self.explain_rx.try_recv() {
                Ok(event) => {
                    if let Some(entry) = super::explain_event_to_log_entry(event) {
                        self.log.push(entry);
                    }
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // Re-subscribe so a dropped or reaped sender does not
                    // silently stop the feed. Telemetry fanout reaps
                    // disconnected receivers on every publish, so we may land
                    // here after a long idle period.
                    self.explain_rx = telemetry::subscribe();
                    return;
                }
            }
        }
    }
}
