//! Suggestion tab widget.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};

use crate::surface::refactor_suggestions::{
    RefactorSuggestionCandidate, RefactorSuggestionMode, RefactorSuggestionReport,
};
use crate::tui::theme::Theme;

/// Render large-file refactor suggestions for the active project.
pub struct SuggestionTabWidget<'a> {
    /// Cached suggestion report, if loaded.
    pub report: Option<&'a RefactorSuggestionReport>,
    /// Active suggestion mode.
    pub mode: RefactorSuggestionMode,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for SuggestionTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(" suggestion: {} ", self.mode.label()))
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
            Paragraph::new(empty_message(report))
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
                .map(|candidate| candidate_item(report.mode, candidate, self.theme)),
        );
        List::new(items)
            .block(block)
            .style(self.theme.base_style())
            .render(area, buf);
    }
}

fn summary_item(report: &RefactorSuggestionReport, theme: &Theme) -> ListItem<'static> {
    match report.mode {
        RefactorSuggestionMode::LineCount => ListItem::new(Line::from(vec![
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
        ])),
        RefactorSuggestionMode::MissingDocs => ListItem::new(Line::from(vec![
            Span::styled(
                format!(" {} files", report.candidate_count),
                theme.agent_style(),
            ),
            Span::styled(" missing public docs", theme.muted_style()),
            Span::styled(
                format!("  omitted {}", report.omitted_count),
                theme.muted_style(),
            ),
        ])),
    }
}

fn candidate_item(
    mode: RefactorSuggestionMode,
    candidate: &RefactorSuggestionCandidate,
    theme: &Theme,
) -> ListItem<'static> {
    let language = candidate.language.as_deref().unwrap_or("unknown");
    let tags = candidate.modularity_tags.join(",");
    match mode {
        RefactorSuggestionMode::LineCount => ListItem::new(Line::from(vec![
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
        ])),
        RefactorSuggestionMode::MissingDocs => {
            let preview = missing_docs_preview(candidate);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:>3} docs ", candidate.missing_public_doc_count),
                    theme.agent_style(),
                ),
                Span::styled(format!("{language:<10} "), theme.muted_style()),
                Span::styled(candidate.path.clone(), theme.base_style()),
                Span::styled("  ", theme.muted_style()),
                Span::styled(preview, theme.stale_style()),
            ]))
        }
    }
}

fn empty_message(report: &RefactorSuggestionReport) -> String {
    match report.mode {
        RefactorSuggestionMode::LineCount => {
            format!(
                "  no non-test source files over {} physical lines.",
                report.threshold
            )
        }
        RefactorSuggestionMode::MissingDocs => {
            "  no public symbols missing parser-extracted docs.".to_string()
        }
    }
}

fn missing_docs_preview(candidate: &RefactorSuggestionCandidate) -> String {
    let mut names = candidate
        .missing_public_docs
        .iter()
        .map(|symbol| symbol.qualified_name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    if names.is_empty() {
        names.push('-');
    }
    if candidate.missing_public_docs_omitted > 0 {
        names.push_str(&format!(" +{}", candidate.missing_public_docs_omitted));
    }
    names
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    use super::*;
    use crate::core::ids::{FileNodeId, SymbolNodeId};
    use crate::surface::refactor_suggestions::{
        MissingPublicDocSymbol, RefactorSuggestionCriteria, RefactorSuggestionGroup,
        RefactorSymbolCounts,
    };

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
            mode: RefactorSuggestionMode::LineCount,
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
            mode: RefactorSuggestionMode::LineCount,
            metric: "physical_lines",
            threshold: 300,
            criteria: RefactorSuggestionCriteria::line_count(300),
            candidate_count: 1,
            omitted_count: 0,
            groups: vec![RefactorSuggestionGroup {
                language: "rust".to_string(),
                count: 1,
                max_line_count: 420,
                max_missing_public_doc_count: 0,
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
                missing_public_doc_count: 0,
                missing_public_docs: Vec::new(),
                missing_public_docs_omitted: 0,
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
            mode: RefactorSuggestionMode::LineCount,
            theme: &theme,
        }
        .render(area, &mut buf);

        let text = rendered_text(&buf, area);
        assert!(text.contains("1 candidates"));
        assert!(text.contains("src/lib.rs"));
        assert!(text.contains("many_symbols"));
    }

    #[test]
    fn renders_missing_docs_candidates() {
        let theme = Theme::plain();
        let report = RefactorSuggestionReport {
            source_store: "graph+filesystem",
            mode: RefactorSuggestionMode::MissingDocs,
            metric: "missing_public_docs",
            threshold: 300,
            criteria: RefactorSuggestionCriteria::missing_docs(300),
            candidate_count: 1,
            omitted_count: 0,
            groups: vec![RefactorSuggestionGroup {
                language: "rust".to_string(),
                count: 1,
                max_line_count: 42,
                max_missing_public_doc_count: 2,
            }],
            candidates: vec![RefactorSuggestionCandidate {
                path: "src/lib.rs".to_string(),
                file_id: FileNodeId(1),
                language: Some("rust".to_string()),
                line_count: 42,
                size_bytes: 1024,
                symbol_counts: RefactorSymbolCounts {
                    total: 3,
                    public: 2,
                    restricted: 0,
                    private: 1,
                },
                missing_public_doc_count: 2,
                missing_public_docs: vec![MissingPublicDocSymbol {
                    symbol_id: SymbolNodeId(1),
                    qualified_name: "missing_docs".to_string(),
                    display_name: "missing_docs".to_string(),
                    kind: crate::structure::graph::SymbolKind::Function,
                    signature: Some("pub fn missing_docs()".to_string()),
                }],
                missing_public_docs_omitted: 1,
                modularity_tags: vec!["missing_public_docs".to_string()],
                suggestion: "Add docs.".to_string(),
                recommended_follow_up: vec![
                    "synrepo_card target=src/lib.rs budget=normal".to_string()
                ],
            }],
        };
        let area = Rect::new(0, 0, 100, 6);
        let mut buf = Buffer::empty(area);
        SuggestionTabWidget {
            report: Some(&report),
            mode: RefactorSuggestionMode::MissingDocs,
            theme: &theme,
        }
        .render(area, &mut buf);

        let text = rendered_text(&buf, area);
        assert!(text.contains("missing public docs"));
        assert!(text.contains("2 docs"));
        assert!(text.contains("missing_docs +1"));
    }
}
