use std::time::{Duration, Instant};

use crate::pipeline::synthesis::{build_synthesis_preview, SynthesisPreview};

use super::AppState;

/// Cached result for one synthesize-status preview scope.
#[derive(Clone, Debug)]
pub enum SynthesisPreviewState {
    /// Preview loaded successfully.
    Ready(Box<SynthesisPreview>),
    /// Preview could not be built.
    Unavailable(String),
}

/// Cached inline preview panel for the Synthesis tab.
#[derive(Clone, Debug)]
pub struct SynthesisPreviewPanel {
    /// Whole-repo preview, matching the `[r]` action.
    pub whole_repo: SynthesisPreviewState,
    /// Changed-files preview, matching the `[c]` action.
    pub changed: SynthesisPreviewState,
    /// Time the preview cache was last rebuilt.
    pub(crate) refreshed_at: Instant,
}

impl SynthesisPreviewPanel {
    fn fresh_enough(&self, interval: Duration) -> bool {
        self.refreshed_at.elapsed() < interval
    }
}

impl AppState {
    pub(super) fn refresh_synthesis_preview(&mut self, force: bool) {
        if !force
            && self
                .synthesis_preview
                .as_ref()
                .is_some_and(|panel| panel.fresh_enough(self.synthesis_preview_refresh_interval))
        {
            return;
        }

        self.synthesis_preview = Some(SynthesisPreviewPanel {
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
) -> SynthesisPreviewState {
    match build_synthesis_preview(repo_root, paths, changed) {
        Ok(preview) => SynthesisPreviewState::Ready(Box::new(preview)),
        Err(err) => SynthesisPreviewState::Unavailable(err.to_string()),
    }
}
