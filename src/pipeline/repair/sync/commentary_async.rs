//! Async commentary item preparation and generation helpers.

use std::path::Path;
use std::sync::Arc;

use crate::{
    core::ids::NodeId,
    overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore},
    pipeline::{
        explain::{
            telemetry, CommentaryGeneration, CommentaryGenerator, CommentarySkip,
            CommentarySkipReason,
        },
        repair::commentary::{resolve_commentary_node, CommentaryNodeSnapshot},
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::with_graph_read_snapshot,
};

use super::{
    commentary_context::build_context_text,
    commentary_generate::{classify_outcome, retry_delay, ItemOutcome, MAX_RATE_LIMIT_ATTEMPTS},
    commentary_plan::CommentaryWorkItem,
};

pub(super) fn prepare_item(
    repo_root: &Path,
    graph: &SqliteGraphStore,
    item: &CommentaryWorkItem,
    max_input_tokens: u32,
) -> crate::Result<Option<PreparedItem>> {
    with_graph_read_snapshot(graph, |g| {
        let Some(snap) = resolve_commentary_node(g, item.node_id)? else {
            return Ok(None);
        };
        let ctx_text = build_context_text(repo_root, g, &snap, max_input_tokens);
        Ok(Some(PreparedItem {
            node_id: item.node_id,
            snap,
            ctx_text,
        }))
    })
}

pub(super) struct PreparedItem {
    node_id: NodeId,
    snap: CommentaryNodeSnapshot,
    ctx_text: String,
}

pub(super) struct CompletedTask {
    pub(super) item: CommentaryWorkItem,
    pub(super) current: usize,
    pub(super) outcome: GeneratedOutcome,
}

pub(super) enum GeneratedOutcome {
    Generated(CommentaryEntry),
    Skipped {
        skip: CommentarySkip,
        retry_attempts: usize,
        queued_for_next_run: bool,
    },
}

pub(super) async fn generate_prepared(
    generator: Arc<dyn CommentaryGenerator>,
    item: CommentaryWorkItem,
    current: usize,
    prepared: PreparedItem,
) -> crate::Result<CompletedTask> {
    let outcome = generate_with_retries(generator.as_ref(), prepared).await?;
    Ok(CompletedTask {
        item,
        current,
        outcome,
    })
}

async fn generate_with_retries(
    generator: &dyn CommentaryGenerator,
    prepared: PreparedItem,
) -> crate::Result<GeneratedOutcome> {
    let mut retry_attempts = 0usize;
    loop {
        let outcome = generate_once_async(generator, prepared.node_id, &prepared.ctx_text).await?;
        match outcome {
            CommentaryGeneration::Generated(mut entry) => {
                entry.provenance = CommentaryProvenance {
                    source_content_hash: prepared.snap.content_hash,
                    ..entry.provenance
                };
                return Ok(GeneratedOutcome::Generated(entry));
            }
            CommentaryGeneration::Skipped(skip)
                if skip.reason == CommentarySkipReason::RateLimited
                    && retry_attempts + 1 < MAX_RATE_LIMIT_ATTEMPTS =>
            {
                retry_attempts += 1;
                tokio::time::sleep(retry_delay(&skip, retry_attempts)).await;
            }
            CommentaryGeneration::Skipped(skip) => {
                let queued_for_next_run = skip.reason == CommentarySkipReason::RateLimited;
                return Ok(GeneratedOutcome::Skipped {
                    skip,
                    retry_attempts,
                    queued_for_next_run,
                });
            }
        }
    }
}

async fn generate_once_async(
    generator: &dyn CommentaryGenerator,
    node_id: NodeId,
    ctx_text: &str,
) -> crate::Result<CommentaryGeneration> {
    let rx = telemetry::subscribe();
    let outcome = generator
        .generate_with_outcome_async(node_id, ctx_text)
        .await?;
    Ok(classify_outcome(outcome, node_id, &rx))
}

pub(super) fn persist_generated_outcome(
    overlay: &mut SqliteOverlayStore,
    outcome: GeneratedOutcome,
) -> crate::Result<ItemOutcome> {
    match outcome {
        GeneratedOutcome::Generated(entry) => {
            overlay.insert_commentary(entry)?;
            Ok(ItemOutcome::Generated)
        }
        GeneratedOutcome::Skipped {
            skip,
            retry_attempts,
            queued_for_next_run,
        } => Ok(ItemOutcome::Skipped {
            skip,
            retry_attempts,
            queued_for_next_run,
        }),
    }
}

pub(super) fn missing_node_outcome() -> ItemOutcome {
    ItemOutcome::Skipped {
        skip: CommentarySkip::new(CommentarySkipReason::GraphNodeMissing),
        retry_attempts: 0,
        queued_for_next_run: false,
    }
}
