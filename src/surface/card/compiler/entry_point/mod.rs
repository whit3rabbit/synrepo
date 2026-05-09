//! Entry-point detection and `EntryPointCard` compilation.
//!
//! Detection runs at card-compile time against already-persisted graph rows.
//! No pipeline stage is added; all signals come from `SymbolNode.qualified_name`,
//! `SymbolNode.kind`, and the file path from `GraphReader::all_file_paths`.
//!
//! Rules (applied in order; first match wins):
//!
//! 1. Binary — `qualified_name == "main"` in Rust binary paths or Dart app paths,
//!    or an Android launcher activity declared by `AndroidManifest.xml`
//! 2. CliCommand — `SymbolKind::Function` in a file whose path segment is `cli`, `command`, or `cmd`
//! 3. HttpHandler — name prefix `handle_`/`serve_`/`route_`, or path segment `handler`/`route`/`router`
//! 4. LibRoot — top-level item in `src/lib.rs` or any `mod.rs`

use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

use crate::{
    core::{
        ids::{FileNodeId, NodeId},
        project_layout::android_launcher_activities,
    },
    structure::graph::{EdgeKind, GraphReader, SymbolKind},
    surface::card::{
        truncate_chars,
        types::{EntryPoint, EntryPointCard, EntryPointKind},
        Budget, ContextAccounting, SourceStore,
    },
};

/// Detect and compile an `EntryPointCard` from graph facts.
pub(super) fn entry_point_card_impl(
    graph: &dyn GraphReader,
    scope: Option<&str>,
    budget: Budget,
    repo_root: Option<&Path>,
) -> crate::Result<EntryPointCard> {
    // Build a fast file-id → path lookup to avoid O(N) get_file calls.
    let file_path_map: HashMap<FileNodeId, String> = graph
        .all_file_paths()?
        .into_iter()
        .map(|(path, id)| (id, path))
        .collect();

    let symbols_summary = graph.all_symbols_summary()?;
    let mut entry_points: Vec<EntryPoint> = Vec::new();
    let android_launchers = repo_root
        .map(android_launcher_activities)
        .unwrap_or_default();

    for (sym_id, file_id, qname, kind_label, _body_hash) in &symbols_summary {
        let Some(path) = file_path_map.get(file_id) else {
            continue;
        };

        // Apply optional scope filter (path prefix).
        if let Some(scope) = scope {
            if !path.starts_with(scope) {
                continue;
            }
        }

        let Some(sym_kind) = SymbolKind::from_label(kind_label) else {
            continue;
        };

        let Some(kind) = classify_kind_with_context(qname, path, sym_kind, &android_launchers)
        else {
            continue;
        };

        // Deferred load: only fetch full symbol for the small set that
        // pass classification, to get budget-sensitive fields.
        let Some(symbol) = graph.get_symbol(*sym_id)? else {
            continue;
        };

        // Build caller count at Normal+ budget.
        let caller_count = if budget != Budget::Tiny {
            let callers = graph.inbound(NodeId::Symbol(*sym_id), Some(EdgeKind::Calls))?;
            Some(callers.len())
        } else {
            None
        };

        let doc_comment = if budget != Budget::Tiny {
            symbol.doc_comment.as_deref().map(|s| truncate_chars(s, 77))
        } else {
            None
        };

        // Full signature at Deep budget only.
        let signature = if budget == Budget::Deep {
            symbol.signature.clone()
        } else {
            None
        };

        let location = format!("{}:{}", path, symbol.body_byte_range.0);

        entry_points.push(EntryPoint {
            symbol: *sym_id,
            qualified_name: qname.clone(),
            location,
            kind,
            caller_count,
            doc_comment,
            signature,
        });
    }

    // Sort: kind order (Binary < CliCommand < HttpHandler < LibRoot), then location.
    entry_points.sort_by(|a, b| {
        kind_order(a.kind)
            .cmp(&kind_order(b.kind))
            .then_with(|| a.location.cmp(&b.location))
    });

    // Cap at 20 entries.
    entry_points.truncate(20);

    let per_entry = match budget {
        Budget::Tiny => 30,
        Budget::Normal => 60,
        Budget::Deep => 150,
    };
    let approx_tokens = entry_points.len() * per_entry + 20;

    Ok(EntryPointCard {
        scope: scope.map(|s| s.to_string()),
        entry_points,
        approx_tokens,
        context_accounting: ContextAccounting::new(budget, approx_tokens, 0, vec![]),
        source_store: SourceStore::Graph,
    })
}

// ---------------------------------------------------------------------------
// Detection helpers
// ---------------------------------------------------------------------------

/// Apply the four detection rules in priority order; return the first match.
pub(super) fn classify_kind(qname: &str, path: &str, kind: SymbolKind) -> Option<EntryPointKind> {
    classify_kind_with_context(qname, path, kind, &BTreeSet::new())
}

fn classify_kind_with_context(
    qname: &str,
    path: &str,
    kind: SymbolKind,
    android_launchers: &BTreeSet<String>,
) -> Option<EntryPointKind> {
    // Rule 1: Binary
    if qname == "main" && is_binary_path(path) {
        return Some(EntryPointKind::Binary);
    }
    if kind == SymbolKind::Class
        && is_android_main_source_path(path)
        && android_launchers.contains(simple_symbol_name(qname))
    {
        return Some(EntryPointKind::Binary);
    }

    // Rule 2: CliCommand — Function in a cli/command/cmd file
    if kind == SymbolKind::Function && path_has_segment(path, &["cli", "command", "cmd"]) {
        return Some(EntryPointKind::CliCommand);
    }

    // Rule 3: HttpHandler — name prefix or path segment
    let name = qname.rsplit("::").next().unwrap_or(qname);
    let has_handler_prefix =
        name.starts_with("handle_") || name.starts_with("serve_") || name.starts_with("route_");
    let has_handler_path = path_has_segment(path, &["handler", "route", "router"]);
    let is_executable_item = matches!(kind, SymbolKind::Function | SymbolKind::Method);

    if has_handler_prefix || (has_handler_path && is_executable_item) {
        return Some(EntryPointKind::HttpHandler);
    }

    // Rule 4: LibRoot — top-level item (no `::`) in src/lib.rs or a mod.rs
    let is_module_root = path == "src/lib.rs" || path.ends_with("/mod.rs");
    let is_pub_item = matches!(kind, SymbolKind::Function | SymbolKind::Class);
    let is_top_level = !qname.contains("::");
    if is_module_root && is_pub_item && is_top_level {
        return Some(EntryPointKind::LibRoot);
    }

    None
}

fn is_binary_path(path: &str) -> bool {
    path == "src/main.rs"
        || (path.starts_with("src/bin/") && path.ends_with(".rs"))
        || path == "lib/main.dart"
        || (path.starts_with("bin/") && path.ends_with(".dart"))
}

fn path_has_segment(path: &str, segments: &[&str]) -> bool {
    path.split('/').any(|seg| segments.contains(&seg))
}

fn is_android_main_source_path(path: &str) -> bool {
    path.contains("/src/main/java/")
        || path.contains("/src/main/kotlin/")
        || path.starts_with("src/main/java/")
        || path.starts_with("src/main/kotlin/")
}

fn simple_symbol_name(qname: &str) -> &str {
    qname.rsplit("::").next().unwrap_or(qname)
}

fn kind_order(kind: EntryPointKind) -> u8 {
    match kind {
        EntryPointKind::Binary => 0,
        EntryPointKind::CliCommand => 1,
        EntryPointKind::HttpHandler => 2,
        EntryPointKind::LibRoot => 3,
    }
}

#[cfg(test)]
mod tests;
