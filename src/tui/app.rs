//! App shell: event loop, key handling, and the state machine that picks
//! between dashboard (poll / live) mode and the various wizards. Wizard
//! rendering still lives in `wizard.rs`; this module only owns the loop.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::surface::status_snapshot::{build_status_snapshot, StatusOptions, StatusSnapshot};
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
    /// Refresh interval for poll mode.
    pub poll_interval: Duration,
    /// Last time we rebuilt the snapshot.
    last_refresh: Instant,
}

impl AppState {
    /// Build a new poll-mode app state for `repo_root`.
    pub fn new_poll(repo_root: &Path, theme: Theme, integration: AgentIntegration) -> Self {
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
            mode: AppMode::DashboardPoll,
            integration,
            snapshot,
            log: EventLog::default(),
            quick_actions: default_poll_actions(),
            should_exit: false,
            poll_interval: Duration::from_secs(2),
            last_refresh: Instant::now(),
        }
    }

    /// Refresh the snapshot if the poll interval has elapsed.
    pub fn tick(&mut self) {
        if self.last_refresh.elapsed() >= self.poll_interval {
            self.refresh_now();
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
