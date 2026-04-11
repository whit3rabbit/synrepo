//! `GraphCardCompiler`: the primary implementation of `CardCompiler` backed
//! by the `SqliteGraphStore`-compatible `GraphStore` trait.
//!
//! ## Phase-1 limitations
//!
//! - `SymbolCard.callers` and `.callees` are empty: stage-4 edges are
//!   file→symbol, not symbol→symbol. Symbol-level call resolution is stage 5+.
//! - `SymbolCard.signature` and `.doc_comment` are `None` until the phase-1
//!   parse TODO in `structure/parse/extract.rs` is resolved.
//! - `FileCard.co_changes` is empty until stage 5 (git mining) is wired.
//! - `FileCard.git_intelligence` is `None` until `git-intelligence-v1`.

use std::path::{Path, PathBuf};

use super::{Budget, CardCompiler, FileCard, FileRef, SourceStore, SymbolCard, SymbolRef};
use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    structure::graph::{EdgeKind, GraphStore},
};

/// A `CardCompiler` backed by a `GraphStore` reference.
pub struct GraphCardCompiler {
    graph: Box<dyn GraphStore>,
    /// Repository root, used to read source bodies at `Deep` budget.
    repo_root: Option<PathBuf>,
}

impl GraphCardCompiler {
    /// Create a compiler from a boxed graph store.
    ///
    /// Pass `repo_root` to enable source-body inclusion at `Deep` budget.
    pub fn new(graph: Box<dyn GraphStore>, repo_root: Option<impl Into<PathBuf>>) -> Self {
        Self {
            graph,
            repo_root: repo_root.map(Into::into),
        }
    }

    /// Access the underlying graph store for direct queries.
    pub fn graph(&self) -> &dyn GraphStore {
        self.graph.as_ref()
    }
}

impl CardCompiler for GraphCardCompiler {
    fn symbol_card(&self, id: SymbolNodeId, budget: Budget) -> crate::Result<SymbolCard> {
        let symbol = self
            .graph
            .get_symbol(id)?
            .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("symbol {id} not found")))?;

        let file = self.graph.get_file(symbol.file_id)?.ok_or_else(|| {
            crate::Error::Other(anyhow::anyhow!("file for symbol {id} not found"))
        })?;

        let defined_at = format!("{}:{}", file.path, symbol.body_byte_range.0);

        // Callers: inbound Calls edges to this symbol.
        // Phase 1: edges are file→symbol, so `from` is a FileNodeId.
        // We report empty callers until symbol→symbol Calls edges land in stage 5.
        let callers: Vec<SymbolRef> = vec![];
        let callees: Vec<SymbolRef> = vec![];

        // Source body: only for Deep budget.
        let source_body = if budget == Budget::Deep {
            read_symbol_body(&self.repo_root, &file.path, symbol.body_byte_range)
        } else {
            None
        };

        // Doc comment: always None for now (phase-1 parse TODO).
        let doc_comment = match budget {
            Budget::Tiny => None,
            _ => symbol.doc_comment.clone(),
        };

        let mut card = SymbolCard {
            symbol: id,
            name: symbol.display_name.clone(),
            qualified_name: symbol.qualified_name.clone(),
            defined_at,
            signature: symbol.signature.clone(),
            doc_comment,
            callers,
            callees,
            tests_touching: vec![],
            last_change: None,
            drift_flag: None,
            source_body,
            approx_tokens: 0,
            source_store: SourceStore::Graph,
            epistemic: symbol.epistemic,
            overlay_commentary: None,
        };

        card.approx_tokens = estimate_tokens_symbol(&card);
        Ok(card)
    }

    fn file_card(&self, id: FileNodeId, budget: Budget) -> crate::Result<FileCard> {
        let file = self
            .graph
            .get_file(id)?
            .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("file {id} not found")))?;

        // Symbols defined in this file via Defines edges.
        let defines = self
            .graph
            .outbound(NodeId::File(id), Some(EdgeKind::Defines))?;

        let callee_limit = match budget {
            Budget::Tiny => 10,
            _ => usize::MAX,
        };

        let mut symbols: Vec<SymbolRef> = Vec::new();
        for edge in defines.iter().take(callee_limit) {
            if let NodeId::Symbol(sym_id) = edge.to {
                if let Some(sym) = self.graph.get_symbol(sym_id)? {
                    symbols.push(SymbolRef {
                        id: sym_id,
                        qualified_name: sym.qualified_name.clone(),
                        location: format!("{}:{}", file.path, sym.body_byte_range.0),
                    });
                }
            }
        }

        // Files that import this file (inbound Imports edges).
        let inbound_imports = self
            .graph
            .inbound(NodeId::File(id), Some(EdgeKind::Imports))?;

        let mut imported_by: Vec<FileRef> = Vec::new();
        for edge in &inbound_imports {
            if let NodeId::File(from_id) = edge.from {
                if let Some(from_file) = self.graph.get_file(from_id)? {
                    imported_by.push(FileRef {
                        id: from_id,
                        path: from_file.path.clone(),
                    });
                }
            }
        }

        // Files this file imports (outbound Imports edges).
        let outbound_imports = self
            .graph
            .outbound(NodeId::File(id), Some(EdgeKind::Imports))?;

        let mut imports: Vec<FileRef> = Vec::new();
        for edge in &outbound_imports {
            if let NodeId::File(to_id) = edge.to {
                if let Some(to_file) = self.graph.get_file(to_id)? {
                    imports.push(FileRef {
                        id: to_id,
                        path: to_file.path.clone(),
                    });
                }
            }
        }

        let _ = budget; // future: truncate symbols/imports for Tiny

        let mut card = FileCard {
            file: id,
            path: file.path.clone(),
            symbols,
            imported_by,
            imports,
            co_changes: vec![],
            git_intelligence: None,
            drift_flag: None,
            approx_tokens: 0,
            source_store: SourceStore::Graph,
        };

        card.approx_tokens = estimate_tokens_file(&card);
        Ok(card)
    }

    fn resolve_target(&self, target: &str) -> crate::Result<Option<NodeId>> {
        // 1. Try exact file path lookup.
        if let Some(file) = self.graph.file_by_path(target)? {
            return Ok(Some(NodeId::File(file.id)));
        }

        // 2. Try symbol name match (display name or qualified name suffix).
        let all_syms = self.graph.all_symbol_names()?;
        for (sym_id, _file_id, qname) in &all_syms {
            let short = qname.rsplit("::").next().unwrap_or(qname.as_str());
            if qname == target || short == target {
                return Ok(Some(NodeId::Symbol(*sym_id)));
            }
        }

        // 3. Try substring match on qualified name.
        for (sym_id, _file_id, qname) in &all_syms {
            if qname.contains(target) {
                return Ok(Some(NodeId::Symbol(*sym_id)));
            }
        }

        Ok(None)
    }
}

/// Read the source body of a symbol from the file on disk.
fn read_symbol_body(
    repo_root: &Option<PathBuf>,
    file_path: &str,
    byte_range: (u32, u32),
) -> Option<String> {
    let root = repo_root.as_deref().unwrap_or(Path::new("."));
    let full_path = root.join(file_path);
    let content = std::fs::read(&full_path).ok()?;
    let start = byte_range.0 as usize;
    let end = (byte_range.1 as usize).min(content.len());
    std::str::from_utf8(content.get(start..end)?)
        .ok()
        .map(str::to_string)
}

/// Estimate token count for a SymbolCard (1 token ≈ 4 chars).
fn estimate_tokens_symbol(card: &SymbolCard) -> usize {
    let mut len = card.name.len()
        + card.qualified_name.len()
        + card.defined_at.len()
        + card.signature.as_deref().map_or(0, str::len)
        + card.doc_comment.as_deref().map_or(0, str::len)
        + card.source_body.as_deref().map_or(0, str::len);

    for sym_ref in card.callers.iter().chain(card.callees.iter()) {
        len += sym_ref.qualified_name.len() + sym_ref.location.len();
    }

    (len / 4).max(10)
}

/// Estimate token count for a FileCard (1 token ≈ 4 chars).
fn estimate_tokens_file(card: &FileCard) -> usize {
    let mut len = card.path.len();
    for sym_ref in &card.symbols {
        len += sym_ref.qualified_name.len() + sym_ref.location.len();
    }
    for file_ref in card.imported_by.iter().chain(card.imports.iter()) {
        len += file_ref.path.len();
    }
    (len / 4).max(10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config, pipeline::structural::run_structural_compile,
        store::sqlite::SqliteGraphStore,
    };
    use std::fs;
    use tempfile::tempdir;

    fn make_compiler(graph: SqliteGraphStore, repo: &tempfile::TempDir) -> GraphCardCompiler {
        GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
    }

    fn bootstrap(repo: &tempfile::TempDir) -> SqliteGraphStore {
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
        run_structural_compile(repo.path(), &Config::default(), &mut graph).unwrap();
        graph
    }

    #[test]
    fn file_card_returns_defined_symbols() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn foo() {}\npub fn bar() {}\n",
        )
        .unwrap();

        let graph = bootstrap(&repo);
        let file_id = graph.file_by_path("src/lib.rs").unwrap().unwrap().id;
        let compiler = make_compiler(graph, &repo);

        let card = compiler.file_card(file_id, Budget::Tiny).unwrap();
        assert_eq!(card.path, "src/lib.rs");
        assert_eq!(card.symbols.len(), 2);
        let names: Vec<&str> = card
            .symbols
            .iter()
            .map(|s| s.qualified_name.as_str())
            .collect();
        assert!(names.contains(&"foo"), "expected foo in {names:?}");
        assert!(names.contains(&"bar"), "expected bar in {names:?}");
    }

    #[test]
    fn resolve_target_finds_by_path_and_by_name() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn my_func() {}\n").unwrap();

        let graph = bootstrap(&repo);
        let compiler = make_compiler(graph, &repo);

        // By path
        let by_path = compiler.resolve_target("src/lib.rs").unwrap();
        assert!(matches!(by_path, Some(NodeId::File(_))));

        // By symbol name
        let by_name = compiler.resolve_target("my_func").unwrap();
        assert!(matches!(by_name, Some(NodeId::Symbol(_))));

        // Non-existent target
        assert!(compiler.resolve_target("nonexistent").unwrap().is_none());
    }

    #[test]
    fn symbol_card_tiny_has_no_source_body() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/lib.rs"),
            "/// Docs.\npub fn documented() -> u32 { 42 }\n",
        )
        .unwrap();

        let graph = bootstrap(&repo);
        let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
        let sym_edge = graph
            .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let sym_id = match sym_edge.to {
            NodeId::Symbol(id) => id,
            _ => panic!("expected symbol"),
        };
        let compiler = make_compiler(graph, &repo);

        let tiny = compiler.symbol_card(sym_id, Budget::Tiny).unwrap();
        assert_eq!(tiny.name, "documented");
        assert!(
            tiny.source_body.is_none(),
            "tiny budget must not include source body"
        );
        assert!(tiny.approx_tokens > 0);
        assert_eq!(tiny.source_store, SourceStore::Graph);

        // Normal: same, still no source body but doc_comment may be populated.
        let graph2 = {
            let graph_dir = repo.path().join(".synrepo/graph");
            SqliteGraphStore::open(&graph_dir).unwrap()
        };
        let compiler2 = GraphCardCompiler::new(Box::new(graph2), Some(repo.path()));
        let normal = compiler2.symbol_card(sym_id, Budget::Normal).unwrap();
        assert!(
            normal.source_body.is_none(),
            "normal budget must not include source body"
        );

        // Deep: source body should be populated.
        let graph3 = {
            let graph_dir = repo.path().join(".synrepo/graph");
            SqliteGraphStore::open(&graph_dir).unwrap()
        };
        let compiler3 = GraphCardCompiler::new(Box::new(graph3), Some(repo.path()));
        let deep = compiler3.symbol_card(sym_id, Budget::Deep).unwrap();
        assert!(
            deep.source_body.is_some(),
            "deep budget must include source body"
        );
        let body = deep.source_body.unwrap();
        assert!(
            body.contains("documented"),
            "source body must contain function text"
        );
    }

    #[test]
    fn file_card_includes_imports_edges() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/utils.ts"),
            "export function helper() {}\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("src/main.ts"),
            "import { helper } from './utils';\nhelper();\n",
        )
        .unwrap();

        let graph = bootstrap(&repo);
        let main_id = graph.file_by_path("src/main.ts").unwrap().unwrap().id;
        let utils_id = graph.file_by_path("src/utils.ts").unwrap().unwrap().id;
        let compiler = make_compiler(graph, &repo);

        let card = compiler.file_card(main_id, Budget::Normal).unwrap();
        assert!(
            card.imports.iter().any(|r| r.id == utils_id),
            "main.ts card must list utils.ts as an import"
        );

        let graph2 = {
            let graph_dir = repo.path().join(".synrepo/graph");
            SqliteGraphStore::open(&graph_dir).unwrap()
        };
        let compiler2 = GraphCardCompiler::new(Box::new(graph2), Some(repo.path()));
        let utils_card = compiler2.file_card(utils_id, Budget::Normal).unwrap();
        assert!(
            utils_card.imported_by.iter().any(|r| r.id == main_id),
            "utils.ts card must list main.ts in imported_by"
        );
    }
}
