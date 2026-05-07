use std::collections::VecDeque;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, LineGauge, List, ListItem, Paragraph};

use crate::substrate::embedding::{EmbeddingBuildEvent, EmbeddingBuildSummary};
use crate::tui::dashboard::DashboardTerminal;

use super::EmbeddingBuildContext;

pub(super) struct EmbeddingBuildUi {
    provider: String,
    model: String,
    dim: u16,
    step: String,
    current: String,
    chunks: usize,
    embedded: usize,
    frame: usize,
    finished: bool,
    pub(super) finished_prompt: bool,
    stop_requested: bool,
    error: Option<String>,
    recent: VecDeque<String>,
}

impl EmbeddingBuildUi {
    pub(super) fn new(context: &EmbeddingBuildContext) -> Self {
        Self {
            provider: context.provider.clone(),
            model: context.model.clone(),
            dim: context.dim,
            step: "Stage 1 of 5: Resolve model".to_string(),
            current: "Preparing embedding backend.".to_string(),
            chunks: 0,
            embedded: 0,
            frame: 0,
            finished: false,
            finished_prompt: false,
            stop_requested: false,
            error: None,
            recent: VecDeque::new(),
        }
    }

    pub(super) fn error(message: String) -> Self {
        let mut ui = Self {
            provider: "none".to_string(),
            model: "none".to_string(),
            dim: 0,
            step: "Embeddings failed".to_string(),
            current: message.clone(),
            chunks: 0,
            embedded: 0,
            frame: 0,
            finished: true,
            finished_prompt: false,
            stop_requested: false,
            error: Some(message),
            recent: VecDeque::new(),
        };
        ui.push_recent(ui.current.clone());
        ui
    }

    pub(super) fn apply_event(&mut self, event: EmbeddingBuildEvent) {
        match event {
            EmbeddingBuildEvent::ResolvingModel {
                provider,
                model,
                dim,
            } => {
                self.provider = provider;
                self.model = model;
                self.dim = dim;
                self.current = format!(
                    "Resolving {} / {} ({}d).",
                    self.provider, self.model, self.dim
                );
            }
            EmbeddingBuildEvent::ModelReady { downloaded, .. } => {
                self.step = "Stage 2 of 5: Initialize backend".to_string();
                self.current = if downloaded {
                    "Model artifacts downloaded and ready.".to_string()
                } else {
                    "Model artifacts ready.".to_string()
                };
                self.push_recent(self.current.clone());
            }
            EmbeddingBuildEvent::InitializingBackend => {
                self.current = "Initializing embedding session.".to_string();
            }
            EmbeddingBuildEvent::PreflightStarted => {
                self.step = "Stage 3 of 5: Preflight provider".to_string();
                self.current = "Checking provider availability and vector dimensions.".to_string();
            }
            EmbeddingBuildEvent::PreflightFinished => {
                self.current = "Provider preflight passed.".to_string();
                self.push_recent(self.current.clone());
            }
            EmbeddingBuildEvent::ExtractingChunks => {
                self.step = "Stage 4 of 5: Extract chunks".to_string();
                self.current = "Reading graph symbols and concepts.".to_string();
            }
            EmbeddingBuildEvent::ChunksReady { chunks } => {
                self.chunks = chunks;
                self.step = "Stage 5 of 5: Embed chunks".to_string();
                self.current = format!("{chunks} chunks ready.");
                self.push_recent(self.current.clone());
            }
            EmbeddingBuildEvent::BatchFinished { current, total } => {
                self.embedded = current;
                self.chunks = total;
                self.current = format!("Embedded {current} / {total} chunks.");
            }
            EmbeddingBuildEvent::SavingIndex { path } => {
                self.current = format!("Saving {}", path.display());
                self.push_recent(self.current.clone());
            }
            EmbeddingBuildEvent::Finished { chunks, path, .. } => {
                self.finished = true;
                self.embedded = chunks;
                self.chunks = chunks;
                self.current = format!("Embedding index built at {}.", path.display());
                self.push_recent(self.current.clone());
            }
        }
    }

    pub(super) fn mark_finished(&mut self, summary: &EmbeddingBuildSummary) {
        self.finished = true;
        self.embedded = summary.chunks;
        self.chunks = summary.chunks;
        self.current = format!("Embedding index built at {}.", summary.index_path.display());
    }

    pub(super) fn mark_error(&mut self, message: String) {
        self.finished = true;
        self.step = "Embeddings failed".to_string();
        self.current = message.clone();
        self.error = Some(message);
    }

    pub(super) fn mark_stop_requested(&mut self) {
        if self.stop_requested {
            return;
        }
        self.stop_requested = true;
        self.push_recent(
            "Stop requested. Will halt after the in-flight batch returns.".to_string(),
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

pub(super) fn draw_progress(
    terminal: &mut DashboardTerminal,
    ui: &EmbeddingBuildUi,
) -> anyhow::Result<()> {
    terminal.draw(|frame| {
        let area = frame.area();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);
        let summary = vec![
            Line::from(vec![
                Span::raw("Provider: "),
                Span::raw(ui.provider.clone()),
            ]),
            Line::from(vec![Span::raw("Model: "), Span::raw(ui.model.clone())]),
            Line::from(vec![
                Span::raw("Dimension: "),
                Span::raw(ui.dim.to_string()),
            ]),
            Line::from(vec![Span::raw("Current: "), Span::raw(ui.current_line())]),
        ];
        frame.render_widget(
            Paragraph::new(summary)
                .block(Block::default().title(" embeddings ").borders(Borders::ALL)),
            layout[0],
        );
        render_progress(frame, ui, layout[1]);
        render_stop(frame, ui, layout[2]);

        let recent: Vec<ListItem> = ui
            .recent
            .iter()
            .map(|line| ListItem::new(Line::from(line.clone())))
            .collect();
        frame.render_widget(
            List::new(recent).block(Block::default().title(" recent ").borders(Borders::ALL)),
            layout[3],
        );
        frame.render_widget(Paragraph::new(footer(ui)), layout[4]);
    })?;
    Ok(())
}

fn render_progress(frame: &mut ratatui::Frame, ui: &EmbeddingBuildUi, area: ratatui::layout::Rect) {
    let ratio = if ui.chunks == 0 {
        0.0
    } else {
        (ui.embedded as f64 / ui.chunks as f64).clamp(0.0, 1.0)
    };
    let progress_block = Block::default()
        .title(ui.step.as_str())
        .borders(Borders::ALL);
    let progress_inner = progress_block.inner(area);
    frame.render_widget(progress_block, area);
    let progress_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(12), Constraint::Length(30)])
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
        Paragraph::new(format!("{} / {} chunks", ui.embedded, ui.chunks)),
        progress_layout[1],
    );
}

fn render_stop(frame: &mut ratatui::Frame, ui: &EmbeddingBuildUi, area: ratatui::layout::Rect) {
    let stop_line = if ui.stop_requested {
        "Stop requested. Waiting for the current batch to return..."
    } else {
        "Press Esc, q, or Ctrl-C to request stop."
    };
    frame.render_widget(
        Paragraph::new(vec![Line::from(stop_line)]).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn footer(ui: &EmbeddingBuildUi) -> &'static str {
    if ui.finished_prompt {
        "Press any key to return to the dashboard."
    } else if ui.error.is_some() {
        "Embeddings failed."
    } else if ui.stop_requested {
        "Stop requested. Finishing the in-flight batch..."
    } else {
        "Embedding build is running."
    }
}
