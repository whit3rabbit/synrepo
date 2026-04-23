//! `PublicAPICard` compilation from graph-derived directory facts.
//!
//! Collects exported symbols from direct-child files by reading the
//! `SymbolNode.visibility` field. Entry points are detected using the same
//! four-rule taxonomy as `EntryPointCard`. Recent API changes are symbols
//! whose containing file was last touched within 30 days (`Deep` budget only).
//!
//! # Cross-language visibility
//!
//! `PublicAPICard` now emits symbols for Rust, Python, TypeScript, and Go.
//! - Rust: `pub` -> Public, `pub(crate)` -> Crate, no prefix -> Private.
//! - Python: dunders and non-underscore names -> Public, `_name` -> Private.
//! - TypeScript: wrapped in `export` -> Public, otherwise -> Public (v1).
//! - Go: uppercase first char -> Public, lowercase -> Private.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    core::ids::FileNodeId,
    structure::graph::{GraphReader, Visibility},
    surface::card::{
        git::symbol_last_change_from_insights,
        types::{PublicAPICard, PublicAPIEntry},
        Budget, ContextAccounting, SourceStore,
    },
};

use super::GraphCardCompiler;

/// Number of days that qualifies a public symbol change as "recent".
const RECENT_API_DAYS: i64 = 30;

/// Compile a `PublicAPICard` for the given directory path.
pub(super) fn public_api_card_impl(
    compiler: &GraphCardCompiler,
    graph: &dyn GraphReader,
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

        let symbols = graph.symbols_for_file(*file_id)?;
        for sym in &symbols {
            // Visibility filter: include Public and Crate, exclude Private and Unknown.
            let is_visible = matches!(sym.visibility, Visibility::Public | Visibility::Crate);
            if !is_visible {
                continue;
            }

            // Signature is optional but we use it if present for the entry.
            let sig = sym.signature.clone().unwrap_or_default();

            public_symbol_count += 1;

            if budget == Budget::Tiny {
                // Count only; don't materialise entries.
                continue;
            }

            let last_change = git_insights
                .as_ref()
                .and_then(|arc| symbol_last_change_from_insights(arc, include_summary, None));

            public_symbols.push(PublicAPIEntry {
                id: sym.id,
                name: sym.display_name.clone(),
                kind: sym.kind,
                signature: sig,
                location: format!("{}:{}", file_path, sym.body_byte_range.0),
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
        context_accounting: ContextAccounting::new(budget, approx_tokens, 0, vec![]),
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

        // All materialised entries have visibility Public or Crate (filtered in compiler).
        // The test above verifies private_helper is excluded.

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

    // Fixture: Python file with public, private, and dunder names.
    fn write_python_fixture(root: &std::path::Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/__init__.py"),
            "class Public:\n\
             pass\n\n\
             class _Private:\n\
             pass\n\n\
             def __init__(self):\n\
             pass\n",
        )
        .unwrap();
    }

    #[test]
    fn public_api_card_emits_for_python_non_dunder_names() {
        let repo = tempdir().unwrap();
        write_python_fixture(repo.path());
        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), None::<&std::path::Path>);

        let card = compiler.public_api_card("src", Budget::Deep).unwrap();

        let names: Vec<_> = card
            .public_symbols
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        // Public and __init__ should be included, _Private excluded.
        assert!(
            names.contains(&"Public"),
            "Public class must be included; got: {names:?}"
        );
        assert!(
            names.contains(&"__init__"),
            "__init__ must be included; got: {names:?}"
        );
        assert!(
            !names.contains(&"_Private"),
            "_Private must be excluded; got: {names:?}"
        );
    }

    // Fixture: TypeScript file with export statement and non-exported class.
    fn write_typescript_fixture(root: &std::path::Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.ts"),
            "export class Foo {}\n\
             class Bar {}\n",
        )
        .unwrap();
    }

    #[test]
    fn public_api_card_emits_for_typescript_export_decl() {
        let repo = tempdir().unwrap();
        write_typescript_fixture(repo.path());
        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), None::<&std::path::Path>);

        let card = compiler.public_api_card("src", Budget::Deep).unwrap();

        let names: Vec<_> = card
            .public_symbols
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        // Foo (exported) must be included.
        assert!(
            names.contains(&"Foo"),
            "Foo must be included; got: {names:?}"
        );
        // Bar: per the design, class-member accessibility_modifier is out of scope
        // for v1, so it defaults to Public. Both are included.
        assert!(
            names.contains(&"Bar"),
            "Bar defaults to Public in v1; got: {names:?}"
        );
    }

    // Fixture: Go file with capitalized and lowercase functions.
    fn write_go_fixture(root: &std::path::Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.go"),
            "package main\n\n\
             func Handle() {}\n\
             func helper() {}\n",
        )
        .unwrap();
    }

    #[test]
    fn public_api_card_emits_for_go_capitalized_ident() {
        let repo = tempdir().unwrap();
        write_go_fixture(repo.path());
        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), None::<&std::path::Path>);

        let card = compiler.public_api_card("src", Budget::Deep).unwrap();

        let names: Vec<_> = card
            .public_symbols
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        // Handle (capitalized) should be included, helper (lowercase) excluded.
        assert!(
            names.contains(&"Handle"),
            "Handle must be included; got: {names:?}"
        );
        assert!(
            !names.contains(&"helper"),
            "helper must be excluded; got: {names:?}"
        );
    }
}
