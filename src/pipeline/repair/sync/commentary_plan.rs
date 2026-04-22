//! Commentary work planning and scope helpers.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::{
    core::ids::{FileNodeId, NodeId},
    pipeline::repair::commentary::{resolve_commentary_node, CommentaryNodeSnapshot},
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

/// Fixed phases for commentary work.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentaryWorkPhase {
    Refresh,
    Seed,
}

/// Planned commentary work item.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentaryWorkItem {
    pub node_id: NodeId,
    pub file_id: FileNodeId,
    pub phase: CommentaryWorkPhase,
    pub path: String,
    pub qualified_name: Option<String>,
}

/// Plan for one `synrepo synthesize` run.
#[allow(missing_docs)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommentaryWorkPlan {
    pub refresh: Vec<CommentaryWorkItem>,
    pub file_seeds: Vec<CommentaryWorkItem>,
    pub symbol_seed_candidates: Vec<CommentaryWorkItem>,
}

#[allow(missing_docs)]
impl CommentaryWorkPlan {
    pub fn refresh_count(&self) -> usize {
        self.refresh.len()
    }

    pub fn file_seed_count(&self) -> usize {
        self.file_seeds.len()
    }

    pub fn symbol_seed_candidate_count(&self) -> usize {
        self.symbol_seed_candidates.len()
    }

    pub fn max_target_count(&self) -> usize {
        self.refresh_count() + self.file_seed_count() + self.symbol_seed_candidate_count()
    }

    pub fn is_empty(&self) -> bool {
        self.max_target_count() == 0
    }
}

/// Structured progress emitted while commentary refresh runs.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommentaryProgressEvent {
    PlanReady {
        refresh: usize,
        file_seeds: usize,
        symbol_seed_candidates: usize,
        max_targets: usize,
    },
    TargetStarted {
        item: CommentaryWorkItem,
        current: usize,
    },
    TargetFinished {
        item: CommentaryWorkItem,
        current: usize,
        generated: bool,
    },
    DocsDirCreated {
        path: PathBuf,
    },
    DocWritten {
        path: PathBuf,
    },
    DocDeleted {
        path: PathBuf,
    },
    IndexDirCreated {
        path: PathBuf,
    },
    IndexUpdated {
        path: PathBuf,
        touched_paths: usize,
    },
    IndexRebuilt {
        path: PathBuf,
        touched_paths: usize,
    },
    PhaseSummary {
        phase: CommentaryWorkPhase,
        attempted: usize,
        generated: usize,
        not_generated: usize,
    },
    RunSummary {
        refreshed: usize,
        seeded: usize,
        not_generated: usize,
        attempted: usize,
    },
}

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
    let scope_prefixes = scope.map(normalize_scope_prefixes);
    let commented: HashSet<NodeId> = rows
        .iter()
        .filter_map(|(id, _)| NodeId::from_str(id).ok())
        .collect();
    let mut refresh = Vec::new();
    let mut file_seeds = Vec::new();
    let mut symbol_seed_candidates = Vec::new();

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

    for (path, file_id) in graph.all_file_paths()? {
        let node_id = NodeId::File(file_id);
        if commented.contains(&node_id) || !in_scope(&path, scope_prefixes.as_deref()) {
            continue;
        }
        let Some(snap) = resolve_commentary_node(graph, node_id)? else {
            continue;
        };
        file_seeds.push(work_item(node_id, &snap, CommentaryWorkPhase::Seed));
    }

    for (sym_id, _file_id, qualified_name, _kind, _body_hash) in graph.all_symbols_summary()? {
        if qualified_name.is_empty() {
            continue;
        }
        let node_id = NodeId::Symbol(sym_id);
        if commented.contains(&node_id) {
            continue;
        }
        let Some(snap) = resolve_commentary_node(graph, node_id)? else {
            continue;
        };
        if !in_scope(&snap.file.path, scope_prefixes.as_deref())
            || commented.contains(&NodeId::File(snap.file.id))
        {
            continue;
        }
        symbol_seed_candidates.push(work_item(node_id, &snap, CommentaryWorkPhase::Seed));
    }

    Ok(CommentaryWorkPlan {
        refresh,
        file_seeds,
        symbol_seed_candidates,
    })
}

fn in_scope(path: &str, prefixes: Option<&[String]>) -> bool {
    match prefixes {
        None => true,
        Some(p) => path_matches_any_prefix(path, p),
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

/// Convert scope `PathBuf`s into `/`-normalized, trailing-slash-terminated
/// string prefixes so a prefix-match cannot spuriously accept sibling
/// directories (`src` matching `src-extra/...`).
pub fn normalize_scope_prefixes(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            let mut s = p.to_string_lossy().replace('\\', "/");
            if !s.is_empty() && !s.ends_with('/') {
                s.push('/');
            }
            s
        })
        .collect()
}

/// True if `file_path` (stored as recorded in the graph, possibly with
/// backslashes on Windows) starts with any of the normalized prefixes.
pub fn path_matches_any_prefix(file_path: &str, prefixes: &[String]) -> bool {
    let normalized = file_path.replace('\\', "/");
    prefixes.iter().any(|p| normalized.starts_with(p.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_is_terminated_with_slash() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert_eq!(prefixes, vec!["src/".to_string()]);
    }

    #[test]
    fn prefix_sibling_does_not_match() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert!(path_matches_any_prefix("src/lib.rs", &prefixes));
        assert!(!path_matches_any_prefix("src-extra/lib.rs", &prefixes));
    }

    #[test]
    fn backslash_paths_match_forward_slash_prefix() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert!(path_matches_any_prefix("src\\lib.rs", &prefixes));
    }

    #[test]
    fn empty_scope_matches_nothing() {
        let prefixes = normalize_scope_prefixes(&[]);
        assert!(!path_matches_any_prefix("src/lib.rs", &prefixes));
    }

    #[test]
    fn nested_prefix_match() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("crates/core/src")]);
        assert!(path_matches_any_prefix("crates/core/src/lib.rs", &prefixes));
        assert!(!path_matches_any_prefix(
            "crates/core/tests/a.rs",
            &prefixes
        ));
    }
}
