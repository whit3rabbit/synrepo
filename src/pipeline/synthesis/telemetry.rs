//! Synthesis telemetry: per-call events emitted by every provider so the TUI
//! can show synthesis activity live and the accounting writer can record
//! accurate per-repo usage totals.
//!
//! Providers call [`publish`] at three moments on every API round-trip:
//!
//! 1. [`SynthesisEvent::CallStarted`] before the HTTP request leaves the process.
//! 2. [`SynthesisEvent::CallCompleted`] after a successful response (with the
//!    provider's reported token counts when the response carried them, or an
//!    explicit [`UsageSource::Estimated`] marker when it did not).
//! 3. [`SynthesisEvent::CallFailed`] on any HTTP error, deserialize failure,
//!    or non-success status.
//!
//! A separate [`SynthesisEvent::BudgetBlocked`] variant covers the case where
//! a provider's chars-per-token budget check refuses a call before the HTTP
//! request. These never hit the network, so they have no usage to report.
//!
//! Publication is a sync fan-out: each subscriber holds a [`crossbeam_channel`]
//! `Sender<SynthesisEvent>`; the publisher tries to send to each and drops the
//! event on full (bounded, 256) or disconnected receivers. Drops are counted
//! and exposed for surface-layer diagnostics.
//!
//! Accounting is a side effect of [`publish`]: if the process-wide
//! `.synrepo/` directory has been registered via [`set_synrepo_dir`], events
//! are synchronously forwarded to [`accounting::record_event`]. That keeps
//! the JSONL + totals snapshot consistent with what the TUI observes,
//! without needing a dedicated writer thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

use crate::core::ids::NodeId;
use crate::overlay::OverlayEdgeKind;

use super::accounting;

/// Source of a reported token count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageSource {
    /// Tokens came directly from the provider's response body (exact).
    Reported,
    /// Tokens were estimated (the response did not carry usage, or an
    /// upstream server omitted it). Surfaces must label these as approximate.
    Estimated,
}

impl UsageSource {
    /// Stable machine-readable label (used by JSONL + JSON snapshots).
    pub fn as_str(&self) -> &'static str {
        match self {
            UsageSource::Reported => "reported",
            UsageSource::Estimated => "estimated",
        }
    }
}

/// Input + output token counts for a single API call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenUsage {
    /// Input / prompt tokens.
    pub input_tokens: u32,
    /// Output / completion tokens.
    pub output_tokens: u32,
    /// Whether these counts came from the provider (Reported) or from a
    /// chars-per-token heuristic (Estimated).
    pub source: UsageSource,
}

impl TokenUsage {
    /// Construct a `Reported` usage record.
    pub fn reported(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            source: UsageSource::Reported,
        }
    }

    /// Construct an `Estimated` usage record. Use only when the provider
    /// response did not include token counts.
    pub fn estimated(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            source: UsageSource::Estimated,
        }
    }

    /// Total tokens (input + output, saturating).
    pub fn total(&self) -> u32 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// What kind of work the call was doing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SynthesisTarget {
    /// Per-node commentary generation.
    Commentary {
        /// Target node.
        node: NodeId,
    },
    /// Cross-link candidate extraction over a single prefiltered pair.
    CrossLink {
        /// Source endpoint (typically a concept or file).
        from: NodeId,
        /// Target endpoint (typically a symbol or file).
        to: NodeId,
        /// Proposed overlay edge kind.
        kind: OverlayEdgeKind,
    },
}

impl SynthesisTarget {
    /// Short display label used in live-feed log lines ("file_…", "sym_… ↔ sym_…").
    pub fn display_label(&self) -> String {
        match self {
            SynthesisTarget::Commentary { node } => format!("{node}"),
            SynthesisTarget::CrossLink { from, to, .. } => format!("{from} → {to}"),
        }
    }
}

/// Kinds of outcomes a synthesis call can have. Used as a stable JSON field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Call completed and produced bytes of output text.
    Success,
    /// Call was refused by the provider's chars-per-token budget check.
    BudgetBlocked,
    /// Call failed (HTTP error, non-success status, parse failure).
    Failed,
}

impl Outcome {
    /// Stable machine-readable label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Success => "success",
            Outcome::BudgetBlocked => "budget_blocked",
            Outcome::Failed => "failed",
        }
    }
}

/// Lifecycle event for one synthesis API call.
#[derive(Clone, Debug)]
pub enum SynthesisEvent {
    /// A call is about to hit the network.
    CallStarted {
        /// Unique, monotonically increasing call id.
        call_id: u64,
        /// Provider name (stable, lowercase).
        provider: &'static str,
        /// Model identifier used for this call.
        model: String,
        /// What the call is working on.
        target: SynthesisTarget,
        /// Unix millis when the call started.
        started_at_ms: u128,
    },
    /// A call completed with a response body.
    CallCompleted {
        /// Correlation id matching the prior `CallStarted`.
        call_id: u64,
        /// Provider name (stable, lowercase).
        provider: &'static str,
        /// Model identifier used for this call.
        model: String,
        /// What the call was working on.
        target: SynthesisTarget,
        /// Wall-clock duration of the HTTP round-trip.
        duration_ms: u64,
        /// Token counts (reported by provider or explicitly estimated).
        usage: TokenUsage,
        /// Size of the accepted response text in bytes.
        output_bytes: u32,
    },
    /// A call was refused by the pre-flight budget check and never hit the
    /// network.
    BudgetBlocked {
        /// Unique call id (matches no prior `CallStarted` because nothing was
        /// sent).
        call_id: u64,
        /// Provider name (stable, lowercase).
        provider: &'static str,
        /// Model identifier that would have been used.
        model: String,
        /// What the call was working on.
        target: SynthesisTarget,
        /// Estimated prompt tokens that tripped the budget.
        estimated_tokens: u32,
        /// Configured budget that rejected them.
        budget: u32,
    },
    /// A call failed after being sent.
    CallFailed {
        /// Correlation id matching the prior `CallStarted`.
        call_id: u64,
        /// Provider name (stable, lowercase).
        provider: &'static str,
        /// Model identifier used for this call.
        model: String,
        /// What the call was working on.
        target: SynthesisTarget,
        /// Wall-clock duration until the error surfaced.
        duration_ms: u64,
        /// Short error tail; no PII, no raw response body.
        error: String,
    },
}

impl SynthesisEvent {
    /// Correlation id.
    pub fn call_id(&self) -> u64 {
        match self {
            SynthesisEvent::CallStarted { call_id, .. }
            | SynthesisEvent::CallCompleted { call_id, .. }
            | SynthesisEvent::BudgetBlocked { call_id, .. }
            | SynthesisEvent::CallFailed { call_id, .. } => *call_id,
        }
    }

    /// Provider label.
    pub fn provider(&self) -> &'static str {
        match self {
            SynthesisEvent::CallStarted { provider, .. }
            | SynthesisEvent::CallCompleted { provider, .. }
            | SynthesisEvent::BudgetBlocked { provider, .. }
            | SynthesisEvent::CallFailed { provider, .. } => provider,
        }
    }

    /// Target the call was working on.
    pub fn target(&self) -> &SynthesisTarget {
        match self {
            SynthesisEvent::CallStarted { target, .. }
            | SynthesisEvent::CallCompleted { target, .. }
            | SynthesisEvent::BudgetBlocked { target, .. }
            | SynthesisEvent::CallFailed { target, .. } => target,
        }
    }
}

/// Bounded buffer size per subscriber. Events are dropped on full rather
/// than back-pressuring the synthesis call site.
const SUBSCRIBER_BUFFER: usize = 256;

/// Process-wide fan-out list. A `Mutex` is acceptable: the publish rate is
/// measured in events per second, dwarfed by the HTTP call that produced the
/// event.
struct Fanout {
    subscribers: Mutex<Vec<Sender<SynthesisEvent>>>,
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

static FANOUT: Fanout = Fanout::new();
static SYNREPO_DIR: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);
static CALL_ID_SEQ: AtomicU64 = AtomicU64::new(1);

/// Register the `.synrepo/` directory the accounting writer should use.
/// Idempotent; callers (CLI commands, MCP server, TUI dashboard) can all
/// call this on startup without racing. Later calls replace the stored
/// value — the last writer wins, which matches the single-repo-per-process
/// invariant.
pub fn set_synrepo_dir<P: Into<std::path::PathBuf>>(path: P) {
    if let Ok(mut guard) = SYNREPO_DIR.lock() {
        *guard = Some(path.into());
    }
}

/// Snapshot of the currently-registered `.synrepo/` directory, if any.
pub fn synrepo_dir() -> Option<std::path::PathBuf> {
    SYNREPO_DIR.lock().ok().and_then(|g| g.clone())
}

/// Register a new subscriber. Returns a receiver the caller drains at its
/// own pace. Dropping the receiver disconnects the subscriber; the next
/// publish reaps it.
pub fn subscribe() -> Receiver<SynthesisEvent> {
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
/// never propagate — telemetry never fails a synthesis call.
pub fn publish(event: SynthesisEvent) {
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
            tracing::warn!(error = %err, "synthesis accounting write failed");
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
    target: SynthesisTarget,
    started: std::time::Instant,
    finished: bool,
}

impl CallCtx {
    /// Open a call context and emit `CallStarted`. The caller MUST end the
    /// ctx via one of `complete`, `fail`, or `budget_blocked` (or let it
    /// drop — drop emits an implicit failure so an un-ended call is visible).
    pub fn start(provider: &'static str, model: &str, target: SynthesisTarget) -> Self {
        let call_id = next_call_id();
        let model_owned = model.to_string();
        publish(SynthesisEvent::CallStarted {
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
    pub fn complete(mut self, usage: TokenUsage, output_bytes: u32) {
        self.finished = true;
        publish(SynthesisEvent::CallCompleted {
            call_id: self.call_id,
            provider: self.provider,
            model: self.model.clone(),
            target: self.target.clone(),
            duration_ms: self.duration_ms(),
            usage,
            output_bytes,
        });
    }

    /// Mark this call as failed. `error` should be a short, non-sensitive tail.
    pub fn fail(mut self, error: impl Into<String>) {
        self.finished = true;
        publish(SynthesisEvent::CallFailed {
            call_id: self.call_id,
            provider: self.provider,
            model: self.model.clone(),
            target: self.target.clone(),
            duration_ms: self.duration_ms(),
            error: truncate(&error.into(), 200),
        });
    }
}

impl Drop for CallCtx {
    fn drop(&mut self) {
        if !self.finished {
            // Someone forgot to close the call (likely an early `return`
            // that skipped `complete`/`fail`). Record a synthetic failure
            // so the event log still reflects the call's existence.
            publish(SynthesisEvent::CallFailed {
                call_id: self.call_id,
                provider: self.provider,
                model: self.model.clone(),
                target: self.target.clone(),
                duration_ms: self.duration_ms(),
                error: "call context dropped without explicit completion".to_string(),
            });
        }
    }
}

/// Publish a `BudgetBlocked` event. Used from providers' chars-per-token
/// pre-flight check.
pub fn publish_budget_blocked(
    provider: &'static str,
    model: &str,
    target: SynthesisTarget,
    estimated_tokens: u32,
    budget: u32,
) {
    publish(SynthesisEvent::BudgetBlocked {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{FileNodeId, NodeId};

    fn file_target(n: u128) -> SynthesisTarget {
        SynthesisTarget::Commentary {
            node: NodeId::File(FileNodeId(n)),
        }
    }

    /// Drain events for up to `timeout`, keeping only ones matching `call_id`.
    /// Telemetry is a process-global fan-out; tests running in parallel can
    /// cross-publish on the same receiver, so filtering by call_id keeps
    /// assertions deterministic.
    fn drain_for(
        rx: &Receiver<SynthesisEvent>,
        call_id: u64,
        wanted: usize,
    ) -> Vec<SynthesisEvent> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        let mut out = Vec::new();
        while out.len() < wanted {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(ev) => {
                    if ev.call_id() == call_id {
                        out.push(ev);
                    }
                }
                Err(_) => break,
            }
        }
        out
    }

    #[test]
    fn subscribe_and_publish_roundtrip() {
        let rx = subscribe();
        let ctx = CallCtx::start("anthropic", "claude-test", file_target(1));
        let call_id = ctx.call_id();
        ctx.complete(TokenUsage::reported(10, 5), 42);

        let events = drain_for(&rx, call_id, 2);
        assert_eq!(events.len(), 2, "expected started + completed");
        assert!(matches!(events[0], SynthesisEvent::CallStarted { .. }));
        assert!(matches!(
            events[1],
            SynthesisEvent::CallCompleted {
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    source: UsageSource::Reported,
                },
                output_bytes: 42,
                ..
            }
        ));
    }

    #[test]
    fn drop_without_complete_publishes_failure() {
        let rx = subscribe();
        let ctx = CallCtx::start("local", "llama3", file_target(2));
        let call_id = ctx.call_id();
        drop(ctx);
        let events = drain_for(&rx, call_id, 2);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], SynthesisEvent::CallStarted { .. }));
        assert!(matches!(events[1], SynthesisEvent::CallFailed { .. }));
    }

    #[test]
    fn budget_blocked_has_no_started_pair() {
        let rx = subscribe();
        let before_seq = CALL_ID_SEQ.load(Ordering::Relaxed);
        publish_budget_blocked("openai", "gpt-4o-mini", file_target(3), 9999, 5000);
        // The call id is the first id issued at or after `before_seq` that
        // arrives as a BudgetBlocked on the stream.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        let mut event = None;
        while event.is_none() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            if let Ok(ev) = rx.recv_timeout(remaining) {
                if let SynthesisEvent::BudgetBlocked { call_id, .. } = &ev {
                    if *call_id >= before_seq {
                        event = Some(ev);
                    }
                }
            }
        }
        assert!(matches!(
            event.expect("budget-blocked event missing"),
            SynthesisEvent::BudgetBlocked {
                estimated_tokens: 9999,
                budget: 5000,
                ..
            }
        ));
    }

    #[test]
    fn disconnected_subscriber_is_reaped() {
        // Serialize this test: it asserts on the process-global subscriber
        // list, which other parallel tests mutate by subscribing. Taking a
        // local mutex quiets that without forcing --test-threads=1 globally.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());

        // Pin baseline AFTER taking the serialization lock so no other
        // telemetry tests can insert subscribers mid-assertion.
        let baseline_after_drop = {
            let _rx = subscribe();
            // receiver dropped at scope exit
            FANOUT
                .subscribers
                .lock()
                .map(|s| s.len())
                .unwrap_or_default()
        };
        // Forcing a publish runs the `retain` closure, which reaps
        // disconnected senders.
        CallCtx::start("gemini", "gemini-flash", file_target(4)).fail("ignored");
        let after_publish = FANOUT
            .subscribers
            .lock()
            .map(|s| s.len())
            .unwrap_or_default();
        assert!(
            after_publish < baseline_after_drop,
            "publish should reap at least one disconnected subscriber \
             (baseline_after_drop={baseline_after_drop}, after_publish={after_publish})"
        );
    }

    #[test]
    fn token_usage_total_is_saturating() {
        let u = TokenUsage::reported(u32::MAX - 1, 100);
        assert_eq!(u.total(), u32::MAX);
    }
}
