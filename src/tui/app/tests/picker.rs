use super::super::*;
use super::support::make_ready_poll_state;
use crossterm::event::{KeyCode, KeyModifiers};

fn prime_picker(state: &mut AppState) {
    state.picker = Some(FolderPickerState {
        folders: vec![
            FolderEntry {
                path: "src/".to_string(),
                indexable_count: 2,
                supported_count: 2,
                checked: true,
            },
            FolderEntry {
                path: "docs/".to_string(),
                indexable_count: 1,
                supported_count: 0,
                checked: false,
            },
        ],
        cursor: 0,
    });
}

#[test]
fn picker_esc_clears_without_exit() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    let consumed = state.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert!(consumed);
    assert!(state.picker.is_none());
    assert!(!state.should_exit, "Esc in picker must not exit the loop");
    assert!(state.pending_explain.is_empty());
}

#[test]
fn picker_enter_with_selection_queues_paths_mode() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    let consumed = state.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(consumed);
    assert!(!state.should_exit);
    match state.pending_explain.front() {
        Some(PendingExplainRun {
            mode: ExplainMode::Paths(paths),
            stopped_watch: false,
        }) => {
            assert_eq!(paths, &vec!["src/".to_string()]);
        }
        other => panic!("expected Paths(..), got {other:?}"),
    }
    assert!(
        state.picker.is_none(),
        "picker cleared on successful commit"
    );
}

#[test]
fn picker_enter_with_empty_selection_stays_open() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    // Uncheck the only checked entry.
    state.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    let consumed = state.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(consumed);
    assert!(
        state.picker.is_some(),
        "empty-selection Enter must keep picker open"
    );
    assert!(!state.should_exit);
    assert!(state.pending_explain.is_empty());
}

#[test]
fn picker_navigation_and_toggle_move_cursor() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    // Start at row 0. Down should move to row 1.
    state.handle_key(KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(state.picker.as_ref().unwrap().cursor, 1);
    // Space toggles docs/.
    state.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    let entry = &state.picker.as_ref().unwrap().folders[1];
    assert_eq!(entry.path, "docs/");
    assert!(entry.checked);
    // `k` moves back up.
    state.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
    assert_eq!(state.picker.as_ref().unwrap().cursor, 0);
}

#[test]
fn picker_tab_switch_clears_it() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    state.handle_key(KeyCode::Char('1'), KeyModifiers::NONE);
    assert_eq!(state.active_tab, ActiveTab::Live);
    assert!(state.picker.is_none());
}

#[test]
fn picker_quit_key_falls_through() {
    let (_repo, mut state) = make_ready_poll_state();
    state.set_tab(ActiveTab::Explain);
    prime_picker(&mut state);
    let consumed = state.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(consumed);
    assert!(state.should_exit, "q must quit even with picker open");
}
