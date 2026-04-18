//! Tests for integration wizard state machine.

#[cfg(test)]
mod tests {
    use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
    use crate::tui::wizard::integration::state::{IntegrationStep, IntegrationWizardState};
    use crossterm::event::{KeyCode, KeyModifiers};

    fn press(state: &mut IntegrationWizardState, code: KeyCode) {
        state.handle_key(code, KeyModifiers::empty());
    }

    fn absent() -> AgentIntegration {
        AgentIntegration::Absent
    }
    fn partial(t: AgentTargetKind) -> AgentIntegration {
        AgentIntegration::Partial { target: t }
    }
    fn complete(t: AgentTargetKind) -> AgentIntegration {
        AgentIntegration::Complete { target: t }
    }

    #[test]
    fn absent_seeds_write_shim_and_register_mcp_on() {
        let s = IntegrationWizardState::new(absent(), vec![]);
        assert_eq!(s.step, IntegrationStep::SelectTarget);
        assert!(s.write_shim);
        assert!(s.register_mcp);
        assert!(!s.overwrite_shim);
    }

    #[test]
    fn partial_seeds_only_register_mcp_on() {
        let s = IntegrationWizardState::new(partial(AgentTargetKind::Claude), vec![]);
        assert_eq!(s.target, AgentTargetKind::Claude);
        assert!(!s.write_shim);
        assert!(s.register_mcp);
        assert!(!s.overwrite_shim);
    }

    #[test]
    fn complete_seeds_all_off_and_forces_explicit_opt_in() {
        // Never overwrite an existing fully-configured integration without an
        // explicit opt-in inside the wizard.
        let s = IntegrationWizardState::new(complete(AgentTargetKind::Cursor), vec![]);
        assert_eq!(s.target, AgentTargetKind::Cursor);
        assert!(!s.write_shim);
        assert!(!s.register_mcp);
        assert!(!s.overwrite_shim);
    }

    #[test]
    fn changing_target_reseeds_defaults() {
        let mut s = IntegrationWizardState::new(complete(AgentTargetKind::Claude), vec![]);
        // Target 0 is Claude (automated) → Complete; defaults off.
        assert_eq!(s.target_cursor, 0);
        assert!(!s.write_shim);
        // Press Down to move to Cursor (target_cursor=1). Cursor is shim-only,
        // so register_mcp stays off by default even when the integration state
        // would otherwise turn it on; write_shim flips on.
        press(&mut s, KeyCode::Down);
        assert_eq!(s.target_cursor, 1);
        assert!(
            s.write_shim,
            "new target with absent integration seeds write_shim=on"
        );
        assert!(
            !s.register_mcp,
            "shim-only target must not default register_mcp=on"
        );
        // Press Down again to Codex (target_cursor=2). Codex is automated, so
        // the reseeded defaults should enable both actions.
        press(&mut s, KeyCode::Down);
        assert_eq!(s.target_cursor, 2);
        assert!(s.write_shim);
        assert!(
            s.register_mcp,
            "automated target defaults register_mcp back on"
        );
    }

    #[test]
    fn shim_only_target_defaults_register_mcp_off() {
        // Cursor, Copilot, Windsurf are all shim-only; their MCP registration
        // checkbox should default off regardless of integration state.
        for target in [
            AgentTargetKind::Cursor,
            AgentTargetKind::Copilot,
            AgentTargetKind::Windsurf,
        ] {
            let s = IntegrationWizardState::new(partial(target), vec![]);
            assert_eq!(s.target, target);
            assert!(
                !s.register_mcp,
                "{target:?} is shim-only; register_mcp must default off"
            );
        }
    }

    #[test]
    fn happy_path_writes_shim_and_registers_mcp() {
        let mut s = IntegrationWizardState::new(absent(), vec![AgentTargetKind::Claude]);
        press(&mut s, KeyCode::Enter); // target → actions
        assert_eq!(s.step, IntegrationStep::SelectActions);
        press(&mut s, KeyCode::Enter); // actions → confirm
        assert_eq!(s.step, IntegrationStep::Confirm);
        press(&mut s, KeyCode::Enter); // confirm → complete
        let plan = s.finalize().expect("plan");
        assert_eq!(plan.target, AgentTargetKind::Claude);
        assert!(plan.write_shim);
        assert!(plan.register_mcp);
        assert!(!plan.overwrite_shim);
    }

    #[test]
    fn overwrite_toggle_requires_explicit_space_keypress() {
        let mut s = IntegrationWizardState::new(complete(AgentTargetKind::Claude), vec![]);
        press(&mut s, KeyCode::Enter); // target → actions
                                       // Defaults are all off; press Enter should refuse to advance (no-op).
        press(&mut s, KeyCode::Enter);
        assert_eq!(
            s.step,
            IntegrationStep::SelectActions,
            "enter with no actions selected must not advance",
        );
        // Navigate to the overwrite row and toggle it on; then Enter should
        // still refuse (overwrite alone doesn't imply write_shim).
        press(&mut s, KeyCode::Down); // cursor → register_mcp
        press(&mut s, KeyCode::Down); // cursor → overwrite_shim
        press(&mut s, KeyCode::Char(' '));
        assert!(s.overwrite_shim);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, IntegrationStep::SelectActions);
        // Now toggle write_shim on; Enter should advance.
        press(&mut s, KeyCode::Up);
        press(&mut s, KeyCode::Up); // cursor → write_shim
        press(&mut s, KeyCode::Char(' '));
        assert!(s.write_shim);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, IntegrationStep::Confirm);
    }

    #[test]
    fn esc_at_target_cancels_without_plan() {
        let mut s = IntegrationWizardState::new(absent(), vec![]);
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn esc_at_actions_returns_to_target_step() {
        let mut s = IntegrationWizardState::new(absent(), vec![]);
        press(&mut s, KeyCode::Enter); // target → actions
        press(&mut s, KeyCode::Esc);
        assert_eq!(s.step, IntegrationStep::SelectTarget);
        assert!(!s.cancelled);
    }

    #[test]
    fn ctrl_c_at_confirm_cancels() {
        let mut s = IntegrationWizardState::new(absent(), vec![]);
        press(&mut s, KeyCode::Enter); // target → actions
        press(&mut s, KeyCode::Enter); // actions → confirm
        s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn back_at_confirm_returns_to_actions() {
        let mut s = IntegrationWizardState::new(absent(), vec![]);
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, IntegrationStep::Confirm);
        press(&mut s, KeyCode::Char('b'));
        assert_eq!(s.step, IntegrationStep::SelectActions);
    }

    #[test]
    fn completion_with_no_actions_yields_no_plan() {
        let mut s = IntegrationWizardState::new(complete(AgentTargetKind::Claude), vec![]);
        press(&mut s, KeyCode::Enter); // target → actions
                                       // Never advance further; simulate the state machine exiting with
                                       // neither flag set.
        s.step = IntegrationStep::Complete;
        assert!(s.finalize().is_none());
    }
}
