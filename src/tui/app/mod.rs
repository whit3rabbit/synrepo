//! App shell: event loop, key handling, and the state machine that picks
//! between dashboard (poll / live) mode and the various wizards. Wizard
//! rendering still lives in `wizard.rs`; this module only owns the loop.

mod synthesis_events;
mod view_state;
mod watch_events;

pub use synthesis_events::synthesis_event_to_log_entry;
pub use view_state::ActiveTab;
pub use watch_events::watch_event_to_log_entry;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::pipeline::synthesis::telemetry::{self, SynthesisEvent};
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
    entries: Vec<LogEntry>,
    cap: usize,
}

impl EventLog {
    /// New empty log with the given capacity (minimum 16).
    pub fn new(cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(cap.max(16)),
            cap: cap.max(16),
        }
    }

    /// Push a new entry, dropping the oldest if the log is full.
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.cap {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    /// Entries oldest-to-newest, borrowed. Cheap to call every render tick.
    pub fn as_slice(&self) -> &[LogEntry] {
        &self.entries
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
    /// Currently selected dashboard tab.
    pub active_tab: ActiveTab,
    /// Rows-up-from-bottom for the Live tab. `0` pins the view to the newest
    /// entry; higher values scroll the frame up.
    pub scroll_offset: usize,
    /// When true, new Live-feed entries snap the view back to the bottom.
    pub follow_mode: bool,
    /// Monotonic tick counter for the header spinner animation.
    pub frame: u32,
    /// True between a `ReconcileStarted` and its matching
    /// `ReconcileFinished`/`Error`. Drives whether the header spinner renders.
    pub reconcile_active: bool,
    /// How long `poll_key` waits for a key event before returning. Set short
    /// so the spinner and snapshot refresh feel live.
    pub poll_timeout: Duration,
    /// Cadence at which file-based status snapshots are rebuilt. Independent
    /// of `poll_timeout` so the spinner can redraw at 10 Hz while the
    /// expensive snapshot refresh stays at 2 s.
    pub snapshot_refresh_interval: Duration,
    /// Last time we rebuilt the snapshot.
    last_refresh: Instant,
    /// Transient footer message. Set by `r` so a refresh gives the operator
    /// confirmation even when the snapshot was already current.
    toast: Option<(String, Instant)>,
    /// Live-mode only: receiver streaming `WatchEvent`s from the hosted watch
    /// service. `None` in poll mode so the dashboard falls back to file-based
    /// snapshot refresh.
    events_rx: Option<Receiver<WatchEvent>>,
    /// Process-global synthesis event stream. Present in both poll and live
    /// modes: if the user triggers synthesis from within the TUI host (future)
    /// or any other in-process call site fires, the events merge into the log.
    synthesis_rx: Receiver<SynthesisEvent>,
}

const TOAST_TTL: Duration = Duration::from_millis(2000);

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
            active_tab: ActiveTab::Live,
            scroll_offset: 0,
            follow_mode: true,
            frame: 0,
            reconcile_active: false,
            poll_timeout: Duration::from_millis(125),
            snapshot_refresh_interval: Duration::from_secs(2),
            last_refresh: Instant::now(),
            toast: None,
            events_rx,
            synthesis_rx: telemetry::subscribe(),
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
        if self.launch_integration {
            DashboardExit::LaunchIntegration
        } else {
            DashboardExit::Quit
        }
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
        self.drain_synthesis_events();
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
                    self.log.push(watch_event_to_log_entry(event));
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

    fn drain_synthesis_events(&mut self) {
        loop {
            match self.synthesis_rx.try_recv() {
                Ok(event) => {
                    if let Some(entry) = synthesis_event_to_log_entry(event) {
                        self.log.push(entry);
                    }
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // Re-subscribe so a dropped or reaped sender does not
                    // silently stop the feed. Telemetry fanout reaps
                    // disconnected receivers on every publish, so we may land
                    // here after a long idle period.
                    self.synthesis_rx = telemetry::subscribe();
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
                self.set_tab(ActiveTab::Actions);
                return true;
            }
            _ => {}
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
mod tests;
