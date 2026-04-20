//! Dashboard layout and render loop. Owns the two-mode (poll vs. live) entry
//! points and composes widgets into a single ratatui frame. The content area
//! is tab-switched between Live, Health, and Actions; the header carries a
//! reconcile spinner so the operator sees liveness feedback on every tab.

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
use crate::tui::app::{poll_key, ActiveTab, AppState, DashboardExit};
use crate::tui::probe::{
    build_activity_vm, build_header_vm, build_health_vm, build_next_actions, display_repo_path,
};
use crate::tui::theme::Theme;
use crate::tui::widgets::{
    ActionsTabWidget, DashboardTabsWidget, FooterWidget, HeaderWidget, HealthWidget,
    LiveFeedWidget, LogEntry, SynthesisTabWidget,
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
    startup_logs: Vec<LogEntry>,
) -> anyhow::Result<DashboardExit> {
    let mut terminal = enter_tui()?;
    let mut state = match events_rx {
        Some(rx) => AppState::new_live(repo_root, theme, integration, rx),
        None => AppState::new_poll_with_logs(repo_root, theme, integration, startup_logs),
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
        // Short key-poll budget so the spinner and follow-mode snapping feel
        // responsive. Snapshot refresh is gated separately on
        // `snapshot_refresh_interval` inside `tick()`.
        if let Some((code, mods)) = poll_key(state.poll_timeout)? {
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
            Constraint::Length(3), // tab bar
            Constraint::Min(0),    // active tab content
            Constraint::Length(1), // footer key hints
        ])
        .split(size);

    // Header with spinner.
    let repo_display = display_repo_path(&state.repo_root);
    let header_vm = build_header_vm(repo_display, &state.snapshot, &state.integration);
    let header = HeaderWidget {
        vm: &header_vm,
        theme: &state.theme,
        frame: state.frame,
        reconcile_active: state.reconcile_active,
    };
    frame.render_widget(header, outer[0]);

    // Tab bar.
    let tabs = DashboardTabsWidget {
        active: state.active_tab,
        theme: &state.theme,
    };
    frame.render_widget(tabs, outer[1]);

    // Active tab content.
    let content_area = outer[2];
    match state.active_tab {
        ActiveTab::Live => {
            let activity_vm = build_activity_vm(&state.snapshot);
            let live = LiveFeedWidget {
                log: state.log.as_slice(),
                activity: &activity_vm,
                scroll_offset: state.scroll_offset,
                follow_mode: state.follow_mode,
                theme: &state.theme,
            };
            frame.render_widget(live, content_area);
        }
        ActiveTab::Health => {
            let health_vm = build_health_vm(&state.snapshot);
            let health = HealthWidget {
                vm: &health_vm,
                theme: &state.theme,
            };
            frame.render_widget(health, content_area);
        }
        ActiveTab::Synthesis => {
            let synthesis = SynthesisTabWidget {
                snapshot: &state.snapshot,
                picker: state.picker.as_ref(),
                theme: &state.theme,
            };
            frame.render_widget(synthesis, content_area);
        }
        ActiveTab::Actions => {
            let next_actions = build_next_actions(&state.snapshot, &state.integration);
            let actions = ActionsTabWidget {
                next_actions: &next_actions,
                quick_actions: &state.quick_actions,
                theme: &state.theme,
            };
            frame.render_widget(actions, content_area);
        }
    }

    // Footer with key hints (or transient toast if one is active).
    let footer = FooterWidget {
        active: state.active_tab,
        follow_mode: state.follow_mode,
        theme: &state.theme,
        toast: state.active_toast(),
        watch_toggle_label: state.watch_toggle_label(),
    };
    frame.render_widget(footer, outer[3]);
}
