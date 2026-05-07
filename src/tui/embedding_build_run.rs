//! In-dashboard embedding index build execution.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::config::Config;
use crate::pipeline::writer::{acquire_write_admission, map_lock_error};
use crate::store::sqlite::SqliteGraphStore;
use crate::substrate::embedding::{
    build_embedding_index_with_progress, EmbeddingBuildEvent, EmbeddingBuildSummary,
};
use crate::tui::actions::now_rfc3339;
use crate::tui::app::{poll_key, AppState, PendingEmbeddingBuild};
use crate::tui::dashboard::DashboardTerminal;
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

mod view;

use view::{draw_progress, EmbeddingBuildUi};

pub(crate) fn run_embedding_build_in_dashboard(
    terminal: &mut DashboardTerminal,
    state: &mut AppState,
    pending: PendingEmbeddingBuild,
) -> anyhow::Result<()> {
    let repo_root = state.repo_root.clone();
    let mut ui = match EmbeddingBuildContext::load(&repo_root) {
        Ok(context) => run_context(terminal, state, context, pending.stopped_watch)?,
        Err(error) => EmbeddingBuildUi::error(format!("{error:#}")),
    };
    ui.finished_prompt = true;
    draw_progress(terminal, &ui)?;
    state.refresh_now();
    state.set_tab(crate::tui::app::ActiveTab::Actions);
    let _ = crossterm::event::read();
    Ok(())
}

fn run_context(
    terminal: &mut DashboardTerminal,
    state: &mut AppState,
    context: EmbeddingBuildContext,
    stopped_watch: bool,
) -> anyhow::Result<EmbeddingBuildUi> {
    let mut ui = EmbeddingBuildUi::new(&context);
    if stopped_watch {
        ui.push_recent("Watch was stopped to free the writer lock.".to_string());
    }
    draw_progress(terminal, &ui)?;

    let _writer_lock = match acquire_write_admission(&context.synrepo_dir, "embeddings build") {
        Ok(lock) => lock,
        Err(error) => {
            let message = map_lock_error("embeddings build", error).to_string();
            state
                .log
                .push(log_entry("embeddings", message.clone(), Severity::Stale));
            ui.mark_error(message);
            return Ok(ui);
        }
    };

    let cancel = Arc::new(AtomicBool::new(false));
    let (event_tx, event_rx) = crossbeam_channel::unbounded::<EmbeddingBuildEvent>();
    let result: crate::Result<EmbeddingBuildSummary> = std::thread::scope(|scope_handle| {
        let config = context.config.clone();
        let synrepo_dir = context.synrepo_dir.clone();
        let cancel_for_worker = Arc::clone(&cancel);
        let event_tx_for_worker = event_tx;

        let worker = scope_handle.spawn(move || -> crate::Result<EmbeddingBuildSummary> {
            let graph = SqliteGraphStore::open(&synrepo_dir.join("graph"))?;
            let mut should_stop = || cancel_for_worker.load(Ordering::Relaxed);
            let mut progress = |event: EmbeddingBuildEvent| {
                let _ = event_tx_for_worker.send(event);
            };
            build_embedding_index_with_progress(
                &graph,
                &config,
                &synrepo_dir,
                Some(&mut progress),
                Some(&mut should_stop),
            )
        });

        let mut last_frame = Instant::now();
        loop {
            let mut had_event = false;
            while let Ok(event) = event_rx.try_recv() {
                ui.apply_event(event);
                had_event = true;
            }
            let animation_due = last_frame.elapsed() >= Duration::from_millis(200);
            if animation_due {
                ui.tick();
                last_frame = Instant::now();
            }
            if had_event || animation_due {
                state.drain_events();
                let _ = draw_progress(terminal, &ui);
            }
            if let Ok(Some((code, mods))) = poll_key(Duration::from_millis(50)) {
                let cancel_now = matches!(code, KeyCode::Esc | KeyCode::Char('q'))
                    || (matches!(code, KeyCode::Char('c')) && mods.contains(KeyModifiers::CONTROL));
                if cancel_now && !cancel.swap(true, Ordering::Relaxed) {
                    ui.mark_stop_requested();
                    let _ = draw_progress(terminal, &ui);
                }
            }
            if worker.is_finished() {
                break;
            }
        }
        while let Ok(event) = event_rx.try_recv() {
            ui.apply_event(event);
        }
        match worker.join() {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    });

    state.drain_events();
    match result {
        Ok(summary) => {
            ui.mark_finished(&summary);
            state.log.push(log_entry(
                "embeddings",
                format!("built {} chunks with {}", summary.chunks, summary.model),
                Severity::Healthy,
            ));
        }
        Err(error) => {
            let message = format!("{error:#}");
            ui.mark_error(message.clone());
            state
                .log
                .push(log_entry("embeddings", message, Severity::Stale));
        }
    }
    Ok(ui)
}

fn log_entry(tag: &str, message: String, severity: Severity) -> LogEntry {
    LogEntry {
        timestamp: now_rfc3339(),
        tag: tag.to_string(),
        message,
        severity,
    }
}

#[derive(Clone, Debug)]
struct EmbeddingBuildContext {
    config: Config,
    synrepo_dir: PathBuf,
    provider: String,
    model: String,
    dim: u16,
}

impl EmbeddingBuildContext {
    fn load(repo_root: &Path) -> anyhow::Result<Self> {
        let config = Config::load(repo_root)
            .map_err(|error| anyhow::anyhow!("embeddings: not initialized ({error})"))?;
        if !config.enable_semantic_triage {
            anyhow::bail!("embeddings are disabled; press T before building vectors");
        }
        Ok(Self {
            provider: config.semantic_embedding_provider.as_str().to_string(),
            model: config.semantic_model.clone(),
            dim: config.embedding_dim,
            synrepo_dir: Config::synrepo_dir(repo_root),
            config,
        })
    }
}
