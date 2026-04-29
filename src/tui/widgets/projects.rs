//! Registry-backed project picker for the global dashboard shell.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::tui::projects::{GlobalAppState, ProjectRef};
use crate::tui::theme::Theme;

/// Render the global project picker.
pub(crate) struct ProjectPickerWidget<'a> {
    /// Global state containing picker filter/selection and project rows.
    pub(crate) state: &'a GlobalAppState,
    /// Active theme.
    pub(crate) theme: &'a Theme,
}

impl Widget for ProjectPickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let picker = self.state.picker.as_ref();
        let title = if let Some(input) = picker.and_then(|picker| picker.rename_input.as_ref()) {
            format!(" rename: {input} ")
        } else if let Some(picker) = picker.filter(|picker| !picker.filter.is_empty()) {
            format!(" projects /{} ", picker.filter)
        } else {
            " projects ".to_string()
        };
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let rows = self.state.filtered_projects();
        let selected = self
            .state
            .picker
            .as_ref()
            .map(|picker| picker.selected)
            .unwrap_or(0);
        let items: Vec<ListItem> = rows
            .iter()
            .enumerate()
            .map(|(idx, project)| project_row(project, idx == selected, self.theme))
            .collect();
        List::new(items).block(block).render(area, buf);
    }
}

fn project_row(project: &ProjectRef, selected: bool, theme: &Theme) -> ListItem<'static> {
    let marker = if selected { ">" } else { " " };
    let style = if selected {
        theme.selected_style()
    } else {
        theme.base_style()
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker} "), style),
        Span::styled(format!("{:<18}", project.name), style),
        Span::styled(format!(" watch:{:<10}", project.watch), theme.muted_style()),
        Span::styled(
            format!(" health:{:<14}", project.health),
            theme.muted_style(),
        ),
        Span::styled(format!(" lock:{:<9}", project.lock), theme.muted_style()),
        Span::styled(
            format!(" integration:{:<12}", project.integration),
            theme.muted_style(),
        ),
        Span::styled(format!(" {}", project.root.display()), theme.muted_style()),
    ]))
}
