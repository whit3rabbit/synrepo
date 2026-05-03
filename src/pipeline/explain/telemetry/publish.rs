//! Publishing infrastructure for explain telemetry events.
//!
//! Provides the global fan-out, call-id allocation, and the [`CallCtx`]
//! lifecycle wrapper providers use to emit matched start/complete/fail events.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

use super::types::{ExplainEvent, ExplainFailure, ExplainTarget, TokenUsage};
use crate::pipeline::explain::accounting;

/// Bounded buffer size per subscriber. Events are dropped on full rather
/// than back-pressuring the explain call site.
const SUBSCRIBER_BUFFER: usize = 256;

/// Process-wide fan-out list. A `Mutex` is acceptable: the publish rate is
/// measured in events per second, dwarfed by the HTTP call that produced the
/// event.
pub(crate) struct Fanout {
    pub(crate) subscribers: Mutex<Vec<Sender<ExplainEvent>>>,
    dropped: AtomicU64,
}

impl Fanout {
    const fn new() -> Self {
        Self {
            subscribers: Mutex::new(Vec::new()),
            dropped: AtomicU64::new(0),
        }
    }
}

pub(crate) static FANOUT: Fanout = Fanout::new();
static SYNREPO_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
pub(crate) static CALL_ID_SEQ: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static SCOPED_SYNREPO_DIRS: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Register the fallback `.synrepo/` directory the accounting writer should
/// use when no narrower call scope is active.
///
/// Kept for single-repo CLI/TUI compatibility. Multi-repo surfaces should use
/// [`with_synrepo_dir`] around the operation that can publish explain events.
pub fn set_synrepo_dir<P: Into<PathBuf>>(path: P) {
    if let Ok(mut guard) = SYNREPO_DIR.lock() {
        *guard = Some(path.into());
    }
}

/// Run `f` with explain accounting directed to `path` on the current thread.
///
/// Nested scopes are supported. The scoped path overrides the process-global
/// fallback even if another caller updates [`set_synrepo_dir`] while the scope
/// is active.
pub fn with_synrepo_dir<R>(path: &Path, f: impl FnOnce() -> R) -> R {
    SCOPED_SYNREPO_DIRS.with(|stack| stack.borrow_mut().push(path.to_path_buf()));
    let _guard = ScopedSynrepoDir;
    f()
}

/// Snapshot of the effective `.synrepo/` directory, if any.
pub fn synrepo_dir() -> Option<PathBuf> {
    scoped_synrepo_dir().or_else(global_synrepo_dir)
}

#[cfg(test)]
pub(super) fn clear_synrepo_dir_for_tests() {
    if let Ok(mut guard) = SYNREPO_DIR.lock() {
        *guard = None;
    }
    SCOPED_SYNREPO_DIRS.with(|stack| stack.borrow_mut().clear());
}

struct ScopedSynrepoDir;

impl Drop for ScopedSynrepoDir {
    fn drop(&mut self) {
        SCOPED_SYNREPO_DIRS.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

fn scoped_synrepo_dir() -> Option<PathBuf> {
    SCOPED_SYNREPO_DIRS.with(|stack| stack.borrow().last().cloned())
}

fn global_synrepo_dir() -> Option<PathBuf> {
    SYNREPO_DIR.lock().ok().and_then(|g| g.clone())
}

/// Register a new subscriber. Returns a receiver the caller drains at its
/// own pace. Dropping the receiver disconnects the subscriber; the next
/// publish reaps it.
pub fn subscribe() -> Receiver<ExplainEvent> {
    let (tx, rx) = bounded(SUBSCRIBER_BUFFER);
    if let Ok(mut subs) = FANOUT.subscribers.lock() {
        subs.push(tx);
    }
    rx
}

/// Allocate a unique call id.
pub fn next_call_id() -> u64 {
    CALL_ID_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Current unix millis.
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

/// Fan an event out to every live subscriber and synchronously record it to
/// the accounting log + totals snapshot when a repo root has been registered.
///
/// Dropped-on-full and disconnected subscribers are counted; see
/// [`dropped_event_count`]. Accounting errors are logged at `warn` level but
/// never propagate — telemetry never fails a explain call.
pub fn publish(event: ExplainEvent) {
    if let Ok(mut subs) = FANOUT.subscribers.lock() {
        subs.retain(|tx| match tx.try_send(event.clone()) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                FANOUT.dropped.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        });
    }

    if let Some(dir) = synrepo_dir() {
        if let Err(err) = accounting::record_event(&dir, &event) {
            tracing::warn!(error = %err, "explain accounting write failed");
        }
    }
}

/// How many events have been dropped by full subscriber buffers since
/// process start. The Health tab surfaces this to make silent loss visible.
pub fn dropped_event_count() -> u64 {
    FANOUT.dropped.load(Ordering::Relaxed)
}

/// Begin a call: allocate an id, publish `CallStarted`, and return a handle
/// providers use to emit the matching completion / failure / budget-block
/// event. The handle encapsulates the start instant so duration is measured
/// consistently across providers.
pub struct CallCtx {
    call_id: u64,
    provider: &'static str,
    model: String,
    target: ExplainTarget,
    started: std::time::Instant,
    finished: bool,
}

impl CallCtx {
    /// Open a call context and emit `CallStarted`. The caller MUST end the
    /// ctx via one of `complete`, `fail`, or `budget_blocked` (or let it
    /// drop — drop emits an implicit failure so an un-ended call is visible).
    pub fn start(provider: &'static str, model: &str, target: ExplainTarget) -> Self {
        let call_id = next_call_id();
        let model_owned = model.to_string();
        publish(ExplainEvent::CallStarted {
            call_id,
            provider,
            model: model_owned.clone(),
            target: target.clone(),
            started_at_ms: now_ms(),
        });
        Self {
            call_id,
            provider,
            model: model_owned,
            target,
            started: std::time::Instant::now(),
            finished: false,
        }
    }

    /// This call's id, useful for manual log correlation.
    pub fn call_id(&self) -> u64 {
        self.call_id
    }

    fn duration_ms(&self) -> u64 {
        self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    /// Mark this call as successfully completed.
    pub fn complete(self, usage: TokenUsage, output_bytes: u32) {
        self.complete_with_cost(usage, None, output_bytes);
    }

    /// Mark this call as successfully completed with an explicit billed cost.
    pub fn complete_with_cost(
        mut self,
        usage: TokenUsage,
        billed_usd_cost: Option<f64>,
        output_bytes: u32,
    ) {
        self.finished = true;
        publish(ExplainEvent::CallCompleted {
            call_id: self.call_id,
            provider: self.provider,
            model: self.model.clone(),
            target: self.target.clone(),
            duration_ms: self.duration_ms(),
            usage,
            billed_usd_cost,
            output_bytes,
        });
    }

    /// Mark this call as failed. `error` should be a short, non-sensitive tail.
    pub fn fail(mut self, error: impl Into<ExplainFailure>) {
        self.finished = true;
        let failure = error.into();
        publish(ExplainEvent::CallFailed {
            call_id: self.call_id,
            provider: self.provider,
            model: self.model.clone(),
            target: self.target.clone(),
            duration_ms: self.duration_ms(),
            error: truncate(&failure.error, 200),
            http_status: failure.http_status,
            retry_after_ms: failure.retry_after_ms,
        });
    }
}

impl Drop for CallCtx {
    fn drop(&mut self) {
        if !self.finished {
            // Someone forgot to close the call (likely an early `return`
            // that skipped `complete`/`fail`). Record a synthetic failure
            // so the event log still reflects the call's existence.
            publish(ExplainEvent::CallFailed {
                call_id: self.call_id,
                provider: self.provider,
                model: self.model.clone(),
                target: self.target.clone(),
                duration_ms: self.duration_ms(),
                error: "call context dropped without explicit completion".to_string(),
                http_status: None,
                retry_after_ms: None,
            });
        }
    }
}

/// Publish a `BudgetBlocked` event. Used from providers' chars-per-token
/// pre-flight check.
pub fn publish_budget_blocked(
    provider: &'static str,
    model: &str,
    target: ExplainTarget,
    estimated_tokens: u32,
    budget: u32,
) {
    publish(ExplainEvent::BudgetBlocked {
        call_id: next_call_id(),
        provider,
        model: model.to_string(),
        target,
        estimated_tokens,
        budget,
    });
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Find the largest char boundary ≤ max so we don't split a multi-byte
    // codepoint (error strings can carry non-ASCII).
    let cut = s
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= max)
        .last()
        .unwrap_or(0);
    let mut out = s[..cut].to_string();
    out.push('…');
    out
}
