//! Trust pane: context quality, advisory overlay health, and change impact.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};

use crate::tui::probe::{Severity, TrustRow, TrustVm};
use crate::tui::theme::Theme;
use crate::tui::widgets::severity_span;

/// Trust-focused dashboard pane.
pub struct TrustWidget<'a> {
    /// Flattened trust view model.
    pub vm: &'a TrustVm,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for TrustWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Floors are 2 borders + 1+ content row each, so the three panes still
        // render their data when the dashboard hits its 11-row content
        // minimum. Above that floor ratatui grows the panes proportionally.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Min(4), Constraint::Min(3)])
            .split(area);

        render_group(
            " context quality ",
            &self.vm.context_rows,
            self.theme,
            chunks[0],
            buf,
        );

        let middle = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);
        render_group(
            " advisory overlay ",
            &self.vm.overlay_rows,
            self.theme,
            middle[0],
            buf,
        );
        render_group(
            " current change ",
            &self.vm.change_rows,
            self.theme,
            middle[1],
            buf,
        );

        if self.vm.degraded_rows.is_empty() {
            Paragraph::new("No degraded trust signals in the current snapshot.")
                .block(
                    Block::default()
                        .title(" remediation ")
                        .borders(Borders::ALL)
                        .border_style(self.theme.border_style()),
                )
                .style(self.theme.healthy_style())
                .render(chunks[2], buf);
        } else {
            render_group(
                " remediation ",
                &self.vm.degraded_rows,
                self.theme,
                chunks[2],
                buf,
            );
        }
    }
}

fn render_group(title: &str, rows: &[TrustRow], theme: &Theme, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme.border_style());
    let inner_width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| ListItem::new(row_line(row, theme, inner_width)))
        .collect();
    List::new(items)
        .block(block)
        .style(theme.base_style())
        .render(area, buf);
}

fn row_line(row: &TrustRow, theme: &Theme, width: usize) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            format!("{:<17}", truncate(&row.label, 16)),
            theme.muted_style(),
        ),
        indicator_span(row, theme),
        Span::raw(" "),
        severity_span(&truncate(&row.value, 20), row.severity, theme),
    ];
    if width >= 54 {
        if let Some(hint) = &row.hint {
            spans.push(Span::styled(
                format!("  {}", truncate(hint, width.saturating_sub(43))),
                theme.muted_style(),
            ));
        }
    }
    Line::from(spans)
}

fn indicator_span(row: &TrustRow, theme: &Theme) -> Span<'static> {
    let label = match row.severity {
        Severity::Blocked => "blocked",
        Severity::Stale if row.amount.is_none() => "gap",
        Severity::Stale => "watch",
        Severity::Healthy if is_clear_zero(row) => "clear",
        Severity::Healthy if row.amount == Some(0) => "idle",
        Severity::Healthy if row.amount.is_some() => "observed",
        Severity::Healthy => "ok",
    };
    let indicator = format!("[{label}]");
    let text = format!("{indicator:<12}");
    let style = match row.severity {
        Severity::Healthy => theme.healthy_style(),
        Severity::Stale => theme.stale_style(),
        Severity::Blocked => theme.blocked_style(),
    };
    Span::styled(text, style)
}

fn is_clear_zero(row: &TrustRow) -> bool {
    row.amount == Some(0)
        && matches!(
            row.label.as_str(),
            "stale responses" | "truncated" | "stale" | "unverified" | "invalid"
        )
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let suffix = "...";
    let keep = max.saturating_sub(suffix.len());
    format!("{}{}", value.chars().take(keep).collect::<String>(), suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_line_fits_narrow_width() {
        let theme = Theme::plain();
        let row = TrustRow {
            label: "affected symbols".to_string(),
            value: "unavailable".to_string(),
            hint: Some("not present in shared snapshot yet".to_string()),
            amount: None,
            total: None,
            severity: Severity::Stale,
        };
        let line = row_line(&row, &theme, 42);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("shared snapshot"));
    }

    #[test]
    fn row_line_uses_semantic_indicator_instead_of_progress_bar() {
        let theme = Theme::plain();
        let row = TrustRow {
            label: "cards served".to_string(),
            value: "6".to_string(),
            hint: Some("source: status context metrics".to_string()),
            amount: Some(5),
            total: Some(5),
            severity: Severity::Healthy,
        };
        let line = row_line(&row, &theme, 80);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("[observed]"));
        assert!(!text.contains("||||"));
        assert!(!text.contains("...."));
    }

    #[test]
    fn row_line_marks_unavailable_rows_as_gaps() {
        let theme = Theme::plain();
        let row = TrustRow {
            label: "affected symbols".to_string(),
            value: "unavailable".to_string(),
            hint: Some("not present in shared snapshot yet".to_string()),
            amount: None,
            total: None,
            severity: Severity::Stale,
        };
        let line = row_line(&row, &theme, 80);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        assert!(text.contains("[gap]"));
    }
}
