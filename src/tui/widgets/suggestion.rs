//! Suggestion tab widget.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};

use crate::surface::refactor_suggestions::{RefactorSuggestionCandidate, RefactorSuggestionReport};
use crate::tui::theme::Theme;

/// Render large-file refactor suggestions for the active project.
pub struct SuggestionTabWidget<'a> {
    /// Cached suggestion report, if loaded.
    pub report: Option<&'a RefactorSuggestionReport>,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for SuggestionTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" suggestion ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let Some(report) = self.report else {
            Paragraph::new("  suggestions are loading or unavailable.")
                .block(block)
                .style(self.theme.muted_style())
                .render(area, buf);
            return;
        };
        if report.candidates.is_empty() {
            Paragraph::new(format!(
                "  no non-test source files over {} physical lines.",
                report.threshold
            ))
            .block(block)
            .style(self.theme.muted_style())
            .render(area, buf);
            return;
        }

        let mut items = Vec::with_capacity(report.candidates.len() + 1);
        items.push(summary_item(report, self.theme));
        items.extend(
            report
                .candidates
                .iter()
                .map(|candidate| candidate_item(candidate, self.theme)),
        );
        List::new(items)
            .block(block)
            .style(self.theme.base_style())
            .render(area, buf);
    }
}

fn summary_item(report: &RefactorSuggestionReport, theme: &Theme) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(
            format!(" {} candidates", report.candidate_count),
            theme.agent_style(),
        ),
        Span::styled(
            format!(" over {} physical lines", report.threshold),
            theme.muted_style(),
        ),
        Span::styled(
            format!("  omitted {}", report.omitted_count),
            theme.muted_style(),
        ),
    ]))
}

fn candidate_item(candidate: &RefactorSuggestionCandidate, theme: &Theme) -> ListItem<'static> {
    let language = candidate.language.as_deref().unwrap_or("unknown");
    let tags = candidate.modularity_tags.join(",");
    ListItem::new(Line::from(vec![
        Span::styled(
            format!(" {:>4} ", candidate.line_count),
            theme.agent_style(),
        ),
        Span::styled(format!("{language:<10} "), theme.muted_style()),
        Span::styled(candidate.path.clone(), theme.base_style()),
        Span::styled(
            format!("  symbols:{} ", candidate.symbol_counts.total),
            theme.muted_style(),
        ),
        Span::styled(tags, theme.stale_style()),
    ]))
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    use super::*;
    use crate::core::ids::FileNodeId;
    use crate::surface::refactor_suggestions::{RefactorSuggestionGroup, RefactorSymbolCounts};

    fn rendered_text(buf: &Buffer, area: Rect) -> String {
        let mut text = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                text.push_str(buf[(x, y)].symbol());
            }
        }
        text
    }

    #[test]
    fn renders_unloaded_state() {
        let theme = Theme::plain();
        let area = Rect::new(0, 0, 80, 5);
        let mut buf = Buffer::empty(area);
        SuggestionTabWidget {
            report: None,
            theme: &theme,
        }
        .render(area, &mut buf);

        let text = rendered_text(&buf, area);
        assert!(text.contains("suggestions are loading or unavailable"));
    }

    #[test]
    fn renders_populated_candidates() {
        let theme = Theme::plain();
        let report = RefactorSuggestionReport {
            source_store: "graph+filesystem",
            metric: "physical_lines",
            threshold: 300,
            candidate_count: 1,
            omitted_count: 0,
            groups: vec![RefactorSuggestionGroup {
                language: "rust".to_string(),
                count: 1,
                max_line_count: 420,
            }],
            candidates: vec![RefactorSuggestionCandidate {
                path: "src/lib.rs".to_string(),
                file_id: FileNodeId(1),
                language: Some("rust".to_string()),
                line_count: 420,
                size_bytes: 1024,
                symbol_counts: RefactorSymbolCounts {
                    total: 12,
                    public: 2,
                    restricted: 1,
                    private: 9,
                },
                modularity_tags: vec!["large_file".to_string(), "many_symbols".to_string()],
                suggestion: "Group related symbols.".to_string(),
                recommended_follow_up: vec![
                    "synrepo_card target=src/lib.rs budget=normal".to_string()
                ],
            }],
        };
        let area = Rect::new(0, 0, 100, 6);
        let mut buf = Buffer::empty(area);
        SuggestionTabWidget {
            report: Some(&report),
            theme: &theme,
        }
        .render(area, &mut buf);

        let text = rendered_text(&buf, area);
        assert!(text.contains("1 candidates"));
        assert!(text.contains("src/lib.rs"));
        assert!(text.contains("many_symbols"));
    }
}
