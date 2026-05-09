//! Explain tab: provider/totals/commentary status plus an action menu and
//! inline queued-work preview.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::pipeline::explain::{ExplainPreviewGroup, ExplainStatus};
use crate::surface::status_snapshot::StatusSnapshot;
use crate::tui::app::{
    ConfirmStopWatchState, ExplainPreviewPanel, ExplainPreviewState, FolderPickerState,
    GenerateCommentaryState,
};
use crate::tui::theme::Theme;
use crate::tui::widgets::confirm_stop_watch::render_confirm_stop_watch;
use crate::tui::widgets::explain_generate::render_generate_commentary;

/// Explain tab widget. Branches on `ExplainStatus` to render either the
/// empty-state onboarding hint or the configured status + action menu. When
/// `picker` is `Some`, the folder-picker sub-view replaces the main body. When
/// `confirm_stop_watch` is `Some`, a blocking confirm prompt replaces the main
/// body and preempts the picker.
pub struct ExplainTabWidget<'a> {
    /// Current status snapshot.
    pub snapshot: &'a StatusSnapshot,
    /// Active folder-picker state, when the sub-view is open.
    pub picker: Option<&'a FolderPickerState>,
    /// Active explicit-generate modal state, when the sub-view is open.
    pub generate_commentary: Option<&'a GenerateCommentaryState>,
    /// Active confirm-stop-watch modal state, when the sub-view is open.
    pub confirm_stop_watch: Option<&'a ConfirmStopWatchState>,
    /// Cached queued-work preview for the tab.
    pub preview_panel: Option<&'a ExplainPreviewPanel>,
    /// Active theme.
    pub theme: &'a Theme,
}

impl Widget for ExplainTabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" explain ")
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());

        let lines = if let Some(confirm) = self.confirm_stop_watch {
            render_confirm_stop_watch(confirm, self.theme)
        } else if let Some(generate) = self.generate_commentary {
            render_generate_commentary(generate, self.theme)
        } else if let Some(picker) = self.picker {
            render_folder_picker(picker, self.theme)
        } else {
            let status = self.snapshot.explain_provider.as_ref().map(|d| &d.status);
            match status {
                Some(ExplainStatus::Enabled) => {
                    render_configured(self.snapshot, self.preview_panel, self.theme)
                }
                Some(ExplainStatus::DisabledKeyDetected { env_var }) => {
                    render_not_configured(Some(env_var), self.preview_panel, self.theme)
                }
                Some(ExplainStatus::Disabled) | None => {
                    render_not_configured(None, self.preview_panel, self.theme)
                }
            }
        };

        let items: Vec<ListItem> = lines.into_iter().map(ListItem::new).collect();
        List::new(items)
            .block(block)
            .style(self.theme.base_style())
            .render(area, buf);
    }
}

fn render_configured(
    snapshot: &StatusSnapshot,
    preview_panel: Option<&ExplainPreviewPanel>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let display = snapshot
        .explain_provider
        .as_ref()
        .expect("configured path requires explain_provider");
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

    let totals_text = match &snapshot.explain_totals {
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
        .explain_totals
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
    append_preview_panel(&mut lines, preview_panel, theme, false);

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Actions:", theme.muted_style())));
    lines.push(Line::from(Span::styled(
        "  Running a / c / f / g opens an in-dashboard Explain progress view.".to_string(),
        theme.muted_style(),
    )));
    lines.push(action_line("r", "Refresh Explain status", theme));
    lines.push(action_line(
        "a",
        "Generate/refresh all stale commentary",
        theme,
    ));
    lines.push(action_line(
        "f",
        "Generate/refresh specific folders...",
        theme,
    ));
    lines.push(action_line(
        "c",
        "Generate/refresh changed files only (last 50 commits)",
        theme,
    ));
    lines.push(action_line(
        "g",
        "Generate commentary for target/file/directory...",
        theme,
    ));
    lines.push(action_line(
        "d",
        "Export docs from overlay, no model calls",
        theme,
    ));
    lines.push(action_line("D", "Force rebuild docs and docs index", theme));
    lines.push(action_line(
        "x",
        "Preview clean of exported docs/index",
        theme,
    ));
    lines.push(action_line(
        "X",
        "Clean exported docs/index, overlay untouched",
        theme,
    ));
    lines.push(action_line("s", "Re-run optional explain setup", theme));
    lines
}

fn append_preview_panel(
    lines: &mut Vec<Line<'static>>,
    preview_panel: Option<&ExplainPreviewPanel>,
    theme: &Theme,
    disabled_mode: bool,
) {
    lines.push(Line::from(""));
    let heading = if disabled_mode {
        "Current backlog if you enable explain:"
    } else {
        "Preview if you run now:"
    };
    lines.push(Line::from(Span::styled(
        heading.to_string(),
        theme.muted_style(),
    )));

    let Some(preview_panel) = preview_panel else {
        lines.push(Line::from(Span::styled(
            "  Preview not loaded yet.".to_string(),
            theme.muted_style(),
        )));
        return;
    };

    append_preview_scope(
        lines,
        "[a] whole repo",
        &preview_panel.whole_repo,
        true,
        theme,
    );
    append_preview_scope(
        lines,
        "[c] recent changes",
        &preview_panel.changed,
        false,
        theme,
    );
}

fn append_preview_scope(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    state: &ExplainPreviewState,
    show_samples: bool,
    theme: &Theme,
) {
    match state {
        ExplainPreviewState::Ready(preview) => {
            lines.push(Line::from(vec![
                Span::styled(format!("  {label}: "), theme.agent_style()),
                Span::styled(preview.overlay_freshness_line.clone(), theme.base_style()),
            ]));
            lines.push(Line::from(Span::styled(
                format!(
                    "    queued {} stale, {} file(s), {} symbol(s), up to {} target(s)",
                    preview.refresh.total_count,
                    preview.file_seeds.total_count,
                    preview.symbol_seeds.total_count,
                    preview.max_target_count
                ),
                theme.base_style(),
            )));
            if show_samples && preview.max_target_count > 0 {
                append_group_sample(lines, &preview.refresh, theme);
                append_group_sample(lines, &preview.file_seeds, theme);
                append_group_sample(lines, &preview.symbol_seeds, theme);
            } else {
                lines.push(Line::from(Span::styled(
                    format!("    {}", preview.summary_line),
                    theme.muted_style(),
                )));
            }
        }
        ExplainPreviewState::Unavailable(message) => {
            lines.push(Line::from(vec![
                Span::styled(format!("  {label}: "), theme.agent_style()),
                Span::styled(format!("unavailable ({message})"), theme.stale_style()),
            ]));
        }
    }
}

fn append_group_sample(lines: &mut Vec<Line<'static>>, group: &ExplainPreviewGroup, theme: &Theme) {
    if group.total_count == 0 {
        return;
    }
    if let Some(first) = group.items.first() {
        let suffix = if group.remaining_count > 0 {
            format!(" (+{} more)", group.remaining_count)
        } else {
            String::new()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("    {}: ", compact_group_label(group.label)),
                theme.muted_style(),
            ),
            Span::styled(format!("{first}{suffix}"), theme.base_style()),
        ]));
    }
}

fn compact_group_label(label: &str) -> &'static str {
    match label {
        "stale commentary to refresh" => "stale",
        "files missing commentary" => "files",
        "symbols missing commentary" => "symbols",
        _ => "targets",
    }
}

fn render_folder_picker(picker: &FolderPickerState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "Generate/refresh explain for which folders?".to_string(),
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

fn render_not_configured(
    env_hint: Option<&'static str>,
    preview_panel: Option<&ExplainPreviewPanel>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "Optional explain is off.".to_string(),
            theme.stale_style(),
        )),
        Line::from(Span::styled(
            "  Commentary, cross-link triage, and refresh-on-stale are inert until a".to_string(),
            theme.muted_style(),
        )),
        Line::from(Span::styled(
            "  provider is selected. synrepo never auto-enables explain even when".to_string(),
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
    append_preview_panel(&mut lines, preview_panel, theme, true);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Actions:".to_string(),
        theme.muted_style(),
    )));
    lines.push(action_line("s", "Configure optional explain", theme));
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
