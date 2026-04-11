use crate::{
    core::ids::{EdgeId, FileNodeId, NodeId, SymbolNodeId},
    structure::graph::{EdgeKind, SymbolKind},
};

/// Derive a stable `FileNodeId` from the content hash of the first-seen version.
///
/// Uses the first 8 bytes of a secondary blake3 hash of the hex hash string.
/// This indirection preserves the "first-seen hash" invariant, for new files
/// the ID is derived from the current content, for existing files the caller
/// uses the stored ID from the graph.
pub(super) fn derive_file_id(content_hash: &str) -> FileNodeId {
    FileNodeId(hash_to_u64(blake3::hash(content_hash.as_bytes())))
}

/// Derive a stable `SymbolNodeId` from `(file_id, qualified_name, kind, body_hash)`.
pub(super) fn derive_symbol_id(
    file_id: FileNodeId,
    qualified_name: &str,
    kind: SymbolKind,
    body_hash: &str,
) -> SymbolNodeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&file_id.0.to_le_bytes());
    hasher.update(qualified_name.as_bytes());
    hasher.update(kind.as_str().as_bytes());
    hasher.update(body_hash.as_bytes());
    SymbolNodeId(hash_to_u64(hasher.finalize()))
}

/// Derive a stable `EdgeId` from `(from_node, to_node, kind)`.
pub(super) fn derive_edge_id(from: NodeId, to: NodeId, kind: EdgeKind) -> EdgeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(from.to_string().as_bytes());
    hasher.update(to.to_string().as_bytes());
    hasher.update(kind.as_str().as_bytes());
    EdgeId(hash_to_u64(hasher.finalize()))
}

/// Take the first 8 bytes of a blake3 hash as a little-endian u64.
fn hash_to_u64(hash: blake3::Hash) -> u64 {
    u64::from_le_bytes(
        hash.as_bytes()[..8]
            .try_into()
            .expect("blake3 output is 32 bytes"),
    )
}
