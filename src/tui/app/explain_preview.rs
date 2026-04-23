use std::time::{Duration, Instant};

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
    /// Time the preview cache was last rebuilt.
    pub(crate) refreshed_at: Instant,
}

impl ExplainPreviewPanel {
    fn fresh_enough(&self, interval: Duration) -> bool {
        self.refreshed_at.elapsed() < interval
    }
}

impl AppState {
    pub(super) fn refresh_explain_preview(&mut self, force: bool) {
        if !force
            && self
                .explain_preview
                .as_ref()
                .is_some_and(|panel| panel.fresh_enough(self.explain_preview_refresh_interval))
        {
            return;
        }

        self.explain_preview = Some(ExplainPreviewPanel {
            whole_repo: load_preview(&self.repo_root, Vec::new(), false),
            changed: load_preview(&self.repo_root, Vec::new(), true),
            refreshed_at: Instant::now(),
        });
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
