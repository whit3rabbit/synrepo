//! Single-flight execution for watch-owned writer operations.

use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use crate::{config::Config, pipeline::repair::SyncOptions};

use super::{
    control::WatchControlResponse,
    embeddings::{run_auto_embedding_refresh, run_manual_embedding_build, EmbeddingJobContext},
    events::{EmbeddingTrigger, ReconcileStartReason, SyncTrigger, WatchEvent},
    lease::WatchStateHandle,
    pending::PendingWatchBatch,
    reconcile::{
        run_reconcile_attempt, run_reconcile_attempt_with_touched_paths, ReconcileOutcome,
    },
    reconcile_state::persist_reconcile_attempt_state,
    sync::{emit_event, run_sync_under_watch_lock, WatchSyncContext},
};

const PROCESS_OWNED_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(super) struct WatchOperationContext {
    repo_root: PathBuf,
    config: Config,
    synrepo_dir: PathBuf,
    events: Option<crossbeam_channel::Sender<WatchEvent>>,
    state_handle: WatchStateHandle,
    stop_flag: Arc<AtomicBool>,
}

impl WatchOperationContext {
    pub(super) fn new(
        repo_root: PathBuf,
        config: Config,
        synrepo_dir: PathBuf,
        events: Option<crossbeam_channel::Sender<WatchEvent>>,
        state_handle: WatchStateHandle,
        stop_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            repo_root,
            config,
            synrepo_dir,
            events,
            state_handle,
            stop_flag,
        }
    }

    pub(super) fn embedding_context(&self) -> EmbeddingJobContext {
        EmbeddingJobContext::new(
            &self.config,
            &self.synrepo_dir,
            self.events.clone(),
            self.state_handle.clone(),
            self.stop_flag.clone(),
        )
    }
}

pub(super) enum ReconcileWork {
    Startup,
    Watch {
        batch: PendingWatchBatch,
        keepalive: bool,
    },
    Manual {
        fast: bool,
    },
}

pub(super) enum WatchOperation {
    Reconcile(ReconcileWork),
    Sync {
        options: SyncOptions,
        trigger: SyncTrigger,
    },
    Embeddings {
        trigger: EmbeddingTrigger,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WatchOperationKind {
    Reconcile,
    Sync,
    Embeddings,
}

impl WatchOperationKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Reconcile => "reconcile",
            Self::Sync => "sync",
            Self::Embeddings => "embeddings",
        }
    }
}

impl WatchOperation {
    fn kind(&self) -> WatchOperationKind {
        match self {
            Self::Reconcile(_) => WatchOperationKind::Reconcile,
            Self::Sync { .. } => WatchOperationKind::Sync,
            Self::Embeddings { .. } => WatchOperationKind::Embeddings,
        }
    }

    fn run(self, context: WatchOperationContext) -> WatchOperationResult {
        #[cfg(test)]
        wait_on_test_gate(self.kind());
        match self {
            Self::Reconcile(work) => run_reconcile_work(&context, work),
            Self::Sync { options, trigger } => {
                let sync_context = WatchSyncContext {
                    repo_root: &context.repo_root,
                    config: &context.config,
                    synrepo_dir: &context.synrepo_dir,
                    events: &context.events,
                    state_handle: &context.state_handle,
                    stop_flag: Some(context.stop_flag.as_ref()),
                };
                let response = run_sync_under_watch_lock(
                    &sync_context,
                    options,
                    (trigger == SyncTrigger::AutoPostReconcile)
                        .then_some(crate::pipeline::repair::CHEAP_AUTO_SYNC_SURFACES),
                    trigger,
                );
                WatchOperationResult::Sync { trigger, response }
            }
            Self::Embeddings {
                trigger: EmbeddingTrigger::Manual,
            } => WatchOperationResult::ManualEmbeddings {
                response: run_manual_embedding_build(context.embedding_context()),
            },
            Self::Embeddings {
                trigger: EmbeddingTrigger::AutoRefresh,
            } => {
                let result = run_auto_embedding_refresh(context.embedding_context())
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                WatchOperationResult::AutoEmbeddings { result }
            }
        }
    }
}

#[cfg(test)]
struct TestOperationGate {
    kind: WatchOperationKind,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}

#[cfg(test)]
static TEST_OPERATION_GATE: std::sync::Mutex<Option<TestOperationGate>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub(super) fn install_test_gate(
    kind: WatchOperationKind,
) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    *TEST_OPERATION_GATE.lock().unwrap() = Some(TestOperationGate {
        kind,
        entered: entered_tx,
        release: release_rx,
    });
    (entered_rx, release_tx)
}

#[cfg(test)]
fn wait_on_test_gate(kind: WatchOperationKind) {
    let gate = {
        let mut gate = TEST_OPERATION_GATE.lock().unwrap();
        if gate.as_ref().is_some_and(|gate| gate.kind == kind) {
            gate.take()
        } else {
            None
        }
    };
    if let Some(gate) = gate {
        let _ = gate.entered.send(());
        let _ = gate.release.recv();
    }
}

pub(super) enum WatchOperationResult {
    Reconcile {
        work: ReconcileWork,
        outcome: ReconcileOutcome,
    },
    Sync {
        trigger: SyncTrigger,
        response: WatchControlResponse,
    },
    ManualEmbeddings {
        response: WatchControlResponse,
    },
    AutoEmbeddings {
        result: Result<(), String>,
    },
}

pub(super) struct WatchOperationCompletion {
    pub(super) kind: WatchOperationKind,
    pub(super) result: Result<WatchOperationResult, String>,
    pub(super) respond_to: Option<mpsc::Sender<WatchControlResponse>>,
}

struct ActiveOperation {
    kind: WatchOperationKind,
    handle: thread::JoinHandle<WatchOperationResult>,
    respond_to: Option<mpsc::Sender<WatchControlResponse>>,
}

#[derive(Default)]
pub(super) struct WatchOperationScheduler {
    active: Option<ActiveOperation>,
}

impl WatchOperationScheduler {
    pub(super) fn is_idle(&self) -> bool {
        self.active.is_none()
    }

    pub(super) fn active_kind(&self) -> Option<WatchOperationKind> {
        self.active.as_ref().map(|active| active.kind)
    }

    pub(super) fn start(
        &mut self,
        context: WatchOperationContext,
        operation: WatchOperation,
        respond_to: Option<mpsc::Sender<WatchControlResponse>>,
    ) -> Result<(), String> {
        if let Some(kind) = self.active_kind() {
            return Err(format!(
                "watch service is busy with {}; retry after it completes",
                kind.as_str()
            ));
        }

        let kind = operation.kind();
        let handle = thread::Builder::new()
            .name(format!("synrepo-watch-{}", kind.as_str()))
            .spawn(move || operation.run(context))
            .map_err(|error| format!("failed to start {} worker: {error}", kind.as_str()))?;
        self.active = Some(ActiveOperation {
            kind,
            handle,
            respond_to,
        });
        Ok(())
    }

    pub(super) fn reap_finished(&mut self) -> Option<WatchOperationCompletion> {
        if !self
            .active
            .as_ref()
            .is_some_and(|active| active.handle.is_finished())
        {
            return None;
        }
        let active = self.active.take().expect("active operation checked above");
        let result = active
            .handle
            .join()
            .map_err(|_| format!("{} worker panicked", active.kind.as_str()));
        Some(WatchOperationCompletion {
            kind: active.kind,
            result,
            respond_to: active.respond_to,
        })
    }

    pub(super) fn reject_active_request_on_stop(&mut self) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        if let Some(respond_to) = active.respond_to.take() {
            let _ = respond_to.send(WatchControlResponse::Error {
                message: "watch service is stopping".to_string(),
            });
        }
    }

    pub(super) fn shutdown(&mut self, process_exits_after_return: bool) {
        self.shutdown_with_timeout(process_exits_after_return, PROCESS_OWNED_SHUTDOWN_TIMEOUT);
    }

    fn shutdown_with_timeout(&mut self, process_exits_after_return: bool, timeout: Duration) {
        self.reject_active_request_on_stop();
        let Some(active) = self.active.take() else {
            return;
        };
        if !process_exits_after_return {
            let _ = active.handle.join();
            return;
        }

        let deadline = Instant::now() + timeout;
        while !active.handle.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        if active.handle.is_finished() {
            let _ = active.handle.join();
        } else {
            tracing::warn!(
                operation = active.kind.as_str(),
                timeout_ms = timeout.as_millis() as u64,
                "watch operation did not finish before process-owned shutdown; detaching"
            );
        }
    }
}

fn run_reconcile_work(
    context: &WatchOperationContext,
    work: ReconcileWork,
) -> WatchOperationResult {
    let (event_count, keepalive, fast, touched_paths, full, reason) = match &work {
        ReconcileWork::Startup => (0, false, false, None, true, None),
        ReconcileWork::Manual { fast } => (0, false, *fast, None, true, None),
        ReconcileWork::Watch { batch, keepalive } => {
            let full = batch.force_full_reconcile || batch.touched_paths.is_empty();
            let reason = (batch.force_full_reconcile && !batch.touched_paths.is_empty())
                .then_some(ReconcileStartReason::WatchPathOverflow);
            let touched_paths = (!full).then_some(batch.touched_paths.as_slice());
            (
                batch.event_count,
                *keepalive,
                *keepalive,
                touched_paths,
                full,
                reason,
            )
        }
    };

    emit_event(&context.events, |now| WatchEvent::ReconcileStarted {
        at: now,
        triggering_events: event_count,
        full,
        reason,
    });
    let attempt = if touched_paths.is_none() {
        run_reconcile_attempt(
            &context.repo_root,
            &context.config,
            &context.synrepo_dir,
            fast,
        )
    } else {
        run_reconcile_attempt_with_touched_paths(
            &context.repo_root,
            &context.config,
            &context.synrepo_dir,
            touched_paths,
            fast,
        )
    };
    let outcome = attempt.outcome.clone();
    persist_reconcile_attempt_state(&context.synrepo_dir, &attempt, event_count);
    context.state_handle.note_reconcile(&outcome, event_count);
    tracing::info!(
        outcome = %outcome.as_str(),
        events = event_count,
        keepalive,
        "reconcile pass complete"
    );
    emit_event(&context.events, |now| WatchEvent::ReconcileFinished {
        at: now,
        outcome: outcome.clone(),
        triggering_events: event_count,
    });
    WatchOperationResult::Reconcile { work, outcome }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
