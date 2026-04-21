//! Coalesced watch-change tracking between the debouncer callback and the loop.

use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct PendingWatchChanges {
    event_count: usize,
    touched_paths: BTreeSet<PathBuf>,
}

pub(super) struct PendingWatchBatch {
    pub event_count: usize,
    pub touched_paths: Vec<PathBuf>,
}

impl PendingWatchChanges {
    pub(super) fn record(
        &mut self,
        event_count: usize,
        touched_paths: Vec<PathBuf>,
        max_paths: usize,
    ) {
        self.event_count = self.event_count.saturating_add(event_count);
        for path in touched_paths {
            if self.touched_paths.len() >= max_paths {
                break;
            }
            self.touched_paths.insert(path);
        }
    }

    pub(super) fn take(&mut self, max_events: usize) -> PendingWatchBatch {
        let event_count = self.event_count.min(max_events);
        self.event_count = 0;
        PendingWatchBatch {
            event_count,
            touched_paths: self.touched_paths.iter().cloned().collect(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.event_count == 0
    }

    pub(super) fn clear_paths(&mut self) {
        self.touched_paths.clear();
    }
}
