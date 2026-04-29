use std::path::Path;

use crate::{
    core::ids::NodeId, core::path_safety::safe_join_in_repo, store::sqlite::SqliteGraphStore,
};

pub fn current_endpoint_hash(
    graph: &SqliteGraphStore,
    node: NodeId,
) -> crate::Result<Option<String>> {
    match node {
        NodeId::File(file_id) => Ok(graph.get_file(file_id)?.map(|file| file.content_hash)),
        NodeId::Symbol(symbol_id) => {
            let Some(symbol) = graph.get_symbol(symbol_id)? else {
                return Ok(None);
            };
            Ok(graph
                .get_file(symbol.file_id)?
                .map(|file| file.content_hash))
        }
        NodeId::Concept(concept_id) => {
            let Some(concept) = graph.get_concept(concept_id)? else {
                return Ok(None);
            };
            if let Some(file) = graph.file_by_path(&concept.path)? {
                return Ok(Some(file.content_hash));
            }
            Ok(concept
                .provenance
                .source_artifacts
                .first()
                .map(|source| source.content_hash.clone()))
        }
    }
}

pub fn load_endpoint_text(
    graph: &SqliteGraphStore,
    repo_root: &Path,
    node: NodeId,
) -> crate::Result<Option<String>> {
    match node {
        NodeId::File(file_id) => {
            let Some(file) = graph.get_file(file_id)? else {
                return Ok(None);
            };
            read_repo_file(repo_root, &file.path)
        }
        NodeId::Symbol(symbol_id) => {
            let Some(symbol) = graph.get_symbol(symbol_id)? else {
                return Ok(None);
            };
            let Some(file) = graph.get_file(symbol.file_id)? else {
                return Ok(None);
            };
            let Some(source) = read_repo_file(repo_root, &file.path)? else {
                return Ok(None);
            };
            let start = symbol.body_byte_range.0 as usize;
            let end = symbol.body_byte_range.1 as usize;
            if let Some(slice) = source.get(start..end) {
                return Ok(Some(slice.to_string()));
            }
            // Recorded byte range is outside the current file. The file likely
            // changed since the graph compile that produced this symbol; we
            // cannot verify against fresh source. Surface the staleness so the
            // verifier handles it instead of silently matching against the
            // entire file (which would mask drift).
            tracing::warn!(
                symbol_id = %symbol_id,
                file_path = %file.path,
                recorded_start = start,
                recorded_end = end,
                source_len = source.len(),
                "symbol body byte range out of bounds; treating as unverifiable"
            );
            Ok(None)
        }
        NodeId::Concept(concept_id) => {
            let Some(concept) = graph.get_concept(concept_id)? else {
                return Ok(None);
            };
            read_repo_file(repo_root, &concept.path)
        }
    }
}

pub fn read_repo_file(repo_root: &Path, relative_path: &str) -> crate::Result<Option<String>> {
    // `relative_path` is attacker-controlled if nodes.db was shipped in the
    // clone. Reject absolute paths and `..` traversals so we never resolve
    // outside the repo.
    let Some(path) = safe_join_in_repo(repo_root, relative_path) else {
        return Ok(None);
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
