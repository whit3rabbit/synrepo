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
mod tests;
