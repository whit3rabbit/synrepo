//! Stage 1-3 for supported-code files: parse, extract symbols, emit Defines
//! edges, and retire no-longer-emitted prior observations.

use std::collections::{HashMap, HashSet};

use super::super::{
    ids::{derive_edge_id, derive_file_id, derive_symbol_id},
    provenance::make_provenance,
    stage4::CrossFilePending,
};
use super::StageState;
use crate::{
    core::ids::{FileNodeId, NodeId},
    structure::{
        graph::{Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolNode},
        parse, rationale,
    },
    substrate::{DiscoveredFile, FileClass},
};

pub(super) fn process_supported_code_files(
    graph: &mut dyn GraphStore,
    discovered: &[DiscoveredFile],
    revision: &str,
    disappeared_by_hash: &HashMap<(String, String), FileNode>,
    rename_matched_old_paths: &mut HashSet<String>,
    state: &mut StageState,
    compile_rev: Option<u64>,
) -> crate::Result<()> {
    for file in discovered {
        if !matches!(file.class, FileClass::SupportedCode { .. }) {
            continue;
        }

        let content = match std::fs::read(&file.absolute_path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        let content_hash = hex::encode(blake3::hash(&content).as_bytes());
        let existing = graph.file_by_root_path(&file.root_discriminant, &file.relative_path)?;
        let is_content_change = existing
            .as_ref()
            .is_some_and(|n| n.content_hash != content_hash);

        if existing.is_some() && !is_content_change {
            continue;
        }

        // Collect prior symbols/edges for this file so we can diff after parsing.
        // Only needed when the file already exists and its content changed.
        let prior_symbols: Vec<SymbolNode> = if is_content_change {
            let fid = existing.as_ref().unwrap().id;
            graph.symbols_for_file(fid)?
        } else {
            Vec::new()
        };

        let file_id = resolve_file_id(
            existing.as_ref(),
            &file.root_discriminant,
            &content_hash,
            disappeared_by_hash,
            rename_matched_old_paths,
            &mut state.identities_resolved,
        );

        let language = match file.class {
            FileClass::SupportedCode { language } => Some(language.to_string()),
            _ => None,
        };
        let inline_decisions = rationale::extract_inline_decisions(&content, &file.class);

        // Upsert file node: preserves FileNodeId, advances content_hash.
        graph.upsert_file(FileNode {
            id: file_id,
            root_id: file.root_discriminant.clone(),
            path: file.relative_path.clone(),
            path_history: disappeared_by_hash
                .get(&(file.root_discriminant.clone(), content_hash.clone()))
                .map(|old| {
                    let mut h = old.path_history.clone();
                    h.insert(0, old.path.clone());
                    h
                })
                .unwrap_or_default(),
            content_hash: content_hash.clone(),
            content_sample_hashes: crate::structure::identity::sampled_content_hashes(&content),
            size_bytes: file.size_bytes,
            language,
            inline_decisions,
            last_observed_rev: compile_rev,
            epistemic: Epistemic::ParserObserved,
            provenance: make_provenance("discover", revision, &file.relative_path, &content_hash),
        })?;
        state.file_map.insert(
            (file.root_discriminant.clone(), file.relative_path.clone()),
            file_id,
        );

        // Track which symbol IDs are emitted this pass so we can retire the rest.
        let mut emitted_symbol_ids = HashSet::new();
        let mut emitted_edge_ids = HashSet::new();

        if let Some(parsed) = parse::parse_file(file.absolute_path.as_path(), &content)? {
            if !parsed.symbols.is_empty() {
                state.files_parsed += 1;
                for symbol in &parsed.symbols {
                    let symbol_id = derive_symbol_id(
                        file_id,
                        &symbol.qualified_name,
                        symbol.kind,
                        &symbol.body_hash,
                    );
                    let provenance =
                        make_provenance("parse_code", revision, &file.relative_path, &content_hash);

                    graph.upsert_symbol(SymbolNode {
                        id: symbol_id,
                        file_id,
                        qualified_name: symbol.qualified_name.clone(),
                        display_name: symbol.display_name.clone(),
                        kind: symbol.kind,
                        visibility: symbol.visibility,
                        body_byte_range: symbol.body_byte_range,
                        body_hash: symbol.body_hash.clone(),
                        signature: symbol.signature.clone(),
                        doc_comment: symbol.doc_comment.clone(),
                        first_seen_rev: None,
                        last_modified_rev: None,
                        last_observed_rev: compile_rev,
                        retired_at_rev: None,
                        epistemic: Epistemic::ParserObserved,
                        provenance: provenance.clone(),
                    })?;

                    let edge_id = derive_edge_id(
                        NodeId::File(file_id),
                        NodeId::Symbol(symbol_id),
                        EdgeKind::Defines,
                    );
                    graph.insert_edge(Edge {
                        id: edge_id,
                        from: NodeId::File(file_id),
                        to: NodeId::Symbol(symbol_id),
                        kind: EdgeKind::Defines,
                        owner_file_id: Some(file_id),
                        last_observed_rev: compile_rev,
                        retired_at_rev: None,
                        epistemic: Epistemic::ParserObserved,
                        provenance,
                    })?;

                    emitted_symbol_ids.insert(symbol_id);
                    emitted_edge_ids.insert(edge_id);
                    state.symbols_extracted += 1;
                    state.edges_added += 1;
                }
            }
            if !parsed.call_refs.is_empty() || !parsed.import_refs.is_empty() {
                state.cross_file_pending.push(CrossFilePending {
                    file_id,
                    root_id: file.root_discriminant.clone(),
                    file_path: file.relative_path.clone(),
                    call_refs: parsed.call_refs,
                    import_refs: parsed.import_refs,
                });
            }
        }

        // Retire symbols and parser-owned edges that were previously active
        // for this file but not re-emitted in the current parse pass. Bulk
        // calls collapse what used to be N+1 UPDATEs per file into one
        // chunked statement; on a 10K-symbol churn this measurably reduces
        // SQLite roundtrips.
        if is_content_change {
            if let Some(rev) = compile_rev {
                let retire_symbols: Vec<_> = prior_symbols
                    .iter()
                    .filter(|p| !emitted_symbol_ids.contains(&p.id))
                    .map(|p| p.id)
                    .collect();
                graph.retire_symbols_bulk(&retire_symbols, rev)?;

                let retire_edges: Vec<_> = graph
                    .edges_owned_by(file_id)?
                    .iter()
                    .filter(|e| {
                        e.epistemic == Epistemic::ParserObserved
                            && !emitted_edge_ids.contains(&e.id)
                    })
                    .map(|e| e.id)
                    .collect();
                graph.retire_edges_bulk(&retire_edges, rev)?;
            }
        }
    }
    Ok(())
}

fn resolve_file_id(
    existing: Option<&FileNode>,
    root_discriminant: &str,
    content_hash: &str,
    disappeared_by_hash: &HashMap<(String, String), FileNode>,
    rename_matched_old_paths: &mut HashSet<String>,
    identities_resolved: &mut usize,
) -> FileNodeId {
    if let Some(file_node) = existing {
        return file_node.id;
    }
    if let Some(old_node) =
        disappeared_by_hash.get(&(root_discriminant.to_string(), content_hash.to_string()))
    {
        rename_matched_old_paths.insert(old_node.path.clone());
        *identities_resolved += 1;
        return old_node.id;
    }
    derive_file_id(root_discriminant, content_hash)
}
