//! Generate-commentary modal for the Explain tab.

use crossterm::event::{KeyCode, KeyModifiers};

use super::{AppState, ExplainMode};

/// Scope selector for explicit commentary generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerateCommentaryScope {
    /// Resolve exactly one path, symbol, or node ID.
    Target,
    /// Resolve a file or symbol, then generate the file and its symbols.
    File,
    /// Generate commentary for stale or missing targets under a directory.
    Directory,
}

impl GenerateCommentaryScope {
    /// Stable display label used in the modal and progress view.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::File => "file",
            Self::Directory => "directory",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Target => Self::File,
            Self::File => Self::Directory,
            Self::Directory => Self::Target,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Target => Self::Directory,
            Self::File => Self::Target,
            Self::Directory => Self::File,
        }
    }
}

/// In-tab modal state for explicit commentary generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerateCommentaryState {
    /// Selected generation scope.
    pub scope: GenerateCommentaryScope,
    /// Raw path, symbol, or node ID typed by the operator.
    pub input: String,
}

impl Default for GenerateCommentaryState {
    fn default() -> Self {
        Self {
            scope: GenerateCommentaryScope::Target,
            input: String::new(),
        }
    }
}

impl GenerateCommentaryState {
    fn target(&self) -> Option<String> {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

impl AppState {
    pub(super) fn open_generate_commentary(&mut self) {
        self.picker = None;
        self.generate_commentary = Some(GenerateCommentaryState::default());
    }

    pub(super) fn handle_generate_commentary_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Option<bool> {
        if matches!(
            code,
            KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('1')
                | KeyCode::Char('2')
                | KeyCode::Char('3')
                | KeyCode::Char('4')
                | KeyCode::Char('5')
                | KeyCode::Char('6')
                | KeyCode::Char('7')
                | KeyCode::Char('8')
        ) {
            self.generate_commentary = None;
            return None;
        }
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.generate_commentary = None;
            return None;
        }

        let state = self
            .generate_commentary
            .as_mut()
            .expect("generate modal key handler requires modal state");
        match code {
            KeyCode::Up => {
                state.scope = state.scope.previous();
                Some(true)
            }
            KeyCode::Down => {
                state.scope = state.scope.next();
                Some(true)
            }
            KeyCode::Backspace => {
                state.input.pop();
                Some(true)
            }
            KeyCode::Enter => {
                let Some(target) = state.target() else {
                    self.set_toast("Enter a path, symbol, or node ID.");
                    return Some(true);
                };
                let scope = state.scope;
                self.generate_commentary = None;
                self.queue_explain(ExplainMode::Generate { scope, target });
                Some(true)
            }
            KeyCode::Esc => {
                self.generate_commentary = None;
                Some(true)
            }
            KeyCode::Char(ch) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                state.input.push(ch);
                Some(true)
            }
            _ => Some(true),
        }
    }
}
