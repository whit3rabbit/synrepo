#[cfg(not(test))]
use std::thread;

#[cfg(not(test))]
use crossbeam_channel::bounded;
use crossbeam_channel::TryRecvError;

use crate::pipeline::explain::{build_explain_preview, ExplainPreview};

use super::AppState;

/// Cached result for one explain-status preview scope.
#[derive(Clone, Debug)]
pub enum ExplainPreviewState {
    /// Preview loaded successfully.
    Ready(Box<ExplainPreview>),
    /// Preview could not be built.
    Unavailable(String),
}

/// Cached inline preview panel for the Explain tab.
#[derive(Clone, Debug)]
pub struct ExplainPreviewPanel {
    /// Whole-repo preview, matching the `[r]` action.
    pub whole_repo: ExplainPreviewState,
    /// Changed-files preview, matching the `[c]` action.
    pub changed: ExplainPreviewState,
}

impl AppState {
    pub(super) fn refresh_explain_preview(&mut self, force: bool) {
        if !force && self.explain_preview.is_some() {
            return;
        }

        self.explain_preview_toast_pending |= force;
        if self.explain_preview_rx.is_some() {
            self.explain_preview_refresh_pending |= force;
            if force {
                self.set_toast("refreshing explain status in background");
            }
            return;
        }

        let repo_root = self.repo_root.clone();
        #[cfg(test)]
        self.apply_explain_preview(load_preview_panel(&repo_root));

        #[cfg(not(test))]
        {
            let (tx, rx) = bounded(1);
            thread::spawn(move || {
                let _ = tx.try_send(load_preview_panel(&repo_root));
            });
            self.explain_preview_rx = Some(rx);
            if force {
                self.set_toast("refreshing explain status in background");
            }
        }
    }

    pub(super) fn drain_explain_preview(&mut self) {
        let Some(rx) = self.explain_preview_rx.as_ref() else {
            return;
        };
        let panel = match rx.try_recv() {
            Ok(panel) => panel,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.explain_preview_rx = None;
                self.explain_preview_refresh_pending = false;
                if self.explain_preview_toast_pending {
                    self.explain_preview_toast_pending = false;
                    self.set_toast("explain status loader stopped unexpectedly");
                }
                return;
            }
        };
        self.explain_preview_rx = None;
        self.apply_explain_preview(panel);
    }

    fn apply_explain_preview(&mut self, panel: ExplainPreviewPanel) {
        self.explain_preview = Some(panel);
        if self.explain_preview_refresh_pending {
            self.explain_preview_refresh_pending = false;
            self.explain_preview = None;
            self.refresh_explain_preview(self.explain_preview_toast_pending);
            return;
        }
        if self.explain_preview_toast_pending {
            self.explain_preview_toast_pending = false;
            self.set_toast("explain status refreshed");
        }
    }

    pub(in crate::tui) fn invalidate_explain_preview(&mut self) {
        self.explain_preview = None;
        if self.explain_preview_rx.is_some() {
            self.explain_preview_refresh_pending = true;
        }
    }
}

fn load_preview_panel(repo_root: &std::path::Path) -> ExplainPreviewPanel {
    ExplainPreviewPanel {
        whole_repo: load_preview(repo_root, Vec::new(), false),
        changed: load_preview(repo_root, Vec::new(), true),
    }
}

fn load_preview(
    repo_root: &std::path::Path,
    paths: Vec<String>,
    changed: bool,
) -> ExplainPreviewState {
    match build_explain_preview(repo_root, paths, changed) {
        Ok(preview) => ExplainPreviewState::Ready(Box::new(preview)),
        Err(err) => ExplainPreviewState::Unavailable(err.to_string()),
    }
}
