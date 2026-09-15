//! Single-flight background loading for repository-wide suggestions.

#[cfg(not(test))]
use std::thread;

#[cfg(not(test))]
use crossbeam_channel::bounded;
use crossbeam_channel::TryRecvError;

use super::AppState;
use crate::surface::refactor_suggestions::{
    collect_refactor_suggestions_for_repo, RefactorSuggestionMode, RefactorSuggestionOptions,
    RefactorSuggestionReport,
};

pub(super) struct SuggestionLoadResult {
    mode: RefactorSuggestionMode,
    result: Result<RefactorSuggestionReport, String>,
}

impl AppState {
    /// Load suggestion rows only when the tab needs them.
    pub(crate) fn ensure_suggestions_loaded(&mut self) {
        if self.suggestion_report.is_none() {
            self.start_suggestion_load(false);
        }
    }

    /// Refresh suggestions without blocking keyboard input or rendering.
    pub(crate) fn refresh_suggestions(&mut self) {
        self.start_suggestion_load(true);
    }

    /// Switch modes. If another scan is active its result is discarded, then
    /// the newly selected mode starts without overlapping database readers.
    pub(crate) fn toggle_suggestion_mode(&mut self) {
        self.suggestion_mode = self.suggestion_mode.toggled();
        self.suggestion_report = None;
        self.start_suggestion_load(true);
    }

    fn start_suggestion_load(&mut self, toast: bool) {
        self.suggestion_toast_pending |= toast;
        if self.suggestion_rx.is_some() {
            self.suggestion_reload_pending |= toast;
            return;
        }
        let repo_root = self.repo_root.clone();
        let mode = self.suggestion_mode;

        #[cfg(test)]
        {
            let result = load_suggestions(&repo_root, mode);
            self.apply_suggestion_result(SuggestionLoadResult { mode, result });
        }

        #[cfg(not(test))]
        {
            let (tx, rx) = bounded(1);
            thread::spawn(move || {
                let result = load_suggestions(&repo_root, mode);
                let _ = tx.try_send(SuggestionLoadResult { mode, result });
            });
            self.suggestion_rx = Some(rx);
            if toast {
                self.set_toast("refreshing suggestions in background");
            }
        }
    }

    pub(super) fn drain_suggestion_load(&mut self) {
        let Some(rx) = self.suggestion_rx.as_ref() else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.suggestion_rx = None;
                self.suggestion_toast_pending = false;
                self.set_toast("suggestion loader stopped unexpectedly");
                return;
            }
        };
        self.suggestion_rx = None;
        self.apply_suggestion_result(result);
    }

    fn apply_suggestion_result(&mut self, loaded: SuggestionLoadResult) {
        if loaded.mode != self.suggestion_mode || self.suggestion_reload_pending {
            self.suggestion_reload_pending = false;
            self.suggestion_report = None;
            self.start_suggestion_load(self.suggestion_toast_pending);
            return;
        }
        match loaded.result {
            Ok(report) => {
                let count = report.candidate_count;
                self.suggestion_report = Some(report);
                if self.suggestion_toast_pending {
                    self.set_toast(format!(
                        "suggestions refreshed: {}: {count} candidates",
                        loaded.mode.label()
                    ));
                }
            }
            Err(error) => {
                self.suggestion_report = None;
                self.set_toast(format!("suggestions unavailable: {error}"));
            }
        }
        self.suggestion_toast_pending = false;
    }

    pub(super) fn invalidate_suggestions(&mut self) {
        self.suggestion_report = None;
        if self.suggestion_rx.is_some() {
            self.suggestion_reload_pending = true;
        }
    }
}

fn load_suggestions(
    repo_root: &std::path::Path,
    mode: RefactorSuggestionMode,
) -> Result<RefactorSuggestionReport, String> {
    collect_refactor_suggestions_for_repo(
        repo_root,
        RefactorSuggestionOptions {
            mode,
            ..RefactorSuggestionOptions::default()
        },
    )
    .map_err(|error| error.to_string())
}
