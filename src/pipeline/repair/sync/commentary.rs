//! Commentary refresh helpers for repair sync.

use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;

use crate::{
    core::ids::NodeId,
    overlay::{CommentaryProvenance, OverlayStore},
    pipeline::{
        repair::commentary::{resolve_commentary_node, CommentaryNodeSnapshot},
        synthesis::{
            build_commentary_generator,
            docs::{reconcile_commentary_docs, sync_commentary_index},
            CommentaryGenerator,
        },
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
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
) -> crate::Result<()> {
    let overlay_dir = context.synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir)?;
    let graph = SqliteGraphStore::open_existing(&context.synrepo_dir.join("graph"))?;
    let generator: Box<dyn CommentaryGenerator> =
        build_commentary_generator(context.config, context.config.commentary_cost_limit);

    let scope_prefixes: Option<Vec<String>> = scope.map(normalize_scope_prefixes);

    // Phase 1: refresh existing stale entries.
    let rows = overlay.commentary_hashes()?;
    let mut commented: HashSet<NodeId> = rows
        .iter()
        .filter_map(|(id, _)| NodeId::from_str(id).ok())
        .collect();
    let mut refreshed = 0usize;
    let mut skipped = 0usize;
    let mut out_of_scope = 0usize;

    for (node_id_str, stored_hash) in &rows {
        let Ok(node_id) = NodeId::from_str(node_id_str) else {
            skipped += 1;
            continue;
        };
        let Some(snap) = resolve_commentary_node(&graph, node_id)? else {
            skipped += 1;
            continue;
        };
        if !in_scope(&snap.file.path, scope_prefixes.as_deref()) {
            out_of_scope += 1;
            continue;
        }
        if snap.content_hash == *stored_hash {
            continue;
        }
        if generate_and_insert(&*generator, &mut overlay, node_id, &snap)? {
            refreshed += 1;
        } else {
            skipped += 1;
        }
    }

    // Phase 2: seed commentary for nodes that lack an overlay entry.
    let mut seeded = 0usize;
    let mut seed_skipped = 0usize;

    let file_nodes = graph.all_file_paths()?;
    let symbol_nodes = graph.all_symbols_summary()?;

    for (path, file_id) in &file_nodes {
        let node_id = NodeId::File(*file_id);
        if commented.contains(&node_id) {
            continue;
        }
        if !in_scope(path, scope_prefixes.as_deref()) {
            continue;
        }
        let Some(snap) = resolve_commentary_node(&graph, node_id)? else {
            continue;
        };
        if generate_and_insert(&*generator, &mut overlay, node_id, &snap)? {
            commented.insert(node_id);
            seeded += 1;
        } else {
            seed_skipped += 1;
        }
    }

    for (sym_id, _file_id, qualified_name, _kind, _body_hash) in &symbol_nodes {
        if qualified_name.is_empty() {
            continue;
        }
        let node_id = NodeId::Symbol(*sym_id);
        if commented.contains(&node_id) {
            continue;
        }
        let Some(snap) = resolve_commentary_node(&graph, node_id)? else {
            continue;
        };
        if !in_scope(&snap.file.path, scope_prefixes.as_deref()) {
            continue;
        }
        // Skip symbols whose containing file already has commentary covering
        // the same source context, including files seeded in the loop above.
        if commented.contains(&NodeId::File(snap.file.id)) {
            continue;
        }
        if generate_and_insert(&*generator, &mut overlay, node_id, &snap)? {
            commented.insert(node_id);
            seeded += 1;
        } else {
            seed_skipped += 1;
        }
    }

    let touched = reconcile_commentary_docs(context.synrepo_dir, &graph, Some(&overlay))?;
    sync_commentary_index(context.synrepo_dir, &touched)?;

    let scope_note = if scope_prefixes.is_some() {
        format!(", {out_of_scope} outside scope")
    } else {
        String::new()
    };
    actions_taken.push(format!(
        "commentary: {seeded} seeded, {refreshed} refreshed, {} skipped{scope_note}",
        skipped + seed_skipped,
    ));
    Ok(())
}

fn in_scope(path: &str, prefixes: Option<&[String]>) -> bool {
    match prefixes {
        None => true,
        Some(p) => path_matches_any_prefix(path, p),
    }
}

fn generate_and_insert(
    generator: &dyn CommentaryGenerator,
    overlay: &mut SqliteOverlayStore,
    node_id: NodeId,
    snap: &CommentaryNodeSnapshot,
) -> crate::Result<bool> {
    let ctx_text = build_context_text(snap);
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

fn build_context_text(snap: &CommentaryNodeSnapshot) -> String {
    match &snap.symbol {
        Some(sym) => format!(
            "Symbol {} in {}\nSignature: {}\nDoc: {}",
            sym.qualified_name,
            snap.file.path,
            sym.signature.clone().unwrap_or_default(),
            sym.doc_comment.clone().unwrap_or_default(),
        ),
        None => format!("File: {}", snap.file.path),
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
