//! Repos tab widget for registry-managed projects.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, Paragraph, Widget};

use crate::tui::projects::ProjectRef;
use crate::tui::theme::Theme;
use crate::tui::widgets::projects::project_row;

/// Render registry-managed repos for the global dashboard.
pub(crate) struct ExploreTabWidget<'a> {
    /// Registry-backed project rows.
    pub(crate) projects: &'a [ProjectRef],
    /// Selected row index.
    pub(crate) selected: usize,
    /// Active project ID, when hosted by the global dashboard shell.
    pub(crate) active_project_id: Option<&'a str>,
    /// Active project root, used by single-project dashboards.
    pub(crate) active_root: Option<&'a std::path::Path>,
    /// Active theme.
    pub(crate) theme: &'a Theme,
}

impl Widget for ExploreTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" repos ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        if self.projects.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from("  no repos registered yet."),
                Line::from(""),
                Line::from("  initialize here:      synrepo init"),
                Line::from("  setup with an agent:  synrepo setup <tool>"),
                Line::from("  register a repo:      synrepo project add <path>"),
            ];
            Paragraph::new(lines)
                .block(block)
                .style(self.theme.muted_style())
                .render(area, buf);
            return;
        }
        let selected = self.selected.min(self.projects.len().saturating_sub(1));
        let items = self
            .projects
            .iter()
            .enumerate()
            .map(|(idx, project)| {
                project_row(
                    project,
                    idx == selected,
                    self.active_project_id == Some(project.id.as_str())
                        || self.active_root == Some(project.root.as_path()),
                    self.theme,
                )
            })
            .collect::<Vec<_>>();
        List::new(items).block(block).render(area, buf);
    }
}
