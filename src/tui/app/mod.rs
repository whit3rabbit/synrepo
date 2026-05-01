//! App shell: event loop, key handling, and the state machine that picks
//! between dashboard (poll / live) mode and the various wizards. Wizard
//! rendering still lives in `wizard.rs`; this module only owns the loop.

mod action_handlers;
mod confirm_stop_watch;
mod explain_events;
mod explain_picker;
mod explain_preview;
mod explore;
mod key_handlers;
mod state_impl;
mod view_state;
mod watch_events;

pub use confirm_stop_watch::{describe_pending_mode, ConfirmStopWatchState};
pub use explain_events::explain_event_to_log_entry;
pub use explain_picker::{FolderEntry, FolderPickerState};
pub use explain_preview::{ExplainPreviewPanel, ExplainPreviewState};
pub use key_handlers::poll_key;
pub use view_state::ActiveTab;
pub use watch_events::watch_event_to_log_entry;

use key_handlers::quick_actions_for;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;

use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::pipeline::explain::telemetry::ExplainEvent;
use crate::pipeline::watch::WatchEvent;
use crate::tui::mcp_status::McpStatusRow;
use crate::tui::projects::ProjectRef;
use crate::tui::theme::Theme;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingQuickConfirm {
    MaterializeGraph,
    DocsCleanApply,
    ToggleAutoSync,
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
    /// Active project ID when the dashboard is hosted by the global shell.
    pub project_id: Option<String>,
    /// Active project display name when hosted by the global shell.
    pub project_name: Option<String>,
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
    /// Active-project MCP status rows shown by the MCP tab.
    pub mcp_rows: Vec<McpStatusRow>,
    /// Registry-backed project rows shown by the Repos tab.
    pub(crate) explore_projects: Vec<ProjectRef>,
    /// Selected Repos-tab row.
    pub(crate) explore_selected: usize,
    /// Explicit dashboard restart target requested from the Repos tab.
    pub(crate) switch_project_root: Option<PathBuf>,
    /// When set, render loop should exit after the current draw.
    pub should_exit: bool,
    /// When set, the caller should launch the integration sub-wizard after the
    /// render loop unwinds. See [`DashboardExit`].
    pub launch_integration: bool,
    /// When set, the caller should launch the explain setup sub-wizard.
    pub launch_explain_setup: bool,
    /// When set, the dashboard loop should run explain in-place after the
    /// current key event is handled.
    pub pending_explain: VecDeque<PendingExplainRun>,
    /// Confirm-stop-watch modal state. `Some` when the operator asked to run
    /// explain while watch was still active; holds the pending mode until
    /// the operator answers yes (stop watch + launch) or no (cancel).
    pub confirm_stop_watch: Option<ConfirmStopWatchState>,
    pending_quick_confirm: Option<PendingQuickConfirm>,
    /// Folder-picker sub-view state. `Some` while the operator is choosing
    /// which top-level directories to scope the next Explain run to; cleared
    /// on Esc, Enter, or any tab switch.
    pub picker: Option<FolderPickerState>,
    /// Cached explain-status preview used by the Explain tab.
    pub explain_preview: Option<ExplainPreviewPanel>,
    /// Currently selected dashboard tab.
    pub active_tab: ActiveTab,
    /// Rows-up-from-bottom for the Live tab. `0` pins the view to the newest
    /// entry; higher values scroll the frame up.
    pub scroll_offset: usize,
    /// Last rendered Live-tab content row count for PageUp/PageDown movement.
    pub live_visible_rows: usize,
    /// When true, new Live-feed entries snap the view back to the bottom.
    pub follow_mode: bool,
    /// Monotonic tick counter for the header spinner animation.
    pub frame: u32,
    /// True between a `ReconcileStarted` and its matching
    /// `ReconcileFinished`/`Error`. Drives whether the header spinner renders.
    pub reconcile_active: bool,
    /// Cached auto-sync flag reflecting the last ack from the watch service.
    /// Seeded from `Config::auto_sync_enabled` at TUI startup; flipped by the
    /// `A` keybinding when the watch service acknowledges the control request.
    /// Does NOT persist to `config.toml`.
    pub auto_sync_enabled: bool,
    /// How long `poll_key` waits for a key event before returning. Set short
    /// so the spinner and snapshot refresh feel live.
    pub poll_timeout: Duration,
    /// Cadence at which file-based status snapshots are rebuilt. Independent
    /// of `poll_timeout` so the spinner can redraw at 10 Hz while the
    /// expensive snapshot refresh stays at 2 s.
    pub snapshot_refresh_interval: Duration,
    /// Last time we rebuilt the snapshot.
    pub(crate) last_refresh: Instant,
    /// How long a cached explain preview stays fresh while the Explain
    /// tab is open before we recompute it.
    pub explain_preview_refresh_interval: Duration,
    /// Transient footer message. Set by `r` so a refresh gives the operator
    /// confirmation even when the snapshot was already current.
    pub(crate) toast: Option<(String, Instant)>,
    /// Live-mode only: receiver streaming `WatchEvent`s from the hosted watch
    /// service. `None` in poll mode so the dashboard falls back to file-based
    /// snapshot refresh.
    pub(crate) events_rx: Option<Receiver<WatchEvent>>,
    /// Process-global explain event stream. Present in both poll and live
    /// modes: if the user triggers explain from within the TUI host (future)
    /// or any other in-process call site fires, the events merge into the log.
    pub(crate) explain_rx: Receiver<ExplainEvent>,
    /// Background-thread supervisor for the auto/manual `bootstrap()` path
    /// that materializes the graph when the dashboard observes
    /// `graph_stats.is_none()`. Lifecycle is owned by the dashboard: the
    /// thread is spawned lazily and reaped during `tick()`.
    pub(crate) materializer: crate::tui::materializer::MaterializerSupervisor,
    /// View-layer mirror of `materializer.state()`. Updated from `tick()`
    /// once per refresh so widgets can render without holding `&mut`.
    pub materialize_state: crate::tui::materializer::MaterializeState,
}

use crate::surface::status_snapshot::StatusSnapshot;
use crate::tui::widgets::{LogEntry, QuickAction};

/// Which explain refresh mode the operator requested from the Explain tab.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplainMode {
    /// Refresh every stale commentary entry (no scope filter).
    AllStale,
    /// Refresh only files hot in recent commit history.
    Changed,
    /// Refresh entries under the given repo-relative path prefixes.
    Paths(Vec<String>),
}

/// Queued in-dashboard explain run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingExplainRun {
    /// Scope of the explain run.
    pub mode: ExplainMode,
    /// `true` when the dashboard stopped watch before queuing the run.
    pub stopped_watch: bool,
}

/// Post-loop intent expressed by the dashboard when it exits. The caller maps
/// this to either "fully done" or "re-enter the dashboard after running a
/// sub-wizard".
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DashboardExit {
    /// Operator quit; caller should tear down and return.
    Quit,
    /// Operator asked for the integration sub-wizard; caller should launch it
    /// and then re-open the dashboard.
    LaunchIntegration,
    /// Operator asked for the explain setup sub-wizard.
    LaunchExplainSetup,
    /// Operator selected another registry project from Repos.
    SwitchProject(PathBuf),
}

#[cfg(test)]
mod tests;
