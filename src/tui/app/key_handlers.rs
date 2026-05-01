//! Key-event handling and quick-action helpers for [`AppState`]. Split out of
//! `app/mod.rs` so the core state machine stays under the 400-line cap.

use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use super::action_handlers::watch_toggle_label_for;
use super::{ActiveTab, AppMode, AppState, ExplainMode, PendingQuickConfirm};
use crate::surface::status_snapshot::StatusSnapshot;
use crate::tui::widgets::QuickAction;

impl AppState {
    /// Handle a key event. Returns true when the event was consumed.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        // Confirm-stop-watch modal takes precedence: it is a blocking decision
        // point ("stop watch and run explain?") that must be answered before
        // any other key can have effect. Tab switches and global quit fall
        // through below so the operator is never trapped.
        if self.confirm_stop_watch.is_some() {
            if let Some(consumed) = self.handle_confirm_stop_watch_key(code, modifiers) {
                return consumed;
            }
        }
        if self.pending_quick_confirm.is_some() {
            return self.handle_quick_confirm_key(code, modifiers);
        }
        // Folder-picker modal: consumes navigation/toggle/commit/cancel keys
        // before anything else. Global quit (q/Ctrl-C) and tab switches still
        // fall through below so the operator is never trapped.
        if self.picker.is_some() {
            if let Some(consumed) = self.handle_picker_key(code, modifiers) {
                return consumed;
            }
        }
        // Global quit bindings.
        if matches!(code, KeyCode::Char('q') | KeyCode::Esc) {
            self.should_exit = true;
            return true;
        }
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
            return true;
        }
        // Tab switching. `Tab` and `Right` cycle forward; `BackTab`
        // (Shift+Tab) and `Left` cycle backward. The Live tab consumes
        // `Up`/`Down` for scrolling further down, but `Left`/`Right` are
        // unused on every tab so they are safe to bind globally here.
        match code {
            KeyCode::Tab | KeyCode::Right => {
                self.cycle_tab();
                return true;
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.cycle_tab_back();
                return true;
            }
            KeyCode::Char('1') => {
                self.set_tab(ActiveTab::Live);
                return true;
            }
            KeyCode::Char('2') => {
                self.set_tab(ActiveTab::Health);
                return true;
            }
            KeyCode::Char('3') => {
                self.set_tab(ActiveTab::Trust);
                return true;
            }
            KeyCode::Char('4') => {
                self.set_tab(ActiveTab::Explain);
                return true;
            }
            KeyCode::Char('5') => {
                self.set_tab(ActiveTab::Actions);
                return true;
            }
            KeyCode::Char('6') => {
                self.set_tab(ActiveTab::Mcp);
                return true;
            }
            KeyCode::Char('7') => {
                self.set_tab(ActiveTab::Explore);
                return true;
            }
            _ => {}
        }
        if matches!(self.active_tab, ActiveTab::Explore) && self.handle_explore_key(code, modifiers)
        {
            return true;
        }
        // Explain-tab key dispatch. Plan-specified bindings:
        //   s — launch explain setup sub-wizard
        //   r — run `synrepo explain` against all stale entries
        //   c — run with `--changed` (recent hotspots)
        //   f — open folder picker sub-view (in-tab, no dashboard exit)
        //   d — export docs from overlay without model calls
        //   D — force rebuild docs/index from overlay
        //   x — preview clean of materialized docs/index
        //   X — clean materialized docs/index, overlay untouched
        if matches!(self.active_tab, ActiveTab::Explain) {
            match code {
                KeyCode::Char('s') => {
                    self.launch_explain_setup = true;
                    self.should_exit = true;
                    return true;
                }
                KeyCode::Char('r') => {
                    self.queue_explain(ExplainMode::AllStale);
                    return true;
                }
                KeyCode::Char('c') => {
                    self.queue_explain(ExplainMode::Changed);
                    return true;
                }
                KeyCode::Char('f') => {
                    self.open_folder_picker();
                    return true;
                }
                KeyCode::Char('d') => {
                    return self.handle_docs_export(false);
                }
                KeyCode::Char('D') => {
                    return self.handle_docs_export(true);
                }
                KeyCode::Char('x') => {
                    return self.handle_docs_clean(false);
                }
                KeyCode::Char('X') => {
                    return self.open_quick_confirm(PendingQuickConfirm::DocsCleanApply);
                }
                _ => {}
            }
        }
        // Live-tab scroll bindings. Disabled on the other tabs so `j`/`k`
        // remain free for future per-tab navigation.
        if matches!(self.active_tab, ActiveTab::Live) {
            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.scroll_up(1);
                    return true;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.scroll_down(1);
                    return true;
                }
                KeyCode::PageUp => {
                    self.page_up();
                    return true;
                }
                KeyCode::PageDown => {
                    self.page_down();
                    return true;
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    self.scroll_home();
                    return true;
                }
                KeyCode::End | KeyCode::Char('G') => {
                    self.scroll_end();
                    return true;
                }
                KeyCode::Char('f') => {
                    self.toggle_follow();
                    return true;
                }
                _ => {}
            }
        }
        match code {
            KeyCode::Char('r') => {
                self.refresh_now();
                let counts = match self.snapshot.graph_stats.as_ref() {
                    Some(g) => format!("{} files, {} symbols", g.file_nodes, g.symbol_nodes),
                    None => "no graph data".to_string(),
                };
                self.set_toast(format!("refreshed: {counts}"));
                true
            }
            KeyCode::Char('R') => self.handle_reconcile_now(),
            KeyCode::Char('S') => self.handle_sync_now(),
            KeyCode::Char('A') | KeyCode::Char('a') => {
                self.open_quick_confirm(PendingQuickConfirm::ToggleAutoSync)
            }
            KeyCode::Char('M') | KeyCode::Char('m') => {
                self.open_quick_confirm(PendingQuickConfirm::MaterializeGraph)
            }
            KeyCode::Char('w') => self.handle_watch_toggle(),
            KeyCode::Char('p') => {
                self.set_tab(ActiveTab::Explore);
                true
            }
            KeyCode::Char('i') => {
                self.launch_integration = true;
                self.should_exit = true;
                true
            }
            KeyCode::Char('e') => {
                self.launch_explain_setup = true;
                self.should_exit = true;
                true
            }
            _ => false,
        }
    }

    fn open_quick_confirm(&mut self, action: PendingQuickConfirm) -> bool {
        self.pending_quick_confirm = Some(action);
        self.set_toast(match action {
            PendingQuickConfirm::MaterializeGraph => {
                "materialize graph: Enter to start, Esc to cancel"
            }
            PendingQuickConfirm::DocsCleanApply => "clean docs: Enter to apply, Esc to cancel",
            PendingQuickConfirm::ToggleAutoSync => {
                "auto-sync toggle: Enter to apply, Esc to cancel"
            }
        });
        true
    }

    fn handle_quick_confirm_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.pending_quick_confirm = None;
            return true;
        }
        match code {
            KeyCode::Enter | KeyCode::Char('y') => {
                let action = self.pending_quick_confirm.take();
                match action {
                    Some(PendingQuickConfirm::MaterializeGraph) => self.handle_materialize_now(),
                    Some(PendingQuickConfirm::DocsCleanApply) => self.handle_docs_clean(true),
                    Some(PendingQuickConfirm::ToggleAutoSync) => self.handle_toggle_auto_sync(),
                    None => true,
                }
            }
            KeyCode::Esc | KeyCode::Char('n') => {
                self.pending_quick_confirm = None;
                self.set_toast("cancelled");
                true
            }
            _ => true,
        }
    }
}

pub(super) fn quick_actions_for(mode: &AppMode, snapshot: &StatusSnapshot) -> Vec<QuickAction> {
    let mut actions = vec![QuickAction {
        key: "r".to_string(),
        label: "refresh snapshot".to_string(),
        disabled: false,
        requires_confirm: false,
        destructive: false,
        expensive: false,
        command_label: Some("refresh snapshot".to_string()),
    }];
    if snapshot.graph_stats.is_none() && snapshot.initialized {
        actions.push(QuickAction {
            key: "M".to_string(),
            label: "generate graph".to_string(),
            disabled: false,
            requires_confirm: true,
            destructive: false,
            expensive: true,
            command_label: Some("materialize graph".to_string()),
        });
    }
    if let Some(watch_label) = watch_toggle_label_for(mode, snapshot) {
        actions.push(QuickAction {
            key: "w".to_string(),
            label: format!("{watch_label} watch"),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: Some(format!("watch {watch_label} current")),
        });
    }
    actions.extend([
        QuickAction {
            key: "i".to_string(),
            label: "agent integration".to_string(),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: Some("agent integration".to_string()),
        },
        QuickAction {
            key: "e".to_string(),
            label: "configure explain".to_string(),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: Some("configure explain".to_string()),
        },
        QuickAction {
            key: "q".to_string(),
            label: "quit".to_string(),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: Some("quit".to_string()),
        },
    ]);
    actions
}

/// Poll the terminal for a key event, honoring a budget tied to the refresh
/// interval. Returns `None` when no key arrived within the budget.
pub fn poll_key(timeout: Duration) -> anyhow::Result<Option<(KeyCode, KeyModifiers)>> {
    if !crossterm::event::poll(timeout)? {
        return Ok(None);
    }
    match crossterm::event::read()? {
        Event::Key(k) if k.kind == KeyEventKind::Press => Ok(Some((k.code, k.modifiers))),
        _ => Ok(None),
    }
}
