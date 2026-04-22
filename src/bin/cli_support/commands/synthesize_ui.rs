use std::collections::VecDeque;
use std::io::{self, Stdout};
use std::path::Path;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use synrepo::pipeline::repair::{CommentaryProgressEvent, CommentaryWorkItem, CommentaryWorkPhase};
use synrepo::pipeline::synthesis::describe_active_provider;
use synrepo::tui::theme::Theme;

use super::synthesize::{SynthesizeRunContext, execute_synthesize_run, write_actions_taken};
use super::synthesize_progress::render_telemetry_summary;
use super::synthesize_tracker::TelemetryTracker;

type SynthesisTerminal = Terminal<CrosstermBackend<Stdout>>;

pub(super) fn run_synthesize_tui(
    repo_root: &Path,
    context: SynthesizeRunContext,
) -> anyhow::Result<()> {
    let mut terminal = enter_tui()?;
    let theme = Theme::from_no_color(std::env::var_os("NO_COLOR").is_some());
    let mut ui = SynthesisProgressUi::new(&context, theme);
    ui.draw(&mut terminal, &TelemetryTracker::empty())?;
    let result = execute_synthesize_run(repo_root, &context, |_, tracker, event| {
        ui.apply_event(event);
        let _ = ui.draw(&mut terminal, tracker);
    });
    let leave_result = leave_tui(&mut terminal);
    leave_result?;
    let (actions_taken, tracker) = result?;
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    render_telemetry_summary(&mut stderr, &tracker)?;
    write_actions_taken(&mut stdout, &actions_taken)
}

fn enter_tui() -> anyhow::Result<SynthesisTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn leave_tui(terminal: &mut SynthesisTerminal) -> anyhow::Result<()> {
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    Ok(())
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
                "sends commentary requests to [{}], which may cost money depending on provider billing",
                active.provider
            )
        } else {
            "provider requests are disabled, only existing overlay content can be reused"
                .to_string()
        };
        let output_files =
            "writes markdown commentary files to .synrepo/synthesis-docs/ and updates .synrepo/synthesis-index/"
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
            current: "Planning commentary work...".to_string(),
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
                    "Calling the provider API and writing markdown commentary output as items complete."
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
            } => {
                self.refreshed = refreshed;
                self.generated = seeded;
                self.not_generated = not_generated;
                self.attempted = attempted;
                self.finished = true;
                self.current = "Synthesis finished. Finalizing docs and index output.".to_string();
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
                ])
                .split(area);

            let intro = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Scope: ", self.theme.muted_style()),
                    Span::styled(self.scope.clone(), self.theme.base_style()),
                ]),
                Line::from(vec![
                    Span::styled("Provider: ", self.theme.muted_style()),
                    Span::styled(self.provider.clone(), self.theme.base_style()),
                ]),
                Line::from(vec![
                    Span::styled("API calls: ", self.theme.muted_style()),
                    Span::styled(self.api_calls.clone(), self.theme.base_style()),
                ]),
                Line::from(vec![
                    Span::styled("Output files: ", self.theme.muted_style()),
                    Span::styled(self.output_files.clone(), self.theme.base_style()),
                ]),
            ])
            .block(
                Block::default()
                    .title(" synthesis ")
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
                .label(format!(
                    "{} attempted / <= {} planned",
                    self.attempted, self.max_targets
                ))
                .ratio(ratio);
            frame.render_widget(gauge, layout[1]);

            let usage = if tracker.total_calls() == 0 {
                "no provider calls recorded yet".to_string()
            } else {
                tracker.summary_label()
            };
            let status = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Plan: ", self.theme.muted_style()),
                    Span::styled(
                        format!(
                            "{} stale, {} files missing commentary, {} symbols missing commentary",
                            self.planned_refresh, self.planned_files, self.planned_symbols
                        ),
                        self.theme.base_style(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Current: ", self.theme.muted_style()),
                    Span::styled(self.current.clone(), self.theme.base_style()),
                ]),
                Line::from(vec![
                    Span::styled("Usage: ", self.theme.muted_style()),
                    Span::styled(usage, self.theme.base_style()),
                ]),
                Line::from(vec![
                    Span::styled("Latest: ", self.theme.muted_style()),
                    Span::styled(self.phase_summary.clone(), self.theme.base_style()),
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
                .map(|line| ListItem::new(Line::from(line.clone())))
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

fn render_target(item: &CommentaryWorkItem) -> String {
    match &item.qualified_name {
        Some(name) => format!("{} :: {}", item.path, name),
        None => item.path.clone(),
    }
}

fn start_label(item: &CommentaryWorkItem) -> &'static str {
    match item.phase {
        CommentaryWorkPhase::Refresh => "Refreshing stale commentary",
        CommentaryWorkPhase::Seed => "Generating missing commentary",
    }
}

fn success_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "refreshed",
        CommentaryWorkPhase::Seed => "generated",
    }
}

fn phase_summary_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "Refresh phase",
        CommentaryWorkPhase::Seed => "Missing commentary phase",
    }
}

fn provider_name(provider_label: &str) -> &str {
    provider_label.split(" / ").next().unwrap_or(provider_label)
}
