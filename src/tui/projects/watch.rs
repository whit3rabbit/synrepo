use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::tui::actions::{
    outcome_to_project_log, start_watch_daemon, stop_watch, ProjectActionContext,
};

use super::GlobalAppState;

impl GlobalAppState {
    pub(super) fn toggle_selected_project_watch(&mut self) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        let ctx = ProjectActionContext::new(&project.id, &project.name, &project.root);
        let action_ctx = ctx.action_context();
        let outcome = match watch_service_status(&ctx.synrepo_dir) {
            WatchServiceStatus::Inactive => start_watch_daemon(&action_ctx),
            WatchServiceStatus::Running(_)
            | WatchServiceStatus::Starting
            | WatchServiceStatus::Stale(_)
            | WatchServiceStatus::Corrupt(_) => stop_watch(&action_ctx),
        };
        let entry = outcome_to_project_log(&ctx, "watch", &outcome);
        if let Some(active) = self.project_states.get_mut(&project.id) {
            active.set_toast(entry.message.clone());
            active.log.push(entry);
        }
        let _ = self.refresh_projects();
    }
}
