//! Commentary refresh helpers for repair sync.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::{
    core::ids::NodeId,
    overlay::{CommentaryProvenance, OverlayStore},
    pipeline::{
        explain::{
            build_commentary_generator,
            docs::{
                docs_root, index_dir, reconcile_commentary_docs, sync_commentary_index,
                CommentaryIndexSyncMode,
            },
            CommentaryGenerator,
        },
        repair::commentary::{resolve_commentary_node, CommentaryNodeSnapshot},
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

use super::commentary_context::build_context_text;
use super::commentary_plan::{
    build_commentary_work_plan_with_progress, CommentaryProgressEvent, CommentaryWorkItem,
    CommentaryWorkPhase, CommentaryWorkPlan,
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
            let hash = if entry.provenance.pass_id.starts_with("commentary-v1")
                || entry.provenance.pass_id.starts_with("commentary-v2")
                || entry.provenance.pass_id.starts_with("commentary-v3")
            {
                String::new()
            } else {
                entry.provenance.source_content_hash
            };
            Ok((entry.node_id.to_string(), hash))
        })
        .collect()
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
    let mut attempted = 0usize;
    let mut refreshed = 0usize;
    let mut seeded = 0usize;
    let mut not_generated = 0usize;
    let mut refresh_attempted = 0usize;
    let mut refresh_generated = 0usize;
    let mut seed_attempted = 0usize;
    let mut seed_generated = 0usize;
    let mut stopped = false;

    for item in &plan.refresh {
        if stop_requested(&mut should_stop) {
            stopped = true;
            break;
        }
        attempted += 1;
        refresh_attempted += 1;
        emit_target_started(&mut progress, item, attempted);
        let generated = execute_item(repo_root, graph, overlay, generator, item)?;
        if generated {
            refreshed += 1;
            refresh_generated += 1;
        } else {
            not_generated += 1;
        }
        emit_target_finished(&mut progress, item, attempted, generated);
    }

    emit(
        &mut progress,
        CommentaryProgressEvent::PhaseSummary {
            phase: CommentaryWorkPhase::Refresh,
            attempted: refresh_attempted,
            generated: refresh_generated,
            not_generated: refresh_attempted.saturating_sub(refresh_generated),
        },
    );

    for item in &plan.file_seeds {
        if stop_requested(&mut should_stop) {
            stopped = true;
            break;
        }
        if commented.contains(&item.node_id) {
            continue;
        }
        attempted += 1;
        seed_attempted += 1;
        emit_target_started(&mut progress, item, attempted);
        let generated = execute_item(repo_root, graph, overlay, generator, item)?;
        if generated {
            commented.insert(item.node_id);
            seeded += 1;
            seed_generated += 1;
        } else {
            not_generated += 1;
        }
        emit_target_finished(&mut progress, item, attempted, generated);
    }

    for item in &plan.symbol_seed_candidates {
        if stop_requested(&mut should_stop) {
            stopped = true;
            break;
        }
        if commented.contains(&item.node_id) || commented.contains(&NodeId::File(item.file_id)) {
            continue;
        }
        attempted += 1;
        seed_attempted += 1;
        emit_target_started(&mut progress, item, attempted);
        let generated = execute_item(repo_root, graph, overlay, generator, item)?;
        if generated {
            commented.insert(item.node_id);
            seeded += 1;
            seed_generated += 1;
        } else {
            not_generated += 1;
        }
        emit_target_finished(&mut progress, item, attempted, generated);
    }

    emit(
        &mut progress,
        CommentaryProgressEvent::PhaseSummary {
            phase: CommentaryWorkPhase::Seed,
            attempted: seed_attempted,
            generated: seed_generated,
            not_generated: seed_attempted.saturating_sub(seed_generated),
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
            refreshed,
            seeded,
            not_generated,
            attempted,
            stopped,
        },
    );
    let stop_suffix = if stopped {
        " (stopped by operator)"
    } else {
        ""
    };
    actions_taken.push(format!(
        "commentary: {seeded} seeded, {refreshed} refreshed, {not_generated} not generated{stop_suffix}"
    ));
    actions_taken.push(format!(
        "commentary docs: {docs_written} written, {docs_removed} removed, {} indexed",
        index_summary.touched_paths
    ));
    Ok(())
}

fn stop_requested(should_stop: &mut Option<&mut dyn FnMut() -> bool>) -> bool {
    match should_stop.as_mut() {
        Some(should_stop) => should_stop(),
        None => false,
    }
}

fn execute_item(
    repo_root: &Path,
    graph: &SqliteGraphStore,
    overlay: &mut SqliteOverlayStore,
    generator: &dyn CommentaryGenerator,
    item: &CommentaryWorkItem,
) -> crate::Result<bool> {
    let Some(snap) = resolve_commentary_node(graph, item.node_id)? else {
        return Ok(false);
    };
    generate_and_insert(repo_root, graph, generator, overlay, item.node_id, &snap)
}

fn emit(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    event: CommentaryProgressEvent,
) {
    if let Some(progress) = progress.as_mut() {
        progress(event);
    }
}

fn emit_target_started(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    item: &CommentaryWorkItem,
    current: usize,
) {
    emit(
        progress,
        CommentaryProgressEvent::TargetStarted {
            item: item.clone(),
            current,
        },
    );
}

fn emit_target_finished(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    item: &CommentaryWorkItem,
    current: usize,
    generated: bool,
) {
    emit(
        progress,
        CommentaryProgressEvent::TargetFinished {
            item: item.clone(),
            current,
            generated,
        },
    );
}

fn emit_docs_events(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    docs_root_path: &Path,
    docs_root_existed: bool,
    touched: &[PathBuf],
) {
    if !docs_root_existed && docs_root_path.exists() {
        emit(
            progress,
            CommentaryProgressEvent::DocsDirCreated {
                path: docs_root_path.to_path_buf(),
            },
        );
    }
    let mut touched_sorted = touched.to_vec();
    touched_sorted.sort();
    for path in touched_sorted {
        let event = if path.exists() {
            CommentaryProgressEvent::DocWritten { path }
        } else {
            CommentaryProgressEvent::DocDeleted { path }
        };
        emit(progress, event);
    }
}

fn emit_index_events(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    index_dir_path: &Path,
    index_dir_existed: bool,
    mode: CommentaryIndexSyncMode,
    touched_paths: usize,
) {
    if !index_dir_existed && index_dir_path.exists() {
        emit(
            progress,
            CommentaryProgressEvent::IndexDirCreated {
                path: index_dir_path.to_path_buf(),
            },
        );
    }
    match mode {
        CommentaryIndexSyncMode::NoChange => {}
        CommentaryIndexSyncMode::Updated => emit(
            progress,
            CommentaryProgressEvent::IndexUpdated {
                path: index_dir_path.to_path_buf(),
                touched_paths,
            },
        ),
        CommentaryIndexSyncMode::Rebuilt => emit(
            progress,
            CommentaryProgressEvent::IndexRebuilt {
                path: index_dir_path.to_path_buf(),
                touched_paths,
            },
        ),
    }
}

fn generate_and_insert(
    repo_root: &Path,
    graph: &SqliteGraphStore,
    generator: &dyn CommentaryGenerator,
    overlay: &mut SqliteOverlayStore,
    node_id: NodeId,
    snap: &CommentaryNodeSnapshot,
) -> crate::Result<bool> {
    let ctx_text = build_context_text(repo_root, graph, snap);
    let Some(mut entry) = generator.generate(node_id, &ctx_text)? else {
        return Ok(false);
    };
    entry.provenance = CommentaryProvenance {
        source_content_hash: snap.content_hash.clone(),
        ..entry.provenance
    };
    overlay.insert_commentary(entry)?;
    Ok(true)
}
