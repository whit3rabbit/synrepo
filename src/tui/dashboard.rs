//! Dashboard layout and render loop. Owns the two-mode (poll vs. live) entry
//! points and composes widgets into a single ratatui frame.

use std::io::{self, Stdout};
use std::path::Path;

use crossbeam_channel::Receiver;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;

use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::pipeline::watch::WatchEvent;
use crate::tui::app::{poll_key, AppState, DashboardExit};
use crate::tui::probe::{
    build_activity_vm, build_header_vm, build_health_vm, build_next_actions, display_repo_path,
};
use crate::tui::theme::Theme;
use crate::tui::widgets::{
    ActivityWidget, HeaderWidget, HealthWidget, LogWidget, NextActionsWidget, QuickActionsWidget,
};

/// Terminal alias used by the render loop.
pub type DashboardTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Enter the alternate screen + raw mode and run the dashboard until the user
/// quits. Always restores the terminal on the way out, even when rendering
/// errors bubble up.
///
/// When `events_rx` is `Some`, the dashboard runs in live mode: the log pane
/// drains `WatchEvent`s from the receiver each tick instead of relying on
/// state-file polling. When `None`, poll mode is used.
pub fn run_poll_dashboard(
    repo_root: &Path,
    integration: AgentIntegration,
    theme: Theme,
    welcome_banner: bool,
    events_rx: Option<Receiver<WatchEvent>>,
) -> anyhow::Result<DashboardExit> {
    let mut terminal = enter_tui()?;
    let mut state = match events_rx {
        Some(rx) => AppState::new_live(repo_root, theme, integration, rx),
        None => AppState::new_poll(repo_root, theme, integration),
    };
    if welcome_banner {
        state.push_welcome_banner();
    }
    let result = render_loop(&mut terminal, &mut state);
    leave_tui(&mut terminal)?;
    result?;
    Ok(state.exit_intent())
}

/// Set up crossterm: raw mode, alt screen, hide cursor.
fn enter_tui() -> anyhow::Result<DashboardTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

/// Tear down crossterm state. Safe to call multiple times.
fn leave_tui(terminal: &mut DashboardTerminal) -> anyhow::Result<()> {
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    Ok(())
}

fn render_loop(terminal: &mut DashboardTerminal, state: &mut AppState) -> anyhow::Result<()> {
    while !state.should_exit {
        state.tick();
        terminal.draw(|frame| draw_dashboard(frame, state))?;
        // Budget at most poll_interval between redraws so the snapshot gets a
        // chance to refresh even when the user isn't pressing keys.
        if let Some((code, mods)) = poll_key(state.poll_interval)? {
            state.handle_key(code, mods);
        }
    }
    Ok(())
}

fn draw_dashboard(frame: &mut ratatui::Frame, state: &AppState) {
    let size = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // header
            Constraint::Min(8),    // middle rows
            Constraint::Length(8), // log
        ])
        .split(size);

    // Header.
    let repo_display = display_repo_path(&state.repo_root);
    let header_vm = build_header_vm(repo_display, &state.snapshot, &state.integration);
    let header = HeaderWidget {
        vm: &header_vm,
        theme: &state.theme,
    };
    frame.render_widget(header, outer[0]);

    // Middle: health + activity on the left, next-actions + quick-actions on the right.
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(outer[1]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(middle[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(middle[1]);

    let health_vm = build_health_vm(&state.snapshot);
    let health = HealthWidget {
        vm: &health_vm,
        theme: &state.theme,
    };
    frame.render_widget(health, left[0]);

    let activity_vm = build_activity_vm(&state.snapshot);
    let activity = ActivityWidget {
        vm: &activity_vm,
        theme: &state.theme,
    };
    frame.render_widget(activity, left[1]);

    let actions = build_next_actions(&state.snapshot, &state.integration);
    let next_actions = NextActionsWidget {
        actions: &actions,
        theme: &state.theme,
    };
    frame.render_widget(next_actions, right[0]);

    let quick = QuickActionsWidget {
        actions: &state.quick_actions,
        theme: &state.theme,
    };
    frame.render_widget(quick, right[1]);

    // Footer log pane.
    let entries = state.log.as_slice();
    let log = LogWidget {
        entries: &entries,
        theme: &state.theme,
    };
    frame.render_widget(log, outer[2]);
}
