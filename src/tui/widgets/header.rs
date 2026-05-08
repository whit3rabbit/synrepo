//! Header widget: repo path, reconcile/watch/lock/integration status row, and a
//! hand-rolled braille spinner that animates while a reconcile pass is in
//! flight. The spinner frame index lives on `AppState::frame`; the widget
//! stays stateless and picks a glyph from `SPINNER_FRAMES` modulo the
//! array length.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::tui::probe::HeaderVm;
use crate::tui::theme::Theme;
use crate::tui::widgets::severity_span;

/// Status entry plus a drop priority. Higher priorities are dropped first
/// when the status row would overflow the header. 0 is essential.
struct StatusEntry {
    priority: u8,
    spans: Vec<Span<'static>>,
}

impl StatusEntry {
    fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }
}

/// If `watch_label` contains "(pid N)" and `lock_label` contains the same
/// "pid N", strip the parenthetical from the watch text. The lock row is
/// canonical for ownership, and duplicating the pid pushes the line over
/// 120 cols on common terminals.
fn dedupe_watch_pid(watch_label: &str, lock_label: &str) -> String {
    let Some(open) = watch_label.find("(pid ") else {
        return watch_label.to_string();
    };
    let Some(close_rel) = watch_label[open..].find(')') else {
        return watch_label.to_string();
    };
    let close = open + close_rel;
    let pid = &watch_label[open + 5..close];
    if !lock_label.contains(&format!("pid {pid}")) {
        return watch_label.to_string();
    }
    let mut out = String::with_capacity(watch_label.len());
    out.push_str(watch_label[..open].trim_end());
    out.push_str(&watch_label[close + 1..]);
    out
}

/// Braille spinner frames. Kept in sync with
/// `throbber_widgets_tui::BRAILLE_SIX` so a future re-introduction of the
/// crate would be drop-in.
const SPINNER_FRAMES: [&str; 10] = [
    "\u{2807}", "\u{2811}", "\u{2819}", "\u{2838}", "\u{2830}", "\u{2834}", "\u{2826}", "\u{2807}",
    "\u{2811}", "\u{2819}",
];

/// Pick the spinner glyph for a given monotonic frame counter.
pub fn spinner_glyph(frame: u32) -> &'static str {
    SPINNER_FRAMES[(frame as usize) % SPINNER_FRAMES.len()]
}

/// Header widget showing repo path, mode, reconcile/watch/lock/integration states,
/// plus a spinner when `reconcile_active` is true.
pub struct HeaderWidget<'a> {
    /// Header view model built from the status snapshot + probe report.
    pub vm: &'a HeaderVm,
    /// Active theme.
    pub theme: &'a Theme,
    /// Monotonic tick counter; advanced each tick by the app shell. Drives
    /// the spinner animation.
    pub frame: u32,
    /// True when a reconcile pass is in flight (spinner visible). False shows
    /// an idle marker instead.
    pub reconcile_active: bool,
}

impl Widget for HeaderWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Span::styled(" synrepo ", self.theme.agent_style()))
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());

        // Left-most column of the repo-path row is a spinner or idle marker so
        // the operator sees liveness feedback regardless of which tab is up.
        let status_span: Span<'static> = if self.reconcile_active {
            let marker =
                if self.theme.accessibility.reduced_motion || self.theme.accessibility.ascii_only {
                    "[RUN]"
                } else {
                    spinner_glyph(self.frame)
                };
            Span::styled(
                format!("{marker} reconciling  "),
                self.theme.watch_active_style(),
            )
        } else {
            Span::styled("[idle]  ", self.theme.muted_style())
        };

        let watch_label = dedupe_watch_pid(&self.vm.watch_label, &self.vm.lock_label);

        // Mode and auto-sync are config state, also visible on the Health
        // tab. Drop them first when the status line would wrap. Reconcile
        // and watch are operational and stay until last.
        let mut entries: Vec<StatusEntry> = Vec::new();
        entries.push(StatusEntry {
            priority: 5,
            spans: vec![Span::styled(
                format!(" mode: {}", self.vm.mode_label),
                self.theme.base_style(),
            )],
        });
        entries.push(StatusEntry {
            priority: 0,
            spans: vec![
                Span::raw("  "),
                severity_span(
                    &format!("reconcile: {}", self.vm.reconcile_label),
                    self.vm.reconcile_severity,
                    self.theme,
                ),
            ],
        });
        entries.push(StatusEntry {
            priority: 1,
            spans: vec![
                Span::raw("  "),
                severity_span(
                    &format!("watch: {watch_label}"),
                    self.vm.watch_severity,
                    self.theme,
                ),
            ],
        });
        if let Some(enabled) = self.vm.auto_sync {
            let label = if enabled { "on" } else { "off" };
            // Stale (off) keeps a higher priority than Healthy (on) so a
            // user who turned auto-sync off still sees that warning when
            // the line would otherwise drop it.
            let (priority, severity) = if enabled {
                (4, crate::tui::probe::Severity::Healthy)
            } else {
                (2, crate::tui::probe::Severity::Stale)
            };
            entries.push(StatusEntry {
                priority,
                spans: vec![
                    Span::raw("  "),
                    severity_span(&format!("auto-sync: {label}"), severity, self.theme),
                ],
            });
        }
        entries.push(StatusEntry {
            priority: 2,
            spans: vec![
                Span::raw("  "),
                severity_span(
                    &format!("lock: {}", self.vm.lock_label),
                    self.vm.lock_severity,
                    self.theme,
                ),
            ],
        });
        entries.push(StatusEntry {
            priority: 3,
            spans: vec![
                Span::raw("  "),
                severity_span(
                    &format!("integrations: {}", self.vm.mcp_label),
                    self.vm.mcp_severity,
                    self.theme,
                ),
            ],
        });

        // Inner width accounts for the two side borders only; the spinner
        // lives on the previous row, not this one.
        let inner_width = area.width.saturating_sub(2) as usize;
        let second_line: Vec<Span<'static>> = fit_status_entries(entries, inner_width);

        let lines = vec![
            Line::from(vec![
                status_span,
                Span::styled(self.vm.repo_display.clone(), self.theme.muted_style()),
            ]),
            Line::from(second_line),
        ];
        // Wrap is intentionally off: at narrow widths, low-priority entries
        // are dropped instead, so the header always renders in exactly two
        // content rows + borders.
        Paragraph::new(lines).block(block).render(area, buf);
    }
}

/// Drop entries by descending priority until the total fits within `width`.
fn fit_status_entries(entries: Vec<StatusEntry>, width: usize) -> Vec<Span<'static>> {
    let total: usize = entries.iter().map(StatusEntry::width).sum();
    let mut kept: Vec<bool> = vec![true; entries.len()];
    if total > width {
        let mut order: Vec<usize> = (0..entries.len())
            .filter(|i| entries[*i].priority > 0)
            .collect();
        order.sort_by(|a, b| {
            entries[*b]
                .priority
                .cmp(&entries[*a].priority)
                .then(b.cmp(a))
        });
        let mut running = total;
        for i in order {
            if running <= width {
                break;
            }
            kept[i] = false;
            running -= entries[i].width();
        }
    }
    entries
        .into_iter()
        .zip(kept)
        .filter(|(_, keep)| *keep)
        .flat_map(|(e, _)| e.spans)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_glyph_wraps_on_frame_overflow() {
        let first = spinner_glyph(0);
        let same = spinner_glyph(SPINNER_FRAMES.len() as u32);
        assert_eq!(
            first,
            same,
            "spinner must wrap back to frame 0 after {} ticks",
            SPINNER_FRAMES.len()
        );
    }

    #[test]
    fn spinner_glyph_advances_between_adjacent_frames() {
        let a = spinner_glyph(0);
        let b = spinner_glyph(1);
        assert_ne!(a, b, "first two frames should differ");
    }

    #[test]
    fn dedupe_watch_pid_strips_pid_when_lock_owns_same() {
        let out = dedupe_watch_pid("daemon (pid 72107)", "held by pid 72107");
        assert_eq!(out, "daemon");
    }

    #[test]
    fn dedupe_watch_pid_keeps_pid_when_lock_is_free() {
        let out = dedupe_watch_pid("daemon (pid 72107)", "free");
        assert_eq!(out, "daemon (pid 72107)");
    }

    #[test]
    fn dedupe_watch_pid_keeps_pid_when_pids_differ() {
        let out = dedupe_watch_pid("daemon (pid 72107)", "held by pid 99999");
        assert_eq!(out, "daemon (pid 72107)");
    }

    #[test]
    fn dedupe_watch_pid_passthrough_when_no_pid_in_watch() {
        let out = dedupe_watch_pid("starting", "free");
        assert_eq!(out, "starting");
    }

    #[test]
    fn fit_status_entries_drops_lowest_priority_first() {
        let mode = StatusEntry {
            priority: 5,
            spans: vec![Span::raw("mode: auto")],
        };
        let reconcile = StatusEntry {
            priority: 0,
            spans: vec![Span::raw("reconcile: current")],
        };
        let watch = StatusEntry {
            priority: 1,
            spans: vec![Span::raw("watch: daemon")],
        };
        let total = mode.width() + reconcile.width() + watch.width();
        let kept = fit_status_entries(vec![mode, reconcile, watch], total - 5);
        let text: String = kept.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("mode:"), "mode should drop first: {text:?}");
        assert!(text.contains("reconcile:"));
        assert!(text.contains("watch:"));
    }

    #[test]
    fn fit_status_entries_keeps_everything_when_it_fits() {
        let entries = vec![
            StatusEntry {
                priority: 5,
                spans: vec![Span::raw("a")],
            },
            StatusEntry {
                priority: 0,
                spans: vec![Span::raw("b")],
            },
        ];
        let kept = fit_status_entries(entries, 80);
        let text: String = kept.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "ab");
    }
}
