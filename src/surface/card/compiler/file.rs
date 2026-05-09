use std::collections::HashSet;

use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::graph::{Edge, EdgeKind, GraphReader},
    surface::card::accounting::{raw_file_token_estimate, ContextAccounting},
    surface::card::git::FileGitIntelligence,
};

use super::{Budget, FileCard, FileRef, GraphCardCompiler, SourceStore, SymbolRef};

pub(super) fn file_card(
    compiler: &GraphCardCompiler,
    graph: &dyn GraphReader,
    id: FileNodeId,
    budget: Budget,
) -> crate::Result<FileCard> {
    let overlay = compiler.overlay.as_ref();
    let file = graph
        .get_file(id)?
        .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("file {id} not found")))?;

    // Symbols defined in this file.
    let all_symbols = graph.symbols_for_file(id)?;

    let symbol_limit = match budget {
        Budget::Tiny => 10,
        _ => usize::MAX,
    };

    let symbols: Vec<SymbolRef> = all_symbols
        .iter()
        .take(symbol_limit)
        .map(|sym| SymbolRef {
            id: sym.id,
            qualified_name: sym.qualified_name.clone(),
            location: format!("{}:{}", file.path, sym.body_byte_range.0),
        })
        .collect();

    // imported_by, imports, and outbound_imports (for co_changes filtering) — all
    // truncated at Tiny budget per progressive disclosure contract.
    let (imported_by, imports, outbound_imports): (Vec<FileRef>, Vec<FileRef>, Vec<Edge>) =
        match budget {
            Budget::Tiny => (vec![], vec![], vec![]),
            Budget::Normal | Budget::Deep => {
                // Files that import this file (inbound Imports edges).
                let inbound_imports = graph.inbound(NodeId::File(id), Some(EdgeKind::Imports))?;

                let mut imported_by = Vec::new();
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

                let mut imports = Vec::new();
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

                (imported_by, imports, outbound_imports)
            }
        };

    let git_intelligence = match budget {
        Budget::Tiny => None,
        Budget::Normal | Budget::Deep => compiler
            .resolve_file_git_intelligence(&file.path)
            .map(|arc| FileGitIntelligence::from(&*arc)),
    };

    // Co-change partners: graph-backed CoChangesWith edges, filtered to
    // hidden-coupling-only (partners without an existing Imports edge).
    let co_changes = match budget {
        Budget::Tiny => vec![],
        Budget::Normal | Budget::Deep => {
            let outbound_cc = graph.outbound(NodeId::File(id), Some(EdgeKind::CoChangesWith))?;
            let inbound_cc = graph.inbound(NodeId::File(id), Some(EdgeKind::CoChangesWith))?;

            // Collect partner FileNodeIds from both directions.
            let mut partner_ids: Vec<FileNodeId> = Vec::new();
            for edge in &outbound_cc {
                if let NodeId::File(to_id) = edge.to {
                    partner_ids.push(to_id);
                }
            }
            for edge in &inbound_cc {
                if let NodeId::File(from_id) = edge.from {
                    partner_ids.push(from_id);
                }
            }
            partner_ids.sort();
            partner_ids.dedup();

            // Build a set of file IDs already connected via Imports.
            let imports_set: HashSet<FileNodeId> = outbound_imports
                .iter()
                .filter_map(|e| {
                    if let NodeId::File(to_id) = e.to {
                        Some(to_id)
                    } else {
                        None
                    }
                })
                .collect();

            // Filter to hidden-coupling partners only, resolve to FileRef.
            partner_ids
                .into_iter()
                .filter(|pid| !imports_set.contains(pid))
                .filter_map(|pid| {
                    graph.get_file(pid).ok().flatten().map(|f| FileRef {
                        id: pid,
                        path: f.path,
                    })
                })
                .collect()
        }
    };

    let mut card = FileCard {
        file: id,
        path: file.path.clone(),
        root_id: file.root_id.clone(),
        is_primary_root: file.root_id == "primary",
        symbols,
        imported_by,
        imports,
        co_changes,
        git_intelligence,
        drift_flag: None,
        approx_tokens: 0,
        context_accounting: ContextAccounting::placeholder(budget),
        source_store: SourceStore::Graph,
        proposed_links: None,
        links_state: None,
    };

    match budget {
        Budget::Tiny | Budget::Normal => {
            card.links_state = Some("budget_withheld".to_string());
        }
        Budget::Deep => {
            let (links, links_state) = super::links::resolve_proposed_links(
                overlay.map(|o| &**o),
                graph,
                NodeId::File(id),
            )?;
            card.proposed_links = links;
            card.links_state = Some(links_state.to_string());
        }
    }

    card.approx_tokens = estimate_tokens_file(&card);
    card.context_accounting = ContextAccounting::new(
        budget,
        card.approx_tokens,
        raw_file_token_estimate(compiler.repo_root.as_deref(), &file.path),
        vec![file.content_hash],
    );
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
