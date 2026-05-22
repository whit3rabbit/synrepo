use std::collections::HashSet;

use super::context::{
    CrossFilePending, NameIndex, QualifiedNameIndex, QualifiedSymbolIndex, SymbolMetaMap,
};
use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    pipeline::structural::{ids::derive_edge_id, provenance::make_provenance},
    structure::graph::{Edge, EdgeKind, Epistemic, GraphStore, Visibility},
};

const SAME_FILE_BONUS: i32 = 100;
const IMPORTED_FILE_BONUS: i32 = 50;
const PUBLIC_BONUS: i32 = 20;
const CRATE_BONUS: i32 = 10;
const PRIVATE_CROSS_FILE_PENALTY: i32 = -100;

pub(super) struct ReferenceResolutionLookups<'a> {
    pub(super) name_index: &'a NameIndex,
    pub(super) qualified_name_index: &'a QualifiedNameIndex,
    pub(super) symbol_meta: &'a SymbolMetaMap,
    pub(super) qualified_index: &'a QualifiedSymbolIndex,
}

pub(super) fn emit_references_for_file(
    graph: &mut dyn GraphStore,
    lookups: ReferenceResolutionLookups<'_>,
    item: &CrossFilePending,
    imports: &HashSet<FileNodeId>,
    revision: &str,
) -> crate::Result<usize> {
    let mut emitted = 0usize;
    for edge_ref in &item.edge_refs {
        let source_ids = lookups
            .qualified_index
            .get(&(item.file_id, edge_ref.from_qualified_name.clone()))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if source_ids.is_empty() {
            continue;
        }
        let target_ids = resolve_reference_target(
            &lookups,
            item.file_id,
            &item.root_id,
            imports,
            &edge_ref.to_reference,
        );
        for source_id in source_ids {
            for target_id in &target_ids {
                graph.insert_edge(build_reference_edge(
                    *source_id,
                    *target_id,
                    edge_ref.kind,
                    item.file_id,
                    revision,
                    &item.file_path,
                ))?;
                emitted += 1;
            }
        }
    }
    Ok(emitted)
}

fn resolve_reference_target(
    lookups: &ReferenceResolutionLookups<'_>,
    file_id: FileNodeId,
    root_id: &str,
    imports: &HashSet<FileNodeId>,
    reference: &str,
) -> Vec<SymbolNodeId> {
    let mut candidates = Vec::new();
    if reference.contains("::") {
        candidates.extend(
            lookups
                .qualified_name_index
                .get(reference)
                .into_iter()
                .flat_map(|ids| ids.iter().copied()),
        );
    }
    if candidates.is_empty() {
        let short = reference.rsplit("::").next().unwrap_or(reference);
        candidates.extend(
            lookups
                .name_index
                .get(short)
                .into_iter()
                .flat_map(|ids| ids.iter().copied()),
        );
    }
    let mut scored = candidates
        .iter()
        .filter_map(|symbol_id| {
            let meta = lookups.symbol_meta.get(symbol_id)?;
            (meta.root_id == root_id).then(|| {
                (
                    *symbol_id,
                    score_candidate(meta.file_id, meta.visibility, file_id, imports),
                )
            })
        })
        .collect::<Vec<_>>();
    let Some(&(_, top_score)) = scored.iter().max_by_key(|(_, score)| *score) else {
        return Vec::new();
    };
    if top_score <= 0 {
        return Vec::new();
    }
    scored.retain(|(_, score)| *score == top_score);
    if scored.len() > 1 {
        return Vec::new();
    }
    scored.into_iter().map(|(symbol_id, _)| symbol_id).collect()
}

fn score_candidate(
    candidate_file: FileNodeId,
    visibility: Visibility,
    source_file: FileNodeId,
    imports: &HashSet<FileNodeId>,
) -> i32 {
    let same_file = candidate_file == source_file;
    let mut score = 0;
    if same_file {
        score += SAME_FILE_BONUS;
    } else if imports.contains(&candidate_file) {
        score += IMPORTED_FILE_BONUS;
    }
    match visibility {
        Visibility::Public => score += PUBLIC_BONUS,
        Visibility::Crate => score += CRATE_BONUS,
        Visibility::Protected => {}
        Visibility::Private if !same_file => score += PRIVATE_CROSS_FILE_PENALTY,
        Visibility::Private | Visibility::Unknown => {}
    }
    score
}

fn build_reference_edge(
    source: SymbolNodeId,
    target: SymbolNodeId,
    kind: EdgeKind,
    owner_file_id: FileNodeId,
    revision: &str,
    file_path: &str,
) -> Edge {
    Edge {
        id: derive_edge_id(NodeId::Symbol(source), NodeId::Symbol(target), kind),
        from: NodeId::Symbol(source),
        to: NodeId::Symbol(target),
        kind,
        owner_file_id: Some(owner_file_id),
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: make_provenance("stage4_references", revision, file_path, ""),
    }
}
