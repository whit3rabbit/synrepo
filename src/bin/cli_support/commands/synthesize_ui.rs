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
    fit_value, phase_summary_label, progress_label, render_target, scan_progress_label,
    scan_work_label, start_label, success_label, work_found_label,
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
    let (actions_taken, tracker) = result?;
    // Redraw with final state so the user can see the result before we exit.
    ui.set_finished_prompt();
    let _ = ui.draw(&mut terminal, &tracker, stop.requested());
    // Block until any keypress so the finished state is visible.
    let _ = crossterm::event::read();
    let leave_result = leave_tui(&mut terminal);
    leave_result?;
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
    scoped_files: usize,
    scoped_symbols: usize,
    file_scan: (usize, usize),
    symbol_scan: (usize, usize),
    max_targets: usize,
    attempted: usize,
    step: String,
    current: String,
    finished: bool,
    finished_prompt: bool,
    recent: VecDeque<String>,
}

impl SynthesisProgressUi {
    fn new(context: &SynthesizeRunContext, theme: Theme) -> Self {
        let active = describe_active_provider(&context.config);
        let provider = active.model.map_or_else(
            || active.provider.to_string(),
            |model| format!("{} / {model}", active.provider),
        );
        let api_calls = if context.config.synthesis.enabled
            || matches!(std::env::var("SYNREPO_LLM_ENABLED").as_deref(), Ok("1"))
        {
            format!(
                "[{}] is only called for items that need new commentary",
                active.provider
            )
        } else {
            "provider calls are off, so synthesis can only reuse commentary already in .synrepo/"
                .to_string()
        };
        let output_files = "saves commentary in .synrepo/overlay/overlay.db; markdown docs exist only for symbol commentary".to_string();
        Self {
            theme,
            scope: context.scope_label(),
            provider,
            api_calls,
            output_files,
            planned_refresh: 0,
            planned_files: 0,
            planned_symbols: 0,
            scoped_files: 0,
            scoped_symbols: 0,
            file_scan: (0, 0),
            symbol_scan: (0, 0),
            max_targets: 0,
            attempted: 0,
            step: "1/4 Scan repository".to_string(),
            current: "Scanning the repository for files and symbols that need commentary."
                .to_string(),
            finished: false,
            finished_prompt: false,
            recent: VecDeque::new(),
        }
    }

    fn set_finished_prompt(&mut self) {
        self.finished_prompt = true;
    }

    fn apply_event(&mut self, event: CommentaryProgressEvent) {
        match event {
            CommentaryProgressEvent::ScanProgress {
                files_scanned,
                files_total,
                symbols_scanned,
                symbols_total,
            } => {
                self.file_scan = (files_scanned, files_total);
                self.symbol_scan = (symbols_scanned, symbols_total);
                self.current =
                    "Checking repository coverage before generating commentary.".to_string();
            }
            CommentaryProgressEvent::PlanReady {
                refresh,
                file_seeds,
                symbol_seed_candidates,
                scoped_files,
                scoped_symbols,
                max_targets,
            } => {
                self.planned_refresh = refresh;
                self.planned_files = file_seeds;
                self.planned_symbols = symbol_seed_candidates;
                self.scoped_files = scoped_files;
                self.scoped_symbols = scoped_symbols;
                self.max_targets = max_targets;
                self.step = if max_targets == 0 {
                    "2/4 Nothing to generate".to_string()
                } else {
                    "2/4 Generate commentary".to_string()
                };
                self.current = if max_targets == 0 {
                    "Repository scan complete. Everything in this scope already has commentary."
                        .to_string()
                } else {
                    format!("Repository scan complete. Found {max_targets} item(s) that need commentary.")
                };
                self.push_recent(work_found_label(
                    scoped_files,
                    scoped_symbols,
                    refresh,
                    file_seeds,
                    symbol_seed_candidates,
                ));
            }
            CommentaryProgressEvent::TargetStarted { item, current } => {
                self.attempted = current;
                self.current = format!("{} {}", start_label(&item), render_target(&item));
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
                    "Skipped"
                };
                self.current = format!("{verb} {}", render_target(&item));
                self.push_recent(self.current.clone());
            }
            CommentaryProgressEvent::DocsDirCreated { path }
            | CommentaryProgressEvent::IndexDirCreated { path } => {
                self.note_write_result(format!("Created {}", path.display()));
            }
            CommentaryProgressEvent::DocWritten { path } => {
                self.note_write_result(format!("Output file {}", path.display()));
            }
            CommentaryProgressEvent::DocDeleted { path } => {
                self.note_write_result(format!("Removed {}", path.display()));
            }
            CommentaryProgressEvent::IndexUpdated {
                path,
                touched_paths,
            } => {
                self.note_write_result(format!(
                    "Output index updated {} ({touched_paths} paths)",
                    path.display()
                ));
            }
            CommentaryProgressEvent::IndexRebuilt {
                path,
                touched_paths,
            } => {
                self.note_write_result(format!(
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
                self.push_recent(format!("{}: attempted {attempted}, generated {generated}, not generated {not_generated}", phase_summary_label(phase)));
            }
            CommentaryProgressEvent::RunSummary {
                attempted,
                stopped,
                refreshed,
                seeded,
                not_generated,
            } => {
                self.attempted = attempted;
                self.finished = true;
                self.step = "4/4 Done".to_string();
                self.current = if stopped {
                    "Stop requested. Finished writing the results already generated.".to_string()
                } else {
                    "Synthesis complete. Results are now available under .synrepo/.".to_string()
                };
                self.push_recent(format!("Finished: refreshed {refreshed}, generated {seeded}, not generated {not_generated}."));
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
                    Span::styled("Generation: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.api_calls, intro_value_width),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Results: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.output_files, intro_value_width),
                        self.theme.base_style(),
                    ),
                ]),
            ])
            .block(
                Block::default()
                    .title(if self.finished_prompt {
                        " synthesis complete [press any key to exit] "
                    } else if stop_requested {
                        " synthesis [stopping after current item] "
                    } else {
                        " synthesis [q/Esc stop after current item] "
                    })
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style()),
            )
            .style(self.theme.base_style());
            frame.render_widget(intro, layout[0]);

            let ratio = if self.scanning() {
                let total = self.file_scan.1 + self.symbol_scan.1;
                if total == 0 {
                    0.0
                } else {
                    (self.file_scan.0 + self.symbol_scan.0) as f64 / total as f64
                }
            } else if self.finished {
                1.0
            } else if self.max_targets == 0 {
                0.0
            } else {
                (self.attempted as f64 / self.max_targets as f64).clamp(0.0, 1.0)
            };
            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .title(" codebase progress ")
                        .borders(Borders::ALL)
                        .border_style(self.theme.border_style()),
                )
                .gauge_style(self.theme.watch_active_style())
                .label(if self.scanning() {
                    scan_progress_label(
                        self.file_scan.0,
                        self.file_scan.1,
                        self.symbol_scan.0,
                        self.symbol_scan.1,
                    )
                } else {
                    progress_label(
                        self.attempted,
                        self.max_targets,
                        self.finished,
                        self.scoped_files,
                        self.scoped_symbols,
                    )
                })
                .ratio(ratio);
            frame.render_widget(gauge, layout[1]);

            let scanning = self.scanning();
            let provider_activity = if scanning {
                "waiting for repository scan to finish".to_string()
            } else if self.max_targets == 0 {
                "no provider calls needed for this scope".to_string()
            } else {
                tracker.usage_label()
            };
            let work_found = if scanning {
                scan_work_label(
                    self.file_scan.0,
                    self.file_scan.1,
                    self.symbol_scan.0,
                    self.symbol_scan.1,
                )
            } else {
                work_found_label(
                    self.scoped_files,
                    self.scoped_symbols,
                    self.planned_refresh,
                    self.planned_files,
                    self.planned_symbols,
                )
            };
            let status = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Step: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&self.step, status_value_width),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Work: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&work_found, status_value_width),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Provider: ", self.theme.muted_style()),
                    Span::styled(
                        fit_value(&provider_activity, status_value_width),
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
                        .title(" recent updates ")
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

    fn scanning(&self) -> bool {
        self.step.starts_with("1/4")
    }

    fn enter_write_results(&mut self) {
        self.step = "3/4 Write results".to_string();
        self.current = "Writing docs and search index.".to_string();
    }

    fn note_write_result(&mut self, line: String) {
        self.enter_write_results();
        self.push_recent(line);
    }
}
