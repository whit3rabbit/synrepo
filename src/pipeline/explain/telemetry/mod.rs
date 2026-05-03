//! Explain telemetry: per-call events emitted by every provider so the TUI
//! can show explain activity live and the accounting writer can record
//! accurate per-repo usage totals.
//!
//! Providers call [`publish::publish`] at three moments on every API round-trip:
//!
//! 1. [`ExplainEvent::CallStarted`] before the HTTP request leaves the process.
//! 2. [`ExplainEvent::CallCompleted`] after a successful response (with the
//!    provider's reported token counts when the response carried them, or an
//!    explicit [`UsageSource::Estimated`] marker when it did not).
//! 3. [`ExplainEvent::CallFailed`] on any HTTP error, deserialize failure,
//!    or non-success status.
//!
//! A separate [`ExplainEvent::BudgetBlocked`] variant covers the case where
//! a provider's chars-per-token budget check refuses a call before the HTTP
//! request. These never hit the network, so they have no usage to report.
//!
//! Publication is a sync fan-out: each subscriber holds a [`crossbeam_channel`]
//! `Sender<ExplainEvent>`; the publisher tries to send to each and drops the
//! event on full (bounded, 256) or disconnected receivers. Drops are counted
//! and exposed for surface-layer diagnostics.
//!
//! Accounting is a side effect of [`publish::publish`]: if a scoped
//! `.synrepo/` directory is active via [`with_synrepo_dir`], or a fallback
//! directory has been registered via [`set_synrepo_dir`], events are
//! synchronously forwarded to [`crate::pipeline::explain::accounting::record_event`]. That keeps
//! the JSONL + totals snapshot consistent with what the TUI observes,
//! without needing a dedicated writer thread.

pub mod publish;
pub mod types;

pub use publish::{
    dropped_event_count, next_call_id, now_ms, publish, publish_budget_blocked, set_synrepo_dir,
    subscribe, synrepo_dir, with_synrepo_dir, CallCtx,
};
pub use types::{ExplainEvent, ExplainFailure, ExplainTarget, Outcome, TokenUsage, UsageSource};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{FileNodeId, NodeId};
    use crate::pipeline::explain::accounting::log_path;
    use crate::pipeline::explain::providers::http::HttpJsonError;
    use std::sync::atomic::Ordering;
    use std::sync::{Mutex, MutexGuard};

    fn file_target(n: u128) -> ExplainTarget {
        ExplainTarget::Commentary {
            node: NodeId::File(FileNodeId(n)),
        }
    }

    /// Drain events for up to `timeout`, keeping only ones matching `call_id`.
    /// Telemetry is a process-global fan-out; tests running in parallel can
    /// cross-publish on the same receiver, so filtering by call_id keeps
    /// assertions deterministic.
    fn drain_for(
        rx: &crossbeam_channel::Receiver<ExplainEvent>,
        call_id: u64,
        wanted: usize,
    ) -> Vec<ExplainEvent> {
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

    static SYNREPO_DIR_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct SynrepoDirReset {
        _guard: MutexGuard<'static, ()>,
    }

    impl SynrepoDirReset {
        fn new() -> Self {
            let guard = SYNREPO_DIR_TEST_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            publish::clear_synrepo_dir_for_tests();
            Self { _guard: guard }
        }
    }

    impl Drop for SynrepoDirReset {
        fn drop(&mut self) {
            publish::clear_synrepo_dir_for_tests();
        }
    }

    #[test]
    fn subscribe_and_publish_roundtrip() {
        let rx = subscribe();
        let ctx = CallCtx::start("anthropic", "claude-test", file_target(1));
        let call_id = ctx.call_id();
        ctx.complete(TokenUsage::reported(10, 5), 42);

        let events = drain_for(&rx, call_id, 2);
        assert_eq!(events.len(), 2, "expected started + completed");
        assert!(matches!(events[0], ExplainEvent::CallStarted { .. }));
        assert!(matches!(
            events[1],
            ExplainEvent::CallCompleted {
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
    fn failure_text_redacts_query_secrets_before_publish_and_log() {
        let dir = tempfile::tempdir().unwrap();
        let _reset = SynrepoDirReset::new();
        set_synrepo_dir(dir.path());
        let rx = subscribe();
        let ctx = CallCtx::start("gemini", "gemini-test", file_target(5));
        let call_id = ctx.call_id();
        ctx.fail(HttpJsonError::Transport(
            "failed request for url (https://example.test/v1?key=SECRET&alt=json)".to_string(),
        ));

        let events = drain_for(&rx, call_id, 2);
        let error = events
            .into_iter()
            .find_map(|event| match event {
                ExplainEvent::CallFailed { error, .. } => Some(error),
                _ => None,
            })
            .expect("failed event missing");
        assert!(!error.contains("SECRET"));
        assert!(!error.contains("key="));

        let log = std::fs::read_to_string(log_path(dir.path())).unwrap();
        assert!(!log.contains("SECRET"));
        assert!(!log.contains("key="));
    }

    #[test]
    fn scoped_synrepo_dir_overrides_global_accounting() {
        let global = tempfile::tempdir().unwrap();
        let scoped = tempfile::tempdir().unwrap();
        let _reset = SynrepoDirReset::new();
        set_synrepo_dir(global.path());

        with_synrepo_dir(scoped.path(), || {
            set_synrepo_dir(global.path());
            publish_budget_blocked("test", "m", file_target(6), 10, 5);
        });

        let scoped_log = std::fs::read_to_string(log_path(scoped.path()))
            .expect("scoped accounting log should be written");
        assert!(scoped_log.contains("\"provider\":\"test\""));
        assert!(
            !log_path(global.path()).exists(),
            "global fallback should not receive scoped events"
        );

        publish_budget_blocked("fallback", "m", file_target(7), 10, 5);
        let global_log = std::fs::read_to_string(log_path(global.path()))
            .expect("fallback accounting log should be written after scope");
        assert!(global_log.contains("\"provider\":\"fallback\""));
    }

    #[test]
    fn drop_without_complete_publishes_failure() {
        let rx = subscribe();
        let ctx = CallCtx::start("local", "llama3", file_target(2));
        let call_id = ctx.call_id();
        drop(ctx);
        let events = drain_for(&rx, call_id, 2);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], ExplainEvent::CallStarted { .. }));
        assert!(matches!(events[1], ExplainEvent::CallFailed { .. }));
    }

    #[test]
    fn budget_blocked_has_no_started_pair() {
        let rx = subscribe();
        let before_seq = publish::CALL_ID_SEQ.load(Ordering::Relaxed);
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
                if let ExplainEvent::BudgetBlocked { call_id, .. } = &ev {
                    if *call_id >= before_seq {
                        event = Some(ev);
                    }
                }
            }
        }
        assert!(matches!(
            event.expect("budget-blocked event missing"),
            ExplainEvent::BudgetBlocked {
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
            publish::FANOUT
                .subscribers
                .lock()
                .map(|s| s.len())
                .unwrap_or_default()
        };
        // Forcing a publish runs the `retain` closure, which reaps
        // disconnected senders.
        CallCtx::start("gemini", "gemini-flash", file_target(4)).fail("ignored");
        let after_publish = publish::FANOUT
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
