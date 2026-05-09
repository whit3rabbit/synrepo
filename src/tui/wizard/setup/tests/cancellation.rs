//! Wizard cancellation must leave the working tree byte-identical.
//!
//! The wizard state machine has no filesystem handle by construction, so
//! these tests exercise the full "drive key events, then compare the
//! tempdir" invariant end-to-end. If any future refactor wires FS access
//! into the state machine or its helpers, these tests will catch it.

use crossterm::event::{KeyCode, KeyModifiers};

use super::support::drive_cancel_and_assert_no_writes;
use crate::tui::wizard::setup::state::SetupStep;

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

#[test]
fn cancel_at_actions_leaves_tree_byte_identical() {
    drive_cancel_and_assert_no_writes(|s| {
        s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // splash → mode
        s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // mode → target
        s.handle_key(KeyCode::Enter, KeyModifiers::empty()); // target → actions
        assert_eq!(s.step, SetupStep::SelectActions);
        s.handle_key(KeyCode::Esc, KeyModifiers::empty());
        assert_eq!(s.step, SetupStep::SelectTarget);
        s.handle_key(KeyCode::Char('q'), KeyModifiers::empty());
    });
}
