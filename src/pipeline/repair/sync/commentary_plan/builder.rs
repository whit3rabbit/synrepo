//! Build a `CommentaryWorkPlan` by scanning the graph and overlay stores.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use super::scope::{in_scope, normalize_scope_prefixes, scan_emit_interval};
use super::types::{
    CommentaryProgressEvent, CommentaryWorkItem, CommentaryWorkPhase, CommentaryWorkPlan,
};
use crate::{
    core::ids::NodeId,
    pipeline::repair::commentary::{resolve_commentary_node, CommentaryNodeSnapshot},
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

/// Load the current commentary work plan without mutating any stores.
pub fn load_commentary_work_plan(
    synrepo_dir: &Path,
    scope: Option<&[PathBuf]>,
) -> crate::Result<CommentaryWorkPlan> {
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;
    let overlay_dir = synrepo_dir.join("overlay");
    let rows = if SqliteOverlayStore::db_path(&overlay_dir).exists() {
        SqliteOverlayStore::open_existing(&overlay_dir)?.commentary_hashes()?
    } else {
        Vec::new()
    };
    build_commentary_work_plan(&graph, &rows, scope)
}

pub(crate) fn build_commentary_work_plan(
    graph: &SqliteGraphStore,
    rows: &[(String, String)],
    scope: Option<&[PathBuf]>,
) -> crate::Result<CommentaryWorkPlan> {
    build_commentary_work_plan_with_progress(graph, rows, scope, None)
}

pub(crate) fn build_commentary_work_plan_with_progress(
    graph: &SqliteGraphStore,
    rows: &[(String, String)],
    scope: Option<&[PathBuf]>,
    mut progress: Option<&mut dyn FnMut(CommentaryProgressEvent)>,
) -> crate::Result<CommentaryWorkPlan> {
    let scope_prefixes = scope.map(normalize_scope_prefixes);
    let commented: HashSet<NodeId> = rows
        .iter()
        .filter_map(|(id, _)| NodeId::from_str(id).ok())
        .collect();
    let mut refresh = Vec::new();
    let mut file_seeds = Vec::new();
    let mut symbol_seed_candidates = Vec::new();
    let mut scoped_files = 0usize;
    let mut scoped_symbols = 0usize;
    let mut file_paths = HashMap::new();
    let mut files_scanned = 0usize;
    let mut symbols_scanned = 0usize;
    let file_rows = graph.all_file_paths()?;
    let symbol_rows = graph.all_symbols_summary()?;
    let file_total = file_rows.len();
    let symbol_total = symbol_rows.len();

    emit_scan_progress(
        &mut progress,
        files_scanned,
        file_total,
        symbols_scanned,
        symbol_total,
    );

    for (node_id_str, stored_hash) in rows {
        let Ok(node_id) = NodeId::from_str(node_id_str) else {
            continue;
        };
        let Some(snap) = resolve_commentary_node(graph, node_id)? else {
            continue;
        };
        if !in_scope(&snap.file.path, scope_prefixes.as_deref())
            || snap.content_hash == *stored_hash
        {
            continue;
        }
        refresh.push(work_item(node_id, &snap, CommentaryWorkPhase::Refresh));
    }

    for (path, file_id) in file_rows {
        files_scanned += 1;
        maybe_emit_scan_progress(
            &mut progress,
            files_scanned,
            file_total,
            symbols_scanned,
            symbol_total,
            scan_emit_interval(file_total),
            false,
        );
        let scoped = in_scope(&path, scope_prefixes.as_deref());
        file_paths.insert(file_id, path.clone());
        if scoped {
            scoped_files += 1;
        }
        let node_id = NodeId::File(file_id);
        if commented.contains(&node_id) || !scoped {
            continue;
        }
        let Some(snap) = resolve_commentary_node(graph, node_id)? else {
            continue;
        };
        file_seeds.push(work_item(node_id, &snap, CommentaryWorkPhase::Seed));
    }

    for (sym_id, _file_id, qualified_name, _kind, _body_hash) in symbol_rows {
        symbols_scanned += 1;
        maybe_emit_scan_progress(
            &mut progress,
            files_scanned,
            file_total,
            symbols_scanned,
            symbol_total,
            scan_emit_interval(symbol_total),
            true,
        );
        if qualified_name.is_empty() {
            continue;
        }
        let Some(path) = file_paths.get(&_file_id) else {
            continue;
        };
        if !in_scope(path, scope_prefixes.as_deref()) {
            continue;
        }
        scoped_symbols += 1;
        let node_id = NodeId::Symbol(sym_id);
        if commented.contains(&node_id) {
            continue;
        }
        let Some(snap) = resolve_commentary_node(graph, node_id)? else {
            continue;
        };
        if commented.contains(&NodeId::File(snap.file.id)) {
            continue;
        }
        symbol_seed_candidates.push(work_item(node_id, &snap, CommentaryWorkPhase::Seed));
    }

    Ok(CommentaryWorkPlan {
        refresh,
        file_seeds,
        symbol_seed_candidates,
        scoped_files,
        scoped_symbols,
    })
}

fn emit_scan_progress(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    files_scanned: usize,
    files_total: usize,
    symbols_scanned: usize,
    symbols_total: usize,
) {
    if let Some(progress) = progress.as_mut() {
        progress(CommentaryProgressEvent::ScanProgress {
            files_scanned,
            files_total,
            symbols_scanned,
            symbols_total,
        });
    }
}

fn maybe_emit_scan_progress(
    progress: &mut Option<&mut dyn FnMut(CommentaryProgressEvent)>,
    files_scanned: usize,
    files_total: usize,
    symbols_scanned: usize,
    symbols_total: usize,
    interval: usize,
    symbol_phase: bool,
) {
    let should_emit = if symbol_phase {
        symbols_scanned == symbols_total
            || (interval > 0 && symbols_scanned > 0 && symbols_scanned.is_multiple_of(interval))
    } else {
        files_scanned == files_total
            || (interval > 0 && files_scanned > 0 && files_scanned.is_multiple_of(interval))
    };
    if should_emit {
        emit_scan_progress(
            progress,
            files_scanned,
            files_total,
            symbols_scanned,
            symbols_total,
        );
    }
}

fn work_item(
    node_id: NodeId,
    snap: &CommentaryNodeSnapshot,
    phase: CommentaryWorkPhase,
) -> CommentaryWorkItem {
    CommentaryWorkItem {
        node_id,
        file_id: snap.file.id,
        phase,
        path: snap.file.path.clone(),
        qualified_name: snap.symbol.as_ref().map(|sym| sym.qualified_name.clone()),
    }
}
