use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::graph::{EdgeKind, GraphStore},
};

use super::{Budget, FileCard, FileRef, SourceStore, SymbolRef};

pub(super) fn file_card(
    graph: &dyn GraphStore,
    id: FileNodeId,
    budget: Budget,
) -> crate::Result<FileCard> {
    let file = graph
        .get_file(id)?
        .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("file {id} not found")))?;

    // Symbols defined in this file via Defines edges.
    let defines = graph.outbound(NodeId::File(id), Some(EdgeKind::Defines))?;

    let symbol_limit = match budget {
        Budget::Tiny => 10,
        _ => usize::MAX,
    };

    let mut symbols: Vec<SymbolRef> = Vec::new();
    for edge in defines.iter().take(symbol_limit) {
        if let NodeId::Symbol(sym_id) = edge.to {
            if let Some(sym) = graph.get_symbol(sym_id)? {
                symbols.push(SymbolRef {
                    id: sym_id,
                    qualified_name: sym.qualified_name.clone(),
                    location: format!("{}:{}", file.path, sym.body_byte_range.0),
                });
            }
        }
    }

    // Files that import this file (inbound Imports edges).
    let inbound_imports = graph.inbound(NodeId::File(id), Some(EdgeKind::Imports))?;

    let mut imported_by: Vec<FileRef> = Vec::new();
    for edge in &inbound_imports {
        if let NodeId::File(from_id) = edge.from {
            if let Some(from_file) = graph.get_file(from_id)? {
                imported_by.push(FileRef {
                    id: from_id,
                    path: from_file.path.clone(),
                });
            }
        }
    }

    // Files this file imports (outbound Imports edges).
    let outbound_imports = graph.outbound(NodeId::File(id), Some(EdgeKind::Imports))?;

    let mut imports: Vec<FileRef> = Vec::new();
    for edge in &outbound_imports {
        if let NodeId::File(to_id) = edge.to {
            if let Some(to_file) = graph.get_file(to_id)? {
                imports.push(FileRef {
                    id: to_id,
                    path: to_file.path.clone(),
                });
            }
        }
    }

    // TODO: truncate imported_by/imports for Tiny budget (symbol_limit already handles symbols)

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

pub(super) fn estimate_tokens_file(card: &FileCard) -> usize {
    let mut len = card.path.len();
    for sym_ref in &card.symbols {
        len += sym_ref.qualified_name.len() + sym_ref.location.len();
    }
    for file_ref in card.imported_by.iter().chain(card.imports.iter()) {
        len += file_ref.path.len();
    }
    (len / 4).max(10)
}
