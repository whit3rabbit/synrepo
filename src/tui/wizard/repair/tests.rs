//! Tests for repair wizard state machine.

#[cfg(test)]
mod tests {
    use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind, Missing};
    use crate::tui::wizard::repair::state::{
        RepairActionKind, RepairPlan, RepairStep, RepairWizardState,
    };
    use crossterm::event::{KeyCode, KeyModifiers};

    fn press(state: &mut RepairWizardState, code: KeyCode) {
        state.handle_key(code, KeyModifiers::empty());
    }

    #[test]
    fn config_missing_row_is_pre_enabled() {
        let s = RepairWizardState::new(&[Missing::ConfigFile], &AgentIntegration::Absent, &[]);
        let row = s
            .rows
            .iter()
            .find(|r| r.kind == RepairActionKind::WriteConfig)
            .expect("config row");
        assert!(row.enabled);
        assert!(!row.destructive);
    }

    #[test]
    fn upgrade_apply_is_never_enabled_by_default() {
        let s = RepairWizardState::new(
            &[Missing::CompatBlocked {
                guidance: vec!["migrate".into()],
            }],
            &AgentIntegration::Absent,
            &[],
        );
        let row = s
            .rows
            .iter()
            .find(|r| r.kind == RepairActionKind::RunUpgradeApply)
            .expect("upgrade row");
        assert!(!row.enabled, "destructive row must default off");
        assert!(row.destructive);
    }

    #[test]
    fn graph_store_missing_enables_reconcile_by_default() {
        let s = RepairWizardState::new(&[Missing::GraphStore], &AgentIntegration::Absent, &[]);
        let row = s
            .rows
            .iter()
            .find(|r| r.kind == RepairActionKind::RunReconcile)
            .expect("reconcile row");
        assert!(row.enabled);
    }

    #[test]
    fn integration_partial_offers_shim_row() {
        let s = RepairWizardState::new(
            &[],
            &AgentIntegration::Partial {
                target: AgentTargetKind::Claude,
            },
            &[],
        );
        let row = s
            .rows
            .iter()
            .find(|r| r.kind == RepairActionKind::WriteShim)
            .expect("shim row");
        assert!(!row.enabled);
        assert_eq!(s.shim_target, Some(AgentTargetKind::Claude));
    }

    #[test]
    fn integration_complete_omits_shim_row() {
        let s = RepairWizardState::new(
            &[Missing::GraphStore],
            &AgentIntegration::Complete {
                target: AgentTargetKind::Claude,
            },
            &[],
        );
        assert!(s.rows.iter().all(|r| r.kind != RepairActionKind::WriteShim));
    }

    #[test]
    fn happy_path_accept_defaults_and_confirm() {
        let mut s = RepairWizardState::new(
            &[Missing::ConfigFile, Missing::GraphStore],
            &AgentIntegration::Absent,
            &[],
        );
        press(&mut s, KeyCode::Enter); // to Confirm
        assert_eq!(s.step, RepairStep::Confirm);
        press(&mut s, KeyCode::Enter); // apply
        assert_eq!(s.step, RepairStep::Complete);
        let plan = s.finalize().expect("plan");
        assert!(plan.write_config);
        assert!(plan.run_reconcile);
        assert!(!plan.run_upgrade_apply);
        assert_eq!(plan.write_shim_for, None);
    }

    #[test]
    fn space_toggles_enable_upgrade_apply() {
        let mut s = RepairWizardState::new(
            &[Missing::CompatBlocked {
                guidance: vec!["migrate".into()],
            }],
            &AgentIntegration::Absent,
            &[],
        );
        // First row is the upgrade-apply destructive row (config wasn't missing).
        assert!(!s.rows[0].enabled);
        press(&mut s, KeyCode::Char(' '));
        assert!(s.rows[0].enabled);
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Enter);
        let plan = s.finalize().expect("plan");
        assert!(plan.run_upgrade_apply);
    }

    #[test]
    fn esc_at_select_cancels_with_no_plan() {
        let mut s = RepairWizardState::new(&[Missing::ConfigFile], &AgentIntegration::Absent, &[]);
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn b_at_confirm_goes_back_without_cancelling() {
        let mut s = RepairWizardState::new(&[Missing::ConfigFile], &AgentIntegration::Absent, &[]);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.step, RepairStep::Confirm);
        press(&mut s, KeyCode::Char('b'));
        assert_eq!(s.step, RepairStep::Select);
        assert!(!s.cancelled);
    }

    #[test]
    fn cancel_before_confirm_leaves_filesystem_untouched() {
        // This is a state-level invariant: finalize() returns None on cancel.
        // The fs-level invariant is guaranteed by design — the wizard never
        // mutates the fs; only the bin-side dispatcher does. See the matching
        // bin test in cli_support/tests/setup.rs.
        let mut s = RepairWizardState::new(
            &[
                Missing::ConfigFile,
                Missing::CompatBlocked {
                    guidance: vec!["migrate".into()],
                },
            ],
            &AgentIntegration::Partial {
                target: AgentTargetKind::Claude,
            },
            &[],
        );
        press(&mut s, KeyCode::Char(' ')); // toggle first row off
        press(&mut s, KeyCode::Esc);
        assert!(s.cancelled);
        assert!(s.finalize().is_none());
    }

    #[test]
    fn compat_blocked_guidance_is_surfaced() {
        let s = RepairWizardState::new(
            &[Missing::CompatBlocked {
                guidance: vec!["run synrepo upgrade --apply".to_string()],
            }],
            &AgentIntegration::Absent,
            &[],
        );
        assert!(s.guidance.iter().any(|g| g.contains("upgrade --apply")));
    }

    #[test]
    fn is_empty_reports_no_actions() {
        let p = RepairPlan::default();
        assert!(p.is_empty());
        let p = RepairPlan {
            write_config: true,
            ..RepairPlan::default()
        };
        assert!(!p.is_empty());
    }
}
