//! AppState constructors, toast management, event draining, and snapshot
//! refresh. Split out of `app/mod.rs` so the core state machine stays under
//! the 400-line cap.

use std::path::Path;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};

use super::{ActiveTab, AppMode, AppState, DashboardExit, EventLog, PendingExplainRun};
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::pipeline::explain::telemetry;
use crate::pipeline::watch::WatchEvent;
use crate::surface::status_snapshot::{build_status_snapshot, StatusOptions};
use crate::tui::actions::now_rfc3339;
use crate::tui::probe::Severity;
use crate::tui::theme::Theme;
use crate::tui::widgets::LogEntry;

use super::quick_actions_for;

const TOAST_TTL: Duration = Duration::from_millis(2000);

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
        let mut log = EventLog::default();
        for entry in startup_logs {
            log.push(entry);
        }
        Self {
            repo_root: repo_root.to_path_buf(),
            theme,
            mode,
            integration,
            snapshot,
            log,
            quick_actions,
            should_exit: false,
            launch_integration: false,
            launch_explain_setup: false,
            pending_explain: None,
            confirm_stop_watch: None,
            picker: None,
            explain_preview: None,
            active_tab: ActiveTab::Live,
            scroll_offset: 0,
            follow_mode: true,
            frame: 0,
            reconcile_active: false,
            poll_timeout: Duration::from_millis(125),
            snapshot_refresh_interval: Duration::from_secs(2),
            last_refresh: Instant::now(),
            explain_preview_refresh_interval: Duration::from_secs(10),
            toast: None,
            events_rx,
            explain_rx: telemetry::subscribe(),
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
        if self.launch_explain_setup {
            return DashboardExit::LaunchExplainSetup;
        }
        if self.launch_integration {
            return DashboardExit::LaunchIntegration;
        }
        DashboardExit::Quit
    }

    /// Take a queued explain run so the dashboard loop can execute it without
    /// leaving the alternate screen.
    pub fn take_pending_explain(&mut self) -> Option<PendingExplainRun> {
        self.pending_explain.take()
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
                        WatchEvent::ReconcileStarted { .. } => self.reconcile_active = true,
                        WatchEvent::ReconcileFinished { .. } | WatchEvent::Error { .. } => {
                            self.reconcile_active = false
                        }
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

    /// Force a snapshot refresh right now.
    pub fn refresh_now(&mut self) {
        self.snapshot = build_status_snapshot(
            &self.repo_root,
            StatusOptions {
                recent: true,
                full: false,
            },
        );
        self.quick_actions = quick_actions_for(&self.mode, &self.snapshot);
        if matches!(self.active_tab, ActiveTab::Explain) {
            self.refresh_explain_preview(false);
        }
        self.last_refresh = Instant::now();
    }

    /// Push the one-shot welcome banner entry that appears on first transition
    /// from the setup wizard. The caller is responsible for only invoking this
    /// when the dashboard is being opened for the first time after a
    /// successful wizard run; the dashboard itself has no persistent state to
    /// enforce the "one-shot" property.
    pub fn push_welcome_banner(&mut self) {
        self.log.push(LogEntry {
            timestamp: now_rfc3339(),
            tag: "synrepo".to_string(),
            message:
                "Welcome to synrepo. Press q to quit, r to refresh. Run `synrepo --help` for more."
                    .to_string(),
            severity: Severity::Healthy,
        });
    }
}
