use super::{persist_watch_state_at, types::WatchStateHandle};
use crate::pipeline::writer::now_rfc3339;

impl WatchStateHandle {
    pub fn note_embedding_stale(&self, stale: bool) {
        let snapshot = {
            let mut state = self.state.lock();
            state.embedding_index_stale = stale;
            if !stale {
                state.embedding_next_retry_at = None;
            }
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_embedding_started(&self, trigger: &str) {
        let snapshot = {
            let mut state = self.state.lock();
            state.embedding_running = true;
            state.embedding_last_started_at = Some(now_rfc3339());
            state.embedding_last_outcome = Some(format!("{trigger}:running"));
            state.embedding_last_error = None;
            state.embedding_progress_phase = Some("starting".to_string());
            state.embedding_progress_current = None;
            state.embedding_progress_total = None;
            state.embedding_next_retry_at = None;
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_embedding_progress(
        &self,
        phase: impl Into<String>,
        current: Option<usize>,
        total: Option<usize>,
    ) {
        let snapshot = {
            let mut state = self.state.lock();
            state.embedding_progress_phase = Some(phase.into());
            state.embedding_progress_current = current;
            state.embedding_progress_total = total;
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_embedding_finished(&self, outcome: impl Into<String>) {
        let snapshot = {
            let mut state = self.state.lock();
            state.embedding_running = false;
            state.embedding_index_stale = false;
            state.embedding_last_finished_at = Some(now_rfc3339());
            state.embedding_last_outcome = Some(outcome.into());
            state.embedding_last_error = None;
            state.embedding_progress_phase = None;
            state.embedding_progress_current = None;
            state.embedding_progress_total = None;
            state.embedding_next_retry_at = None;
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_embedding_error(&self, message: impl Into<String>) {
        let message = message.into();
        let snapshot = {
            let mut state = self.state.lock();
            state.embedding_running = false;
            state.embedding_last_finished_at = Some(now_rfc3339());
            state.embedding_last_outcome = Some("error".to_string());
            state.embedding_last_error = Some(message);
            state.embedding_progress_phase = None;
            state.embedding_progress_current = None;
            state.embedding_progress_total = None;
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_embedding_retry_after(&self, retry_at: String) {
        let snapshot = {
            let mut state = self.state.lock();
            state.embedding_next_retry_at = Some(retry_at);
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }
}
