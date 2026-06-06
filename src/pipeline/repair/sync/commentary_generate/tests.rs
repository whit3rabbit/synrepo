use super::*;
use crate::core::ids::{FileNodeId, NodeId};
use crate::core::provenance::{Provenance, SourceRef};
use crate::overlay::{CommentaryEntry, CommentaryProvenance};
use crate::pipeline::explain::telemetry::{TokenUsage, UsageSource};
use crate::pipeline::repair::commentary::CommentaryNodeSnapshot;
use crate::structure::graph::{Epistemic, FileNode};
use std::sync::atomic::{AtomicUsize, Ordering};
use time::OffsetDateTime;

struct EventGenerator {
    event: ExplainEvent,
}

impl CommentaryGenerator for EventGenerator {
    fn generate(&self, _node: NodeId, _context: &str) -> crate::Result<Option<CommentaryEntry>> {
        telemetry::publish(self.event.clone());
        Ok(None)
    }
}

fn node() -> NodeId {
    NodeId::File(FileNodeId(1))
}

#[test]
fn generate_once_reports_budget_block_reason() {
    let gen = EventGenerator {
        event: ExplainEvent::BudgetBlocked {
            call_id: 1,
            provider: "test",
            model: "m".to_string(),
            target: ExplainTarget::Commentary { node: node() },
            estimated_tokens: 5888,
            budget: 5000,
        },
    };

    let outcome = generate_once(&gen, node(), "ctx").unwrap();
    let CommentaryGeneration::Skipped(skip) = outcome else {
        panic!("expected skipped outcome");
    };
    assert_eq!(skip.reason, CommentarySkipReason::BudgetBlocked);
    assert_eq!(skip.display(), "5888 est. tokens > 5000 budget");
}

#[test]
fn generate_once_reports_rate_limit_with_retry_after() {
    let gen = EventGenerator {
        event: ExplainEvent::CallFailed {
            call_id: 2,
            provider: "test",
            model: "m".to_string(),
            target: ExplainTarget::Commentary { node: node() },
            duration_ms: 10,
            error: "non-success status: 429 Too Many Requests".to_string(),
            http_status: Some(429),
            retry_after_ms: Some(250),
        },
    };

    let outcome = generate_once(&gen, node(), "ctx").unwrap();
    let CommentaryGeneration::Skipped(skip) = outcome else {
        panic!("expected skipped outcome");
    };
    assert_eq!(skip.reason, CommentarySkipReason::RateLimited);
    assert_eq!(skip.retry_after, Some(Duration::from_millis(250)));
}

#[test]
fn generate_once_reports_invalid_output_after_empty_completion() {
    let gen = EventGenerator {
        event: ExplainEvent::CallCompleted {
            call_id: 3,
            provider: "test",
            model: "m".to_string(),
            target: ExplainTarget::Commentary { node: node() },
            duration_ms: 5,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 0,
                source: UsageSource::Estimated,
            },
            billed_usd_cost: None,
            output_bytes: 0,
        },
    };

    let outcome = generate_once(&gen, node(), "ctx").unwrap();
    let CommentaryGeneration::Skipped(skip) = outcome else {
        panic!("expected skipped outcome");
    };
    assert_eq!(skip.reason, CommentarySkipReason::InvalidOutput);
    assert!(skip.display().contains("incomplete commentary"));
}

#[test]
fn rate_limit_exhaustion_returns_queued_outcome() {
    struct RateLimitedGenerator {
        calls: AtomicUsize,
    }

    impl CommentaryGenerator for RateLimitedGenerator {
        fn generate(
            &self,
            _node: NodeId,
            _context: &str,
        ) -> crate::Result<Option<CommentaryEntry>> {
            Ok(None)
        }

        fn generate_with_outcome(
            &self,
            _node: NodeId,
            _context: &str,
        ) -> crate::Result<CommentaryGeneration> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CommentaryGeneration::Skipped(CommentarySkip::rate_limited(
                "rate limited",
                Some(Duration::ZERO),
            )))
        }
    }

    let repo = tempfile::tempdir().unwrap();
    let mut overlay = SqliteOverlayStore::open(&repo.path().join(".synrepo/overlay")).unwrap();
    let generator = RateLimitedGenerator {
        calls: AtomicUsize::new(0),
    };
    let snap = CommentaryNodeSnapshot {
        content_hash: "hash".to_string(),
        file: file_node(),
        symbol: None,
    };

    let outcome = generate_and_insert(&generator, &mut overlay, node(), &snap, "ctx").unwrap();

    assert_eq!(
        generator.calls.load(Ordering::SeqCst),
        super::super::commentary_retry::MAX_RATE_LIMIT_ATTEMPTS
    );
    match outcome {
        ItemOutcome::Skipped {
            skip,
            retry_attempts,
            queued_for_next_run,
        } => {
            assert_eq!(skip.reason, CommentarySkipReason::RateLimited);
            assert_eq!(
                retry_attempts,
                super::super::commentary_retry::MAX_RATE_LIMIT_ATTEMPTS - 1
            );
            assert!(queued_for_next_run);
        }
        ItemOutcome::Generated => panic!("rate-limited generator must not produce commentary"),
    }
}

#[test]
fn short_retry_after_retries_and_succeeds() {
    struct SucceedsAfterRateLimit {
        calls: AtomicUsize,
    }

    impl CommentaryGenerator for SucceedsAfterRateLimit {
        fn generate(
            &self,
            _node: NodeId,
            _context: &str,
        ) -> crate::Result<Option<CommentaryEntry>> {
            Ok(None)
        }

        fn generate_with_outcome(
            &self,
            node: NodeId,
            _context: &str,
        ) -> crate::Result<CommentaryGeneration> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                return Ok(CommentaryGeneration::Skipped(CommentarySkip::rate_limited(
                    "rate limited",
                    Some(Duration::ZERO),
                )));
            }
            Ok(CommentaryGeneration::Generated(entry(node, "generated")))
        }
    }

    let repo = tempfile::tempdir().unwrap();
    let mut overlay = SqliteOverlayStore::open(&repo.path().join(".synrepo/overlay")).unwrap();
    let generator = SucceedsAfterRateLimit {
        calls: AtomicUsize::new(0),
    };
    let snap = CommentaryNodeSnapshot {
        content_hash: "hash".to_string(),
        file: file_node(),
        symbol: None,
    };

    let outcome = generate_and_insert(&generator, &mut overlay, node(), &snap, "ctx").unwrap();

    assert_eq!(generator.calls.load(Ordering::SeqCst), 2);
    assert!(matches!(outcome, ItemOutcome::Generated));
    let persisted = overlay.commentary_for(node()).unwrap().unwrap();
    assert_eq!(persisted.text, "generated");
    assert_eq!(persisted.provenance.source_content_hash, "hash");
}

#[test]
fn long_retry_after_queues_without_immediate_retry() {
    struct LongRetryAfterGenerator {
        calls: AtomicUsize,
    }

    impl CommentaryGenerator for LongRetryAfterGenerator {
        fn generate(
            &self,
            _node: NodeId,
            _context: &str,
        ) -> crate::Result<Option<CommentaryEntry>> {
            Ok(None)
        }

        fn generate_with_outcome(
            &self,
            _node: NodeId,
            _context: &str,
        ) -> crate::Result<CommentaryGeneration> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CommentaryGeneration::Skipped(CommentarySkip::rate_limited(
                "rate limited",
                Some(Duration::from_secs(30)),
            )))
        }
    }

    let repo = tempfile::tempdir().unwrap();
    let mut overlay = SqliteOverlayStore::open(&repo.path().join(".synrepo/overlay")).unwrap();
    let generator = LongRetryAfterGenerator {
        calls: AtomicUsize::new(0),
    };
    let snap = CommentaryNodeSnapshot {
        content_hash: "hash".to_string(),
        file: file_node(),
        symbol: None,
    };

    let outcome = generate_and_insert(&generator, &mut overlay, node(), &snap, "ctx").unwrap();

    assert_eq!(generator.calls.load(Ordering::SeqCst), 1);
    match outcome {
        ItemOutcome::Skipped {
            skip,
            retry_attempts,
            queued_for_next_run,
        } => {
            assert_eq!(skip.reason, CommentarySkipReason::RateLimited);
            assert_eq!(retry_attempts, 0);
            assert!(queued_for_next_run);
        }
        ItemOutcome::Generated => panic!("long retry-after must not retry immediately"),
    }
}

#[test]
fn non_rate_limit_skip_is_not_retried() {
    struct ProviderFailedGenerator {
        calls: AtomicUsize,
    }

    impl CommentaryGenerator for ProviderFailedGenerator {
        fn generate(
            &self,
            _node: NodeId,
            _context: &str,
        ) -> crate::Result<Option<CommentaryEntry>> {
            Ok(None)
        }

        fn generate_with_outcome(
            &self,
            _node: NodeId,
            _context: &str,
        ) -> crate::Result<CommentaryGeneration> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CommentaryGeneration::Skipped(
                CommentarySkip::new(CommentarySkipReason::ProviderFailed)
                    .with_detail("provider failed"),
            ))
        }
    }

    let repo = tempfile::tempdir().unwrap();
    let mut overlay = SqliteOverlayStore::open(&repo.path().join(".synrepo/overlay")).unwrap();
    let generator = ProviderFailedGenerator {
        calls: AtomicUsize::new(0),
    };
    let snap = CommentaryNodeSnapshot {
        content_hash: "hash".to_string(),
        file: file_node(),
        symbol: None,
    };

    let outcome = generate_and_insert(&generator, &mut overlay, node(), &snap, "ctx").unwrap();

    assert_eq!(generator.calls.load(Ordering::SeqCst), 1);
    match outcome {
        ItemOutcome::Skipped {
            skip,
            retry_attempts,
            queued_for_next_run,
        } => {
            assert_eq!(skip.reason, CommentarySkipReason::ProviderFailed);
            assert_eq!(retry_attempts, 0);
            assert!(!queued_for_next_run);
        }
        ItemOutcome::Generated => panic!("provider failure must not retry"),
    }
}

fn entry(node: NodeId, text: &str) -> CommentaryEntry {
    CommentaryEntry {
        node_id: node,
        text: text.to_string(),
        provenance: CommentaryProvenance {
            source_content_hash: String::new(),
            pass_id: "test".to_string(),
            model_identity: "fixture".to_string(),
            generated_at: OffsetDateTime::now_utc(),
        },
    }
}

fn file_node() -> FileNode {
    FileNode {
        id: FileNodeId(1),
        root_id: "primary".to_string(),
        path: "src/lib.rs".to_string(),
        path_history: Vec::new(),
        content_hash: "hash".to_string(),
        content_sample_hashes: Vec::new(),
        size_bytes: 0,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        last_observed_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: Provenance::structural(
            "test",
            "rev",
            vec![SourceRef {
                file_id: Some(FileNodeId(1)),
                path: "src/lib.rs".to_string(),
                content_hash: "hash".to_string(),
            }],
        ),
    }
}
