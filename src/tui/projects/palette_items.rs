use crossterm::event::KeyCode;

use crate::tui::app::ActiveTab;

use super::super::GlobalAppState;
use super::{CommandPaletteAction, CommandPaletteItem};

impl GlobalAppState {
    pub(super) fn command_palette_items(&self) -> Vec<CommandPaletteItem> {
        let has_active = self.active_state().is_some();
        let no_active = (!has_active).then(|| "no active project".to_string());
        let active = self.active_state();
        let graph_missing = active
            .map(|active| active.snapshot.initialized && active.snapshot.graph_stats.is_none())
            .unwrap_or(false);
        let embeddings_enabled = active
            .and_then(|active| active.snapshot.config.as_ref())
            .map(|config| config.enable_semantic_triage)
            .unwrap_or(false);

        vec![
            CommandPaletteItem::new(
                "refresh snapshot",
                "refresh snapshot",
                CommandPaletteAction::ActiveKey(KeyCode::Char('r')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "reconcile now",
                "reconcile graph",
                CommandPaletteAction::ActiveKey(KeyCode::Char('R')),
                no_active.clone(),
                false,
                false,
                true,
            ),
            CommandPaletteItem::new(
                "sync repair surfaces",
                "sync repair",
                CommandPaletteAction::ActiveKey(KeyCode::Char('S')),
                no_active.clone(),
                false,
                false,
                true,
            ),
            CommandPaletteItem::new(
                "generate missing graph",
                "materialize graph",
                CommandPaletteAction::ActiveKey(KeyCode::Char('M')),
                if !has_active {
                    no_active.clone()
                } else if graph_missing {
                    None
                } else {
                    Some("graph already exists".to_string())
                },
                true,
                false,
                true,
            ),
            CommandPaletteItem::new(
                "toggle watch",
                "watch start stop",
                CommandPaletteAction::ActiveKey(KeyCode::Char('w')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "agent integration",
                "configure agent integration",
                CommandPaletteAction::ActiveKey(KeyCode::Char('i')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "project MCP install",
                "repo mcp install",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Mcp, KeyCode::Char('i')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "configure explain",
                "explain setup",
                CommandPaletteAction::ActiveKey(KeyCode::Char('e')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "refresh explain status",
                "explain refresh status",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Explain, KeyCode::Char('r')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "run all stale explain",
                "explain all stale",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Explain, KeyCode::Char('a')),
                no_active.clone(),
                false,
                false,
                true,
            ),
            CommandPaletteItem::new(
                "run changed explain",
                "explain changed",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Explain, KeyCode::Char('c')),
                no_active.clone(),
                false,
                false,
                true,
            ),
            CommandPaletteItem::new(
                "choose explain folders",
                "explain folders",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Explain, KeyCode::Char('f')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "export explain docs",
                "explain docs export",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Explain, KeyCode::Char('d')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "rebuild explain docs",
                "explain docs rebuild",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Explain, KeyCode::Char('D')),
                no_active.clone(),
                false,
                false,
                true,
            ),
            CommandPaletteItem::new(
                "preview docs clean",
                "explain docs clean preview",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Explain, KeyCode::Char('x')),
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "clean docs",
                "explain docs clean apply",
                CommandPaletteAction::ActiveTabKey(ActiveTab::Explain, KeyCode::Char('X')),
                no_active.clone(),
                true,
                true,
                false,
            ),
            CommandPaletteItem::new(
                if embeddings_enabled {
                    "disable embeddings"
                } else {
                    "configure embeddings"
                },
                "embeddings toggle configure",
                CommandPaletteAction::ActiveKey(KeyCode::Char('T')),
                no_active.clone(),
                embeddings_enabled,
                false,
                !embeddings_enabled,
            ),
            CommandPaletteItem::new(
                "build embeddings",
                "embedding index build",
                CommandPaletteAction::ActiveKey(KeyCode::Char('B')),
                if embeddings_enabled {
                    no_active.clone()
                } else {
                    Some("embeddings disabled".to_string())
                },
                false,
                false,
                true,
            ),
            CommandPaletteItem::new(
                "switch project",
                "project switch",
                CommandPaletteAction::ProjectSwitch,
                None,
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "rename active project",
                "project rename",
                CommandPaletteAction::ProjectRename,
                no_active.clone(),
                false,
                false,
                false,
            ),
            CommandPaletteItem::new(
                "detach active project",
                "project detach remove",
                CommandPaletteAction::ProjectDetach,
                no_active,
                true,
                true,
                false,
            ),
            CommandPaletteItem::new(
                "quit",
                "quit dashboard",
                CommandPaletteAction::Quit,
                None,
                false,
                false,
                false,
            ),
        ]
    }
}
