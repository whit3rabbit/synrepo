//! Synthesis tab: provider/totals/commentary status plus an action menu.
//!
//! Driven entirely by `StatusSnapshot`: no live overlay probe happens from the
//! render path. Commentary staleness comes from `status_snapshot.commentary_coverage`
//! when a full scan was performed, falling back to "N entries" when only the
//! cheap count is available.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::pipeline::synthesis::providers::SynthesisStatus;
use crate::surface::status_snapshot::StatusSnapshot;
use crate::tui::app::{describe_pending_mode, ConfirmStopWatchState, FolderPickerState};
use crate::tui::theme::Theme;

/// Synthesis tab widget. Branches on `SynthesisStatus` to render either the
/// empty-state onboarding hint or the configured status + action menu. When
/// `picker` is `Some`, the folder-picker sub-view replaces the main body. When
/// `confirm_stop_watch` is `Some`, a blocking confirm prompt replaces the main
/// body and preempts the picker.
pub struct SynthesisTabWidget<'a> {
    /// Current status snapshot.
    pub snapshot: &'a StatusSnapshot,
    /// Active folder-picker state, when the sub-view is open.
    pub picker: Option<&'a FolderPickerState>,
    /// Active confirm-stop-watch modal state, when the sub-view is open.
    pub confirm_stop_watch: Option<&'a ConfirmStopWatchState>,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for SynthesisTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" synthesis ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());

        let lines = if let Some(confirm) = self.confirm_stop_watch {
            render_confirm_stop_watch(confirm, self.theme)
        } else if let Some(picker) = self.picker {
            render_folder_picker(picker, self.theme)
        } else {
            let status = self.snapshot.synthesis_provider.as_ref().map(|d| &d.status);
            match status {
                Some(SynthesisStatus::Enabled) => render_configured(self.snapshot, self.theme),
                Some(SynthesisStatus::DisabledKeyDetected { env_var }) => {
                    render_not_configured(Some(env_var), self.theme)
                }
                Some(SynthesisStatus::Disabled) | None => render_not_configured(None, self.theme),
            }
        };

        let items: Vec<ListItem> = lines.into_iter().map(ListItem::new).collect();
        List::new(items)
            .block(block)
            .style(self.theme.base_style())
            .render(area, buf);
    }
}

fn render_configured(snapshot: &StatusSnapshot, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let display = snapshot
        .synthesis_provider
        .as_ref()
        .expect("configured path requires synthesis_provider");
    let provider_line = format!(
        "{}{}",
        display.provider,
        display
            .model
            .as_ref()
            .map(|m| format!(" / {m}"))
            .unwrap_or_default()
    );
    lines.push(label_value("Provider", provider_line, theme));

    if let Some(endpoint) = &display.local_endpoint {
        lines.push(label_value("Endpoint", endpoint.clone(), theme));
    }

    let totals_text = match &snapshot.synthesis_totals {
        Some(totals) => {
            let est = if totals.any_estimated { " est." } else { "" };
            format!(
                "{} calls  ·  ${:.2}  ·  {} in / {} out{}",
                totals.calls, totals.usd_cost, totals.input_tokens, totals.output_tokens, est
            )
        }
        None => "no calls recorded yet".to_string(),
    };
    lines.push(label_value("Totals", totals_text, theme));

    if let Some(last) = snapshot
        .synthesis_totals
        .as_ref()
        .and_then(|t| t.updated_at.clone())
    {
        lines.push(label_value("Last run", last, theme));
    }

    lines.push(label_value(
        "Commentary",
        snapshot.commentary_coverage.display.clone(),
        theme,
    ));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Actions:", theme.muted_style())));
    lines.push(action_line("r", "Refresh all stale commentary", theme));
    lines.push(action_line("f", "Refresh specific folders...", theme));
    lines.push(action_line(
        "c",
        "Refresh changed files only (last 50 commits)",
        theme,
    ));
    lines.push(action_line("s", "Re-run synthesis setup", theme));
    lines
}

fn render_folder_picker(picker: &FolderPickerState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "Refresh synthesis for which folders?".to_string(),
            theme.base_style(),
        )),
        Line::from(Span::styled(
            "  Toggle with Space. Enter to run. Esc to cancel.".to_string(),
            theme.muted_style(),
        )),
        Line::from(""),
    ];
    if picker.folders.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no top-level folders found)".to_string(),
            theme.muted_style(),
        )));
        return lines;
    }
    for (idx, entry) in picker.folders.iter().enumerate() {
        let cursor_marker = if idx == picker.cursor { ">" } else { " " };
        let check = if entry.checked { "[x]" } else { "[ ]" };
        let path = entry.path.clone();
        let count = format!(
            "({} indexable{})",
            entry.indexable_count,
            if entry.supported_count > 0 {
                format!(", {} parser-supported", entry.supported_count)
            } else {
                String::new()
            }
        );
        let path_style = if idx == picker.cursor {
            theme.agent_style()
        } else {
            theme.base_style()
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {cursor_marker} "), theme.muted_style()),
            Span::styled(format!("{check} "), theme.base_style()),
            Span::styled(format!("{path:<24}"), path_style),
            Span::styled(format!(" {count}"), theme.muted_style()),
        ]));
    }
    lines
}

fn render_confirm_stop_watch(confirm: &ConfirmStopWatchState, theme: &Theme) -> Vec<Line<'static>> {
    let scope = describe_pending_mode(&confirm.pending_mode);
    vec![
        Line::from(Span::styled(
            "Watch service is active.".to_string(),
            theme.stale_style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Synthesis needs the writer lock, which watch currently holds.".to_string(),
            theme.muted_style(),
        )),
        Line::from(Span::styled(
            "  Stop watch to run synthesis; restart it later with `synrepo watch`.".to_string(),
            theme.muted_style(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Scope: ".to_string(), theme.muted_style()),
            Span::styled(scope, theme.base_style()),
        ]),
        Line::from(""),
        action_line("y", "Stop watch and run synthesis", theme),
        action_line("n", "Cancel", theme),
    ]
}

fn render_not_configured(env_hint: Option<&'static str>, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "Synthesis is off.".to_string(),
            theme.stale_style(),
        )),
        Line::from(Span::styled(
            "  Commentary, cross-link triage, and refresh-on-stale are inert until a".to_string(),
            theme.muted_style(),
        )),
        Line::from(Span::styled(
            "  provider is selected. synrepo never auto-enables synthesis even when".to_string(),
            theme.muted_style(),
        )),
        Line::from(Span::styled(
            "  provider API keys are present in the environment.".to_string(),
            theme.muted_style(),
        )),
    ];
    if let Some(var) = env_hint {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Detected ${var} in the environment. Run setup to opt in."),
            theme.agent_style(),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Actions:".to_string(),
        theme.muted_style(),
    )));
    lines.push(action_line("s", "Configure synthesis", theme));
    lines
}

fn label_value(label: &str, value: String, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<14}", format!("{label}:")), theme.muted_style()),
        Span::styled(value, theme.base_style()),
    ])
}

fn action_line(key: &str, label: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled("  [".to_string(), theme.muted_style()),
        Span::styled(key.to_string(), theme.agent_style()),
        Span::styled("] ".to_string(), theme.muted_style()),
        Span::styled(label.to_string(), theme.base_style()),
    ])
}
