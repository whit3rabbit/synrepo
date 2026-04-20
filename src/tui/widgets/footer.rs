//! One-line footer showing scroll + follow + quit hints. Rendered as a thin
//! strip at the bottom of the dashboard so the operator always sees the
//! keys available in the current tab.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::tui::app::ActiveTab;
use crate::tui::theme::Theme;

/// Key-hint footer strip.
pub struct FooterWidget<'a> {
    /// Current tab; footer content varies slightly per tab (e.g., scroll
    /// hints only make sense on Live).
    pub active: ActiveTab,
    /// Whether the Live feed is in follow-bottom mode.
    pub follow_mode: bool,
    /// Active theme.
    pub theme: &'a Theme,
    /// Transient message rendered in place of the hint row (e.g.
    /// "Refreshed: N files, M symbols").
    pub toast: Option<&'a str>,
    /// Poll-mode watch toggle label, when the dashboard exposes `w`.
    pub watch_toggle_label: Option<&'a str>,
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
            // Healthy style so it reads as confirmation rather than alert.
            let line = Line::from(Span::styled(format!(" {msg}"), self.theme.healthy_style()));
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
                Span::styled(" tabs ", self.theme.muted_style()),
                Span::styled("[Tab/1/2/3/4]", self.theme.agent_style()),
            ],
        });
        if matches!(self.active, ActiveTab::Live) {
            // Scroll hint is the widest group and the easiest to drop first.
            groups.push(HintGroup {
                priority: 5,
                spans: vec![
                    Span::styled("  scroll ", self.theme.muted_style()),
                    Span::styled(
                        "[\u{2191}/\u{2193} PgUp/PgDn Home/End]",
                        self.theme.agent_style(),
                    ),
                ],
            });
            // Follow mode toggle. Ranked above `integration` because it
            // exposes visible state (on/off) that the operator reads rather
            // than a menu key they trigger.
            let follow_label = if self.follow_mode { "on" } else { "off" };
            let follow_style = if self.follow_mode {
                self.theme.healthy_style()
            } else {
                self.theme.stale_style()
            };
            groups.push(HintGroup {
                priority: 3,
                spans: vec![
                    Span::styled("  follow ", self.theme.muted_style()),
                    Span::styled("[f] ", self.theme.agent_style()),
                    Span::styled(follow_label.to_string(), follow_style),
                ],
            });
        }
        groups.push(HintGroup {
            priority: 2,
            spans: vec![
                Span::styled("  refresh ", self.theme.muted_style()),
                Span::styled("[r]", self.theme.agent_style()),
            ],
        });
        if let Some(label) = self.watch_toggle_label {
            groups.push(HintGroup {
                priority: 2,
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
        // Last so `[q]` anchors the right edge once trimming kicks in.
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

/// Select hint groups that fit into `width` display columns. Drops
/// higher-priority-number groups first, preserving essential tabs/quit hints
/// even when the terminal is very narrow.
fn fit_groups(groups: Vec<HintGroup>, width: usize) -> Vec<Span<'static>> {
    let total: usize = groups.iter().map(HintGroup::width).sum();
    let mut kept: Vec<bool> = vec![true; groups.len()];

    if total > width {
        // Walk groups in drop-first order (highest priority number, then
        // rightmost index) and mark them dropped until the remaining set fits.
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

    groups
        .into_iter()
        .zip(kept)
        .filter(|(_, keep)| *keep)
        .flat_map(|(g, _)| g.spans)
        .collect()
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
        };
        (widget.build_hints(), theme)
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
                    text.contains("tabs") && text.contains("[q]"),
                    "tab={active:?} width={width} must keep tabs+quit, got {text:?}"
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
        // 80 cols should not fit scroll+follow+all, but should keep follow.
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
    fn toast_replaces_hint_row_when_set() {
        let theme = Theme::plain();
        let widget = FooterWidget {
            active: ActiveTab::Live,
            follow_mode: false,
            theme: &theme,
            toast: Some("Refreshed: 12 files, 34 symbols"),
            watch_toggle_label: Some("stop"),
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
            !row.contains("tabs"),
            "toast must suppress the hint row: {row:?}"
        );
    }

    #[test]
    fn very_narrow_terminal_keeps_only_essentials() {
        // Essentials (" tabs [Tab/1/2/3]  quit [q]") total 27 cols. At a
        // width below that, fit_groups still returns them (ratatui clamps the
        // remainder visually); we just verify the non-essential groups are
        // all dropped.
        let (groups, _) = footer(ActiveTab::Live, false, Some("start"));
        let spans = fit_groups(groups, 30);
        let text = rendered_text(&spans);
        assert!(text.contains("tabs"));
        assert!(text.contains("[q]"));
        assert!(!text.contains("scroll"));
        assert!(!text.contains("follow"));
        assert!(!text.contains("integration"));
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
}
