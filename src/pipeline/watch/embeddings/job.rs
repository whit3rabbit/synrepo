use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    config::Config,
    pipeline::{
        watch::{
            control::WatchControlResponse,
            events::{EmbeddingTrigger, WatchEvent},
            lease::WatchStateHandle,
            sync::emit_event,
        },
        writer::{acquire_writer_lock, LockError},
    },
    store::sqlite::SqliteGraphStore,
    substrate::embedding::{
        build_embedding_index_with_progress, refresh_existing_embedding_index_with_progress,
        EmbeddingBuildEvent, EmbeddingBuildSummary,
    },
};

#[derive(Clone)]
pub(in crate::pipeline::watch) struct EmbeddingJobContext {
    pub(in crate::pipeline::watch) config: Config,
    pub(in crate::pipeline::watch) synrepo_dir: PathBuf,
    pub(in crate::pipeline::watch) events: Option<crossbeam_channel::Sender<WatchEvent>>,
    pub(in crate::pipeline::watch) state_handle: WatchStateHandle,
    pub(in crate::pipeline::watch) stop_flag: Arc<AtomicBool>,
}

impl EmbeddingJobContext {
    pub(in crate::pipeline::watch) fn new(
        config: &Config,
        synrepo_dir: &Path,
        events: Option<crossbeam_channel::Sender<WatchEvent>>,
        state_handle: WatchStateHandle,
        stop_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config: config.clone(),
            synrepo_dir: synrepo_dir.to_path_buf(),
            events,
            state_handle,
            stop_flag,
        }
    }
}

pub(in crate::pipeline::watch) fn run_manual_embedding_build(
    context: EmbeddingJobContext,
) -> WatchControlResponse {
    match run_embedding_job(context, EmbeddingTrigger::Manual, true) {
        Ok(Some(summary)) => WatchControlResponse::EmbeddingsBuild { summary },
        Ok(None) => WatchControlResponse::Error {
            message: "embeddings build produced no index".to_string(),
        },
        Err(err) => WatchControlResponse::Error {
            message: err.to_string(),
        },
    }
}

pub(super) fn run_auto_embedding_refresh(
    context: EmbeddingJobContext,
) -> crate::Result<Option<EmbeddingBuildSummary>> {
    run_embedding_job(context, EmbeddingTrigger::AutoRefresh, false)
}

fn run_embedding_job(
    context: EmbeddingJobContext,
    trigger: EmbeddingTrigger,
    allow_download: bool,
) -> crate::Result<Option<EmbeddingBuildSummary>> {
    let trigger_label = trigger.as_str();
    context.state_handle.note_embedding_started(trigger_label);
    emit_event(&context.events, |now| WatchEvent::EmbeddingStarted {
        at: now,
        trigger,
    });

    let result = run_embedding_job_inner(&context, trigger, allow_download);
    match result {
        Ok(Some(summary)) => {
            context
                .state_handle
                .note_embedding_finished(format!("{trigger_label}:completed"));
            emit_event(&context.events, |now| WatchEvent::EmbeddingFinished {
                at: now,
                trigger,
                summary: Some(summary.clone()),
                error: None,
            });
            Ok(Some(summary))
        }
        Ok(None) => {
            context
                .state_handle
                .note_embedding_finished(format!("{trigger_label}:skipped"));
            emit_event(&context.events, |now| WatchEvent::EmbeddingFinished {
                at: now,
                trigger,
                summary: None,
                error: None,
            });
            Ok(None)
        }
        Err(err) => {
            let message = err.to_string();
            context.state_handle.note_embedding_error(message.clone());
            emit_event(&context.events, |now| WatchEvent::EmbeddingFinished {
                at: now,
                trigger,
                summary: None,
                error: Some(message),
            });
            Err(err)
        }
    }
}

fn run_embedding_job_inner(
    context: &EmbeddingJobContext,
    trigger: EmbeddingTrigger,
    allow_download: bool,
) -> crate::Result<Option<EmbeddingBuildSummary>> {
    let _lock = match acquire_writer_lock(&context.synrepo_dir) {
        Ok(lock) => lock,
        Err(LockError::HeldByOther { pid, .. }) => {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "writer lock held by pid {pid}"
            )));
        }
        Err(err) => return Err(crate::Error::Other(anyhow::anyhow!(err.to_string()))),
    };

    let graph = SqliteGraphStore::open(&context.synrepo_dir.join("graph"))?;
    let events = context.events.clone();
    let state = context.state_handle.clone();
    let mut progress = |event: EmbeddingBuildEvent| {
        let (phase, current, total) = progress_state(&event);
        state.note_embedding_progress(phase, current, total);
        emit_event(&events, |now| WatchEvent::EmbeddingProgress {
            at: now,
            trigger,
            progress: event.clone(),
        });
    };
    let mut should_stop = || context.stop_flag.load(Ordering::Relaxed);

    if allow_download {
        build_embedding_index_with_progress(
            &graph,
            &context.config,
            &context.synrepo_dir,
            Some(&mut progress),
            Some(&mut should_stop),
        )
        .map(Some)
    } else {
        refresh_existing_embedding_index_with_progress(
            &graph,
            &context.config,
            &context.synrepo_dir,
            Some(&mut progress),
            Some(&mut should_stop),
        )
    }
}

fn progress_state(event: &EmbeddingBuildEvent) -> (&'static str, Option<usize>, Option<usize>) {
    match event {
        EmbeddingBuildEvent::ResolvingModel { .. } => ("resolving_model", None, None),
        EmbeddingBuildEvent::ModelReady { .. } => ("model_ready", None, None),
        EmbeddingBuildEvent::InitializingBackend => ("initializing_backend", None, None),
        EmbeddingBuildEvent::PreflightStarted => ("preflight", None, None),
        EmbeddingBuildEvent::PreflightFinished => ("preflight_ok", None, None),
        EmbeddingBuildEvent::ExtractingChunks => ("extracting_chunks", None, None),
        EmbeddingBuildEvent::ChunksReady { chunks } => ("chunks_ready", Some(0), Some(*chunks)),
        EmbeddingBuildEvent::BatchFinished { current, total } => {
            ("embedding_batches", Some(*current), Some(*total))
        }
        EmbeddingBuildEvent::SavingIndex { .. } => ("saving_index", None, None),
        EmbeddingBuildEvent::Finished { chunks, .. } => ("finished", Some(*chunks), Some(*chunks)),
    }
}

impl EmbeddingTrigger {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AutoRefresh => "auto_refresh",
        }
    }
}
