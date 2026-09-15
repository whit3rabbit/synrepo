//! Confirm-stop-watch modal handoff with the background action worker: the
//! stop runs single-flight, the deferred explain launch fires on success, and
//! a failed stop hands the modal back to the operator.

use super::super::background_actions::{BackgroundActionKind, BackgroundActionResult};
use super::super::*;
use super::support::make_ready_poll_state;
use crate::tui::actions::ActionOutcome;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn confirm_modal_y_while_action_running_keeps_modal_and_toasts() {
    let (_repo, mut state) = make_ready_poll_state();
    state.confirm_stop_watch = Some(ConfirmStopWatchState {
        pending: PendingStopWatchAction::Explain(ExplainMode::AllStale),
    });
    let (_tx, rx) = crossbeam_channel::bounded::<BackgroundActionResult>(1);
    state.background_action_rx = Some(rx);

    let consumed = state.handle_key(KeyCode::Char('y'), KeyModifiers::NONE);

    assert!(consumed);
    assert!(
        state.confirm_stop_watch.is_some(),
        "busy worker must keep the modal open"
    );
    assert!(state.pending_explain.is_empty());
    assert!(state.pending_after_watch_stop.is_none());
    let toast = state.active_toast().expect("busy guard should set a toast");
    assert!(toast.contains("another dashboard action"), "got {toast:?}");
}

#[test]
fn watch_stop_completion_enqueues_deferred_explain() {
    let (_repo, mut state) = make_ready_poll_state();
    state.pending_after_watch_stop = Some(PendingStopWatchAction::Explain(ExplainMode::Changed));

    state.finish_background_action(BackgroundActionResult::for_test(
        BackgroundActionKind::WatchToggle { stop: true },
        ActionOutcome::Completed {
            message: "no active watch service".to_string(),
        },
    ));

    assert!(matches!(
        state.pending_explain.front(),
        Some(PendingExplainRun {
            mode: ExplainMode::Changed,
            stopped_watch: true,
        })
    ));
    assert!(state.confirm_stop_watch.is_none());
    assert!(state.pending_after_watch_stop.is_none());
}

#[test]
fn watch_stop_failure_restores_confirm_modal() {
    let (_repo, mut state) = make_ready_poll_state();
    state.pending_after_watch_stop = Some(PendingStopWatchAction::Explain(ExplainMode::AllStale));

    state.finish_background_action(BackgroundActionResult::for_test(
        BackgroundActionKind::WatchToggle { stop: true },
        ActionOutcome::Error {
            message: "watch state did not settle within 30000 ms".to_string(),
        },
    ));

    assert!(
        state.pending_explain.is_empty(),
        "failed stop must not queue explain"
    );
    assert_eq!(
        state.confirm_stop_watch,
        Some(ConfirmStopWatchState {
            pending: PendingStopWatchAction::Explain(ExplainMode::AllStale),
        })
    );
    let toast = state.active_toast().expect("failure should set a toast");
    assert!(toast.contains("watch stop failed"), "got {toast:?}");
}
