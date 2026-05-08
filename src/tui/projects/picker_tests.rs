use crossterm::event::{KeyCode, KeyModifiers};
use tempfile::tempdir;

use super::{home_guard, make_partial_project};
use crate::registry;
use crate::tui::projects::{GlobalAppState, ProjectPickerState};
use crate::tui::theme::Theme;

#[test]
fn picker_filter_change_clamps_selection() {
    let (_lock, home, _guard) = home_guard();
    let first = home.path().join("first");
    let second = home.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    make_partial_project(&first);
    make_partial_project(&second);
    let first_entry = registry::record_project(&first).unwrap();
    let second_entry = registry::record_project(&second).unwrap();
    registry::rename_project(&first_entry.id, "onyx").unwrap();
    registry::rename_project(&second_entry.id, "quartz").unwrap();
    let mut state = GlobalAppState::new(home.path(), Theme::plain(), true).unwrap();
    state.picker.as_mut().unwrap().selected = 99;

    for ch in "onyx".chars() {
        assert!(state.handle_key(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    assert_eq!(state.picker.as_ref().unwrap().selected, 0);
    assert_eq!(state.filtered_projects().len(), 1);
    assert_eq!(state.selected_project().unwrap().id, first_entry.id);
}

#[test]
fn enter_on_empty_filter_does_nothing_with_toast() {
    let (_lock, home, _guard) = home_guard();
    let project = home.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    make_partial_project(&project);
    let entry = registry::record_project(&project).unwrap();
    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();
    state.picker = Some(ProjectPickerState::default());

    state.picker.as_mut().unwrap().filter = "__definitely_no_match__".to_string();
    state.clamp_picker_selection();
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(state.active_project_id.as_deref(), Some(entry.id.as_str()));
    assert!(state
        .active_state()
        .unwrap()
        .active_toast()
        .unwrap_or_default()
        .contains("no matching project"));
}

#[test]
fn picker_detach_requires_confirmation() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
    make_partial_project(project.path());
    let entry = registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), true).unwrap();

    assert!(state.handle_key(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(registry::resolve_project(&entry.id).is_ok());
    assert_eq!(
        state.picker.as_ref().unwrap().detach_confirm.as_deref(),
        Some(entry.id.as_str())
    );

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(registry::resolve_project(&entry.id).is_err());
}

#[test]
fn rename_empty_alias_is_rejected() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
    make_partial_project(project.path());
    let entry = registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), false).unwrap();
    state.picker = Some(ProjectPickerState {
        rename_input: Some("   ".to_string()),
        ..ProjectPickerState::default()
    });

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(registry::resolve_project(&entry.id).unwrap().name.is_none());
    assert!(state
        .active_state()
        .unwrap()
        .active_toast()
        .unwrap_or_default()
        .contains("cannot be empty"));
}

#[test]
fn global_i_launches_integration_for_active_project() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
    make_partial_project(project.path());
    registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), false).unwrap();

    assert!(state.handle_key(KeyCode::Char('i'), KeyModifiers::NONE));

    let active = state.active_state().unwrap();
    assert!(active.launch_integration);
    assert!(state.should_exit);
}

#[test]
fn command_palette_filters_and_launches_active_action() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
    make_partial_project(project.path());
    registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), false).unwrap();

    assert!(state.handle_key(KeyCode::Char(':'), KeyModifiers::NONE));
    for ch in "integration".chars() {
        assert!(state.handle_key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let labels = state
        .filtered_command_palette_items()
        .iter()
        .map(|item| item.label.clone())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["agent integration".to_string()]);
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(state.active_state().unwrap().launch_integration);
    assert!(state.should_exit);
}

#[test]
fn command_palette_keeps_hidden_graph_action_disabled_when_graph_present() {
    let (_lock, _home, _guard) = home_guard();
    let project = tempdir().unwrap();
    std::fs::write(project.path().join("README.md"), "ready\n").unwrap();
    crate::bootstrap::bootstrap(project.path(), None, false).unwrap();
    registry::record_project(project.path()).unwrap();
    let mut state = GlobalAppState::new(project.path(), Theme::plain(), false).unwrap();

    assert!(state.handle_key(KeyCode::Char(':'), KeyModifiers::NONE));
    for ch in "materialize".chars() {
        assert!(state.handle_key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let item = state
        .filtered_command_palette_items()
        .into_iter()
        .find(|item| item.label.contains("generate"))
        .expect("generate graph command remains discoverable");
    assert_eq!(item.prefix(), "x");
    assert_eq!(
        item.disabled_reason.as_deref(),
        Some("graph already exists")
    );
}
