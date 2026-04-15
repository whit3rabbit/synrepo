//! `PublicAPICard` compilation from graph-derived directory facts.
//!
//! Collects exported symbols from direct-child files by inspecting
//! `SymbolNode.signature` for a `pub` prefix. Entry points are detected
//! using the same four-rule taxonomy as `EntryPointCard`. Recent API changes
//! are symbols whose containing file was last touched within 30 days
//! (`Deep` budget only).
//!
//! # Visibility heuristic
//!
//! `signature.starts_with("pub")` matches Rust's `pub`, `pub(crate)`,
//! `pub(super)`, and `pub(in path)`. For Python, TypeScript, and Go — where
//! visibility is not expressed as a `pub` keyword — `public_symbols` will be
//! empty in v1. A dedicated `visibility` field on `SymbolNode` is the right
//! long-term fix; deferred.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::graph::{EdgeKind, GraphStore},
    surface::card::{
        git::symbol_last_change_from_insights,
        types::{PublicAPICard, PublicAPIEntry},
        Budget, SourceStore,
    },
};

use super::GraphCardCompiler;

/// Number of days that qualifies a public symbol change as "recent".
const RECENT_API_DAYS: i64 = 30;

/// Compile a `PublicAPICard` for the given directory path.
pub(super) fn public_api_card_impl(
    compiler: &GraphCardCompiler,
    graph: &dyn GraphStore,
    path: &str,
    budget: Budget,
) -> crate::Result<PublicAPICard> {
    // Normalise: ensure the prefix ends with `/` for correct child matching.
    let prefix = if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    };

    let all_paths = graph.all_file_paths()?;

    // Collect direct-child file paths (not deeper descendants).
    let mut direct_files: Vec<(String, FileNodeId)> = Vec::new();
    for (file_path, file_id) in &all_paths {
        let Some(suffix) = file_path.strip_prefix(&prefix) else {
            continue;
        };
        if suffix.is_empty() || suffix.contains('/') {
            continue;
        }
        direct_files.push((file_path.clone(), *file_id));
    }
    direct_files.sort_by(|a, b| a.0.cmp(&b.0));

    // Build a map from FileNodeId to path for location formatting.
    let path_map: HashMap<FileNodeId, &str> = direct_files
        .iter()
        .map(|(p, id)| (*id, p.as_str()))
        .collect();

    let include_git = budget != Budget::Tiny;
    let include_summary = budget == Budget::Deep;
    let now = now_unix();

    let mut public_symbols: Vec<PublicAPIEntry> = Vec::new();
    let mut public_symbol_count: usize = 0;

    for (file_path, file_id) in &direct_files {
        let git_insights = if include_git {
            compiler.resolve_file_git_intelligence(file_path)
        } else {
            None
        };

        let defines = graph.outbound(NodeId::File(*file_id), Some(EdgeKind::Defines))?;
        for edge in &defines {
            let NodeId::Symbol(sym_id) = edge.to else {
                continue;
            };
            let Some(sym) = graph.get_symbol(sym_id)? else {
                continue;
            };

            // Visibility filter: Rust `pub` prefix only.
            let sig = match &sym.signature {
                Some(s) if s.starts_with("pub") => s.clone(),
                _ => continue,
            };

            public_symbol_count += 1;

            if budget == Budget::Tiny {
                // Count only; don't materialise entries.
                continue;
            }

            let file_path_str = path_map.get(file_id).copied().unwrap_or("");
            let last_change = git_insights
                .as_ref()
                .and_then(|arc| symbol_last_change_from_insights(arc, include_summary, None));

            public_symbols.push(PublicAPIEntry {
                id: sym_id,
                name: sym.display_name.clone(),
                kind: sym.kind,
                signature: sig,
                location: format!("{}:{}", file_path_str, sym.body_byte_range.0),
                last_change,
            });
        }
    }

    // Public entry points: public symbols that also match an entry-point rule.
    let public_entry_points: Vec<PublicAPIEntry> = if budget == Budget::Tiny {
        vec![]
    } else {
        public_symbols
            .iter()
            .filter(|e| super::entry_point::classify_kind(&e.name, &e.location, e.kind).is_some())
            .cloned()
            .collect()
    };

    // Recent API changes (Deep only): public symbols last touched within 30 days.
    let recent_api_changes: Vec<PublicAPIEntry> = if budget == Budget::Deep {
        public_symbols
            .iter()
            .filter(|e| {
                e.last_change
                    .as_ref()
                    .map(|lc| lc.committed_at_unix > now - RECENT_API_DAYS * 86400)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    } else {
        vec![]
    };

    let per_symbol: usize = match budget {
        Budget::Tiny => 10,
        Budget::Normal => 30,
        Budget::Deep => 60,
    };
    let approx_tokens = public_symbol_count * per_symbol + 20;

    Ok(PublicAPICard {
        path: prefix,
        public_symbols,
        public_symbol_count,
        public_entry_points,
        recent_api_changes,
        approx_tokens,
        source_store: SourceStore::Graph,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::card::compiler::test_support::bootstrap;
    use crate::surface::card::compiler::{CardCompiler, GraphCardCompiler};
    use std::fs;
    use tempfile::tempdir;

    // Fixture: a directory with two Rust files, mix of pub and private symbols.
    fn write_auth_fixture(root: &std::path::Path) {
        fs::create_dir_all(root.join("src/auth")).unwrap();
        fs::write(
            root.join("src/auth/mod.rs"),
            "pub fn authenticate(user: &str) -> bool { true }\n\
             pub(crate) fn internal_check() {}\n\
             fn private_helper() {}\n\
             pub struct Token { pub value: String }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/auth/session.rs"),
            "pub fn create_session() -> u64 { 0 }\n",
        )
        .unwrap();
    }

    #[test]
    fn public_api_card_tiny_returns_count_only() {
        let repo = tempdir().unwrap();
        write_auth_fixture(repo.path());
        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

        let card = compiler.public_api_card("src/auth", Budget::Tiny).unwrap();

        // Tiny: symbols not materialised, but count is populated.
        assert!(card.public_symbol_count > 0, "expected some public symbols");
        assert!(
            card.public_symbols.is_empty(),
            "Tiny must not materialise symbols"
        );
        assert!(card.public_entry_points.is_empty());
        assert!(card.recent_api_changes.is_empty());
        assert!(card.path.ends_with('/'));
    }

    #[test]
    fn public_api_card_normal_excludes_private() {
        let repo = tempdir().unwrap();
        write_auth_fixture(repo.path());
        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

        let card = compiler
            .public_api_card("src/auth", Budget::Normal)
            .unwrap();

        // `private_helper` must not appear.
        let names: Vec<&str> = card
            .public_symbols
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(
            !names.contains(&"private_helper"),
            "private_helper must be excluded; got: {names:?}"
        );

        // All materialised entries must have a pub signature.
        for entry in &card.public_symbols {
            assert!(
                entry.signature.starts_with("pub"),
                "non-pub signature slipped through: {}",
                entry.signature
            );
        }

        // recent_api_changes empty at Normal.
        assert!(card.recent_api_changes.is_empty());
    }

    #[test]
    fn public_api_card_count_matches_normal_list_length() {
        let repo = tempdir().unwrap();
        write_auth_fixture(repo.path());
        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

        let tiny = compiler.public_api_card("src/auth", Budget::Tiny).unwrap();
        let normal = compiler
            .public_api_card("src/auth", Budget::Normal)
            .unwrap();

        assert_eq!(
            tiny.public_symbol_count, normal.public_symbol_count,
            "count must agree across budgets"
        );
        assert_eq!(
            normal.public_symbols.len(),
            normal.public_symbol_count,
            "Normal list length must equal count"
        );
    }

    #[test]
    fn public_api_card_empty_directory_returns_zero() {
        let repo = tempdir().unwrap();
        // Bootstrap needs at least one file.
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn noop() {}\n").unwrap();
        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

        let card = compiler
            .public_api_card("src/empty", Budget::Normal)
            .unwrap();

        assert_eq!(card.public_symbol_count, 0);
        assert!(card.public_symbols.is_empty());
        assert!(card.public_entry_points.is_empty());
    }

    #[test]
    fn public_api_card_deep_no_git_has_empty_recent() {
        let repo = tempdir().unwrap();
        write_auth_fixture(repo.path());
        let graph = bootstrap(&repo);
        // No repo_root → git context absent → recent_api_changes must be empty.
        let compiler = GraphCardCompiler::new(Box::new(graph), None::<&std::path::Path>);

        let card = compiler.public_api_card("src/auth", Budget::Deep).unwrap();

        assert!(
            card.recent_api_changes.is_empty(),
            "no git context → recent_api_changes must be empty"
        );
        // Symbols are still materialised at Deep.
        assert!(!card.public_symbols.is_empty());
    }
}
