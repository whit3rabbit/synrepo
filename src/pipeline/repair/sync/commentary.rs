//! Commentary refresh helpers for repair sync.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::{
    core::ids::NodeId,
    pipeline::explain::{
        build_commentary_generator,
        docs::{docs_root, index_dir, reconcile_commentary_docs, sync_commentary_index},
        CommentaryGenerator,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

use super::commentary_generate::execute_item;
use super::commentary_plan::{
    build_commentary_work_plan_with_progress, CommentaryProgressEvent, CommentaryWorkItem,
    CommentaryWorkPhase, CommentaryWorkPlan,
};
use super::commentary_progress::{
    emit, emit_docs_events, emit_index_events, emit_target_started, record_item_outcome,
    skip_reason_summary,
};
use super::handlers::ActionContext;

/// Generate or refresh commentary entries.
///
/// Seeds commentary for graph nodes that lack an overlay entry, then refreshes
/// existing entries whose source content hash has changed. When `scope` is
/// `Some(paths)`, only files whose path starts with one of the prefixes are
/// considered. Prefixes are repo-root-relative; each is normalized to end in
/// `/` so `src` cannot spuriously match `src-extra/...`.
pub fn refresh_commentary(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
    scope: Option<&[PathBuf]>,
    mut progress: Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    should_stop: Option<&mut dyn FnMut() -> bool>,
) -> crate::Result<()> {
    let overlay_dir = context.synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir)?;
    let graph = SqliteGraphStore::open_existing(&context.synrepo_dir.join("graph"))?;
    let generator: Box<dyn CommentaryGenerator> =
        build_commentary_generator(context.config, context.config.commentary_cost_limit);
    let rows = commentary_rows_for_refresh(&overlay)?;
    let plan = match progress.as_mut() {
        Some(progress) => {
            build_commentary_work_plan_with_progress(&graph, &rows, scope, Some(&mut **progress))?
        }
        None => build_commentary_work_plan_with_progress(&graph, &rows, scope, None)?,
    };
    refresh_commentary_with_generator(
        context,
        actions_taken,
        context.repo_root,
        &graph,
        &mut overlay,
        &*generator,
        rows,
        plan,
        progress,
        should_stop,
    )
}

fn commentary_rows_for_refresh(
    overlay: &SqliteOverlayStore,
) -> crate::Result<Vec<(String, String)>> {
    overlay
        .all_commentary_entries()?
        .into_iter()
        .map(|entry| {
            Ok((
                entry.node_id.to_string(),
                entry.provenance.source_content_hash,
            ))
        })
        .collect()
}

struct ItemExecutor<'a> {
    repo_root: &'a Path,
    graph: &'a SqliteGraphStore,
    overlay: &'a mut SqliteOverlayStore,
    generator: &'a dyn CommentaryGenerator,
    max_input_tokens: u32,
    max_targets: usize,
}

impl ItemExecutor<'_> {
    fn execute(
        &mut self,
        item: &CommentaryWorkItem,
    ) -> crate::Result<super::commentary_generate::ItemOutcome> {
        execute_item(
            self.repo_root,
            self.graph,
            self.overlay,
            self.generator,
            item,
            self.max_input_tokens,
        )
    }
}

#[derive(Default)]
struct RunTotals {
    attempted: usize,
    not_generated: usize,
    queued_for_next_run: usize,
    skip_reasons: BTreeMap<String, usize>,
    stopped: bool,
    halted_for_rate_limit: bool,
}

impl RunTotals {
    fn can_continue(&self) -> bool {
        !self.stopped && !self.halted_for_rate_limit
    }
}

#[derive(Default)]
struct PhaseStats {
    attempted: usize,
    generated: usize,
}

impl PhaseStats {
    fn add(&mut self, other: Self) {
        self.attempted += other.attempted;
        self.generated += other.generated;
    }
}

#[derive(Clone, Copy)]
enum RunPhase {
    Refresh,
    FileSeed,
    SymbolSeed,
}

#[allow(clippy::too_many_arguments)]
fn refresh_commentary_with_generator(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
    repo_root: &Path,
    graph: &SqliteGraphStore,
    overlay: &mut SqliteOverlayStore,
    generator: &dyn CommentaryGenerator,
    rows: Vec<(String, String)>,
    plan: CommentaryWorkPlan,
    mut progress: Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    mut should_stop: Option<&mut dyn FnMut() -> bool>,
) -> crate::Result<()> {
    emit(
        &mut progress,
        CommentaryProgressEvent::PlanReady {
            refresh: plan.refresh_count(),
            file_seeds: plan.file_seed_count(),
            symbol_seed_candidates: plan.symbol_seed_candidate_count(),
            scoped_files: plan.scoped_file_count(),
            scoped_symbols: plan.scoped_symbol_count(),
            max_targets: plan.max_target_count(),
        },
    );

    let docs_root_path = docs_root(context.synrepo_dir);
    let index_dir_path = index_dir(context.synrepo_dir);
    let docs_root_existed = docs_root_path.exists();
    let index_dir_existed = index_dir_path.exists();

    let mut commented: HashSet<NodeId> = rows
        .iter()
        .filter_map(|(id, _)| NodeId::from_str(id).ok())
        .collect();
    let max_targets = plan.max_target_count();
    let mut totals = RunTotals::default();
    let (refresh_stats, seed_stats) = {
        let mut executor = ItemExecutor {
            repo_root,
            graph,
            overlay,
            generator,
            max_input_tokens: context.config.commentary_cost_limit,
            max_targets,
        };

        let refresh_stats = run_phase(
            &mut executor,
            &mut progress,
            &mut should_stop,
            &mut totals,
            &plan.refresh,
            RunPhase::Refresh,
            &mut commented,
        )?;

        emit(
            &mut progress,
            CommentaryProgressEvent::PhaseSummary {
                phase: CommentaryWorkPhase::Refresh,
                attempted: refresh_stats.attempted,
                generated: refresh_stats.generated,
                not_generated: refresh_stats
                    .attempted
                    .saturating_sub(refresh_stats.generated),
            },
        );

        let mut seed_stats = PhaseStats::default();
        if totals.can_continue() {
            seed_stats.add(run_phase(
                &mut executor,
                &mut progress,
                &mut should_stop,
                &mut totals,
                &plan.file_seeds,
                RunPhase::FileSeed,
                &mut commented,
            )?);
        }
        if totals.can_continue() {
            seed_stats.add(run_phase(
                &mut executor,
                &mut progress,
                &mut should_stop,
                &mut totals,
                &plan.symbol_seed_candidates,
                RunPhase::SymbolSeed,
                &mut commented,
            )?);
        }
        (refresh_stats, seed_stats)
    };

    emit(
        &mut progress,
        CommentaryProgressEvent::PhaseSummary {
            phase: CommentaryWorkPhase::Seed,
            attempted: seed_stats.attempted,
            generated: seed_stats.generated,
            not_generated: seed_stats.attempted.saturating_sub(seed_stats.generated),
        },
    );

    let touched = reconcile_commentary_docs(context.synrepo_dir, graph, Some(overlay))?;
    let index_summary = sync_commentary_index(context.synrepo_dir, &touched)?;
    let docs_written = touched.iter().filter(|path| path.exists()).count();
    let docs_removed = touched.len().saturating_sub(docs_written);
    emit_docs_events(&mut progress, &docs_root_path, docs_root_existed, &touched);
    emit_index_events(
        &mut progress,
        &index_dir_path,
        index_dir_existed,
        index_summary.mode,
        index_summary.touched_paths,
    );

    emit(
        &mut progress,
        CommentaryProgressEvent::RunSummary {
            refreshed: refresh_stats.generated,
            seeded: seed_stats.generated,
            not_generated: totals.not_generated,
            attempted: totals.attempted,
            stopped: totals.stopped,
            queued_for_next_run: totals.queued_for_next_run,
            skip_reasons: skip_reason_summary(&totals.skip_reasons),
        },
    );
    let stop_suffix = if totals.stopped {
        " (stopped by operator)"
    } else {
        ""
    };
    let queue_suffix = if totals.queued_for_next_run > 0 {
        format!("; {} queued for next run", totals.queued_for_next_run)
    } else {
        String::new()
    };
    actions_taken.push(format!(
        "commentary: {} seeded, {} refreshed, {} not generated{queue_suffix}{stop_suffix}",
        seed_stats.generated, refresh_stats.generated, totals.not_generated
    ));
    actions_taken.push(format!(
        "commentary docs: {docs_written} written, {docs_removed} removed, {} indexed",
        index_summary.touched_paths
    ));
    Ok(())
}

fn run_phase(
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
        totals.attempted += 1;
        stats.attempted += 1;
        emit_target_started(progress, item, totals.attempted);
        let outcome = record_item_outcome(
            progress,
            item,
            totals.attempted,
            executor.max_targets,
            executor.execute(item)?,
            &mut totals.queued_for_next_run,
            &mut totals.skip_reasons,
        );
        if outcome.generated {
            stats.generated += 1;
            mark_generated(phase, item, commented);
        } else {
            totals.not_generated += 1;
        }
        if outcome.halted {
            totals.halted_for_rate_limit = true;
            break;
        }
    }
    Ok(stats)
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
