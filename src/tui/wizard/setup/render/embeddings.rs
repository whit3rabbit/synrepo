//! Rendering for the optional embeddings setup step.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::super::state::SetupWizardState;
use crate::tui::theme::Theme;

pub(super) fn draw_embeddings_step(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &SetupWizardState,
    theme: &Theme,
) {
    let rows = [
        "Skip: leave embeddings off (recommended default).",
        "Enable embeddings: build local vectors for semantic routing and hybrid search.",
    ];
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let selected = i == state.embeddings_cursor;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                theme.agent_style()
            } else {
                theme.base_style()
            };
            ListItem::new(Line::from(Span::styled(format!("{marker}{label}"), style)))
        })
        .collect();

    let block = Block::default()
        .title(" embeddings (optional) ")
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    frame.render_widget(List::new(items).block(block), area);
}
