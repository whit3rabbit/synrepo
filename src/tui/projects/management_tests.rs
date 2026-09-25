use super::*;

#[test]
fn all_missing_projects_open_picker_and_bulk_prune_requires_confirmation() {
    let (_lock, home, _guard) = home_guard();
    let missing = home.path().join("moved-away");
    make_partial_project(&missing);
    registry::record_project(&missing).unwrap();
    std::fs::remove_dir_all(&missing).unwrap();

    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();
    assert!(state.picker.is_some());
    assert!(state.handle_key(KeyCode::Char('P'), KeyModifiers::SHIFT));
    assert!(matches!(
        state.manage_prompt,
        Some(RepoManagementPrompt::Prune { count: 1, .. })
    ));
    assert!(state.handle_key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(registry::load().unwrap().projects.len(), 1);
    assert!(state.handle_key(KeyCode::Char('P'), KeyModifiers::SHIFT));
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(registry::load().unwrap().projects.is_empty());
    assert!(state.projects.is_empty());
}

#[test]
fn repos_tab_detaches_selected_missing_project_only_after_confirmation() {
    let (_lock, home, _guard) = home_guard();
    let active = home.path().join("active");
    let missing = home.path().join("missing");
    make_partial_project(&active);
    make_partial_project(&missing);
    let active_entry = registry::record_project(&active).unwrap();
    let missing_entry = registry::record_project(&missing).unwrap();
    std::fs::remove_dir_all(&missing).unwrap();

    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();
    assert_eq!(
        state.active_project_id.as_deref(),
        Some(active_entry.id.as_str())
    );
    state.open_explore_tab();
    state.explore_selected = state
        .projects
        .iter()
        .position(|p| p.id == missing_entry.id)
        .unwrap();
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        state.active_project_id.as_deref(),
        Some(active_entry.id.as_str())
    );
    assert!(state.picker_message.as_deref().unwrap().contains("missing"));
    assert!(state.handle_key(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(state.handle_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(registry::resolve_project(&missing_entry.id).is_ok());
    assert!(state.handle_key(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(registry::resolve_project(&missing_entry.id).is_err());
    assert!(registry::resolve_project(&active_entry.id).is_ok());
}

#[test]
fn repos_tab_relinks_when_old_directory_was_recreated() {
    let (_lock, home, _guard) = home_guard();
    let active = home.path().join("active");
    let old = home.path().join("old");
    let moved = home.path().join("moved");
    make_partial_project(&active);
    make_partial_project(&old);
    std::fs::write(old.join(".synrepo/config.toml"), "mode = 'auto'\n").unwrap();
    registry::record_project(&active).unwrap();
    let entry = registry::record_project(&old).unwrap();
    std::fs::rename(&old, &moved).unwrap();
    std::fs::create_dir(&old).unwrap();

    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();
    state.open_explore_tab();
    state.explore_selected = state
        .projects
        .iter()
        .position(|p| p.id == entry.id)
        .unwrap();
    assert!(state.handle_key(KeyCode::Char('l'), KeyModifiers::NONE));
    assert!(matches!(
        state.manage_prompt,
        Some(RepoManagementPrompt::Relink { .. })
    ));
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        registry::resolve_project(&entry.id).unwrap().path,
        moved.canonicalize().unwrap()
    );
    assert!(old.exists());
}

#[test]
fn detaching_active_project_returns_to_picker_without_deleting_files() {
    let (_lock, home, _guard) = home_guard();
    let active = home.path().join("active");
    make_partial_project(&active);
    let entry = registry::record_project(&active).unwrap();
    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();
    state.open_explore_tab();

    assert!(state.handle_key(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(state.active_project_id.is_none());
    assert!(state.project_states.is_empty());
    assert!(state.picker.is_some());
    assert!(registry::resolve_project(&entry.id).is_err());
    assert!(active.join(".synrepo").exists());
}

#[test]
fn rename_in_repos_updates_alias_without_restarting_active_project() {
    let (_lock, home, _guard) = home_guard();
    let active = home.path().join("active");
    make_partial_project(&active);
    let entry = registry::record_project(&active).unwrap();
    let mut state = GlobalAppState::new(home.path(), Theme::plain(), false).unwrap();
    state.open_explore_tab();

    assert!(state.handle_key(KeyCode::Char('n'), KeyModifiers::NONE));
    for _ in 0.."active".len() {
        assert!(state.handle_key(KeyCode::Backspace, KeyModifiers::NONE));
    }
    for ch in "renamed".chars() {
        assert!(state.handle_key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert!(state.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        registry::resolve_project(&entry.id)
            .unwrap()
            .name
            .as_deref(),
        Some("renamed")
    );
    assert_eq!(state.active_project_id.as_deref(), Some(entry.id.as_str()));
    assert_eq!(
        state.active_state().unwrap().project_name.as_deref(),
        Some("renamed")
    );
    assert!(state.picker.is_none());
}
