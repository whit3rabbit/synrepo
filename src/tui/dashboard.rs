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
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use crate::bootstrap::runtime_probe::{probe, AgentIntegration};
use crate::pipeline::watch::WatchEvent;
use crate::surface::readiness::ReadinessMatrix;
use crate::tui::app::{poll_key, ActiveTab, AppState, DashboardExit};
use crate::tui::explain_run::run_explain_in_dashboard;
use crate::tui::materializer::MaterializeState;
use crate::tui::probe::{
    build_activity_vm, build_header_vm, build_health_vm, build_next_actions, build_trust_vm,
    display_repo_path, HealthRow, HealthVm, Severity,
};
use crate::tui::projects::GlobalAppState;
use crate::tui::theme::Theme;
use crate::tui::widgets::{
    ActionsTabWidget, DashboardTabsWidget, ExplainTabWidget, FooterWidget, HeaderWidget,
    HealthWidget, LiveFeedWidget, LogEntry, ProjectPickerWidget, TrustWidget,
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

/// Enter the global registry-backed project dashboard.
pub fn run_global_dashboard(
    cwd: &Path,
    theme: Theme,
    open_picker: bool,
) -> anyhow::Result<DashboardExit> {
    let mut terminal = enter_tui()?;
    let mut state = GlobalAppState::new(cwd, theme, open_picker)?;
    let result = render_global_loop(&mut terminal, &mut state);
    leave_tui(&mut terminal)?;
    result?;
    Ok(DashboardExit::Quit)
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
        if let Some(pending) = state.take_pending_explain() {
            run_explain_in_dashboard(terminal, state, pending)?;
        }
    }
    Ok(())
}

fn render_global_loop(
    terminal: &mut DashboardTerminal,
    state: &mut GlobalAppState,
) -> anyhow::Result<()> {
    while !state.should_exit {
        state.tick();
        terminal.draw(|frame| draw_global_dashboard(frame, state))?;
        let timeout = state
            .active_state()
            .map(|active| active.poll_timeout)
            .unwrap_or(std::time::Duration::from_millis(125));
        if let Some((code, mods)) = poll_key(timeout)? {
            state.handle_key(code, mods);
        }
        if let Some(active) = state.active_state_mut() {
            if let Some(pending) = active.take_pending_explain() {
                run_explain_in_dashboard(terminal, active, pending)?;
            }
        }
    }
    Ok(())
}

fn draw_global_dashboard(frame: &mut ratatui::Frame, state: &mut GlobalAppState) {
    if state.help_visible {
        draw_help(frame, state.theme);
        return;
    }
    if state.command_palette {
        draw_command_palette(frame, state.theme);
        return;
    }
    if state.picker.is_some() || state.active_state().is_none() {
        let picker = ProjectPickerWidget {
            state,
            theme: &state.theme,
        };
        frame.render_widget(picker, frame.area());
        return;
    }
    if let Some(active) = state.active_state_mut() {
        draw_dashboard(frame, active);
    }
}

fn draw_help(frame: &mut ratatui::Frame, theme: Theme) {
    let block = Block::default()
        .title(" help ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    let lines = vec![
        Line::from("[p] projects    [?] help    [:] commands    [q] quit"),
        Line::from("[Tab/1-5] tabs  [r] refresh  [w] watch for active project"),
        Line::from("Project picker: filter, Enter switch, r rename, a add cwd, d detach, w watch"),
    ];
    let paragraph = Paragraph::new(lines).block(block).style(theme.base_style());
    frame.render_widget(paragraph, frame.area());
}

fn draw_command_palette(frame: &mut ratatui::Frame, theme: Theme) {
    let block = Block::default()
        .title(" commands ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    let lines = vec![
        Line::from("project switch"),
        Line::from("project add current directory"),
        Line::from("project detach selected"),
        Line::from("watch start/stop selected project"),
    ];
    let paragraph = Paragraph::new(lines).block(block).style(theme.base_style());
    frame.render_widget(paragraph, frame.area());
}

fn draw_dashboard(frame: &mut ratatui::Frame, state: &mut AppState) {
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
    let repo_path = display_repo_path(&state.repo_root);
    let repo_display = state
        .project_name
        .as_ref()
        .map(|name| format!("{name}  {repo_path}"))
        .unwrap_or(repo_path);
    let header_vm = build_header_vm(
        repo_display,
        &state.snapshot,
        &state.integration,
        Some(state.auto_sync_enabled),
    );
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
            state.live_visible_rows = content_area.height.saturating_sub(2).max(1) as usize;
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
            let mut health_vm = build_health_vm(&state.snapshot);
            override_graph_row_when_materializing(&mut health_vm, &state.materialize_state);
            append_readiness_rows(&mut health_vm, &state.repo_root, &state.snapshot);
            let health = HealthWidget {
                vm: &health_vm,
                theme: &state.theme,
            };
            frame.render_widget(health, content_area);
        }
        ActiveTab::Trust => {
            let trust_vm = build_trust_vm(&state.snapshot);
            let trust = TrustWidget {
                vm: &trust_vm,
                theme: &state.theme,
            };
            frame.render_widget(trust, content_area);
        }
        ActiveTab::Explain => {
            let explain = ExplainTabWidget {
                snapshot: &state.snapshot,
                picker: state.picker.as_ref(),
                confirm_stop_watch: state.confirm_stop_watch.as_ref(),
                preview_panel: state.explain_preview.as_ref(),
                theme: &state.theme,
            };
            frame.render_widget(explain, content_area);
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
        materialize_hint_visible: state.snapshot.graph_stats.is_none()
            && state.snapshot.initialized,
    };
    frame.render_widget(footer, outer[3]);
}

/// Replace the "graph: not materialized" row label while a bootstrap thread
/// is in flight so the operator sees "materializing... (Ns)" with elapsed
/// time, matching the way the spinner reflects the watch reconcile.
fn override_graph_row_when_materializing(vm: &mut HealthVm, state: &MaterializeState) {
    let MaterializeState::Running { started_at } = state else {
        return;
    };
    if let Some(row) = vm.rows.iter_mut().find(|r| r.label == "graph") {
        let elapsed = started_at.elapsed().as_secs();
        row.value = format!("materializing... ({elapsed}s)");
        row.severity = Severity::Stale;
    }
}

/// Append capability-readiness rows to the Health pane so the dashboard shows
/// the same degraded/disabled/stale/blocked states that `synrepo status` and
/// `synrepo doctor` report. Rows are labelled with a `readiness:` prefix so
/// they do not shadow the existing per-subsystem rows.
fn append_readiness_rows(
    vm: &mut HealthVm,
    repo_root: &std::path::Path,
    snapshot: &crate::surface::status_snapshot::StatusSnapshot,
) {
    if !snapshot.initialized {
        return;
    }
    let probe_report = probe(repo_root);
    let cfg = snapshot.config.clone().unwrap_or_default();
    let matrix = ReadinessMatrix::build(repo_root, &probe_report, snapshot, &cfg);
    for row in &matrix.rows {
        vm.rows.push(HealthRow {
            label: format!("readiness:{}", row.capability.as_str()),
            value: format!("{} — {}", row.state.as_str(), row.detail),
            severity: row.state.severity(),
        });
    }
}
