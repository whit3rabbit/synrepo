//! Path-convention route extraction for text files that are not parsed by
//! tree-sitter, currently SvelteKit `.svelte` page route modules.

use std::collections::{HashMap, HashSet};

use super::super::{
    ids::{derive_edge_id, derive_symbol_id},
    provenance::make_provenance,
    stage4::CrossFilePending,
};
use super::{code::resolve_file_id, StageState};
use crate::{
    core::ids::NodeId,
    structure::{
        graph::{Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolNode},
        parse, rationale,
    },
    substrate::{DiscoveredFile, FileClass},
};

pub(super) fn process_text_file_routes(
    graph: &mut dyn GraphStore,
    discovered: &[DiscoveredFile],
    revision: &str,
    disappeared_by_hash: &HashMap<(String, String), FileNode>,
    rename_matched_old_paths: &mut HashSet<String>,
    state: &mut StageState,
    compile_rev: Option<u64>,
) -> crate::Result<()> {
    for file in discovered {
        if matches!(file.class, FileClass::SupportedCode { .. }) {
            continue;
        }
        let content = match std::fs::read(&file.absolute_path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let routes = parse::extract_file_routes(&file.relative_path, &content);
        if routes.0.is_empty() {
            continue;
        }
        emit_file_routes(
            graph,
            file,
            content,
            routes,
            revision,
            disappeared_by_hash,
            rename_matched_old_paths,
            state,
            compile_rev,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_file_routes(
    graph: &mut dyn GraphStore,
    file: &DiscoveredFile,
    content: Vec<u8>,
    routes: (Vec<parse::ExtractedSymbol>, Vec<parse::ExtractedEdge>),
    revision: &str,
    disappeared_by_hash: &HashMap<(String, String), FileNode>,
    rename_matched_old_paths: &mut HashSet<String>,
    state: &mut StageState,
    compile_rev: Option<u64>,
) -> crate::Result<()> {
    let content_hash = hex::encode(blake3::hash(&content).as_bytes());
    let existing = graph.file_by_root_path(&file.root_discriminant, &file.relative_path)?;
    let is_content_change = existing
        .as_ref()
        .is_some_and(|n| n.content_hash != content_hash);
    let routes_missing = if let Some(existing) = existing.as_ref() {
        !is_content_change && has_missing_symbols(graph, existing.id, &routes.0)?
    } else {
        false
    };
    if existing.is_some() && !is_content_change && !routes_missing {
        return Ok(());
    }

    let prior_symbols = if is_content_change {
        graph.symbols_for_file(existing.as_ref().unwrap().id)?
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
    graph.upsert_file(FileNode {
        id: file_id,
        root_id: file.root_discriminant.clone(),
        path: file.relative_path.clone(),
        path_history: disappeared_by_hash
            .get(&(file.root_discriminant.clone(), content_hash.clone()))
            .map(|old| {
                let mut history = old.path_history.clone();
                history.insert(0, old.path.clone());
                history
            })
            .unwrap_or_default(),
        content_hash: content_hash.clone(),
        content_sample_hashes: crate::structure::identity::sampled_content_hashes(&content),
        size_bytes: file.size_bytes,
        language: None,
        inline_decisions: rationale::extract_inline_decisions(&content, &file.class),
        last_observed_rev: compile_rev,
        epistemic: Epistemic::ParserObserved,
        provenance: make_provenance("file_route", revision, &file.relative_path, &content_hash),
    })?;
    state.file_map.insert(
        (file.root_discriminant.clone(), file.relative_path.clone()),
        file_id,
    );

    let mut emitted_symbol_ids = HashSet::new();
    let mut emitted_edge_ids = HashSet::new();
    for symbol in &routes.0 {
        let provenance =
            make_provenance("file_route", revision, &file.relative_path, &content_hash);
        let symbol_id = derive_symbol_id(
            file_id,
            &symbol.qualified_name,
            symbol.kind,
            &symbol.body_hash,
        );
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
    if !routes.1.is_empty() {
        state.cross_file_pending.push(CrossFilePending {
            file_id,
            root_id: file.root_discriminant.clone(),
            file_path: file.relative_path.clone(),
            call_refs: Vec::new(),
            import_refs: Vec::new(),
            edge_refs: routes.1,
        });
    }
    retire_stale_routes(
        graph,
        file_id,
        &prior_symbols,
        emitted_symbol_ids,
        emitted_edge_ids,
        is_content_change,
        compile_rev,
    )
}

fn retire_stale_routes(
    graph: &mut dyn GraphStore,
    file_id: crate::core::ids::FileNodeId,
    prior_symbols: &[SymbolNode],
    emitted_symbol_ids: HashSet<crate::core::ids::SymbolNodeId>,
    emitted_edge_ids: HashSet<crate::core::ids::EdgeId>,
    is_content_change: bool,
    compile_rev: Option<u64>,
) -> crate::Result<()> {
    let Some(rev) = compile_rev.filter(|_| is_content_change) else {
        return Ok(());
    };
    let retire_symbols: Vec<_> = prior_symbols
        .iter()
        .filter(|symbol| !emitted_symbol_ids.contains(&symbol.id))
        .map(|symbol| symbol.id)
        .collect();
    graph.retire_symbols_bulk(&retire_symbols, rev)?;
    let retire_edges: Vec<_> = graph
        .edges_owned_by(file_id)?
        .iter()
        .filter(|edge| {
            edge.epistemic == Epistemic::ParserObserved && !emitted_edge_ids.contains(&edge.id)
        })
        .map(|edge| edge.id)
        .collect();
    graph.retire_edges_bulk(&retire_edges, rev)
}

fn has_missing_symbols(
    graph: &mut dyn GraphStore,
    file_id: crate::core::ids::FileNodeId,
    expected: &[parse::ExtractedSymbol],
) -> crate::Result<bool> {
    let existing = graph.symbols_for_file(file_id)?;
    Ok(expected.iter().any(|expected| {
        !existing.iter().any(|symbol| {
            symbol.qualified_name == expected.qualified_name
                && symbol.body_hash == expected.body_hash
        })
    }))
}
