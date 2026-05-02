//! Rendering helpers for global-only dashboard tabs.

use ratatui::layout::{Constraint, Direction, Layout};

use crate::tui::app::{repo_display, ActiveTab};
use crate::tui::probe::build_header_vm;
use crate::tui::projects::GlobalAppState;
use crate::tui::widgets::{DashboardTabsWidget, ExploreTabWidget, FooterWidget, HeaderWidget};

/// Render the global Repos tab using the active project's dashboard chrome.
pub(crate) fn draw_global_explore_dashboard(
    frame: &mut ratatui::Frame,
    state: &mut GlobalAppState,
) {
    let Some(active) = state.active_state() else {
        return;
    };
    let size = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(size);

    let header_vm = build_header_vm(
        repo_display(&active.repo_root, active.project_name.as_deref()),
        &active.snapshot,
        &active.integration,
        Some(active.auto_sync_enabled),
    );
    let header = HeaderWidget {
        vm: &header_vm,
        theme: &active.theme,
        frame: active.frame,
        reconcile_active: active.reconcile_active,
    };
    frame.render_widget(header, outer[0]);
    frame.render_widget(
        DashboardTabsWidget {
            active: ActiveTab::Repos,
            theme: &active.theme,
        },
        outer[1],
    );
    frame.render_widget(
        ExploreTabWidget {
            projects: &state.projects,
            selected: state.explore_selected_index(),
            active_project_id: state.active_project_id.as_deref(),
            active_root: state
                .active_state()
                .map(|active| active.repo_root.as_path()),
            theme: &active.theme,
        },
        outer[2],
    );
    frame.render_widget(
        FooterWidget {
            active: ActiveTab::Repos,
            follow_mode: active.follow_mode,
            theme: &active.theme,
            toast: active.active_toast(),
            watch_toggle_label: active.watch_toggle_label(),
            materialize_hint_visible: active.snapshot.graph_stats.is_none()
                && active.snapshot.initialized,
        },
        outer[3],
    );
}
