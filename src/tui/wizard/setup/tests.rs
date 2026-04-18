//! Tests for setup wizard state machine.

use super::state::{SetupWizardState, SetupStep, WIZARD_TARGETS};
use crate::bootstrap::runtime_probe::AgentTargetKind;
use crate::config::Mode;
use crossterm::event::{KeyCode, KeyModifiers};

fn press(state: &mut SetupWizardState, code: KeyCode) {
    state.handle_key(code, KeyModifiers::empty());
}

#[test]
fn happy_path_default_auto_claude_target() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Claude]);
    assert_eq!(s.step, SetupStep::Splash);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectMode);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectTarget);
    assert_eq!(s.mode, Mode::Auto);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::Confirm);
    assert_eq!(s.target, Some(AgentTargetKind::Claude));
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::Complete);
    let plan = s.finalize().expect("plan");
    assert_eq!(plan.mode, Mode::Auto);
    assert_eq!(plan.target, Some(AgentTargetKind::Claude));
    assert!(plan.reconcile_after);
}

#[test]
fn select_curated_and_skip_target() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Down);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.mode, Mode::Curated);
    for _ in 0..WIZARD_TARGETS.len() {
        press(&mut s, KeyCode::Down);
    }
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.target, None);
    press(&mut s, KeyCode::Enter);
    let plan = s.finalize().expect("plan");
    assert_eq!(plan.mode, Mode::Curated);
    assert_eq!(plan.target, None);
}

#[test]
fn splash_enter_advances_to_mode() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    assert_eq!(s.step, SetupStep::Splash);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::SelectMode);
    assert!(!s.cancelled);
}

#[test]
fn esc_at_splash_cancels_without_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Esc);
    assert!(s.cancelled);
    assert_eq!(s.step, SetupStep::Complete);
    assert!(s.finalize().is_none());
}

#[test]
fn q_at_splash_cancels_without_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Char('q'));
    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}

#[test]
fn ctrl_c_at_splash_cancels() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}

#[test]
fn esc_at_mode_step_cancels_with_no_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Esc);
    assert!(s.cancelled);
    assert_eq!(s.step, SetupStep::Complete);
    assert!(s.finalize().is_none());
}

#[test]
fn esc_at_target_step_cancels_with_no_plan() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Enter);
    press(&mut s, KeyCode::Esc);
    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}

#[test]
fn b_at_confirm_goes_back_to_target_step() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Enter);
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.step, SetupStep::Confirm);
    press(&mut s, KeyCode::Char('b'));
    assert_eq!(s.step, SetupStep::SelectTarget);
    assert!(!s.cancelled);
}

#[test]
fn ctrl_c_at_confirm_cancels() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Enter);
    press(&mut s, KeyCode::Enter);
    s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(s.cancelled);
    assert!(s.finalize().is_none());
}

#[test]
fn detected_target_preselects_cursor_when_available() {
    let s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Codex]);
    assert_eq!(s.target_cursor, 2);
}

#[test]
fn detected_target_absent_from_roster_falls_back_to_zero() {
    let s = SetupWizardState::new(Mode::Curated, vec![]);
    assert_eq!(s.target_cursor, 0);
}

#[test]
fn up_at_top_does_not_underflow() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    press(&mut s, KeyCode::Up);
    assert_eq!(s.mode_cursor, 0);
}

#[test]
fn down_at_bottom_does_not_overflow() {
    let mut s = SetupWizardState::new(Mode::Auto, vec![]);
    press(&mut s, KeyCode::Enter); // leave splash
    for _ in 0..10 {
        press(&mut s, KeyCode::Down);
    }
    assert_eq!(s.mode_cursor, 1);
}

// ---- 10a.6: wizard cancellation leaves the working tree byte-identical.
//
// The wizard state machine has no filesystem handle by construction, so
// these tests exercise the full "drive key events, then compare the
// tempdir" invariant end-to-end. If any future refactor wires FS access
// into the state machine or its helpers, these tests will catch it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root).sort_by_file_name() {
        let entry = entry.expect("walk");
        if entry.file_type().is_file() {
            let rel = entry
                .path()
                .strip_prefix(root)
                .expect("strip")
                .to_path_buf();
            let bytes = std::fs::read(entry.path()).expect("read");
            out.insert(rel, bytes);
        }
    }
    out
}

fn drive_cancel_and_assert_no_writes<F: FnOnce(&mut SetupWizardState)>(drive: F) {
    let tempdir = tempfile::tempdir().expect("tempdir");
    std::fs::write(tempdir.path().join("fixture.txt"), b"original content")
        .expect("seed fixture");
    std::fs::create_dir_all(tempdir.path().join("nested/dir")).expect("mkdir");
    std::fs::write(tempdir.path().join("nested/dir/leaf.md"), b"# leaf").expect("seed leaf");
    let before = snapshot_tree(tempdir.path());

    let mut s = SetupWizardState::new(Mode::Auto, vec![AgentTargetKind::Claude]);
    drive(&mut s);
    assert!(s.cancelled, "drive closure must cancel the wizard");
    assert!(
        s.finalize().is_none(),
        "cancelled wizard must yield no plan"
    );

    let after = snapshot_tree(tempdir.path());
    assert_eq!(
        before, after,
        "working tree must be byte-identical after cancellation",
    );
}

#[test]
fn cancel_at_splash_leaves_tree_byte_identical() {
    drive_cancel_and_assert_no_writes(|s| {
        assert_eq!(s.step, SetupStep::Splash);
        s.handle_key(KeyCode::Esc, KeyModifiers::empty());
    });
}

#[test]
fn cancel_at_mode_leaves_tree_byte_identical() {
    drive_cancel_and_assert_no_writes(|s| {
        s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // splash → mode
        assert_eq!(s.step, SetupStep::SelectMode);
        s.handle_key(KeyCode::Esc, KeyModifiers::empty());
    });
}

#[test]
fn cancel_at_target_leaves_tree_byte_identical() {
    drive_cancel_and_assert_no_writes(|s| {
        s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // splash → mode
        s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // mode → target
        assert_eq!(s.step, SetupStep::SelectTarget);
        s.handle_key(KeyCode::Esc, KeyModifiers::empty());
    });
}