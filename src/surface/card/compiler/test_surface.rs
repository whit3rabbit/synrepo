//! TestSurfaceCard compilation via path-convention heuristics.

use std::collections::HashMap;

use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::graph::{EdgeKind, GraphReader, SymbolKind, SymbolNode},
    surface::card::{
        accounting::ContextAccounting,
        types::{TestAssociation, TestEntry, TestSurfaceCard},
        Budget, SourceStore,
    },
    util::test_paths,
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
            let test_symbols = find_test_symbols(graph, test_file_id, &test_file_path)?;

            for symbol in test_symbols {
                let association = compute_association(&symbol.kind, &test_file_path, source_path);
                let (risk_score, risk_reasons) =
                    test_risk(graph, symbol.id, source_path, association)?;

                let entry = TestEntry {
                    symbol_id: symbol.id,
                    qualified_name: symbol.qualified_name.clone(),
                    file_path: test_file_path.clone(),
                    source_file: source_path.clone(),
                    association,
                    signature: None,
                    doc_comment: None,
                    covers: None,
                    risk_score: Some(risk_score),
                    risk_reasons,
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

    let mut final_tests = final_tests;
    final_tests.sort_by(|a, b| {
        b.risk_score
            .unwrap_or(0.0)
            .partial_cmp(&a.risk_score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });

    let test_symbol_count = final_tests.len();
    let approx_tokens = estimate_test_surface_tokens(&final_tests, budget);

    Ok(TestSurfaceCard {
        scope: scope.to_string(),
        tests: final_tests,
        test_file_count,
        test_symbol_count,
        approx_tokens,
        context_accounting: ContextAccounting::new(budget, approx_tokens, 0, vec![]),
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
    let mut test_files = path_map
        .keys()
        .filter(|path| test_paths::matches_path_convention(path, source_path))
        .cloned()
        .collect::<Vec<_>>();
    test_files.sort();
    test_files.dedup();
    test_files
}

/// Find test symbols from a test file by name convention.
fn find_test_symbols(
    graph: &dyn GraphReader,
    file_id: FileNodeId,
    file_path: &str,
) -> crate::Result<Vec<SymbolNode>> {
    let all_symbols = graph.symbols_for_file(file_id)?;
    let mut test_symbols: Vec<_> = all_symbols
        .into_iter()
        .filter(|sym| is_test_symbol(&sym.qualified_name))
        .collect();
    if test_symbols.is_empty() && is_dart_test_path(file_path) {
        test_symbols = graph
            .symbols_for_file(file_id)?
            .into_iter()
            .filter(|sym| sym.qualified_name == "main")
            .collect();
    }

    Ok(test_symbols)
}

fn is_dart_test_path(path: &str) -> bool {
    path.starts_with("test/") && path.ends_with("_test.dart")
}

/// Compute the association field.
fn compute_association(_kind: &SymbolKind, test_path: &str, source_path: &str) -> TestAssociation {
    if test_paths::matches_path_convention(test_path, source_path) {
        TestAssociation::PathConvention
    } else {
        TestAssociation::SymbolKind
    }
}

fn test_risk(
    graph: &dyn GraphReader,
    test_symbol_id: crate::core::ids::SymbolNodeId,
    source_path: &str,
    association: TestAssociation,
) -> crate::Result<(f32, Vec<String>)> {
    let mut score: f32 = match association {
        TestAssociation::Both => 0.55,
        TestAssociation::SymbolKind => 0.4,
        TestAssociation::PathConvention => 0.3,
    };
    let mut reasons = vec![association.as_risk_reason().to_string()];

    for edge in graph.outbound(NodeId::Symbol(test_symbol_id), Some(EdgeKind::Calls))? {
        let NodeId::Symbol(target_id) = edge.to else {
            continue;
        };
        let Some(target) = graph.get_symbol(target_id)? else {
            continue;
        };
        let Some(file) = graph.get_file(target.file_id)? else {
            continue;
        };
        if file.path == source_path {
            score = score.max(0.9);
            reasons.push("direct_call_to_source_symbol".to_string());
            break;
        }
    }

    reasons.sort();
    reasons.dedup();
    Ok((score, reasons))
}

impl TestAssociation {
    fn as_risk_reason(self) -> &'static str {
        match self {
            Self::SymbolKind => "test_symbol_name",
            Self::PathConvention => "path_convention",
            Self::Both => "path_and_symbol_signals",
        }
    }
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
