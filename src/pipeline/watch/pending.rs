//! Coalesced watch-change tracking between the debouncer callback and the loop.

use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct PendingWatchChanges {
    event_count: usize,
    touched_paths: BTreeSet<PathBuf>,
    overflowed_paths: bool,
}

pub(super) struct PendingWatchBatch {
    pub event_count: usize,
    pub touched_paths: Vec<PathBuf>,
    pub force_full_reconcile: bool,
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
            if self.touched_paths.contains(&path) {
                continue;
            }
            if self.touched_paths.len() >= max_paths {
                self.overflowed_paths = true;
                break;
            }
            self.touched_paths.insert(path);
        }
    }

    pub(super) fn record_full(&mut self, event_count: usize) {
        self.event_count = self.event_count.saturating_add(event_count);
        self.overflowed_paths = true;
    }

    pub(super) fn take(&mut self, max_events: usize) -> PendingWatchBatch {
        let event_count = self.event_count.min(max_events);
        let force_full_reconcile = self.overflowed_paths;
        self.event_count = 0;
        self.overflowed_paths = false;
        PendingWatchBatch {
            event_count,
            touched_paths: self.touched_paths.iter().cloned().collect(),
            force_full_reconcile,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.event_count == 0
    }

    pub(super) fn clear_paths(&mut self) {
        self.touched_paths.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_cap_overflow_forces_full_reconcile() {
        let mut pending = PendingWatchChanges::default();
        pending.record(3, vec!["a.rs".into(), "b.rs".into(), "c.rs".into()], 2);

        let batch = pending.take(2);

        assert_eq!(batch.event_count, 2);
        assert_eq!(batch.touched_paths.len(), 2);
        assert!(batch.force_full_reconcile);
    }

    #[test]
    fn full_reconcile_marker_forces_full_reconcile() {
        let mut pending = PendingWatchChanges::default();
        pending.record_full(1);

        let batch = pending.take(10);

        assert_eq!(batch.event_count, 1);
        assert!(batch.touched_paths.is_empty());
        assert!(batch.force_full_reconcile);
    }
}
