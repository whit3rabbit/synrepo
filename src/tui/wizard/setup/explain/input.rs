//! Single-line text input widget for the explain setup wizard.

use crossterm::event::{KeyCode, KeyModifiers};

/// Single-line text input field used by the endpoint-edit step.
///
/// Deliberately narrow — one buffer, one cursor, Char / Backspace / Left /
/// Right / Home / End / Ctrl-U (clear). No multi-line, no selection, no
/// validation beyond "non-empty" at commit time. Tests drive it via
/// [`TextInputField::handle_key`] the same way they drive the rest of the
/// wizard state machine.
#[derive(Clone, Debug)]
pub struct TextInputField {
    buffer: String,
    cursor: usize,
}

impl TextInputField {
    /// Construct with a pre-filled value; cursor lands at end of text.
    pub fn with_value(initial: &str) -> Self {
        Self {
            buffer: initial.to_string(),
            cursor: initial.chars().count(),
        }
    }

    /// Replace the entire buffer and move the cursor to the end. Used when
    /// the user switches preset after already typing: the text input is
    /// re-seeded with the new preset's default endpoint.
    pub fn reset(&mut self, value: &str) {
        self.buffer = value.to_string();
        self.cursor = self.buffer.chars().count();
    }

    /// Borrow the current buffer contents.
    pub fn value(&self) -> &str {
        &self.buffer
    }

    /// Cursor position (in chars, not bytes).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Handle a key event. Returns `true` when the input was modified (the
    /// render loop should redraw). `Enter` and `Esc` are NOT handled here —
    /// the parent state machine observes them to drive the step transition.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match code {
            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.buffer.clear();
                self.cursor = 0;
                true
            }
            KeyCode::Char(c) => {
                // Unicode-safe insert at cursor position.
                let byte_index = self
                    .buffer
                    .char_indices()
                    .nth(self.cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.buffer.len());
                self.buffer.insert(byte_index, c);
                self.cursor += 1;
                true
            }
            KeyCode::Backspace => {
                if self.cursor == 0 {
                    return false;
                }
                let prev_byte = self
                    .buffer
                    .char_indices()
                    .nth(self.cursor - 1)
                    .map(|(i, _)| i)
                    .expect("cursor > 0 implies a char exists");
                let this_byte = self
                    .buffer
                    .char_indices()
                    .nth(self.cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.buffer.len());
                self.buffer.drain(prev_byte..this_byte);
                self.cursor -= 1;
                true
            }
            KeyCode::Left if self.cursor > 0 => {
                self.cursor -= 1;
                true
            }
            KeyCode::Left => false,
            KeyCode::Right if self.cursor < self.buffer.chars().count() => {
                self.cursor += 1;
                true
            }
            KeyCode::Right => false,
            KeyCode::Home => {
                self.cursor = 0;
                true
            }
            KeyCode::End => {
                self.cursor = self.buffer.chars().count();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(f: &mut TextInputField, code: KeyCode) {
        f.handle_key(code, KeyModifiers::empty());
    }

    #[test]
    fn with_value_places_cursor_at_end() {
        let f = TextInputField::with_value("abc");
        assert_eq!(f.value(), "abc");
        assert_eq!(f.cursor(), 3);
    }

    #[test]
    fn typed_chars_insert_at_cursor() {
        let mut f = TextInputField::with_value("hello");
        press(&mut f, KeyCode::Home);
        press(&mut f, KeyCode::Char('!'));
        assert_eq!(f.value(), "!hello");
        assert_eq!(f.cursor(), 1);
    }

    #[test]
    fn backspace_removes_previous_char() {
        let mut f = TextInputField::with_value("abc");
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.value(), "ab");
        assert_eq!(f.cursor(), 2);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut f = TextInputField::with_value("x");
        press(&mut f, KeyCode::Home);
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.value(), "x");
        assert_eq!(f.cursor(), 0);
    }

    #[test]
    fn left_right_do_not_wrap() {
        let mut f = TextInputField::with_value("ab");
        press(&mut f, KeyCode::Right);
        assert_eq!(f.cursor(), 2);
        press(&mut f, KeyCode::Home);
        press(&mut f, KeyCode::Left);
        assert_eq!(f.cursor(), 0);
    }

    #[test]
    fn ctrl_u_clears_buffer() {
        let mut f = TextInputField::with_value("http://localhost");
        f.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(f.value(), "");
        assert_eq!(f.cursor(), 0);
    }

    #[test]
    fn reset_replaces_buffer() {
        let mut f = TextInputField::with_value("old");
        f.reset("brand-new-value");
        assert_eq!(f.value(), "brand-new-value");
        assert_eq!(f.cursor(), "brand-new-value".chars().count());
    }

    #[test]
    fn unicode_insert_and_backspace() {
        let mut f = TextInputField::with_value("");
        press(&mut f, KeyCode::Char('é'));
        press(&mut f, KeyCode::Char('x'));
        assert_eq!(f.value(), "éx");
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.value(), "é");
        press(&mut f, KeyCode::Backspace);
        assert_eq!(f.value(), "");
    }
}
