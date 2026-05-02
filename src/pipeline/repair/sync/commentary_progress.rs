//! Commentary refresh progress helpers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::pipeline::explain::docs::CommentaryIndexSyncMode;
use crate::pipeline::explain::{CommentarySkip, CommentarySkipReason};

use super::commentary_generate::ItemOutcome;
use super::commentary_plan::{CommentaryProgressEvent, CommentaryWorkItem};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecordedItemOutcome {
    pub(super) generated: bool,
    pub(super) halted: bool,
}

pub(super) fn emit(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    event: CommentaryProgressEvent,
) {
    if let Some(progress) = progress.as_mut() {
        progress(event);
    }
}

pub(super) fn emit_target_started(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    item: &CommentaryWorkItem,
    current: usize,
) {
    emit(
        progress,
        CommentaryProgressEvent::TargetStarted {
            item: item.clone(),
            current,
        },
    );
}

pub(super) fn record_item_outcome(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    item: &CommentaryWorkItem,
    current: usize,
    max_targets: usize,
    outcome: ItemOutcome,
    queued_for_next_run: &mut usize,
    skip_reasons: &mut BTreeMap<String, usize>,
) -> RecordedItemOutcome {
    match outcome {
        ItemOutcome::Generated => {
            emit_target_finished(progress, item, current, true, None, 0, false);
            RecordedItemOutcome {
                generated: true,
                halted: false,
            }
        }
        ItemOutcome::Skipped {
            skip,
            retry_attempts,
            queued_for_next_run: queued,
        } => {
            record_skip_reason(skip_reasons, skip.reason);
            let halted = if queued {
                if *queued_for_next_run == 0 {
                    *queued_for_next_run = max_targets.saturating_sub(current).saturating_add(1);
                }
                true
            } else {
                false
            };
            emit_target_finished(
                progress,
                item,
                current,
                false,
                Some(skip),
                retry_attempts,
                queued,
            );
            RecordedItemOutcome {
                generated: false,
                halted,
            }
        }
    }
}

pub(super) fn skip_reason_summary(skip_reasons: &BTreeMap<String, usize>) -> Vec<(String, usize)> {
    skip_reasons
        .iter()
        .map(|(reason, count)| (reason.clone(), *count))
        .collect()
}

pub(super) fn emit_docs_events(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    docs_root_path: &Path,
    docs_root_existed: bool,
    touched: &[PathBuf],
) {
    if !docs_root_existed && docs_root_path.exists() {
        emit(
            progress,
            CommentaryProgressEvent::DocsDirCreated {
                path: docs_root_path.to_path_buf(),
            },
        );
    }
    let mut touched_sorted = touched.to_vec();
    touched_sorted.sort();
    for path in touched_sorted {
        let event = if path.exists() {
            CommentaryProgressEvent::DocWritten { path }
        } else {
            CommentaryProgressEvent::DocDeleted { path }
        };
        emit(progress, event);
    }
}

pub(super) fn emit_index_events(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    index_dir_path: &Path,
    index_dir_existed: bool,
    mode: CommentaryIndexSyncMode,
    touched_paths: usize,
) {
    if !index_dir_existed && index_dir_path.exists() {
        emit(
            progress,
            CommentaryProgressEvent::IndexDirCreated {
                path: index_dir_path.to_path_buf(),
            },
        );
    }
    match mode {
        CommentaryIndexSyncMode::NoChange => {}
        CommentaryIndexSyncMode::Updated => emit(
            progress,
            CommentaryProgressEvent::IndexUpdated {
                path: index_dir_path.to_path_buf(),
                touched_paths,
            },
        ),
        CommentaryIndexSyncMode::Rebuilt => emit(
            progress,
            CommentaryProgressEvent::IndexRebuilt {
                path: index_dir_path.to_path_buf(),
                touched_paths,
            },
        ),
    }
}

fn record_skip_reason(skip_reasons: &mut BTreeMap<String, usize>, reason: CommentarySkipReason) {
    *skip_reasons.entry(reason.as_str().to_string()).or_insert(0) += 1;
}

fn emit_target_finished(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    item: &CommentaryWorkItem,
    current: usize,
    generated: bool,
    skip: Option<CommentarySkip>,
    retry_attempts: usize,
    queued_for_next_run: bool,
) {
    let (skip_reason, skip_message) = match skip {
        Some(skip) => (Some(skip.reason), Some(skip.display())),
        None => (None, None),
    };
    emit(
        progress,
        CommentaryProgressEvent::TargetFinished {
            item: item.clone(),
            current,
            generated,
            skip_reason,
            skip_message,
            retry_attempts,
            queued_for_next_run,
        },
    );
}
