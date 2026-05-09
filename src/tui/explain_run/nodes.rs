use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;
use parking_lot::Mutex;

use crate::config::Config;
use crate::core::ids::{FileNodeId, NodeId};
use crate::pipeline::explain::build_commentary_generator;
use crate::pipeline::repair::{CommentaryProgressEvent, CommentaryWorkItem, CommentaryWorkPhase};
use crate::store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore};
use crate::surface::card::compiler::GraphCardCompiler;
use crate::surface::commentary_scope::{self, CommentaryRefreshScope};
use crate::tui::app::GenerateCommentaryScope;

pub(super) fn resolve_scoped_nodes(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    scope: GenerateCommentaryScope,
    target: &str,
) -> anyhow::Result<Vec<NodeId>> {
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;
    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo_root)).with_config(config.clone());
    commentary_scope::resolve_refresh_nodes(
        &compiler,
        synrepo_dir,
        to_refresh_scope(scope),
        Some(target),
    )
}

pub(super) fn run_scoped_nodes(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    nodes: &[NodeId],
    event_tx: &Sender<CommentaryProgressEvent>,
    cancel: &AtomicBool,
) -> crate::Result<()> {
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;
    let overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay"))?;
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo_root))
        .with_config(config.clone())
        .with_overlay(Some(Arc::new(Mutex::new(overlay))));
    let generator = build_commentary_generator(config, config.commentary_cost_limit);

    let total = nodes.len();
    emit(
        event_tx,
        CommentaryProgressEvent::ScanProgress {
            files_scanned: total,
            files_total: total,
            symbols_scanned: 0,
            symbols_total: 0,
        },
    );
    emit(
        event_tx,
        CommentaryProgressEvent::PlanReady {
            refresh: total,
            file_seeds: 0,
            symbol_seed_candidates: 0,
            scoped_files: total,
            scoped_symbols: 0,
            max_targets: total,
        },
    );

    let mut attempted = 0usize;
    let mut generated = 0usize;
    let mut not_generated = 0usize;
    let mut stopped = false;
    for node_id in nodes {
        if cancel.load(Ordering::Relaxed) {
            stopped = true;
            break;
        }
        attempted += 1;
        let item = work_item_for_node(&compiler, *node_id)?;
        emit(
            event_tx,
            CommentaryProgressEvent::TargetStarted {
                item: item.clone(),
                current: attempted,
            },
        );
        match compiler.refresh_commentary(*node_id, &*generator) {
            Ok(Some(_)) => {
                generated += 1;
                emit_finished(event_tx, item, attempted, true, None);
            }
            Ok(None) => {
                not_generated += 1;
                emit_finished(
                    event_tx,
                    item,
                    attempted,
                    false,
                    Some("provider returned no commentary".to_string()),
                );
            }
            Err(error) => {
                not_generated += 1;
                emit_finished(event_tx, item, attempted, false, Some(error.to_string()));
            }
        }
    }
    emit(
        event_tx,
        CommentaryProgressEvent::RunSummary {
            refreshed: generated,
            seeded: 0,
            not_generated,
            attempted,
            stopped,
            queued_for_next_run: 0,
            skip_reasons: Vec::new(),
        },
    );
    Ok(())
}

fn to_refresh_scope(scope: GenerateCommentaryScope) -> CommentaryRefreshScope {
    match scope {
        GenerateCommentaryScope::Target => CommentaryRefreshScope::Target,
        GenerateCommentaryScope::File => CommentaryRefreshScope::File,
        GenerateCommentaryScope::Directory => CommentaryRefreshScope::Directory,
    }
}

fn work_item_for_node(
    compiler: &GraphCardCompiler,
    node_id: NodeId,
) -> crate::Result<CommentaryWorkItem> {
    let reader = compiler.reader();
    match node_id {
        NodeId::File(file_id) => {
            let path = reader
                .get_file(file_id)?
                .map(|file| file.path)
                .unwrap_or_else(|| node_id.to_string());
            Ok(CommentaryWorkItem {
                node_id,
                file_id,
                phase: CommentaryWorkPhase::Seed,
                path,
                qualified_name: None,
            })
        }
        NodeId::Symbol(symbol_id) => {
            let Some(symbol) = reader.get_symbol(symbol_id)? else {
                return Ok(fallback_work_item(node_id));
            };
            let path = reader
                .get_file(symbol.file_id)?
                .map(|file| file.path)
                .unwrap_or_else(|| symbol.file_id.to_string());
            Ok(CommentaryWorkItem {
                node_id,
                file_id: symbol.file_id,
                phase: CommentaryWorkPhase::Seed,
                path,
                qualified_name: Some(symbol.qualified_name),
            })
        }
        NodeId::Concept(_) => Ok(fallback_work_item(node_id)),
    }
}

fn fallback_work_item(node_id: NodeId) -> CommentaryWorkItem {
    CommentaryWorkItem {
        node_id,
        file_id: FileNodeId(0),
        phase: CommentaryWorkPhase::Seed,
        path: node_id.to_string(),
        qualified_name: None,
    }
}

fn emit(event_tx: &Sender<CommentaryProgressEvent>, event: CommentaryProgressEvent) {
    let _ = event_tx.try_send(event);
}

fn emit_finished(
    event_tx: &Sender<CommentaryProgressEvent>,
    item: CommentaryWorkItem,
    current: usize,
    generated: bool,
    skip_message: Option<String>,
) {
    emit(
        event_tx,
        CommentaryProgressEvent::TargetFinished {
            item,
            current,
            generated,
            skip_reason: None,
            skip_message,
            retry_attempts: 0,
            queued_for_next_run: false,
        },
    );
}
