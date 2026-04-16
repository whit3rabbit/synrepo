//! CallPathCard compilation via backward BFS over `Calls` edges.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    core::ids::{NodeId, SymbolNodeId},
    structure::graph::{Edge, EdgeKind, GraphStore, SymbolKind, SymbolNode},
    surface::card::{
        types::{CallPath, CallPathCard, SymbolRef},
        Budget, SourceStore,
    },
};

/// Compile a CallPathCard by tracing backward from the target symbol to entry points.
pub(super) fn call_path_card_impl(
    graph: &dyn GraphStore,
    target: SymbolNodeId,
    budget: Budget,
) -> crate::Result<CallPathCard> {
    // Get target symbol info.
    let Some(target_symbol) = graph.get_symbol(target)? else {
        // Target doesn't exist - return empty card.
        return Ok(CallPathCard {
            target: SymbolRef {
                id: target,
                qualified_name: String::new(),
                location: String::new(),
            },
            paths: vec![],
            paths_omitted: 0,
            approx_tokens: 0,
            source_store: SourceStore::Graph,
        });
    };

    let target_ref = make_symbol_ref(&target_symbol, graph)?;

    // Determine max depth based on budget.
    let max_depth = match budget {
        Budget::Tiny => 8,
        Budget::Normal => 8,
        Budget::Deep => 12,
    };

    // Find all entry points in the graph.
    let entry_points = find_entry_points(graph)?;

    // Build adjacency map: for each symbol, who calls it?
    let calls_into: HashMap<SymbolNodeId, Vec<Edge>> = build_call_graph(graph)?;

    // Backward BFS from target to find paths to entry points.
    let paths = backward_bfs(graph, target, &calls_into, &entry_points, max_depth)?;

    // Deduplicate and cap at 3 paths per (entry_point, target) pair.
    let (deduplicated_paths, total_omitted) = deduplicate_paths(paths);

    // Convert to CallPath objects and apply budget-tier truncation.
    let call_paths = convert_to_call_paths(graph, deduplicated_paths, budget)?;

    // Calculate approximate tokens.
    let approx_tokens = estimate_call_path_tokens(&call_paths, budget);

    Ok(CallPathCard {
        target: target_ref,
        paths: call_paths,
        paths_omitted: total_omitted,
        approx_tokens,
        source_store: SourceStore::Graph,
    })
}

/// Find all entry point symbols in the graph.
fn find_entry_points(graph: &dyn GraphStore) -> crate::Result<HashSet<SymbolNodeId>> {
    let symbol_names = graph.all_symbol_names()?;
    let file_paths = graph.all_file_paths()?;

    let path_map: HashMap<_, _> = file_paths.into_iter().map(|(p, id)| (id, p)).collect();

    let mut entry_points = HashSet::new();

    for (sym_id, file_id, qname) in &symbol_names {
        let Some(path) = path_map.get(file_id) else {
            continue;
        };

        let Some(symbol) = graph.get_symbol(*sym_id)? else {
            continue;
        };

        if is_entry_point(qname, path, symbol.kind) {
            entry_points.insert(*sym_id);
        }
    }

    Ok(entry_points)
}

/// Check if a symbol is an entry point.
fn is_entry_point(qname: &str, path: &str, kind: SymbolKind) -> bool {
    // Binary entry: main in src/main.rs or src/bin/
    if qname == "main" && (path.ends_with("src/main.rs") || path.contains("src/bin/")) {
        return true;
    }

    // CLI command: functions in cli/command/cmd paths
    if matches!(kind, SymbolKind::Function) {
        let path_lower = path.to_lowercase();
        if path_lower.contains("/cli/")
            || path_lower.contains("/command/")
            || path_lower.contains("/cmd/")
        {
            return true;
        }
        // Also check name prefixes for common CLI patterns.
        if qname.starts_with("handle_")
            || qname.starts_with("serve_")
            || qname.starts_with("route_")
        {
            return true;
        }
    }

    // LibRoot: top-level items in lib.rs or mod.rs
    if path.ends_with("lib.rs") || path.ends_with("mod.rs") {
        // Top-level means no :: in qualified name.
        if !qname.contains("::") {
            return true;
        }
    }

    false
}

/// Build a map from target symbol to incoming Calls edges.
fn build_call_graph(graph: &dyn GraphStore) -> crate::Result<HashMap<SymbolNodeId, Vec<Edge>>> {
    let edges = graph.active_edges()?;
    let mut calls_into: HashMap<SymbolNodeId, Vec<Edge>> = HashMap::new();

    for edge in edges {
        if edge.kind == EdgeKind::Calls {
            if let NodeId::Symbol(to_id) = edge.to {
                calls_into.entry(to_id).or_default().push(edge);
            }
        }
    }

    Ok(calls_into)
}

/// Backward BFS from target to entry points.
fn backward_bfs(
    _graph: &dyn GraphStore,
    target: SymbolNodeId,
    calls_into: &HashMap<SymbolNodeId, Vec<Edge>>,
    entry_points: &HashSet<SymbolNodeId>,
    max_depth: usize,
) -> crate::Result<Vec<Vec<SymbolNodeId>>> {
    let mut all_paths: Vec<Vec<SymbolNodeId>> = Vec::new();
    let mut visited = HashSet::new();

    // Queue: (current_symbol, current_path)
    let mut queue: VecDeque<(SymbolNodeId, Vec<SymbolNodeId>)> = VecDeque::new();
    queue.push_back((target, vec![target]));

    while let Some((current, path)) = queue.pop_front() {
        // If we reached an entry point, record this path.
        if entry_points.contains(&current) {
            // Reverse the path to go from entry point to target.
            let mut complete_path: Vec<SymbolNodeId> = path.clone();
            complete_path.reverse();
            all_paths.push(complete_path);
            continue;
        }

        // If we've hit max depth, record truncated path.
        if path.len() >= max_depth {
            let mut truncated_path: Vec<SymbolNodeId> = path.clone();
            truncated_path.reverse();
            all_paths.push(truncated_path);
            continue;
        }

        // Get incoming calls edges to this symbol.
        if let Some(incoming) = calls_into.get(&current) {
            for edge in incoming {
                if let NodeId::Symbol(from_id) = edge.from {
                    // Create new path with this predecessor.
                    let mut new_path = path.clone();
                    new_path.push(from_id);

                    // Avoid cycles.
                    let path_key = (from_id, new_path.len());
                    if !visited.contains(&path_key) {
                        visited.insert(path_key);
                        queue.push_back((from_id, new_path));
                    }
                }
            }
        }
    }

    Ok(all_paths)
}

/// Deduplicate paths by (entry_point, target) pair, keeping at most 3 per pair.
fn deduplicate_paths(paths: Vec<Vec<SymbolNodeId>>) -> (Vec<Vec<SymbolNodeId>>, usize) {
    if paths.is_empty() {
        return (vec![], 0);
    }

    // Group paths by (first_symbol, last_symbol) = (entry_point, target).
    let mut groups: HashMap<(SymbolNodeId, SymbolNodeId), Vec<Vec<SymbolNodeId>>> = HashMap::new();

    for path in paths {
        if path.len() < 2 {
            continue;
        }
        let key = (path[0], *path.last().unwrap());
        groups.entry(key).or_default().push(path);
    }

    let mut result: Vec<Vec<SymbolNodeId>> = Vec::new();
    let mut omitted = 0;

    for (_, mut group_paths) in groups {
        // Keep at most 3 paths per (entry, target) pair.
        if group_paths.len() > 3 {
            omitted += group_paths.len() - 3;
            group_paths.truncate(3);
        }
        result.extend(group_paths);
    }

    (result, omitted)
}

/// Convert raw paths to CallPath objects with budget-tier truncation.
fn convert_to_call_paths(
    graph: &dyn GraphStore,
    paths: Vec<Vec<SymbolNodeId>>,
    budget: Budget,
) -> crate::Result<Vec<CallPath>> {
    let mut call_paths = Vec::new();

    for path in paths {
        if path.is_empty() {
            continue;
        }

        let entry_id = path[0];
        let target_id = *path.last().unwrap();

        // Get entry point and target symbol refs.
        let entry_symbol = graph.get_symbol(entry_id)?;
        let target_symbol = graph.get_symbol(target_id)?;

        let entry_ref = match entry_symbol {
            Some(s) => make_symbol_ref(&s, graph)?,
            None => SymbolRef {
                id: entry_id,
                qualified_name: String::new(),
                location: String::new(),
            },
        };

        let target_ref = match target_symbol {
            Some(s) => make_symbol_ref(&s, graph)?,
            None => SymbolRef {
                id: target_id,
                qualified_name: String::new(),
                location: String::new(),
            },
        };

        // Build edges based on budget.
        let edges = match budget {
            Budget::Tiny => vec![], // Tiny: no edges, just entry and target.
            Budget::Normal | Budget::Deep => {
                let mut edge_list = Vec::new();
                for i in 0..path.len() - 1 {
                    let from_id = path[i];
                    let to_id = path[i + 1];

                    let from_symbol = graph.get_symbol(from_id)?;
                    let to_symbol = graph.get_symbol(to_id)?;

                    let from_ref = match from_symbol {
                        Some(s) => make_symbol_ref(&s, graph)?,
                        None => SymbolRef {
                            id: from_id,
                            qualified_name: String::new(),
                            location: String::new(),
                        },
                    };

                    let to_ref = match to_symbol {
                        Some(s) => make_symbol_ref(&s, graph)?,
                        None => SymbolRef {
                            id: to_id,
                            qualified_name: String::new(),
                            location: String::new(),
                        },
                    };

                    // Check if this edge is truncated (last edge if path exceeds depth).
                    let truncated = if i == path.len() - 2 {
                        Some(false) // Not truncated - this is the full path.
                    } else {
                        None
                    };

                    edge_list.push(super::super::types::CallPathEdge {
                        from: from_ref,
                        to: to_ref,
                        edge_kind: "Calls".to_string(),
                        truncated,
                    });
                }
                edge_list
            }
        };

        call_paths.push(CallPath {
            entry_point: entry_ref,
            target: target_ref,
            edges,
            paths_omitted: None,
        });
    }

    Ok(call_paths)
}

/// Convert a SymbolNode to a SymbolRef.
fn make_symbol_ref(symbol: &SymbolNode, graph: &dyn GraphStore) -> crate::Result<SymbolRef> {
    let file_path = graph
        .get_file(symbol.file_id)?
        .map(|f| f.path.clone())
        .unwrap_or_default();

    let location = format!("{}:{}", file_path, symbol.body_byte_range.0);

    Ok(SymbolRef {
        id: symbol.id,
        qualified_name: symbol.qualified_name.clone(),
        location,
    })
}

/// Estimate token count for the paths.
fn estimate_call_path_tokens(paths: &[CallPath], budget: Budget) -> usize {
    let base = 50; // Card overhead.
    match budget {
        Budget::Tiny => base + paths.len() * 30,
        Budget::Normal => base + paths.iter().map(|p| 20 + p.edges.len() * 40).sum::<usize>(),
        Budget::Deep => base + paths.iter().map(|p| 30 + p.edges.len() * 80).sum::<usize>(),
    }
}
