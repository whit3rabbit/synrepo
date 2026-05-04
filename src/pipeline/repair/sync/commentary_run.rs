//! Commentary refresh phase execution.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use tokio::task::JoinSet;

use crate::{
    core::ids::NodeId,
    pipeline::explain::CommentaryGenerator,
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

use super::{
    commentary_async::{
        generate_prepared, missing_node_outcome, persist_generated_outcome, prepare_item,
    },
    commentary_generate::ItemOutcome,
    commentary_plan::{CommentaryProgressEvent, CommentaryWorkItem},
    commentary_progress::{emit_target_started, record_item_outcome},
};

#[derive(Default)]
pub(super) struct RunTotals {
    pub(super) attempted: usize,
    pub(super) not_generated: usize,
    pub(super) queued_for_next_run: usize,
    pub(super) skip_reasons: BTreeMap<String, usize>,
    pub(super) stopped: bool,
    pub(super) halted_for_rate_limit: bool,
}

impl RunTotals {
    pub(super) fn can_continue(&self) -> bool {
        !self.stopped && !self.halted_for_rate_limit
    }
}

#[derive(Default)]
pub(super) struct PhaseStats {
    pub(super) attempted: usize,
    pub(super) generated: usize,
}

impl PhaseStats {
    pub(super) fn add(&mut self, other: Self) {
        self.attempted += other.attempted;
        self.generated += other.generated;
    }
}

#[derive(Clone, Copy)]
pub(super) enum RunPhase {
    Refresh,
    FileSeed,
    SymbolSeed,
}

pub(super) struct ItemExecutor<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) graph: &'a SqliteGraphStore,
    pub(super) overlay: &'a mut SqliteOverlayStore,
    pub(super) generator: Arc<dyn CommentaryGenerator>,
    pub(super) max_input_tokens: u32,
    pub(super) max_targets: usize,
    pub(super) concurrency: usize,
}

impl ItemExecutor<'_> {
    fn execute_serial(&mut self, item: &CommentaryWorkItem) -> crate::Result<ItemOutcome> {
        super::commentary_generate::execute_item(
            self.repo_root,
            self.graph,
            self.overlay,
            self.generator.as_ref(),
            item,
            self.max_input_tokens,
        )
    }
}

pub(super) fn run_phase(
    executor: &mut ItemExecutor<'_>,
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    should_stop: &mut Option<&mut dyn FnMut() -> bool>,
    totals: &mut RunTotals,
    items: &[CommentaryWorkItem],
    phase: RunPhase,
    commented: &mut HashSet<NodeId>,
) -> crate::Result<PhaseStats> {
    if executor.concurrency <= 1 {
        return run_phase_serial(
            executor,
            progress,
            should_stop,
            totals,
            items,
            phase,
            commented,
        );
    }
    run_phase_concurrent(
        executor,
        progress,
        should_stop,
        totals,
        items,
        phase,
        commented,
    )
}

fn run_phase_serial(
    executor: &mut ItemExecutor<'_>,
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    should_stop: &mut Option<&mut dyn FnMut() -> bool>,
    totals: &mut RunTotals,
    items: &[CommentaryWorkItem],
    phase: RunPhase,
    commented: &mut HashSet<NodeId>,
) -> crate::Result<PhaseStats> {
    let mut stats = PhaseStats::default();
    for item in items {
        if stop_requested(should_stop) {
            totals.stopped = true;
            break;
        }
        if should_skip_item(phase, item, commented) {
            continue;
        }
        let current = next_attempt(totals, &mut stats);
        emit_target_started(progress, item, current);
        let outcome = record_completed_item(
            executor.max_targets,
            progress,
            totals,
            item,
            current,
            executor.execute_serial(item)?,
        );
        if outcome.generated {
            mark_generated(phase, item, commented);
            stats.generated += 1;
        }
        if outcome.halted {
            totals.halted_for_rate_limit = true;
            break;
        }
    }
    Ok(stats)
}

fn run_phase_concurrent(
    executor: &mut ItemExecutor<'_>,
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    should_stop: &mut Option<&mut dyn FnMut() -> bool>,
    totals: &mut RunTotals,
    items: &[CommentaryWorkItem],
    phase: RunPhase,
    commented: &mut HashSet<NodeId>,
) -> crate::Result<PhaseStats> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(executor.concurrency)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        run_phase_concurrent_inner(
            executor,
            progress,
            should_stop,
            totals,
            items,
            phase,
            commented,
        )
        .await
    })
}

async fn run_phase_concurrent_inner(
    executor: &mut ItemExecutor<'_>,
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    should_stop: &mut Option<&mut dyn FnMut() -> bool>,
    totals: &mut RunTotals,
    items: &[CommentaryWorkItem],
    phase: RunPhase,
    commented: &mut HashSet<NodeId>,
) -> crate::Result<PhaseStats> {
    let mut stats = PhaseStats::default();
    let mut pending = JoinSet::new();
    let mut index = 0usize;

    loop {
        while pending.len() < executor.concurrency && index < items.len() && totals.can_continue() {
            let item = &items[index];
            index += 1;
            if stop_requested(should_stop) {
                totals.stopped = true;
                break;
            }
            if should_skip_item(phase, item, commented) {
                continue;
            }
            let current = next_attempt(totals, &mut stats);
            emit_target_started(progress, item, current);
            match prepare_item(
                executor.repo_root,
                executor.graph,
                item,
                executor.max_input_tokens,
            )? {
                Some(prepared) => {
                    let generator = Arc::clone(&executor.generator);
                    let item = item.clone();
                    pending.spawn(async move {
                        generate_prepared(generator, item, current, prepared).await
                    });
                }
                None => {
                    let outcome = missing_node_outcome();
                    let recorded = record_completed_item(
                        executor.max_targets,
                        progress,
                        totals,
                        item,
                        current,
                        outcome,
                    );
                    if recorded.generated {
                        stats.generated += 1;
                        mark_generated(phase, item, commented);
                    }
                }
            }
        }

        if pending.is_empty() {
            break;
        }

        let completed = pending.join_next().await.expect("pending task exists");
        let completed = completed.map_err(|err| {
            crate::Error::Other(anyhow::anyhow!("commentary task failed: {err}"))
        })??;
        let outcome = persist_generated_outcome(executor.overlay, completed.outcome)?;
        let recorded = record_completed_item(
            executor.max_targets,
            progress,
            totals,
            &completed.item,
            completed.current,
            outcome,
        );
        if recorded.generated {
            stats.generated += 1;
            mark_generated(phase, &completed.item, commented);
        }
        if recorded.halted {
            totals.halted_for_rate_limit = true;
        }
    }

    Ok(stats)
}

fn next_attempt(totals: &mut RunTotals, stats: &mut PhaseStats) -> usize {
    totals.attempted += 1;
    stats.attempted += 1;
    totals.attempted
}

fn record_completed_item(
    max_targets: usize,
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    totals: &mut RunTotals,
    item: &CommentaryWorkItem,
    current: usize,
    outcome: ItemOutcome,
) -> super::commentary_progress::RecordedItemOutcome {
    let recorded = record_item_outcome(
        progress,
        item,
        current,
        max_targets,
        outcome,
        &mut totals.queued_for_next_run,
        &mut totals.skip_reasons,
    );
    if !recorded.generated {
        totals.not_generated += 1;
    }
    recorded
}

fn should_skip_item(
    phase: RunPhase,
    item: &CommentaryWorkItem,
    commented: &HashSet<NodeId>,
) -> bool {
    match phase {
        RunPhase::Refresh => false,
        RunPhase::FileSeed => commented.contains(&item.node_id),
        RunPhase::SymbolSeed => {
            commented.contains(&item.node_id) || commented.contains(&NodeId::File(item.file_id))
        }
    }
}

fn mark_generated(phase: RunPhase, item: &CommentaryWorkItem, commented: &mut HashSet<NodeId>) {
    if matches!(phase, RunPhase::FileSeed | RunPhase::SymbolSeed) {
        commented.insert(item.node_id);
    }
}

fn stop_requested(should_stop: &mut Option<&mut dyn FnMut() -> bool>) -> bool {
    match should_stop.as_mut() {
        Some(should_stop) => should_stop(),
        None => false,
    }
}

#[cfg(test)]
mod tests;
