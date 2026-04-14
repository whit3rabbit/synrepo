//! `ModuleCard` compilation from graph-derived directory facts.
//!
//! Scopes to the immediate children of the requested directory path:
//! - Files directly inside (not in subdirectories) go into `files`.
//! - Immediate subdirectory paths go into `nested_modules`.
//! - Public symbols are populated at `Normal` and `Deep` budget.

use std::collections::{HashMap, HashSet};

use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::graph::{EdgeKind, GraphStore},
    surface::card::{types::ModuleCard, Budget, FileRef, SourceStore, SymbolRef},
};

/// Compile a `ModuleCard` for the given directory path.
pub(super) fn module_card_impl(
    graph: &dyn GraphStore,
    path: &str,
    budget: Budget,
) -> crate::Result<ModuleCard> {
    // Normalise: ensure the prefix ends with `/` for correct child matching.
    let prefix = if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    };

    let all_paths = graph.all_file_paths()?;

    let mut files: Vec<FileRef> = Vec::new();
    let mut nested_modules: HashSet<String> = HashSet::new();

    // Separate direct children from deeper descendants.
    for (file_path, file_id) in &all_paths {
        let Some(suffix) = file_path.strip_prefix(&prefix) else {
            continue;
        };
        if suffix.is_empty() {
            // Exact match of the prefix itself — skip.
            continue;
        }

        if suffix.contains('/') {
            // Deeper than direct child: extract first segment as subdirectory.
            let subdir = &suffix[..suffix.find('/').unwrap()];
            nested_modules.insert(format!("{prefix}{subdir}"));
        } else {
            // Direct child file.
            files.push(FileRef {
                id: *file_id,
                path: file_path.clone(),
            });
        }
    }

    // Sort for stable output.
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut nested_modules: Vec<String> = nested_modules.into_iter().collect();
    nested_modules.sort();

    // Collect public symbols based on budget.
    let (public_symbols, total_symbol_count) = collect_symbols(graph, &files, budget)?;

    let file_tokens = 15 * files.len();
    let symbol_tokens = match budget {
        Budget::Tiny => 0,
        Budget::Normal => 20 * public_symbols.len(),
        Budget::Deep => 40 * public_symbols.len(),
    };
    let approx_tokens = file_tokens + symbol_tokens + 20;

    Ok(ModuleCard {
        path: prefix,
        files,
        nested_modules,
        public_symbols,
        total_symbol_count,
        approx_tokens,
        source_store: SourceStore::Graph,
    })
}

/// Collect symbols from the given direct-child files.
/// At `Tiny` budget, returns an empty list but still counts symbols.
fn collect_symbols(
    graph: &dyn GraphStore,
    files: &[FileRef],
    budget: Budget,
) -> crate::Result<(Vec<SymbolRef>, usize)> {
    // Build a file-id → path map for location formatting.
    let path_map: HashMap<FileNodeId, &str> =
        files.iter().map(|f| (f.id, f.path.as_str())).collect();

    let mut public_symbols: Vec<SymbolRef> = Vec::new();
    let mut total: usize = 0;

    for file_ref in files {
        let defines = graph.outbound(NodeId::File(file_ref.id), Some(EdgeKind::Defines))?;
        for edge in &defines {
            let NodeId::Symbol(sym_id) = edge.to else {
                continue;
            };
            total += 1;
            // Only materialise symbol details at Normal+ budget.
            if budget != Budget::Tiny {
                if let Some(sym) = graph.get_symbol(sym_id)? {
                    let file_path = path_map.get(&file_ref.id).copied().unwrap_or("");
                    public_symbols.push(SymbolRef {
                        id: sym_id,
                        qualified_name: sym.qualified_name.clone(),
                        location: format!("{}:{}", file_path, sym.body_byte_range.0),
                    });
                }
            }
        }
    }

    Ok((public_symbols, total))
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

    // 7.3: subdirectory files excluded from `files`, appear in `nested_modules`

    #[test]
    fn module_card_excludes_subdirectory_files() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src/auth/jwt")).unwrap();
        fs::write(repo.path().join("src/auth/mod.rs"), "pub fn new() {}\n").unwrap();
        fs::write(
            repo.path().join("src/auth/jwt/verify.rs"),
            "pub fn verify() {}\n",
        )
        .unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.module_card("src/auth", Budget::Tiny).unwrap();

        // Direct child
        assert!(
            card.files.iter().any(|f| f.path == "src/auth/mod.rs"),
            "src/auth/mod.rs must be in files"
        );

        // Subdirectory file must NOT appear in files
        assert!(
            card.files
                .iter()
                .all(|f| f.path != "src/auth/jwt/verify.rs"),
            "jwt/verify.rs must not appear in files directly"
        );

        // Subdirectory path must appear in nested_modules
        assert!(
            card.nested_modules.iter().any(|m| m.contains("jwt")),
            "jwt subdirectory must appear in nested_modules; got {:?}",
            card.nested_modules
        );
    }

    // 7.4: empty directory returns empty file list, no error

    #[test]
    fn module_card_empty_directory_returns_empty_list() {
        let repo = tempdir().unwrap();
        // Bootstrap a minimal repo so the graph store exists.
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn noop() {}\n").unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

        // Request a directory with no indexed files.
        let card = compiler
            .module_card("src/nonexistent", Budget::Tiny)
            .unwrap();
        assert!(
            card.files.is_empty(),
            "empty directory must return empty file list"
        );
        assert_eq!(card.total_symbol_count, 0);
    }
}
