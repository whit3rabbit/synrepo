use std::collections::VecDeque;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, LineGauge, List, ListItem, Paragraph};

use crate::pipeline::repair::{CommentaryProgressEvent, CommentaryWorkItem};
use crate::tui::dashboard::DashboardTerminal;

use super::ExplainRunContext;

pub(super) struct ExplainRunUi {
    scope: String,
    provider: String,
    api_line: String,
    step: String,
    current: String,
    file_scan: (usize, usize),
    symbol_scan: (usize, usize),
    max_targets: usize,
    attempted: usize,
    generated: usize,
    skipped: usize,
    frame: usize,
    finished: bool,
    pub(super) finished_prompt: bool,
    stop_requested: bool,
    error: Option<String>,
    recent: VecDeque<String>,
}

impl ExplainRunUi {
    pub(super) fn new(context: &ExplainRunContext) -> Self {
        Self {
            scope: context.scope_label(),
            provider: context.provider_label(),
            api_line: context.api_line(),
            step: "Stage 1 of 4: Scan repository".to_string(),
            current: "Scanning files and symbols that need commentary.".to_string(),
            file_scan: (0, 0),
            symbol_scan: (0, 0),
            max_targets: 0,
            attempted: 0,
            generated: 0,
            skipped: 0,
            frame: 0,
            finished: false,
            finished_prompt: false,
            stop_requested: false,
            error: None,
            recent: VecDeque::new(),
        }
    }

    pub(super) fn message(title: &str, message: &str) -> Self {
        Self {
            scope: "recent changes".to_string(),
            provider: "none".to_string(),
            api_line: "no provider calls".to_string(),
            step: title.to_string(),
            current: message.to_string(),
            file_scan: (0, 0),
            symbol_scan: (0, 0),
            max_targets: 0,
            attempted: 0,
            generated: 0,
            skipped: 0,
            frame: 0,
            finished: true,
            finished_prompt: false,
            stop_requested: false,
            error: None,
            recent: VecDeque::new(),
        }
    }

    pub(super) fn error(message: String) -> Self {
        let mut ui = Self::message("Explain failed", &message);
        ui.error = Some(message);
        ui
    }

    pub(super) fn apply_event(&mut self, event: CommentaryProgressEvent) {
        match event {
            CommentaryProgressEvent::ScanProgress {
                files_scanned,
                files_total,
                symbols_scanned,
                symbols_total,
            } => {
                self.file_scan = (files_scanned, files_total);
                self.symbol_scan = (symbols_scanned, symbols_total);
                self.current = "Checking commentary freshness.".to_string();
            }
            CommentaryProgressEvent::PlanReady {
                refresh,
                file_seeds,
                symbol_seed_candidates,
                max_targets,
                ..
            } => {
                self.max_targets = max_targets;
                self.step = if max_targets == 0 {
                    "Stage 2 of 4: Nothing to generate".to_string()
                } else {
                    "Stage 2 of 4: Generate commentary".to_string()
                };
                self.current = format!(
                    "{max_targets} candidate target(s): {refresh} stale, {file_seeds} files missing, {symbol_seed_candidates} symbols missing."
                );
                self.push_recent(self.current.clone());
            }
            CommentaryProgressEvent::TargetStarted { item, current } => {
                self.attempted = current;
                let target = target_label(&item);
                self.current = format!("Generating commentary for {target}");
                self.push_recent(self.current.clone());
            }
            CommentaryProgressEvent::TargetFinished {
                item,
                current,
                generated,
                skip_message,
                retry_attempts,
                queued_for_next_run,
                ..
            } => {
                self.attempted = current;
                self.generated += usize::from(generated);
                self.skipped += usize::from(!generated);
                let target = target_label(&item);
                self.current = target_finished_line(
                    generated,
                    &target,
                    skip_message.as_deref(),
                    retry_attempts,
                    queued_for_next_run,
                );
                self.push_recent(self.current.clone());
            }
            CommentaryProgressEvent::DocsDirCreated { path }
            | CommentaryProgressEvent::IndexDirCreated { path } => {
                self.step = "Stage 3 of 4: Export docs".to_string();
                self.push_recent(format!("Created {}", path.display()));
            }
            CommentaryProgressEvent::DocWritten { path } => {
                self.step = "Stage 3 of 4: Export docs".to_string();
                self.push_recent(format!("Wrote {}", path.display()));
            }
            CommentaryProgressEvent::DocDeleted { path } => {
                self.step = "Stage 3 of 4: Export docs".to_string();
                self.push_recent(format!("Removed {}", path.display()));
            }
            CommentaryProgressEvent::IndexUpdated { path, .. }
            | CommentaryProgressEvent::IndexRebuilt { path, .. } => {
                self.step = "Stage 3 of 4: Export docs".to_string();
                self.push_recent(format!("Updated {}", path.display()));
            }
            CommentaryProgressEvent::PhaseSummary {
                phase,
                attempted,
                generated,
                not_generated,
            } => {
                self.push_recent(format!(
                    "{phase:?}: attempted {attempted}, generated {generated}, not generated {not_generated}"
                ));
            }
            CommentaryProgressEvent::RunSummary {
                attempted,
                stopped,
                refreshed,
                seeded,
                not_generated,
                queued_for_next_run,
                skip_reasons,
            } => {
                self.attempted = attempted;
                self.generated = refreshed + seeded;
                self.skipped = not_generated;
                self.finished = true;
                self.step = "Stage 4 of 4: Done".to_string();
                self.current = if stopped {
                    "Stop requested. Wrote completed commentary before returning.".to_string()
                } else if queued_for_next_run > 0 {
                    format!(
                        "Rate limit hit. {queued_for_next_run} target(s) remain queued for the next explain run."
                    )
                } else {
                    "Explain complete. Commentary docs were exported to .synrepo/explain-docs."
                        .to_string()
                };
                self.push_recent(format!(
                    "Finished: refreshed {refreshed}, generated {seeded}, not generated {not_generated}{}.",
                    reason_suffix(&skip_reasons)
                ));
            }
        }
    }

    pub(super) fn mark_finished(&mut self) {
        self.finished = true;
        self.step = "Stage 4 of 4: Done".to_string();
    }

    pub(super) fn mark_error(&mut self, message: String) {
        self.finished = true;
        self.step = "Explain failed".to_string();
        self.current = message.clone();
        self.error = Some(message);
    }

    pub(super) fn mark_stop_requested(&mut self) {
        if self.stop_requested {
            return;
        }
        self.stop_requested = true;
        self.push_recent(
            "Stop requested. Will halt after the in-flight provider call returns.".to_string(),
        );
    }

    pub(super) fn push_recent(&mut self, line: String) {
        if self.recent.len() >= 8 {
            self.recent.pop_front();
        }
        self.recent.push_back(line);
    }

    pub(super) fn tick(&mut self) {
        if !self.finished {
            self.frame = self.frame.wrapping_add(1);
        }
    }

    fn current_line(&self) -> String {
        if self.finished || self.error.is_some() {
            return self.current.clone();
        }
        const FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
        format!("[{}] {}", FRAMES[self.frame % FRAMES.len()], self.current)
    }
}

fn target_finished_line(
    generated: bool,
    path: &str,
    skip_message: Option<&str>,
    retry_attempts: usize,
    queued_for_next_run: bool,
) -> String {
    if generated {
        return format!("Generated {path}");
    }
    let retry = if retry_attempts > 0 {
        format!(" after {retry_attempts} retry")
    } else {
        String::new()
    };
    let queued = if queued_for_next_run { " (queued)" } else { "" };
    match skip_message {
        Some(message) => format!("Skipped {path}{retry}: {message}{queued}"),
        None => format!("Skipped {path}{retry}{queued}"),
    }
}

fn target_label(item: &CommentaryWorkItem) -> String {
    match &item.qualified_name {
        Some(name) => format!("{}::{name}", item.path),
        None => item.path.clone(),
    }
}

fn reason_suffix(skip_reasons: &[(String, usize)]) -> String {
    if skip_reasons.is_empty() {
        return String::new();
    }
    let joined = skip_reasons
        .iter()
        .map(|(reason, count)| format!("{reason}={count}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" ({joined})")
}

#[cfg(test)]
mod tests;

pub(super) fn draw_progress(
    terminal: &mut DashboardTerminal,
    ui: &ExplainRunUi,
) -> anyhow::Result<()> {
    terminal.draw(|frame| {
        let area = frame.area();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        let summary = vec![
            Line::from(vec![Span::raw("Scope: "), Span::raw(ui.scope.clone())]),
            Line::from(vec![
                Span::raw("Provider: "),
                Span::raw(ui.provider.clone()),
            ]),
            Line::from(vec![Span::raw("API: "), Span::raw(ui.api_line.clone())]),
            Line::from(vec![Span::raw("Current: "), Span::raw(ui.current_line())]),
        ];
        frame.render_widget(
            Paragraph::new(summary)
                .block(Block::default().title(" explain ").borders(Borders::ALL)),
            layout[0],
        );

        let ratio = if ui.max_targets == 0 {
            0.0
        } else {
            (ui.attempted as f64 / ui.max_targets as f64).clamp(0.0, 1.0)
        };
        let progress_block = Block::default()
            .title(ui.step.as_str())
            .borders(Borders::ALL);
        let progress_inner = progress_block.inner(layout[1]);
        frame.render_widget(progress_block, layout[1]);
        let progress_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(12), Constraint::Length(46)])
            .split(progress_inner);
        frame.render_widget(
            LineGauge::default()
                .filled_symbol(symbols::line::THICK_HORIZONTAL)
                .unfilled_symbol(" ")
                .label("")
                .ratio(ratio),
            progress_layout[0],
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{} / {} attempted, {} generated, {} skipped",
                ui.attempted, ui.max_targets, ui.generated, ui.skipped
            )),
            progress_layout[1],
        );

        let stop_line = if ui.stop_requested {
            "Stop requested. Waiting for the current provider call to return..."
        } else {
            "Press Esc, q, or Ctrl-C to request stop."
        };
        let scans = vec![
            Line::from(format!(
                "Files scanned: {} / {}",
                ui.file_scan.0, ui.file_scan.1
            )),
            Line::from(format!(
                "Symbols scanned: {} / {}",
                ui.symbol_scan.0, ui.symbol_scan.1
            )),
            Line::from(stop_line),
        ];
        frame.render_widget(
            Paragraph::new(scans).block(Block::default().borders(Borders::ALL)),
            layout[2],
        );

        let recent: Vec<ListItem> = ui
            .recent
            .iter()
            .map(|line| ListItem::new(Line::from(line.clone())))
            .collect();
        frame.render_widget(
            List::new(recent).block(Block::default().title(" recent ").borders(Borders::ALL)),
            layout[3],
        );

        let footer = if ui.finished_prompt {
            "Press any key to return to the dashboard."
        } else if ui.finished {
            "Returning to the dashboard..."
        } else if ui.error.is_some() {
            "Explain failed."
        } else if ui.stop_requested {
            "Stop requested. Finishing the in-flight provider call..."
        } else {
            "Explain is running."
        };
        frame.render_widget(Paragraph::new(footer), layout[4]);
    })?;
    Ok(())
}
