//! TestSurfaceCard compilation via path-convention heuristics.

use std::collections::HashMap;

use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::graph::{EdgeKind, GraphReader, SymbolKind, SymbolNode},
    surface::card::{
        types::{TestAssociation, TestEntry, TestSurfaceCard},
        Budget, SourceStore,
    },
};

/// Compile a TestSurfaceCard by discovering test files related to a scope.
pub(super) fn test_surface_card_impl(
    graph: &dyn GraphReader,
    scope: &str,
    budget: Budget,
) -> crate::Result<TestSurfaceCard> {
    // Get all file paths and their IDs.
    let file_paths = graph.all_file_paths()?;
    let path_map: HashMap<String, FileNodeId> =
        file_paths.iter().map(|(p, id)| (p.clone(), *id)).collect();

    // Filter files that match the scope (path prefix).
    let scope_files: Vec<(FileNodeId, String)> = file_paths
        .iter()
        .filter(|(path, _)| path.starts_with(scope))
        .map(|(path, id)| (*id, path.clone()))
        .collect();

    // For each source file, find associated test files.
    let mut all_tests: Vec<TestEntry> = Vec::new();
    let mut test_file_count = 0;

    for (_source_file_id, source_path) in &scope_files {
        let test_files = find_associated_test_files(source_path, &path_map);

        if !test_files.is_empty() {
            test_file_count += 1;
        }

        for test_file_path in test_files {
            let Some(test_file_id) = path_map.get(&test_file_path).copied() else {
                continue;
            };

            // Get test symbols from the test file.
            let test_symbols = find_test_symbols(graph, test_file_id)?;

            for symbol in test_symbols {
                let association = compute_association(&symbol.kind, &test_file_path, source_path);

                let entry = TestEntry {
                    symbol_id: symbol.id,
                    qualified_name: symbol.qualified_name.clone(),
                    file_path: test_file_path.clone(),
                    source_file: source_path.clone(),
                    association,
                    signature: None,
                    doc_comment: None,
                    covers: None,
                };

                all_tests.push(entry);
            }
        }
    }

    // Apply budget-tier truncation.
    let (final_tests, include_deep_fields) = match budget {
        Budget::Tiny => {
            // Tiny: counts only, no individual entries.
            (vec![], false)
        }
        Budget::Normal => (all_tests, false),
        Budget::Deep => (all_tests, true),
    };

    // Populate deep fields if needed.
    let final_tests = if include_deep_fields {
        final_tests
            .into_iter()
            .map(|mut entry| {
                // Add signature and doc_comment from the symbol.
                if let Ok(Some(symbol)) = graph.get_symbol(entry.symbol_id) {
                    entry.signature = symbol.signature.clone();
                    entry.doc_comment = symbol.doc_comment.as_ref().map(|s| {
                        if s.len() > 120 {
                            format!("{}…", &s[..120])
                        } else {
                            s.clone()
                        }
                    });
                    // Add covers field from Calls edges at Deep budget.
                    let calls =
                        graph.outbound(NodeId::Symbol(entry.symbol_id), Some(EdgeKind::Calls));
                    entry.covers = calls.ok().map(|edges| {
                        edges
                            .iter()
                            .filter_map(|e| {
                                if let NodeId::Symbol(to_id) = e.to {
                                    Some(to_id)
                                } else {
                                    None
                                }
                            })
                            .collect()
                    });
                }
                entry
            })
            .collect()
    } else {
        final_tests
    };

    let test_symbol_count = final_tests.len();
    let approx_tokens = estimate_test_surface_tokens(&final_tests, budget);

    Ok(TestSurfaceCard {
        scope: scope.to_string(),
        tests: final_tests,
        test_file_count,
        test_symbol_count,
        approx_tokens,
        source_store: SourceStore::Graph,
    })
}

/// Check if a symbol name looks like a test (common naming conventions).
fn is_test_symbol(name: &str) -> bool {
    name.starts_with("test_")
        || name.starts_with("Test")
        || name.ends_with("_test")
        || name.ends_with("_tests")
        || name.contains("test_")
        || name.contains("_test")
}

/// Find test files associated with a source file using path conventions.
fn find_associated_test_files(
    source_path: &str,
    path_map: &HashMap<String, FileNodeId>,
) -> Vec<String> {
    let mut test_files = Vec::new();

    // Get directory and stem of source file.
    let (dir, stem) = match source_path.rsplit_once('/') {
        Some((d, s)) => (d.to_string(), s),
        None => return vec![],
    };

    // Remove extension to get stem.
    let stem = stem.rsplit_once('.').map(|(s, _)| s).unwrap_or(stem);

    for path in path_map.keys() {
        // 1. Sibling test file patterns.
        let sibling_patterns = vec![
            // Rust: stem_test.rs, tests/stem.rs
            format!("{}/{}_test.rs", dir, stem),
            format!("{}/tests/{}.rs", dir, stem),
            format!("{}/test_{}.rs", dir, stem),
            // Python: test_stem.py
            format!("{}/test_{}.py", dir, stem),
            format!("{}/tests/test_{}.py", dir, stem),
            // TypeScript/TSX: stem.test.ts, stem.spec.ts
            format!("{}/{}.test.ts", dir, stem),
            format!("{}/{}.spec.ts", dir, stem),
            format!("{}/{}.test.tsx", dir, stem),
            format!("{}/{}.spec.tsx", dir, stem),
            // Go: stem_test.go
            format!("{}/{}_test.go", dir, stem),
            // Parallel test directory patterns.
            format!("tests/{}.rs", stem),
            format!("tests/{}.py", stem),
        ];

        for pattern in &sibling_patterns {
            if path.as_str() == pattern.as_str() && !test_files.contains(path) {
                test_files.push(path.clone());
            }
        }

        // 2. Nested test module: <source_dir>/tests/ or <source_dir>/__tests__/
        if (path.starts_with(&format!("{}/tests/", dir))
            || path.starts_with(&format!("{}/__tests__/", dir)))
            && !test_files.contains(path)
        {
            test_files.push(path.clone());
        }
    }

    test_files
}

/// Find test symbols from a test file by name convention.
fn find_test_symbols(
    graph: &dyn GraphReader,
    file_id: FileNodeId,
) -> crate::Result<Vec<SymbolNode>> {
    let all_symbols = graph.symbols_for_file(file_id)?;
    let test_symbols = all_symbols
        .into_iter()
        .filter(|sym| is_test_symbol(&sym.qualified_name))
        .collect();

    Ok(test_symbols)
}

/// Compute the association field.
fn compute_association(_kind: &SymbolKind, test_path: &str, source_path: &str) -> TestAssociation {
    // For now, path convention is the primary signal since SymbolKind::Test doesn't exist.
    // We check if the test file path matches path conventions.
    let has_path_convention = test_matches_path_convention(test_path, source_path);

    if has_path_convention {
        TestAssociation::PathConvention
    } else {
        TestAssociation::SymbolKind
    }
}

/// Check if a test file matches path convention for a source file.
fn test_matches_path_convention(test_path: &str, source_path: &str) -> bool {
    // Extract stem from source path.
    let source_stem = source_path
        .rsplit_once('/')
        .map(|(_, s)| s)
        .unwrap_or(source_path);
    let source_stem = source_stem
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(source_stem);

    // Check if test path matches any convention.
    let test_name = test_path
        .rsplit_once('/')
        .map(|(_, n)| n)
        .unwrap_or(test_path);

    // Check patterns.
    let patterns = [
        format!("{}_test", source_stem),
        format!("test_{}", source_stem),
        format!("{}.test", source_stem),
        format!("{}.spec", source_stem),
    ];

    for pattern in &patterns {
        if test_name.starts_with(pattern) || test_name.contains(pattern) {
            return true;
        }
    }

    // Check nested test directory.
    let source_dir = source_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    if test_path.starts_with(&format!("{}/tests/", source_dir))
        || test_path.starts_with(&format!("{}/__tests__/", source_dir))
    {
        return true;
    }

    false
}

/// Estimate token count for test surface.
fn estimate_test_surface_tokens(entries: &[TestEntry], budget: Budget) -> usize {
    let base = 50; // Card overhead.
    match budget {
        Budget::Tiny => base + 20, // Just counts.
        Budget::Normal => base + entries.len() * 40,
        Budget::Deep => base + entries.len() * 100,
    }
}
