//! Confirm-enable-explain modal.
//!
//! Shown when the operator tries to run Explain while provider calls are still
//! opted out. This keeps the no-op provider path from looking like useful work.

use crossterm::event::{KeyCode, KeyModifiers};

use super::{AppState, ExplainMode};

/// Modal state. Owned by `AppState` while the confirm prompt is visible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfirmEnableExplainState {
    /// Explain run mode the operator tried to launch.
    pub mode: ExplainMode,
}

impl AppState {
    pub(super) fn handle_confirm_enable_explain_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Option<bool> {
        if modal_fallthrough_key(code)
            || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL))
        {
            self.confirm_enable_explain = None;
            return None;
        }

        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.confirm_enable_explain = None;
                self.launch_explain_setup = true;
                self.should_exit = true;
                Some(true)
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm_enable_explain = None;
                Some(true)
            }
            _ => Some(true),
        }
    }
}

fn modal_fallthrough_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('q')
            | KeyCode::Char('1')
            | KeyCode::Char('2')
            | KeyCode::Char('3')
            | KeyCode::Char('4')
            | KeyCode::Char('5')
            | KeyCode::Char('6')
            | KeyCode::Char('7')
            | KeyCode::Char('8')
    )
}
