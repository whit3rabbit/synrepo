//! Per-target commentary generation with skip classification and retry.

use std::path::Path;
use std::time::Duration;

use crossbeam_channel::Receiver;

use crate::{
    core::ids::NodeId,
    overlay::{CommentaryProvenance, OverlayStore},
    pipeline::{
        explain::{
            telemetry::{self, ExplainEvent, ExplainTarget},
            CommentaryGeneration, CommentaryGenerator, CommentarySkip, CommentarySkipReason,
        },
        repair::commentary::{resolve_commentary_node, CommentaryNodeSnapshot},
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::with_graph_read_snapshot,
};

use super::commentary_context::build_context_text;
use super::commentary_plan::CommentaryWorkItem;

pub(super) const MAX_RATE_LIMIT_ATTEMPTS: usize = 3;
pub(super) const DEFAULT_RATE_LIMIT_BACKOFF: Duration = Duration::from_millis(750);
pub(super) const MAX_RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub(super) enum ItemOutcome {
    Generated,
    Skipped {
        skip: CommentarySkip,
        retry_attempts: usize,
        queued_for_next_run: bool,
    },
}

pub(super) fn execute_item(
    repo_root: &Path,
    graph: &SqliteGraphStore,
    overlay: &mut SqliteOverlayStore,
    generator: &dyn CommentaryGenerator,
    item: &CommentaryWorkItem,
    max_input_tokens: u32,
) -> crate::Result<ItemOutcome> {
    // Single read snapshot so the prompt cannot mix two committed epochs.
    // Released before the LLM call so a slow provider does not block writers.
    let prepared = with_graph_read_snapshot(graph, |g| {
        let Some(snap) = resolve_commentary_node(g, item.node_id)? else {
            return Ok(None);
        };
        let ctx_text = build_context_text(repo_root, g, &snap, max_input_tokens);
        Ok(Some((snap, ctx_text)))
    })?;
    let Some((snap, ctx_text)) = prepared else {
        return Ok(ItemOutcome::Skipped {
            skip: CommentarySkip::new(CommentarySkipReason::GraphNodeMissing),
            retry_attempts: 0,
            queued_for_next_run: false,
        });
    };
    generate_and_insert(generator, overlay, item.node_id, &snap, &ctx_text)
}

fn generate_and_insert(
    generator: &dyn CommentaryGenerator,
    overlay: &mut SqliteOverlayStore,
    node_id: NodeId,
    snap: &CommentaryNodeSnapshot,
    ctx_text: &str,
) -> crate::Result<ItemOutcome> {
    let mut retry_attempts = 0usize;
    loop {
        let outcome = generate_once(generator, node_id, ctx_text)?;
        match outcome {
            CommentaryGeneration::Generated(mut entry) => {
                entry.provenance = CommentaryProvenance {
                    source_content_hash: snap.content_hash.clone(),
                    ..entry.provenance
                };
                overlay.insert_commentary(entry)?;
                return Ok(ItemOutcome::Generated);
            }
            CommentaryGeneration::Skipped(skip)
                if skip.reason == CommentarySkipReason::RateLimited
                    && retry_attempts + 1 < MAX_RATE_LIMIT_ATTEMPTS =>
            {
                retry_attempts += 1;
                std::thread::sleep(retry_delay(&skip, retry_attempts));
            }
            CommentaryGeneration::Skipped(skip) => {
                let queued_for_next_run = skip.reason == CommentarySkipReason::RateLimited;
                return Ok(ItemOutcome::Skipped {
                    skip,
                    retry_attempts,
                    queued_for_next_run,
                });
            }
        }
    }
}

fn generate_once(
    generator: &dyn CommentaryGenerator,
    node_id: NodeId,
    ctx_text: &str,
) -> crate::Result<CommentaryGeneration> {
    let rx = telemetry::subscribe();
    let outcome = generator.generate_with_outcome(node_id, ctx_text)?;
    Ok(classify_outcome(outcome, node_id, &rx))
}

pub(super) fn classify_outcome(
    outcome: CommentaryGeneration,
    node_id: NodeId,
    rx: &Receiver<ExplainEvent>,
) -> CommentaryGeneration {
    match outcome {
        CommentaryGeneration::Generated(entry) => CommentaryGeneration::Generated(entry),
        CommentaryGeneration::Skipped(skip) if skip.reason != CommentarySkipReason::Unknown => {
            CommentaryGeneration::Skipped(skip)
        }
        CommentaryGeneration::Skipped(skip) => {
            CommentaryGeneration::Skipped(classify_skip(skip, node_id, rx))
        }
    }
}

fn classify_skip(
    fallback: CommentarySkip,
    node_id: NodeId,
    rx: &Receiver<ExplainEvent>,
) -> CommentarySkip {
    let mut completed_empty = false;
    for event in rx.try_iter() {
        match event {
            ExplainEvent::BudgetBlocked {
                target: ExplainTarget::Commentary { node },
                estimated_tokens,
                budget,
                ..
            } if node == node_id => {
                return CommentarySkip::budget_blocked(estimated_tokens, budget)
            }
            ExplainEvent::CallFailed {
                target: ExplainTarget::Commentary { node },
                error,
                http_status,
                retry_after_ms,
                ..
            } if node == node_id && http_status == Some(429) => {
                return CommentarySkip::rate_limited(
                    error,
                    retry_after_ms.map(Duration::from_millis),
                );
            }
            ExplainEvent::CallFailed {
                target: ExplainTarget::Commentary { node },
                error,
                ..
            } if node == node_id => {
                return CommentarySkip::new(CommentarySkipReason::ProviderFailed)
                    .with_detail(error);
            }
            ExplainEvent::CallCompleted {
                target: ExplainTarget::Commentary { node },
                output_bytes,
                ..
            } if node == node_id && output_bytes == 0 => {
                completed_empty = true;
            }
            _ => {}
        }
    }
    if completed_empty {
        return CommentarySkip::new(CommentarySkipReason::InvalidOutput)
            .with_detail("provider returned empty or incomplete commentary");
    }
    fallback
}

pub(super) fn retry_delay(skip: &CommentarySkip, retry_attempts: usize) -> Duration {
    let base = skip.retry_after.unwrap_or(DEFAULT_RATE_LIMIT_BACKOFF);
    let scaled = base.saturating_mul(retry_attempts as u32);
    scaled.min(MAX_RATE_LIMIT_BACKOFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{FileNodeId, NodeId};
    use crate::core::provenance::{Provenance, SourceRef};
    use crate::overlay::CommentaryEntry;
    use crate::pipeline::explain::telemetry::{TokenUsage, UsageSource};
    use crate::pipeline::repair::commentary::CommentaryNodeSnapshot;
    use crate::structure::graph::{Epistemic, FileNode};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct EventGenerator {
        event: ExplainEvent,
    }

    impl CommentaryGenerator for EventGenerator {
        fn generate(
            &self,
            _node: NodeId,
            _context: &str,
        ) -> crate::Result<Option<CommentaryEntry>> {
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
    fn retry_delay_uses_retry_after_and_caps_growth() {
        let skip = CommentarySkip::rate_limited("limited", Some(Duration::from_secs(3)));
        assert_eq!(retry_delay(&skip, 1), Duration::from_secs(3));
        assert_eq!(retry_delay(&skip, 4), MAX_RATE_LIMIT_BACKOFF);
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
            MAX_RATE_LIMIT_ATTEMPTS
        );
        match outcome {
            ItemOutcome::Skipped {
                skip,
                retry_attempts,
                queued_for_next_run,
            } => {
                assert_eq!(skip.reason, CommentarySkipReason::RateLimited);
                assert_eq!(retry_attempts, MAX_RATE_LIMIT_ATTEMPTS - 1);
                assert!(queued_for_next_run);
            }
            ItemOutcome::Generated => panic!("rate-limited generator must not produce commentary"),
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
}
