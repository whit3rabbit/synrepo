//! App shell: event loop, key handling, and the state machine that picks
//! between dashboard (poll / live) mode and the various wizards. Wizard
//! rendering still lives in `wizard.rs`; this module only owns the loop.

mod watch_events;

pub use watch_events::watch_event_to_log_entry;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::pipeline::watch::WatchEvent;
use crate::surface::status_snapshot::{build_status_snapshot, StatusOptions, StatusSnapshot};
use crate::tui::probe::Severity;
use crate::tui::theme::Theme;
use crate::tui::widgets::{LogEntry, QuickAction};

/// Which high-level mode the app is currently in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppMode {
    /// Poll-mode dashboard (bare `synrepo`, `synrepo dashboard`).
    DashboardPoll,
    /// Live-mode dashboard (foreground `synrepo watch` hosting the service).
    DashboardLive,
    /// Guided setup wizard (reached on uninitialized repos).
    SetupWizard,
    /// Guided repair wizard (reached on partial repos).
    RepairWizard,
    /// Agent-integration sub-wizard, launchable from the dashboard.
    IntegrationWizard,
}

/// Bounded in-memory event log, used by both poll and live modes. Capped at
/// 128 entries so a long-running dashboard doesn't leak memory.
#[derive(Clone, Debug)]
pub struct EventLog {
    entries: std::collections::VecDeque<LogEntry>,
    cap: usize,
}

impl EventLog {
    /// New empty log with the given capacity (minimum 16).
    pub fn new(cap: usize) -> Self {
        Self {
            entries: std::collections::VecDeque::with_capacity(cap.max(16)),
            cap: cap.max(16),
        }
    }

    /// Push a new entry, dropping the oldest if the log is full.
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Iterate over entries oldest-to-newest.
    pub fn as_slice(&self) -> Vec<LogEntry> {
        self.entries.iter().cloned().collect()
    }
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new(128)
    }
}

/// Mutable app state passed through the render loop.
pub struct AppState {
    /// Active repo root.
    pub repo_root: PathBuf,
    /// Active theme.
    pub theme: Theme,
    /// Current app mode.
    pub mode: AppMode,
    /// Current probe-derived agent integration signal. Refreshed on each
    /// snapshot rebuild.
    pub integration: AgentIntegration,
    /// Most recent status snapshot; refreshed every poll tick.
    pub snapshot: StatusSnapshot,
    /// Bounded event log.
    pub log: EventLog,
    /// Quick actions for the current mode.
    pub quick_actions: Vec<QuickAction>,
    /// When set, render loop should exit after the current draw.
    pub should_exit: bool,
    /// When set, the caller should launch the integration sub-wizard after the
    /// render loop unwinds. See [`DashboardExit`].
    pub launch_integration: bool,
    /// Refresh interval for poll mode.
    pub poll_interval: Duration,
    /// Last time we rebuilt the snapshot.
    last_refresh: Instant,
    /// Live-mode only: receiver streaming `WatchEvent`s from the hosted watch
    /// service. `None` in poll mode so the dashboard falls back to file-based
    /// snapshot refresh.
    events_rx: Option<Receiver<WatchEvent>>,
}

/// Post-loop intent expressed by the dashboard when it exits. The caller maps
/// this to either "fully done" or "re-enter the dashboard after running a
/// sub-wizard".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DashboardExit {
    /// Operator quit; caller should tear down and return.
    Quit,
    /// Operator asked for the integration sub-wizard; caller should launch it
    /// and then re-open the dashboard.
    LaunchIntegration,
}

impl AppState {
    /// Build a new poll-mode app state for `repo_root`.
    pub fn new_poll(repo_root: &Path, theme: Theme, integration: AgentIntegration) -> Self {
        Self::new(repo_root, theme, integration, AppMode::DashboardPoll, None)
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
        )
    }

    fn new(
        repo_root: &Path,
        theme: Theme,
        integration: AgentIntegration,
        mode: AppMode,
        events_rx: Option<Receiver<WatchEvent>>,
    ) -> Self {
        let snapshot = build_status_snapshot(
            repo_root,
            StatusOptions {
                recent: true,
                full: false,
            },
        );
        Self {
            repo_root: repo_root.to_path_buf(),
            theme,
            mode,
            integration,
            snapshot,
            log: EventLog::default(),
            quick_actions: default_poll_actions(),
            should_exit: false,
            launch_integration: false,
            poll_interval: Duration::from_secs(2),
            last_refresh: Instant::now(),
            events_rx,
        }
    }

    /// Compute the post-loop exit intent. Called after the render loop unwinds.
    pub fn exit_intent(&self) -> DashboardExit {
        if self.launch_integration {
            DashboardExit::LaunchIntegration
        } else {
            DashboardExit::Quit
        }
    }

    /// Refresh the snapshot if the poll interval has elapsed. In live mode,
    /// this also drains any pending `WatchEvent`s from the bus into the log
    /// pane before the file-based snapshot refresh runs.
    pub fn tick(&mut self) {
        self.drain_events();
        if self.last_refresh.elapsed() >= self.poll_interval {
            self.refresh_now();
        }
    }

    /// Drain all pending events from the watch bus into the log pane. Called
    /// from `tick()`. A disconnected sender clears the receiver so subsequent
    /// calls are no-ops. Best-effort: a full log just drops oldest entries.
    pub fn drain_events(&mut self) {
        let Some(rx) = self.events_rx.as_ref() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    self.log.push(watch_event_to_log_entry(event));
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.events_rx = None;
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
        self.last_refresh = Instant::now();
    }

    /// Push the one-shot welcome banner entry that appears on first transition
    /// from the setup wizard. The caller is responsible for only invoking this
    /// when the dashboard is being opened for the first time after a
    /// successful wizard run; the dashboard itself has no persistent state to
    /// enforce the "one-shot" property.
    pub fn push_welcome_banner(&mut self) {
        self.log.push(LogEntry {
            timestamp: crate::tui::actions::now_rfc3339(),
            tag: "synrepo".to_string(),
            message:
                "Welcome to synrepo. Press q to quit, r to refresh. Run `synrepo --help` for more."
                    .to_string(),
            severity: Severity::Healthy,
        });
    }

    /// Handle a key event. Returns true when the event was consumed.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        // Global quit bindings.
        if matches!(code, KeyCode::Char('q') | KeyCode::Esc) {
            self.should_exit = true;
            return true;
        }
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
            return true;
        }
        match code {
            KeyCode::Char('r') => {
                self.refresh_now();
                true
            }
            KeyCode::Char('i') => {
                self.launch_integration = true;
                self.should_exit = true;
                true
            }
            _ => false,
        }
    }
}

fn default_poll_actions() -> Vec<QuickAction> {
    vec![
        QuickAction {
            key: "r".to_string(),
            label: "refresh snapshot".to_string(),
            disabled: false,
        },
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
    ]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::runtime_probe::AgentIntegration;
    use crate::tui::theme::Theme;

    #[test]
    fn new_poll_starts_with_empty_log() {
        let tempdir = tempfile::tempdir().unwrap();
        let state = AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
        assert!(state.log.as_slice().is_empty());
    }

    #[test]
    fn push_welcome_banner_seeds_exactly_one_entry() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut state =
            AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
        state.push_welcome_banner();
        let entries = state.log.as_slice();
        assert_eq!(entries.len(), 1);
        let banner = &entries[0];
        assert_eq!(banner.tag, "synrepo");
        assert!(
            banner.message.to_ascii_lowercase().contains("welcome"),
            "banner message should greet the user: {:?}",
            banner.message
        );
        assert!(matches!(banner.severity, Severity::Healthy));
    }

    #[test]
    fn push_welcome_banner_is_idempotent_per_call_but_caller_must_only_invoke_once() {
        // The state machine itself does not dedupe — the caller is responsible
        // for the one-shot property. This test pins that contract so a future
        // refactor that *does* dedupe internally does not silently drop the
        // banner when the caller relies on exactly-one-push semantics.
        let tempdir = tempfile::tempdir().unwrap();
        let mut state =
            AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
        state.push_welcome_banner();
        state.push_welcome_banner();
        assert_eq!(state.log.as_slice().len(), 2);
    }

    #[test]
    fn drain_events_pulls_all_pending_into_log() {
        let tempdir = tempfile::tempdir().unwrap();
        let (tx, rx) = crossbeam_channel::bounded::<WatchEvent>(8);
        let mut state =
            AppState::new_live(tempdir.path(), Theme::plain(), AgentIntegration::Absent, rx);
        tx.send(WatchEvent::ReconcileStarted {
            at: "t0".to_string(),
            triggering_events: 0,
        })
        .unwrap();
        tx.send(WatchEvent::Error {
            at: "t1".to_string(),
            message: "x".to_string(),
        })
        .unwrap();
        state.drain_events();
        let log = state.log.as_slice();
        assert_eq!(log.len(), 2);
        assert!(log.iter().all(|e| e.tag == "watch"));
    }

    #[test]
    fn drain_events_is_noop_in_poll_mode() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut state =
            AppState::new_poll(tempdir.path(), Theme::plain(), AgentIntegration::Absent);
        state.drain_events();
        assert!(state.log.as_slice().is_empty());
    }
}
