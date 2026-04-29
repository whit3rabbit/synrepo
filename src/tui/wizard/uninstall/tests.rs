//! Tests for uninstall wizard state machine.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::tui::wizard::uninstall::state::{
        UninstallActionKind, UninstallStep, UninstallWizardState,
    };
    use crossterm::event::{KeyCode, KeyModifiers};

    fn press(state: &mut UninstallWizardState, code: KeyCode) {
        state.handle_key(code, KeyModifiers::empty());
    }

    fn seed_actions() -> Vec<UninstallActionKind> {
        vec![
            UninstallActionKind::RemoveShim {
                tool: "claude".to_string(),
                path: PathBuf::from("/repo/.claude/skills/synrepo/SKILL.md"),
            },
            UninstallActionKind::RemoveMcpEntry {
                tool: "claude".to_string(),
                path: PathBuf::from("/repo/.mcp.json"),
            },
            UninstallActionKind::RemoveGitignoreLine {
                entry: ".synrepo/".to_string(),
            },
            UninstallActionKind::DeleteSynrepoDir,
        ]
    }

    #[test]
    fn installed_rows_start_checked_by_default() {
        let s = UninstallWizardState::new(&seed_actions(), &[]);
        assert!(s.rows[0].enabled, "shim row must start checked");
        assert!(s.rows[1].enabled, "mcp row must start checked");
        assert!(s.rows[2].enabled, "gitignore row must start checked");
    }

    #[test]
    fn delete_synrepo_dir_row_starts_unchecked_and_destructive() {
        let s = UninstallWizardState::new(&seed_actions(), &[]);
        let row = s
            .rows
            .iter()
            .find(|r| matches!(r.kind, UninstallActionKind::DeleteSynrepoDir))
            .expect("delete-synrepo-dir row");
        assert!(!row.enabled, "destructive row must default off");
        assert!(row.destructive);
    }

    #[test]
    fn guided_data_rows_start_unchecked() {
        let actions = vec![
            UninstallActionKind::DeleteProjectSynrepoDir {
                project: PathBuf::from("/repo"),
                path: PathBuf::from("/repo/.synrepo"),
            },
            UninstallActionKind::RemoveExportDir {
                project: PathBuf::from("/repo"),
                path: PathBuf::from("/repo/synrepo-context"),
            },
            UninstallActionKind::DeleteGlobalSynrepoDir {
                path: PathBuf::from("/home/user/.synrepo"),
            },
        ];
        let s = UninstallWizardState::new(&actions, &[]);
        assert!(s.rows.iter().all(|row| row.destructive));
        assert!(s.rows.iter().all(|row| !row.enabled));
    }

    #[test]
    fn safe_binary_delete_row_starts_checked_but_destructive() {
        let actions = vec![UninstallActionKind::DeleteBinary {
            path: PathBuf::from("/home/user/.local/bin/synrepo"),
        }];
        let s = UninstallWizardState::new(&actions, &[]);
        assert!(s.rows[0].destructive);
        assert!(s.rows[0].enabled);
    }

    #[test]
    fn labels_are_rendered_for_each_action() {
        let s = UninstallWizardState::new(&seed_actions(), &[]);
        assert!(s.rows[0].label.contains("claude"));
        assert!(s.rows[0].label.contains("SKILL.md"));
        assert!(s.rows[1].label.contains("MCP"));
        assert!(s.rows[2].label.contains(".gitignore"));
        assert!(s.rows[3].label.starts_with('!'));
    }

    #[test]
    fn preserved_list_is_surfaced_in_state() {
        let preserved = vec![PathBuf::from("/repo/.mcp.json.bak")];
        let s = UninstallWizardState::new(&seed_actions(), &preserved);
        assert_eq!(s.preserved, preserved);
    }

    #[test]
    fn happy_path_confirm_produces_default_plan_without_delete_synrepo_dir() {
        let mut s = UninstallWizardState::new(&seed_actions(), &[]);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, UninstallStep::Confirm);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, UninstallStep::Complete);

        let plan = s.finalize().expect("plan");
        assert_eq!(plan.actions.len(), 3, "destructive row off by default");
        assert!(plan
            .actions
            .iter()
            .all(|a| !matches!(a, UninstallActionKind::DeleteSynrepoDir)));
    }

    #[test]
    fn toggling_the_destructive_row_opts_into_full_deletion() {
        let mut s = UninstallWizardState::new(&seed_actions(), &[]);
        // Cursor starts at 0; DeleteSynrepoDir is the last row.
        for _ in 0..(s.rows.len() - 1) {
            press(&mut s, KeyCode::Down);
        }
        press(&mut s, KeyCode::Char(' '));
        assert!(
            s.rows.last().unwrap().enabled,
            "Space must toggle the destructive row on"
        );

        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        let plan = s.finalize().expect("plan");
        assert!(plan
            .actions
            .iter()
            .any(|a| matches!(a, UninstallActionKind::DeleteSynrepoDir)));
    }

    #[test]
    fn toggling_off_an_installed_row_drops_it_from_the_plan() {
        let mut s = UninstallWizardState::new(&seed_actions(), &[]);
        // Cursor at row 0 (shim). Untick it.
        press(&mut s, KeyCode::Char(' '));
        assert!(!s.rows[0].enabled);

        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        let plan = s.finalize().expect("plan");
        assert!(
            !plan
                .actions
                .iter()
                .any(|a| matches!(a, UninstallActionKind::RemoveShim { .. })),
            "shim must not appear in plan after toggle-off"
        );
    }

    #[test]
    fn esc_at_select_cancels_with_no_plan() {
        let mut s = UninstallWizardState::new(&seed_actions(), &[]);
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert_eq!(s.step, UninstallStep::Complete);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn b_at_confirm_goes_back_without_cancelling() {
        let mut s = UninstallWizardState::new(&seed_actions(), &[]);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, UninstallStep::Confirm);
        press(&mut s, KeyCode::Char('b'));
        assert_eq!(s.step, UninstallStep::Select);
        assert!(!s.cancelled);
    }

    #[test]
    fn cancel_before_confirm_produces_no_plan() {
        // State-level invariant: finalize returns None on cancel. The fs-side
        // guarantee is that the wizard never writes; only the bin-side
        // dispatcher does.
        let mut s = UninstallWizardState::new(&seed_actions(), &[]);
        press(&mut s, KeyCode::Char(' '));
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn ctrl_c_from_confirm_cancels() {
        let mut s = UninstallWizardState::new(&seed_actions(), &[]);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, UninstallStep::Confirm);
        s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(s.cancelled);
        assert_eq!(s.step, UninstallStep::Complete);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn empty_installed_list_produces_empty_plan_after_confirm() {
        let mut s = UninstallWizardState::new(&[], &[]);
        assert!(s.rows.is_empty());
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        let plan = s.finalize().expect("plan");
        assert!(plan.is_empty());
    }

    #[test]
    fn cursor_clamps_at_bounds() {
        let mut s = UninstallWizardState::new(&seed_actions(), &[]);
        press(&mut s, KeyCode::Up);
        assert_eq!(s.cursor, 0, "up at index 0 stays at 0");
        for _ in 0..20 {
            press(&mut s, KeyCode::Down);
        }
        assert_eq!(s.cursor, s.rows.len() - 1);
    }
}
