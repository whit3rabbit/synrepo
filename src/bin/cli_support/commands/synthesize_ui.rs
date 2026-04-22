use std::collections::VecDeque;
use std::io;
use std::path::Path;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use synrepo::pipeline::repair::CommentaryProgressEvent;
use synrepo::pipeline::synthesis::describe_active_provider;
use synrepo::tui::theme::Theme;

use super::synthesize::{execute_synthesize_run, write_actions_taken, SynthesizeRunContext};
use super::synthesize_progress::render_telemetry_summary;
use super::synthesize_tracker::TelemetryTracker;
use super::synthesize_ui_input::StopKeyState;
use super::synthesize_ui_terminal::{enter_tui, leave_tui, SynthesisTerminal};
use super::synthesize_ui_text::{
    fit_value, phase_summary_label, progress_label, provider_name, render_target, start_label,
    success_label,
};

pub(super) fn run_synthesize_tui(
    repo_root: &Path,
    context: SynthesizeRunContext,
) -> anyhow::Result<()> {
    let mut terminal = enter_tui()?;
    let theme = Theme::from_no_color(std::env::var_os("NO_COLOR").is_some());
    let mut ui = SynthesisProgressUi::new(&context, theme);
    let stop = StopKeyState::default();
    ui.draw(&mut terminal, &TelemetryTracker::empty(), stop.requested())?;
    let mut should_stop = || stop.poll();
    let result = execute_synthesize_run(
        repo_root,
        &context,
        |_, tracker, event| {
            ui.apply_event(event);
            let _ = stop.poll();
            let _ = ui.draw(&mut terminal, tracker, stop.requested());
        },
        Some(&mut should_stop),
    );
    let leave_result = leave_tui(&mut terminal);
    leave_result?;
    let (actions_taken, tracker) = result?;
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    render_telemetry_summary(&mut stderr, &tracker)?;
    write_actions_taken(&mut stdout, &actions_taken)
}

struct SynthesisProgressUi {
    theme: Theme,
    scope: String,
    provider: String,
    api_calls: String,
    output_files: String,
    planned_refresh: usize,
    planned_files: usize,
    planned_symbols: usize,
    max_targets: usize,
    attempted: usize,
    refreshed: usize,
    generated: usize,
    not_generated: usize,
    current: String,
    phase_summary: String,
    finished: bool,
    recent: VecDeque<String>,
}

impl SynthesisProgressUi {
    fn new(context: &SynthesizeRunContext, theme: Theme) -> Self {
        let active = describe_active_provider(&context.config);
        let provider = match active.model {
            Some(model) => format!("{} / {model}", active.provider),
            None => active.provider.to_string(),
        };
        let api_calls = if context.config.synthesis.enabled
            || matches!(std::env::var("SYNREPO_LLM_ENABLED").as_deref(), Ok("1"))
        {
            format!(
                "calls [{}] to write advisory commentary under .synrepo/, never your tracked source files",
                active.provider
            )
        } else {
            "provider requests are disabled, only existing overlay commentary can be reused"
                .to_string()
        };
        let output_files =
            "updates the overlay DB plus .synrepo/synthesis-docs/ and .synrepo/synthesis-index/"
                .to_string();
        Self {
            theme,
            scope: context.scope_label(),
            provider,
            api_calls,
            output_files,
            planned_refresh: 0,
            planned_files: 0,
            planned_symbols: 0,
            max_targets: 0,
            attempted: 0,
            refreshed: 0,
            generated: 0,
            not_generated: 0,
            current: "Planning commentary work under .synrepo/...".to_string(),
            phase_summary: String::new(),
            finished: false,
            recent: VecDeque::new(),
        }
    }

    fn apply_event(&mut self, event: CommentaryProgressEvent) {
        match event {
            CommentaryProgressEvent::PlanReady {
                refresh,
                file_seeds,
                symbol_seed_candidates,
                max_targets,
            } => {
                self.planned_refresh = refresh;
                self.planned_files = file_seeds;
                self.planned_symbols = symbol_seed_candidates;
                self.max_targets = max_targets;
                self.current =
                    "Calling the provider API and updating advisory commentary under .synrepo/."
                        .to_string();
                self.push_recent(format!(
                    "Planned {refresh} stale item(s), {file_seeds} file(s) without commentary, {symbol_seed_candidates} symbol candidate(s)."
                ));
            }
            CommentaryProgressEvent::TargetStarted { item, current } => {
                self.attempted = current;
                self.current = format!(
                    "[{} API] {}: {}",
                    provider_name(&self.provider),
                    start_label(&item),
                    render_target(&item)
                );
                self.push_recent(self.current.clone());
            }
            CommentaryProgressEvent::TargetFinished {
                item,
                current,
                generated,
            } => {
                self.attempted = current;
                let verb = if generated {
                    success_label(item.phase)
                } else {
                    "skipped"
                };
                self.current = format!(
                    "[{} API] {verb}: {}",
                    provider_name(&self.provider),
                    render_target(&item)
                );
                self.push_recent(self.current.clone());
            }
            CommentaryProgressEvent::DocsDirCreated { path } => {
                self.push_recent(format!("Created {}", path.display()));
            }
            CommentaryProgressEvent::DocWritten { path } => {
                self.push_recent(format!("Output file {}", path.display()));
            }
            CommentaryProgressEvent::DocDeleted { path } => {
                self.push_recent(format!("Removed {}", path.display()));
            }
            CommentaryProgressEvent::IndexDirCreated { path } => {
                self.push_recent(format!("Created {}", path.display()));
            }
            CommentaryProgressEvent::IndexUpdated {
                path,
                touched_paths,
            } => {
                self.push_recent(format!(
                    "Output index updated {} ({touched_paths} paths)",
                    path.display()
                ));
            }
            CommentaryProgressEvent::IndexRebuilt {
                path,
                touched_paths,
            } => {
                self.push_recent(format!(
                    "Output index rebuilt {} ({touched_paths} paths)",
                    path.display()
                ));
            }
            CommentaryProgressEvent::PhaseSummary {
                phase,
                attempted,
                generated,
                not_generated,
            } => {
                self.phase_summary = format!(
                    "{}: attempted {attempted}, generated {generated}, not generated {not_generated}",
                    phase_summary_label(phase)
                );
                self.push_recent(self.phase_summary.clone());
            }
            CommentaryProgressEvent::RunSummary {
                refreshed,
                seeded,
                not_generated,
                attempted,
                stopped,
            } => {
                self.refreshed = refreshed;
                self.generated = seeded;
                self.not_generated = not_generated;
                self.attempted = attempted;
                self.finished = true;
                self.current = if stopped {
                    "Stop requested, finalizing overlay docs and index output.".to_string()
                } else {
                    "Synthesis finished. Finalizing overlay docs and index output.".to_string()
                };
                self.push_recent(format!(
                    "Finished: refreshed {refreshed}, generated {seeded}, not generated {not_generated}."
                ));
            }
        }
    }

    fn draw(
        &self,
        terminal: &mut SynthesisTerminal,
        tracker: &TelemetryTracker,
        stop_requested: bool,
    ) -> anyhow::Result<()> {
        terminal.draw(|frame| {
            let area = frame.area();
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(6),
                    Constraint::Length(3),
                    Constraint::Length(6),
                    Constraint::Min(0),
                ])
                .split(area);
            let intro_value_width = layout[0].width.saturating_sub(14) as usize;
            let status_value_width = layout[2].width.saturating_sub(10) as usize;
            let recent_width = layout[3].width.saturating_sub(2) as usize;

            let intro = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Scope: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.scope, intro_value_width),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Provider: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.provider, intro_value_width),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("API calls: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.api_calls, intro_value_width),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Output files: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.output_files, intro_value_width),
                        self.theme.base_style(),
                    ),
                ]),
            ])
            .block(
                Block::default()
                    .title(if stop_requested {
                        " synthesis [stopping after current item] "
                    } else {
                        " synthesis [q/Esc stop after current item] "
                    })
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style()),
            )
            .style(self.theme.base_style());
            frame.render_widget(intro, layout[0]);

            let ratio = if self.finished {
                1.0
            } else if self.max_targets == 0 {
                0.0
            } else {
                (self.attempted as f64 / self.max_targets as f64).clamp(0.0, 1.0)
            };
            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .title(" progress ")
                        .borders(Borders::ALL)
                        .border_style(self.theme.border_style()),
                )
                .gauge_style(self.theme.watch_active_style())
                .label(progress_label(
                    self.attempted,
                    self.max_targets,
                    self.finished,
                ))
                .ratio(ratio);
            frame.render_widget(gauge, layout[1]);

            let usage = tracker.usage_label();
            let status = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Plan: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(
                            &format!(
                            "{} stale, {} files missing commentary, {} symbols missing commentary",
                            self.planned_refresh, self.planned_files, self.planned_symbols
                            ),
                            status_value_width,
                        ),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Current: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.current, status_value_width),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Usage: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&usage, status_value_width),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Latest: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.phase_summary, status_value_width),
                        self.theme.base_style(),
                    ),
                ]),
            ])
            .block(
                Block::default()
                    .title(" status ")
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style()),
            )
            .style(self.theme.base_style());
            frame.render_widget(status, layout[2]);

            let items: Vec<ListItem> = self
                .recent
                .iter()
                .map(|line| ListItem::new(Line::from(fit_value(line, recent_width))))
                .collect();
            let recent = List::new(items)
                .block(
                    Block::default()
                        .title(" recent activity ")
                        .borders(Borders::ALL)
                        .border_style(self.theme.border_style()),
                )
                .style(self.theme.base_style());
            frame.render_widget(recent, layout[3]);
        })?;
        Ok(())
    }

    fn push_recent(&mut self, line: String) {
        if self.recent.len() == 10 {
            self.recent.pop_front();
        }
        self.recent.push_back(line);
    }
}
