//! One-line footer showing dashboard key hints.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::tui::app::ActiveTab;
use crate::tui::theme::Theme;

/// Key-hint footer strip.
pub struct FooterWidget<'a> {
    /// Current tab; footer content varies slightly per tab.
    pub active: ActiveTab,
    /// Whether the Live feed is in follow-bottom mode.
    pub follow_mode: bool,
    /// Active theme.
    pub theme: &'a Theme,
    /// Transient message rendered in the footer row.
    pub toast: Option<&'a str>,
    /// Poll-mode watch toggle label, when the dashboard exposes `w`.
    pub watch_toggle_label: Option<&'a str>,
    /// Render the `[M] generate graph` hint when graph stats are missing.
    pub materialize_hint_visible: bool,
}

/// A hint group (label + key) with a priority used to drop low-value hints
/// when the terminal is too narrow to fit them all on one line. Priority 0 is
/// essential (never dropped); higher numbers are dropped first.
struct HintGroup {
    priority: u8,
    spans: Vec<Span<'static>>,
}

impl HintGroup {
    fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }
}

impl Widget for FooterWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if let Some(msg) = self.toast {
            let suffix = vec![
                Span::styled("    projects ", self.theme.muted_style()),
                Span::styled("[p]", self.theme.agent_style()),
                Span::styled("  help ", self.theme.muted_style()),
                Span::styled("[?]", self.theme.agent_style()),
                Span::styled("  run ", self.theme.muted_style()),
                Span::styled("[:]", self.theme.agent_style()),
                Span::styled("  quit ", self.theme.muted_style()),
                Span::styled("[q]", self.theme.agent_style()),
            ];
            let suffix_width: usize = suffix.iter().map(Span::width).sum();
            let max_msg_width = (area.width as usize).saturating_sub(suffix_width);
            let msg = truncate_message(msg, max_msg_width);
            let line = Line::from(
                vec![Span::styled(format!(" {msg}"), self.theme.healthy_style())]
                    .into_iter()
                    .chain(suffix)
                    .collect::<Vec<_>>(),
            );
            Paragraph::new(line)
                .style(self.theme.base_style())
                .render(area, buf);
            return;
        }
        let groups = self.build_hints();
        let spans = fit_groups(groups, area.width as usize);
        Paragraph::new(Line::from(spans))
            .style(self.theme.base_style())
            .render(area, buf);
    }
}

fn truncate_message(msg: &str, max_width: usize) -> String {
    if msg.chars().count() < max_width {
        return msg.to_string();
    }
    let take = max_width.saturating_sub(2);
    let mut out = msg.chars().take(take).collect::<String>();
    out.push('~');
    out
}

impl FooterWidget<'_> {
    fn build_hints(&self) -> Vec<HintGroup> {
        let mut groups = vec![
            HintGroup {
                priority: 0,
                spans: vec![
                    Span::styled(" proj ", self.theme.muted_style()),
                    Span::styled("[p]", self.theme.agent_style()),
                ],
            },
            HintGroup {
                priority: 0,
                spans: vec![
                    Span::styled("  help ", self.theme.muted_style()),
                    Span::styled("[?]", self.theme.agent_style()),
                ],
            },
            HintGroup {
                priority: 0,
                spans: vec![
                    Span::styled("  run ", self.theme.muted_style()),
                    Span::styled("[:]", self.theme.agent_style()),
                ],
            },
            HintGroup {
                priority: 2,
                spans: vec![
                    Span::styled(" tabs ", self.theme.muted_style()),
                    Span::styled("[Tab/1-8]", self.theme.agent_style()),
                ],
            },
        ];
        if matches!(self.active, ActiveTab::Live) {
            groups.push(HintGroup {
                priority: 5,
                spans: vec![
                    Span::styled("  scroll ", self.theme.muted_style()),
                    Span::styled(
                        if self.theme.accessibility.ascii_only {
                            "[Up/Down PgUp/PgDn Home/End]"
                        } else {
                            "[\u{2191}/\u{2193} PgUp/PgDn Home/End]"
                        },
                        self.theme.agent_style(),
                    ),
                ],
            });
            let follow_label = if self.follow_mode { "on" } else { "off" };
            let follow_style = if self.follow_mode {
                self.theme.healthy_style()
            } else {
                self.theme.stale_style()
            };
            groups.push(HintGroup {
                priority: 2,
                spans: vec![
                    Span::styled("  follow ", self.theme.muted_style()),
                    Span::styled("[f] ", self.theme.agent_style()),
                    Span::styled(follow_label.to_string(), follow_style),
                ],
            });
        }
        if matches!(self.active, ActiveTab::Explain) {
            // Tab-scoped: surface the explain run + docs keys so an operator
            // on the Explain tab can see the available actions without having
            // to read the in-pane help text. Bundled into one group so the
            // pair survives or drops together at common widths.
            groups.push(HintGroup {
                priority: 4,
                spans: vec![
                    Span::styled("  explain ", self.theme.muted_style()),
                    Span::styled("[r/a/c/f]", self.theme.agent_style()),
                    Span::styled("  docs ", self.theme.muted_style()),
                    Span::styled("[d/D/x/X]", self.theme.agent_style()),
                ],
            });
        }
        groups.push(HintGroup {
            priority: 0,
            spans: vec![
                Span::styled("  quit ", self.theme.muted_style()),
                Span::styled("[q]", self.theme.agent_style()),
            ],
        });
        groups
    }
}

/// Select hint groups that fit into `width` display columns.
fn fit_groups(groups: Vec<HintGroup>, width: usize) -> Vec<Span<'static>> {
    let total: usize = groups.iter().map(HintGroup::width).sum();
    let mut kept: Vec<bool> = vec![true; groups.len()];

    if total > width {
        let mut order: Vec<usize> = (0..groups.len())
            .filter(|i| groups[*i].priority > 0)
            .collect();
        order.sort_by(|a, b| groups[*b].priority.cmp(&groups[*a].priority).then(b.cmp(a)));
        let mut running = total;
        for i in order {
            if running <= width {
                break;
            }
            kept[i] = false;
            running -= groups[i].width();
        }
    }

    let visible: usize = groups
        .iter()
        .zip(&kept)
        .filter(|(_, keep)| **keep)
        .map(|(group, _)| group.width())
        .sum();
    if visible > width {
        return compact_essential_hints(&groups, width);
    }

    groups
        .into_iter()
        .zip(kept)
        .filter(|(_, keep)| *keep)
        .flat_map(|(g, _)| g.spans)
        .collect()
}

fn compact_essential_hints(groups: &[HintGroup], width: usize) -> Vec<Span<'static>> {
    let mut keys: Vec<Span<'static>> = groups
        .iter()
        .filter(|group| group.priority == 0)
        .filter_map(|group| {
            group
                .spans
                .iter()
                .find(|span| span.content.as_ref().starts_with('['))
                .cloned()
        })
        .collect();

    let key_width = keys.iter().map(Span::width).sum::<usize>() + keys.len().saturating_sub(1);
    if key_width <= width {
        return join_key_spans(keys);
    }

    keys.retain(|span| span.content.as_ref() == "[q]");
    join_key_spans(keys)
}

fn join_key_spans(keys: Vec<Span<'static>>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for key in keys {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(key);
    }
    spans
}

#[cfg(test)]
#[path = "footer_tests.rs"]
mod tests;
