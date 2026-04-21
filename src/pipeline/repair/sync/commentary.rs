//! Commentary refresh helpers for repair sync.

use std::path::PathBuf;
use std::str::FromStr;

use crate::{
    core::ids::NodeId,
    overlay::{CommentaryProvenance, OverlayStore},
    pipeline::{
        repair::commentary::resolve_commentary_node,
        synthesis::{
            build_commentary_generator,
            docs::{reconcile_commentary_docs, sync_commentary_index},
            CommentaryGenerator,
        },
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

use super::handlers::ActionContext;

/// Refresh stale commentary entries.
///
/// When `scope` is `Some(paths)`, only files whose path starts with one of the
/// prefixes is considered. Prefixes are repo-root-relative; each is normalized
/// to end in `/` so `src` cannot spuriously match `src-extra/...`.
pub fn refresh_commentary(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
    scope: Option<&[PathBuf]>,
) -> crate::Result<()> {
    let overlay_dir = context.synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open_existing(&overlay_dir)?;
    let graph = SqliteGraphStore::open_existing(&context.synrepo_dir.join("graph"))?;
    let generator: Box<dyn CommentaryGenerator> =
        build_commentary_generator(context.config, context.config.commentary_cost_limit);

    let scope_prefixes: Option<Vec<String>> = scope.map(normalize_scope_prefixes);

    let rows = overlay.commentary_hashes()?;
    let mut refreshed = 0usize;
    let mut skipped = 0usize;
    let mut out_of_scope = 0usize;

    for (node_id_str, stored_hash) in rows {
        let Ok(node_id) = NodeId::from_str(&node_id_str) else {
            skipped += 1;
            continue;
        };
        let Some(snap) = resolve_commentary_node(&graph, node_id)? else {
            skipped += 1;
            continue;
        };
        if let Some(prefixes) = &scope_prefixes {
            if !path_matches_any_prefix(&snap.file.path, prefixes) {
                out_of_scope += 1;
                continue;
            }
        }
        if snap.content_hash == stored_hash {
            continue;
        }

        let ctx_text = match &snap.symbol {
            Some(sym) => format!(
                "Symbol {} in {}\nSignature: {}\nDoc: {}",
                sym.qualified_name,
                snap.file.path,
                sym.signature.clone().unwrap_or_default(),
                sym.doc_comment.clone().unwrap_or_default(),
            ),
            None => format!("File: {}", snap.file.path),
        };

        let Some(mut entry) = generator.generate(node_id, &ctx_text)? else {
            skipped += 1;
            continue;
        };
        entry.provenance = CommentaryProvenance {
            source_content_hash: snap.content_hash,
            ..entry.provenance
        };
        overlay.insert_commentary(entry)?;
        refreshed += 1;
    }

    let touched = reconcile_commentary_docs(context.synrepo_dir, &graph, Some(&overlay))?;
    sync_commentary_index(context.synrepo_dir, &touched)?;

    let scope_note = if scope_prefixes.is_some() {
        format!(", {out_of_scope} outside scope")
    } else {
        String::new()
    };
    actions_taken.push(format!(
        "commentary refresh: {refreshed} regenerated, {skipped} skipped (no hash change or no generator output){scope_note}"
    ));
    Ok(())
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
