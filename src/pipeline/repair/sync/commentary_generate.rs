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

use super::{
    commentary_context::build_context_text,
    commentary_plan::CommentaryWorkItem,
    commentary_retry::{CommentaryRetry, RetryAction},
};

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
    let mut retry = CommentaryRetry::new();
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
            CommentaryGeneration::Skipped(skip) => match retry.next_action(&skip) {
                RetryAction::Retry { delay } => std::thread::sleep(delay),
                RetryAction::Stop {
                    queued_for_next_run,
                } => {
                    return Ok(ItemOutcome::Skipped {
                        skip,
                        retry_attempts: retry.retry_attempts(),
                        queued_for_next_run,
                    });
                }
            },
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

#[cfg(test)]
mod tests;
