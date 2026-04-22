use std::cell::Cell;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

#[derive(Default)]
pub(super) struct StopKeyState {
    requested: Cell<bool>,
}

impl StopKeyState {
    pub(super) fn poll(&self) -> bool {
        if self.requested.get() {
            return true;
        }
        while event::poll(Duration::from_millis(0)).unwrap_or(false) {
            let Ok(Event::Key(key)) = event::read() else {
                continue;
            };
            if is_stop_key(key) {
                self.requested.set(true);
                return true;
            }
        }
        false
    }

    pub(super) fn requested(&self) -> bool {
        self.requested.get()
    }
}

fn is_stop_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc | KeyCode::Char('q'))
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_keys_include_escape_q_and_ctrl_c() {
        assert!(is_stop_key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::empty()
        )));
        assert!(is_stop_key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::empty()
        )));
        assert!(is_stop_key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_stop_key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::empty()
        )));
    }
}
