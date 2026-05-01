//! System-health pane: graph counts, export freshness, commentary coverage,
//! overlay cost, explain state. Each row is a label/value pair with a
//! severity color on the value.

use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::tui::probe::{HealthRow, HealthVm};
use crate::tui::theme::Theme;
use crate::tui::widgets::severity_span;

/// System-health pane widget.
pub struct HealthWidget<'a> {
    /// Flattened rows.
    pub vm: &'a HealthVm,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for HealthWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" system health ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let items = grouped_items(
            &self.vm.rows,
            area.width.saturating_sub(2) as usize,
            area.height.saturating_sub(2) as usize,
            self.theme,
        );
        List::new(items)
            .block(block)
            .style(self.theme.base_style())
            .render(area, buf);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HealthSectionKind {
    Repository,
    Context,
    Explain,
    Readiness,
    Other,
}

impl HealthSectionKind {
    fn title(self) -> &'static str {
        match self {
            HealthSectionKind::Repository => "repository state",
            HealthSectionKind::Context => "context serving",
            HealthSectionKind::Explain => "explain",
            HealthSectionKind::Readiness => "capability readiness",
            HealthSectionKind::Other => "other",
        }
    }
}

struct HealthSection<'a> {
    kind: HealthSectionKind,
    rows: Vec<DisplayRow<'a>>,
}

struct DisplayRow<'a> {
    label: Cow<'a, str>,
    row: &'a HealthRow,
}

fn grouped_items<'a>(
    rows: &'a [HealthRow],
    content_width: usize,
    content_height: usize,
    theme: &Theme,
) -> Vec<ListItem<'a>> {
    let sections = health_sections(rows);
    let section_count = sections.len();
    let row_count: usize = sections.iter().map(|section| section.rows.len()).sum();
    let show_headers = section_count > 1 && row_count + section_count <= content_height;
    let mut items = Vec::new();
    for section in sections {
        if show_headers {
            items.push(section_header(section.kind.title(), content_width, theme));
        }
        let label_width = section
            .rows
            .iter()
            .map(|row| row.label.len() + 2)
            .max()
            .unwrap_or(0);
        for row in section.rows {
            items.push(row_item(row, label_width, theme));
        }
    }
    items
}

fn health_sections(rows: &[HealthRow]) -> Vec<HealthSection<'_>> {
    let mut sections = vec![
        HealthSection {
            kind: HealthSectionKind::Repository,
            rows: Vec::new(),
        },
        HealthSection {
            kind: HealthSectionKind::Context,
            rows: Vec::new(),
        },
        HealthSection {
            kind: HealthSectionKind::Explain,
            rows: Vec::new(),
        },
        HealthSection {
            kind: HealthSectionKind::Readiness,
            rows: Vec::new(),
        },
        HealthSection {
            kind: HealthSectionKind::Other,
            rows: Vec::new(),
        },
    ];
    for row in rows {
        let kind = section_for(&row.label);
        let section = sections
            .iter_mut()
            .find(|section| section.kind == kind)
            .expect("all section kinds are pre-seeded");
        section.rows.push(DisplayRow {
            label: display_label(&row.label),
            row,
        });
    }
    sections
        .into_iter()
        .filter(|section| !section.rows.is_empty())
        .collect()
}

fn section_for(label: &str) -> HealthSectionKind {
    match label {
        "repo" | "graph" | "export" | "commentary" | "overlay cost" => {
            HealthSectionKind::Repository
        }
        "context" | "tokens avoided" | "stale responses" | "mcp" => HealthSectionKind::Context,
        "explain" | "explain usage" | "explain skipped" => HealthSectionKind::Explain,
        _ if label.starts_with("readiness:") => HealthSectionKind::Readiness,
        _ => HealthSectionKind::Other,
    }
}

fn display_label(label: &str) -> Cow<'_, str> {
    if let Some(readiness_label) = label.strip_prefix("readiness:") {
        Cow::Owned(readiness_label.replace('-', " "))
    } else {
        Cow::Borrowed(label)
    }
}

fn section_header<'a>(title: &str, content_width: usize, theme: &Theme) -> ListItem<'a> {
    let label = format!(" {title} ");
    let rule_len = content_width.saturating_sub(label.len()).max(2);
    let rule = if theme.accessibility.ascii_only {
        "-"
    } else {
        "\u{2500}"
    };
    ListItem::new(Line::from(Span::styled(
        format!("{label}{}", rule.repeat(rule_len)),
        theme.muted_style(),
    )))
}

fn row_item<'a>(row: DisplayRow<'a>, label_width: usize, theme: &Theme) -> ListItem<'a> {
    let label = Span::styled(
        format!("{:<label_width$}", format!("{}:", row.label)),
        theme.muted_style(),
    );
    let value = severity_span(&row.row.value, row.row.severity, theme);
    ListItem::new(Line::from(vec![label, value]))
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;

    use super::*;
    use crate::tui::probe::Severity;

    fn row(label: &str, value: &str) -> HealthRow {
        HealthRow {
            label: label.to_string(),
            value: value.to_string(),
            severity: Severity::Healthy,
        }
    }

    #[test]
    fn groups_health_rows_into_readable_sections() {
        let rows = vec![
            row("graph", "1 file"),
            row("context", "2 cards"),
            row("explain usage", "3 calls"),
            row("readiness:git-intelligence", "supported"),
        ];

        let sections = health_sections(&rows);
        let titles: Vec<_> = sections
            .iter()
            .map(|section| section.kind.title())
            .collect();

        assert_eq!(
            titles,
            vec![
                "repository state",
                "context serving",
                "explain",
                "capability readiness"
            ]
        );
        assert_eq!(sections[3].rows[0].label, "git intelligence");
    }

    #[test]
    fn renders_section_dividers_and_clean_readiness_labels() {
        let rows = vec![
            row("graph", "1 file"),
            row("readiness:index-freshness", "stale"),
        ];
        let vm = HealthVm { rows };
        let theme = Theme::plain();
        let widget = HealthWidget {
            vm: &vm,
            theme: &theme,
        };
        let area = Rect::new(0, 0, 80, 8);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        let rendered = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("repository state"));
        assert!(rendered.contains("capability readiness"));
        assert!(rendered.contains("graph: [ok] 1 file"));
        assert!(rendered.contains("index freshness: [ok] stale"));
        assert!(!rendered.contains("readiness:index-freshness"));
    }

    #[test]
    fn omits_section_dividers_when_height_is_tight() {
        let rows = vec![
            row("graph", "1 file"),
            row("context", "2 cards"),
            row("readiness:index-freshness", "stale"),
        ];
        let items = grouped_items(&rows, 80, 3, &Theme::plain());

        assert_eq!(items.len(), 3);
    }
}
