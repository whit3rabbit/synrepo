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
            let line = Line::from(vec![
                Span::styled(format!(" {msg}"), self.theme.healthy_style()),
                Span::styled("    projects ", self.theme.muted_style()),
                Span::styled("[p]", self.theme.agent_style()),
                Span::styled("  help ", self.theme.muted_style()),
                Span::styled("[?]", self.theme.agent_style()),
                Span::styled("  quit ", self.theme.muted_style()),
                Span::styled("[q]", self.theme.agent_style()),
            ]);
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

impl FooterWidget<'_> {
    fn build_hints(&self) -> Vec<HintGroup> {
        let mut groups = Vec::new();
        groups.push(HintGroup {
            priority: 0,
            spans: vec![
                Span::styled(" proj ", self.theme.muted_style()),
                Span::styled("[p]", self.theme.agent_style()),
            ],
        });
        groups.push(HintGroup {
            priority: 0,
            spans: vec![
                Span::styled("  help ", self.theme.muted_style()),
                Span::styled("[?]", self.theme.agent_style()),
            ],
        });
        groups.push(HintGroup {
            priority: 1,
            spans: vec![
                Span::styled(" tabs ", self.theme.muted_style()),
                Span::styled("[Tab/1-7]", self.theme.agent_style()),
            ],
        });
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
        groups.push(HintGroup {
            priority: 3,
            spans: vec![
                Span::styled("  refresh ", self.theme.muted_style()),
                Span::styled("[r]", self.theme.agent_style()),
            ],
        });
        if self.materialize_hint_visible {
            groups.push(HintGroup {
                priority: 1,
                spans: vec![
                    Span::styled("  graph ", self.theme.muted_style()),
                    Span::styled("[M] ", self.theme.agent_style()),
                    Span::styled("generate", self.theme.stale_style()),
                ],
            });
        }
        if let Some(label) = self.watch_toggle_label {
            groups.push(HintGroup {
                priority: 3,
                spans: vec![
                    Span::styled("  watch ", self.theme.muted_style()),
                    Span::styled("[w] ", self.theme.agent_style()),
                    Span::styled(label.to_string(), self.theme.base_style()),
                ],
            });
        }
        groups.push(HintGroup {
            priority: 4,
            spans: vec![
                Span::styled("  integration ", self.theme.muted_style()),
                Span::styled("[i]", self.theme.agent_style()),
            ],
        });
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
mod tests {
    use super::*;
    use crate::tui::theme::Theme;

    fn footer(
        active: ActiveTab,
        follow: bool,
        watch_toggle_label: Option<&str>,
    ) -> (Vec<HintGroup>, Theme) {
        let theme = Theme::plain();
        let widget = FooterWidget {
            active,
            follow_mode: follow,
            theme: &theme,
            toast: None,
            watch_toggle_label,
            materialize_hint_visible: false,
        };
        (widget.build_hints(), theme)
    }

    fn footer_with_materialize(
        active: ActiveTab,
        watch_toggle_label: Option<&str>,
        materialize_hint_visible: bool,
    ) -> Vec<HintGroup> {
        let theme = Theme::plain();
        let widget = FooterWidget {
            active,
            follow_mode: false,
            theme: &theme,
            toast: None,
            watch_toggle_label,
            materialize_hint_visible,
        };
        widget.build_hints()
    }

    fn rendered_text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn wide_terminal_keeps_every_hint() {
        let (groups, _) = footer(ActiveTab::Live, true, Some("stop"));
        let spans = fit_groups(groups, 200);
        let text = rendered_text(&spans);
        assert!(text.contains("scroll"));
        assert!(text.contains("follow"));
        assert!(text.contains("refresh"));
        assert!(text.contains("watch") && text.contains("stop"));
        assert!(text.contains("integration"));
        assert!(text.ends_with("[q]"));
    }

    #[test]
    fn narrow_terminal_preserves_quit_hint() {
        for active in [ActiveTab::Live, ActiveTab::Health, ActiveTab::Actions] {
            for width in [40usize, 50, 60, 70, 80] {
                let (groups, _) = footer(active, false, Some("start"));
                let spans = fit_groups(groups, width);
                let text = rendered_text(&spans);
                assert!(
                    text.contains("[p]") && text.contains("[?]") && text.contains("[q]"),
                    "tab={active:?} width={width} must keep project/help/quit, got {text:?}"
                );
                let visible: usize = spans.iter().map(|s| s.width()).sum();
                assert!(
                    visible <= width,
                    "tab={active:?} width={width} overflowed: visible={visible} text={text:?}"
                );
            }
        }
    }

    #[test]
    fn narrow_live_drops_scroll_before_follow() {
        let (groups, _) = footer(ActiveTab::Live, true, Some("stop"));
        let spans = fit_groups(groups, 80);
        let text = rendered_text(&spans);
        assert!(
            !text.contains("scroll"),
            "scroll should drop first: {text:?}"
        );
        assert!(
            text.contains("follow"),
            "follow should survive at 80 cols: {text:?}"
        );
    }

    #[test]
    fn toast_keeps_essential_hints_when_set() {
        let theme = Theme::plain();
        let widget = FooterWidget {
            active: ActiveTab::Live,
            follow_mode: false,
            theme: &theme,
            toast: Some("Refreshed: 12 files, 34 symbols"),
            watch_toggle_label: Some("stop"),
            materialize_hint_visible: false,
        };
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        let row: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            row.contains("Refreshed: 12 files, 34 symbols"),
            "toast text must appear in footer row: {row:?}"
        );
        assert!(
            row.contains("[p]") && row.contains("[?]") && row.contains("[q]"),
            "toast must keep essential hints: {row:?}"
        );
    }

    #[test]
    fn very_narrow_terminal_keeps_only_essentials() {
        let (groups, _) = footer(ActiveTab::Live, false, Some("start"));
        let spans = fit_groups(groups, 30);
        let text = rendered_text(&spans);
        assert!(text.contains("[p]"));
        assert!(text.contains("[?]"));
        assert!(text.contains("[q]"));
        assert!(!text.contains("tabs"));
        assert!(!text.contains("scroll"));
        assert!(!text.contains("follow"));
        assert!(!text.contains("integration"));
        assert!(!text.contains("watch"));
        assert!(!text.contains("refresh"));
        let visible: usize = spans.iter().map(|s| s.width()).sum();
        assert!(visible <= 30, "visible={visible} text={text:?}");
    }

    #[test]
    fn omits_watch_hint_when_toggle_is_unavailable() {
        let (groups, _) = footer(ActiveTab::Health, false, None);
        let spans = fit_groups(groups, 200);
        let text = rendered_text(&spans);
        assert!(!text.contains("watch"));
    }

    #[test]
    fn shows_materialize_hint_when_graph_missing() {
        let groups = footer_with_materialize(ActiveTab::Health, None, true);
        let spans = fit_groups(groups, 200);
        let text = rendered_text(&spans);
        assert!(text.contains("[M]"), "missing [M] hint: {text:?}");
        assert!(
            text.contains("generate"),
            "missing generate label: {text:?}"
        );
    }

    #[test]
    fn omits_materialize_hint_when_graph_present() {
        let groups = footer_with_materialize(ActiveTab::Health, None, false);
        let spans = fit_groups(groups, 200);
        let text = rendered_text(&spans);
        assert!(!text.contains("[M]"), "stray [M] hint: {text:?}");
    }
}
