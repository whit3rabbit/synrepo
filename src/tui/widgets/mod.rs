//! Widget tree for the dashboard: header, system-health, recent-activity,
//! next-actions, quick-actions, and event-log panes. Each widget takes a
//! pre-computed view model and the shared theme; none touch the graph store,
//! the file system, or stdout directly.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap};

use crate::tui::probe::{ActivityVm, HeaderVm, HealthVm, NextAction, Severity};
use crate::tui::theme::Theme;

/// Header widget showing repo path, mode, reconcile/watch/lock/MCP states.
pub struct HeaderWidget<'a> {
    /// Header view model built from the status snapshot + probe report.
    pub vm: &'a HeaderVm,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for HeaderWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Span::styled(" synrepo ", self.theme.agent_style()))
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());

        let mode_span = Span::styled(
            format!(" mode: {}", self.vm.mode_label),
            self.theme.base_style(),
        );
        let reconcile_span = severity_span(
            &format!("reconcile: {}", self.vm.reconcile_label),
            self.vm.reconcile_severity,
            self.theme,
        );
        let watch_span = severity_span(
            &format!("watch: {}", self.vm.watch_label),
            self.vm.watch_severity,
            self.theme,
        );
        let lock_span = severity_span(
            &format!("lock: {}", self.vm.lock_label),
            self.vm.lock_severity,
            self.theme,
        );
        let mcp_span = severity_span(
            &format!("mcp: {}", self.vm.mcp_label),
            self.vm.mcp_severity,
            self.theme,
        );

        let lines = vec![
            Line::from(Span::styled(
                self.vm.repo_display.clone(),
                self.theme.muted_style(),
            )),
            Line::from(vec![
                mode_span,
                Span::raw("  "),
                reconcile_span,
                Span::raw("  "),
                watch_span,
                Span::raw("  "),
                lock_span,
                Span::raw("  "),
                mcp_span,
            ]),
        ];
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

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
        let items: Vec<ListItem> = self
            .vm
            .rows
            .iter()
            .map(|row| {
                let label = Span::styled(
                    format!("{:<14}", format!("{}:", row.label)),
                    self.theme.muted_style(),
                );
                let value = severity_span(&row.value, row.severity, self.theme);
                ListItem::new(Line::from(vec![label, value]))
            })
            .collect();
        List::new(items)
            .block(block)
            .style(self.theme.base_style())
            .render(area, buf);
    }
}

/// Recent-activity pane widget.
pub struct ActivityWidget<'a> {
    /// Activity entries, newest-first.
    pub vm: &'a ActivityVm,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for ActivityWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" recent activity ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        if self.vm.entries.is_empty() {
            Paragraph::new("(no recent activity)")
                .style(self.theme.muted_style())
                .block(block)
                .render(area, buf);
            return;
        }
        let items: Vec<ListItem> = self
            .vm
            .entries
            .iter()
            .map(|entry| {
                let ts = if entry.timestamp.is_empty() {
                    "      -      ".to_string()
                } else {
                    entry.timestamp.clone()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(ts, self.theme.muted_style()),
                    Span::raw(" "),
                    Span::styled(format!("[{}]", entry.kind), self.theme.watch_active_style()),
                    Span::raw(" "),
                    Span::styled(entry.payload.clone(), self.theme.base_style()),
                ]))
            })
            .collect();
        List::new(items).block(block).render(area, buf);
    }
}

/// Next-actions pane widget.
pub struct NextActionsWidget<'a> {
    /// Next actions derived from health signals.
    pub actions: &'a [NextAction],
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for NextActionsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" next actions ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let items: Vec<ListItem> = self
            .actions
            .iter()
            .map(|a| ListItem::new(Line::from(severity_span(&a.label, a.severity, self.theme))))
            .collect();
        List::new(items).block(block).render(area, buf);
    }
}

/// One row in the quick-actions pane. Key binding + label + optional disabled
/// state.
#[derive(Clone, Debug)]
pub struct QuickAction {
    /// Key binding display label (e.g. "s").
    pub key: String,
    /// Human-readable action label.
    pub label: String,
    /// True when the action is disabled in the current context (e.g. "stop
    /// watch" when nothing is running).
    pub disabled: bool,
}

/// Quick-actions pane widget.
pub struct QuickActionsWidget<'a> {
    /// Actions to render.
    pub actions: &'a [QuickAction],
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for QuickActionsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" quick actions ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        let items: Vec<ListItem> = self
            .actions
            .iter()
            .map(|a| {
                let style = if a.disabled {
                    self.theme.muted_style()
                } else {
                    self.theme.base_style()
                };
                let key_style = if a.disabled {
                    self.theme.muted_style()
                } else {
                    self.theme.agent_style()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" [{}] ", a.key), key_style),
                    Span::styled(a.label.clone(), style),
                ]))
            })
            .collect();
        List::new(items).block(block).render(area, buf);
    }
}

/// One ring-buffer entry for the event/notification log pane.
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// RFC-3339 timestamp when the entry was pushed. Empty for unknown.
    pub timestamp: String,
    /// Tag such as "watch", "reconcile", "lock".
    pub tag: String,
    /// Free-form line.
    pub message: String,
    /// Severity used for color.
    pub severity: Severity,
}

/// Event/notification log widget backed by a bounded ring buffer.
pub struct LogWidget<'a> {
    /// Entries, oldest-to-newest.
    pub entries: &'a [LogEntry],
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for LogWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" log ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());
        if self.entries.is_empty() {
            Paragraph::new("(no events yet)")
                .style(self.theme.muted_style())
                .block(block)
                .render(area, buf);
            return;
        }
        // Show the last area.height lines (minus borders).
        let max_lines = area.height.saturating_sub(2) as usize;
        let start = self.entries.len().saturating_sub(max_lines.max(1));
        let items: Vec<ListItem> = self.entries[start..]
            .iter()
            .map(|entry| {
                let ts = if entry.timestamp.is_empty() {
                    "      -      ".to_string()
                } else {
                    entry.timestamp.clone()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(ts, self.theme.muted_style()),
                    Span::raw(" "),
                    Span::styled(format!("[{}]", entry.tag), self.theme.muted_style()),
                    Span::raw(" "),
                    severity_span(&entry.message, entry.severity, self.theme),
                ]))
            })
            .collect();
        List::new(items).block(block).render(area, buf);
    }
}

fn severity_span(text: &str, sev: Severity, theme: &Theme) -> Span<'static> {
    let style = match sev {
        Severity::Healthy => theme.healthy_style(),
        Severity::Stale => theme.stale_style(),
        Severity::Blocked => theme.blocked_style(),
    };
    let prefix = match theme.variant {
        // On plain terminals, embed a glyph prefix so the severity distinction
        // survives the loss of color. On dark terminals, color does the work
        // and a glyph would just add noise.
        crate::tui::theme::ThemeVariant::Plain => match sev {
            Severity::Healthy => "",
            Severity::Stale => "! ",
            Severity::Blocked => "!! ",
        },
        crate::tui::theme::ThemeVariant::Dark => "",
    };
    Span::styled(format!("{prefix}{text}"), style)
}
