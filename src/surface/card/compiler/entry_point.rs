//! Entry-point detection and `EntryPointCard` compilation.
//!
//! Detection runs at card-compile time against already-persisted graph rows.
//! No pipeline stage is added; all signals come from `SymbolNode.qualified_name`,
//! `SymbolNode.kind`, and the file path from `GraphStore::all_file_paths`.
//!
//! Rules (applied in order; first match wins):
//!
//! 1. Binary — `qualified_name == "main"` in `src/main.rs` or `src/bin/`
//! 2. CliCommand — `SymbolKind::Function` in a file whose path segment is `cli`, `command`, or `cmd`
//! 3. HttpHandler — name prefix `handle_`/`serve_`/`route_`, or path segment `handler`/`route`/`router`
//! 4. LibRoot — top-level item in `src/lib.rs` or any `mod.rs`

use std::collections::HashMap;

use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::graph::{EdgeKind, GraphStore, SymbolKind},
    surface::card::{
        types::{EntryPoint, EntryPointCard, EntryPointKind},
        Budget, SourceStore,
    },
};

/// Detect and compile an `EntryPointCard` from graph facts.
pub(super) fn entry_point_card_impl(
    graph: &dyn GraphStore,
    scope: Option<&str>,
    budget: Budget,
) -> crate::Result<EntryPointCard> {
    // Build a fast file-id → path lookup to avoid O(N) get_file calls.
    let file_path_map: HashMap<FileNodeId, String> = graph
        .all_file_paths()?
        .into_iter()
        .map(|(path, id)| (id, path))
        .collect();

    let symbol_names = graph.all_symbol_names()?;
    let mut entry_points: Vec<EntryPoint> = Vec::new();

    for (sym_id, file_id, qname) in &symbol_names {
        let Some(path) = file_path_map.get(file_id) else {
            continue;
        };

        // Apply optional scope filter (path prefix).
        if let Some(scope) = scope {
            if !path.starts_with(scope) {
                continue;
            }
        }

        // Load symbol to get kind and budget-sensitive fields.
        let Some(symbol) = graph.get_symbol(*sym_id)? else {
            continue;
        };

        let Some(kind) = classify_kind(qname, path, symbol.kind) else {
            continue;
        };

        // Build caller count at Normal+ budget.
        let caller_count = if budget != Budget::Tiny {
            let callers = graph.inbound(NodeId::Symbol(*sym_id), Some(EdgeKind::Calls))?;
            Some(callers.len())
        } else {
            None
        };

        // Doc comment truncated to 80 chars at Normal+ budget.
        let doc_comment = if budget != Budget::Tiny {
            symbol.doc_comment.as_deref().map(|s| {
                if s.len() > 80 {
                    format!("{}…", &s[..77])
                } else {
                    s.to_string()
                }
            })
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
        source_store: SourceStore::Graph,
    })
}

// ---------------------------------------------------------------------------
// Detection helpers
// ---------------------------------------------------------------------------

/// Apply the four detection rules in priority order; return the first match.
fn classify_kind(qname: &str, path: &str, kind: SymbolKind) -> Option<EntryPointKind> {
    // Rule 1: Binary
    if qname == "main" && is_binary_path(path) {
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
    if has_handler_prefix || has_handler_path {
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
    path == "src/main.rs" || (path.starts_with("src/bin/") && path.ends_with(".rs"))
}

fn path_has_segment(path: &str, segments: &[&str]) -> bool {
    path.split('/').any(|seg| segments.contains(&seg))
}

fn kind_order(kind: EntryPointKind) -> u8 {
    match kind {
        EntryPointKind::Binary => 0,
        EntryPointKind::CliCommand => 1,
        EntryPointKind::HttpHandler => 2,
        EntryPointKind::LibRoot => 3,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config, pipeline::structural::run_structural_compile,
        store::sqlite::SqliteGraphStore,
        surface::card::compiler::{GraphCardCompiler, CardCompiler},
    };
    use std::fs;
    use tempfile::tempdir;

    fn bootstrap(repo: &tempfile::TempDir) -> SqliteGraphStore {
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
        run_structural_compile(repo.path(), &Config::default(), &mut graph).unwrap();
        graph
    }

    // 7.1: one test per detection rule — match and non-match cases

    #[test]
    fn binary_rule_matches_main_in_src_main() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/main.rs"), "fn main() {}\n").unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        let kinds: Vec<EntryPointKind> = card.entry_points.iter().map(|e| e.kind).collect();
        assert!(
            kinds.contains(&EntryPointKind::Binary),
            "expected Binary in {kinds:?}"
        );
        let binary = card
            .entry_points
            .iter()
            .find(|e| e.kind == EntryPointKind::Binary)
            .unwrap();
        assert_eq!(binary.qualified_name, "main");
    }

    #[test]
    fn binary_rule_does_not_match_main_in_lib_rs() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        // main in lib.rs is NOT a binary entry point
        fs::write(repo.path().join("src/lib.rs"), "fn main() {}\n").unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        assert!(
            card.entry_points
                .iter()
                .all(|e| e.kind != EntryPointKind::Binary),
            "lib.rs main must not be Binary"
        );
    }

    #[test]
    fn cli_command_rule_matches_function_in_cli_path() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src/cli")).unwrap();
        fs::write(
            repo.path().join("src/cli/mod.rs"),
            "pub fn run() {}\n",
        )
        .unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        assert!(
            card.entry_points
                .iter()
                .any(|e| e.kind == EntryPointKind::CliCommand),
            "expected CliCommand in {:?}",
            card.entry_points
                .iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn cli_command_rule_does_not_match_non_cli_path() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        // A function named `run` in a non-cli file should not be CliCommand.
        fs::write(repo.path().join("src/service.rs"), "pub fn run() {}\n").unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        assert!(
            card.entry_points
                .iter()
                .all(|e| e.kind != EntryPointKind::CliCommand),
            "service.rs run() must not be CliCommand"
        );
    }

    #[test]
    fn http_handler_rule_matches_handle_prefix() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/server.rs"),
            "fn handle_request() {}\n",
        )
        .unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        assert!(
            card.entry_points
                .iter()
                .any(|e| e.kind == EntryPointKind::HttpHandler),
            "expected HttpHandler in {:?}",
            card.entry_points
                .iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn http_handler_rule_does_not_match_plain_function() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/server.rs"), "fn process() {}\n").unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        assert!(
            card.entry_points
                .iter()
                .all(|e| e.kind != EntryPointKind::HttpHandler),
            "process() must not be HttpHandler"
        );
    }

    #[test]
    fn lib_root_rule_matches_function_in_lib_rs() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn init() {}\n").unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        assert!(
            card.entry_points
                .iter()
                .any(|e| e.kind == EntryPointKind::LibRoot),
            "expected LibRoot in {:?}",
            card.entry_points
                .iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn lib_root_rule_does_not_match_non_module_root() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        // init() in a regular file should NOT be LibRoot.
        fs::write(repo.path().join("src/service.rs"), "pub fn init() {}\n").unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        assert!(
            card.entry_points
                .iter()
                .all(|e| e.kind != EntryPointKind::LibRoot),
            "service.rs init() must not be LibRoot"
        );
    }

    // 7.2: rule ordering — first matching rule wins

    #[test]
    fn rule_ordering_cli_path_beats_handle_prefix() {
        let repo = tempdir().unwrap();
        // handle_command in src/cli/handler.rs matches CliCommand (rule 2) before
        // HttpHandler (rule 3) because `cli` appears in the path.
        fs::create_dir_all(repo.path().join("src/cli")).unwrap();
        fs::write(
            repo.path().join("src/cli/handler.rs"),
            "pub fn handle_command() {}\n",
        )
        .unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        let entry = card
            .entry_points
            .iter()
            .find(|e| e.qualified_name == "handle_command");
        assert!(entry.is_some(), "handle_command should be detected");
        assert_eq!(
            entry.unwrap().kind,
            EntryPointKind::CliCommand,
            "cli path segment must take priority over handle_ prefix"
        );
    }

    #[test]
    fn rule_ordering_only_one_entry_per_symbol() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src/cli")).unwrap();
        fs::write(
            repo.path().join("src/cli/handler.rs"),
            "pub fn handle_command() {}\n",
        )
        .unwrap();

        let graph = bootstrap(&repo);
        let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
        let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

        let count = card
            .entry_points
            .iter()
            .filter(|e| e.qualified_name == "handle_command")
            .count();
        assert_eq!(count, 1, "handle_command must appear exactly once");
    }
}
