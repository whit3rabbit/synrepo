//! Key-event handling and quick-action helpers for [`AppState`]. Split out of
//! `app/mod.rs` so the core state machine stays under the 400-line cap.

use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use super::{ActiveTab, AppState, ExplainMode, PendingQuickConfirm};

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
                self.set_tab(ActiveTab::Repos);
                return true;
            }
            KeyCode::Char('2') => {
                self.set_tab(ActiveTab::Live);
                return true;
            }
            KeyCode::Char('3') => {
                self.set_tab(ActiveTab::Health);
                return true;
            }
            KeyCode::Char('4') => {
                self.set_tab(ActiveTab::Trust);
                return true;
            }
            KeyCode::Char('5') => {
                self.set_tab(ActiveTab::Explain);
                return true;
            }
            KeyCode::Char('6') => {
                self.set_tab(ActiveTab::Actions);
                return true;
            }
            KeyCode::Char('7') => {
                self.set_tab(ActiveTab::Mcp);
                return true;
            }
            KeyCode::Char('8') => {
                self.set_tab(ActiveTab::Suggestion);
                return true;
            }
            _ => {}
        }
        if matches!(self.active_tab, ActiveTab::Repos) && self.handle_explore_key(code, modifiers) {
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
                if matches!(self.active_tab, ActiveTab::Suggestion) {
                    self.refresh_suggestions();
                    return true;
                }
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
            KeyCode::Char('U') | KeyCode::Char('u') => {
                self.open_quick_confirm(PendingQuickConfirm::ApplyCompatibility)
            }
            KeyCode::Char('A') | KeyCode::Char('a') => {
                self.open_quick_confirm(PendingQuickConfirm::ToggleAutoSync)
            }
            KeyCode::Char('T') | KeyCode::Char('t') => {
                let embeddings_enabled = self
                    .snapshot
                    .config
                    .as_ref()
                    .map(|config| config.enable_semantic_triage)
                    .unwrap_or(false);
                if embeddings_enabled {
                    self.open_quick_confirm(PendingQuickConfirm::ToggleEmbeddings)
                } else {
                    self.handle_toggle_semantic_triage()
                }
            }
            KeyCode::Char('B') | KeyCode::Char('b') => {
                self.queue_embedding_build();
                true
            }
            KeyCode::Char('M') | KeyCode::Char('m') => {
                self.open_quick_confirm(PendingQuickConfirm::MaterializeGraph)
            }
            KeyCode::Char('w') => self.handle_watch_toggle(),
            KeyCode::Char('p') => {
                self.set_tab(ActiveTab::Repos);
                true
            }
            KeyCode::Char('i') => {
                if matches!(self.active_tab, ActiveTab::Mcp) {
                    self.launch_project_mcp_install = true;
                } else {
                    self.launch_integration = true;
                }
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
            PendingQuickConfirm::ToggleEmbeddings => {
                "embeddings toggle: Enter to apply, Esc to cancel"
            }
            PendingQuickConfirm::ApplyCompatibility => {
                "compatibility apply: Enter to apply, Esc to cancel"
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
                    Some(PendingQuickConfirm::ToggleEmbeddings) => {
                        self.handle_toggle_semantic_triage()
                    }
                    Some(PendingQuickConfirm::ApplyCompatibility) => {
                        self.handle_apply_compatibility_now()
                    }
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
