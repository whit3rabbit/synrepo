use super::super::*;
use crate::bootstrap::runtime_probe::AgentIntegration;
use crate::registry;
use crate::tui::theme::Theme;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn single_project_repos_blocks_missing_open_and_prunes_after_preview() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempfile::tempdir().unwrap();
    let _guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let active = home.path().join("active");
    let missing = home.path().join("missing");
    std::fs::create_dir_all(active.join(".synrepo")).unwrap();
    std::fs::create_dir_all(missing.join(".synrepo")).unwrap();
    let active_entry = registry::record_project(&active).unwrap();
    let missing_entry = registry::record_project(&missing).unwrap();
    std::fs::remove_dir_all(&missing).unwrap();
    let mut state = AppState::new_poll(&active, Theme::plain(), AgentIntegration::Absent);
    state.project_id = Some(active_entry.id);
    state.set_tab(ActiveTab::Repos);
    state.explore_selected = state
        .explore_projects
        .iter()
        .position(|project| project.id == missing_entry.id)
        .unwrap();

    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!state.should_exit);
    assert!(state.handle_key(KeyCode::Char('P'), KeyModifiers::SHIFT));
    assert!(state.repo_manage_prompt.is_some());
    assert!(state.handle_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(registry::resolve_project(&missing_entry.id).is_ok());
    assert!(state.handle_key(KeyCode::Char('P'), KeyModifiers::SHIFT));
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(registry::resolve_project(&missing_entry.id).is_err());
    assert_eq!(state.explore_projects.len(), 1);
}

#[test]
fn relinking_current_project_requests_dashboard_restart_at_new_path() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempfile::tempdir().unwrap();
    let _guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let old = home.path().join("old");
    let moved = home.path().join("moved");
    std::fs::create_dir_all(old.join(".synrepo")).unwrap();
    std::fs::write(old.join(".synrepo/config.toml"), "mode = 'auto'\n").unwrap();
    let entry = registry::record_project(&old).unwrap();
    let mut state = AppState::new_poll(&old, Theme::plain(), AgentIntegration::Absent);
    state.project_id = Some(entry.id.clone());
    std::fs::rename(&old, &moved).unwrap();
    std::fs::create_dir(&old).unwrap();
    state.set_tab(ActiveTab::Repos);

    assert!(state.handle_key(KeyCode::Char('l'), KeyModifiers::NONE));
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        registry::resolve_project(&entry.id).unwrap().path,
        moved.canonicalize().unwrap()
    );
    assert_eq!(
        state.switch_project_root,
        Some(moved.canonicalize().unwrap())
    );
    assert!(state.should_exit);
}
