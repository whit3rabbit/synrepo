//! Registry-backed project picker for the global dashboard shell.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};

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
        let title = if picker
            .and_then(|picker| picker.detach_confirm.as_ref())
            .is_some()
        {
            " detach project? ".to_string()
        } else if let Some(input) = picker.and_then(|picker| picker.rename_input.as_ref()) {
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
        if rows.is_empty() {
            let message = picker
                .filter(|picker| !picker.filter.is_empty())
                .map(|picker| format!("  no projects match /{}", picker.filter))
                .unwrap_or_else(|| "  no registered projects to show".to_string());
            Paragraph::new(Line::from(message))
                .block(block)
                .style(self.theme.muted_style())
                .render(area, buf);
            return;
        }
        let selected = self
            .state
            .picker
            .as_ref()
            .map(|picker| picker.selected)
            .unwrap_or(0);
        let items: Vec<ListItem> = rows
            .iter()
            .enumerate()
            .map(|(idx, project)| {
                project_row(
                    project,
                    idx == selected,
                    self.state.active_project_id.as_deref() == Some(project.id.as_str()),
                    self.theme,
                )
            })
            .collect();
        List::new(items).block(block).render(area, buf);
    }
}

pub(crate) fn project_row(
    project: &ProjectRef,
    selected: bool,
    active: bool,
    theme: &Theme,
) -> ListItem<'static> {
    let marker = match (selected, active) {
        (true, true) => "!*",
        (true, false) => ">",
        (false, true) => "*",
        (false, false) => " ",
    };
    let style = if selected {
        theme.selected_style()
    } else {
        theme.base_style()
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker:<2}"), style),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::projects::ProjectPickerState;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn empty_filter_result_renders_empty_state() {
        let theme = Theme::plain();
        let state = GlobalAppState {
            projects: Vec::new(),
            active_project_id: None,
            project_states: HashMap::new(),
            picker: Some(ProjectPickerState {
                filter: "none".to_string(),
                ..ProjectPickerState::default()
            }),
            explore_selected: 0,
            help_visible: false,
            command_palette: None,
            cwd: PathBuf::from("/tmp"),
            theme,
            should_exit: false,
        };
        let area = Rect::new(0, 0, 60, 5);
        let mut buf = Buffer::empty(area);
        ProjectPickerWidget {
            state: &state,
            theme: &theme,
        }
        .render(area, &mut buf);

        let rendered = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("no projects match /none"), "{rendered}");
    }
}
