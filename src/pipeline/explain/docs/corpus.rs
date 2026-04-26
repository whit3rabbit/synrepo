//! Commentary-doc materialization helpers.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use time::format_description::well_known::Rfc3339;

use crate::core::ids::{FileNodeId, NodeId};
use crate::overlay::{CommentaryEntry, FreshnessState};
use crate::store::overlay::{derive_freshness, SqliteOverlayStore};
use crate::structure::graph::{FileNode, GraphReader};
use crate::util::atomic_write;

/// Render-time metadata for a symbol commentary doc.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentaryDocSymbolMetadata {
    /// Qualified symbol name.
    pub qualified_name: String,
    /// Repo-relative source file path.
    pub source_path: String,
}

/// Parsed metadata header from a materialized commentary doc.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentaryDocHeader {
    /// Commentary target node ID.
    pub node_id: String,
    /// Target kind label: `file`, `symbol`, or `concept`.
    pub node_kind: String,
    /// Qualified symbol name, empty for file commentary.
    pub qualified_name: String,
    /// Repo-relative source path the commentary describes.
    pub source_path: String,
    /// Source content hash recorded when the doc was generated or imported.
    pub source_content_hash: String,
    /// Freshness label relative to current graph content when materialized.
    pub commentary_state: String,
    /// RFC3339 timestamp from the overlay provenance.
    pub generated_at: String,
    /// Model or actor identity from the overlay provenance.
    pub model_identity: String,
}

/// Root directory for materialized explaind docs.
pub fn docs_root(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("explain-docs")
}

/// Dedicated syntext index directory for explaind docs.
pub fn index_dir(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("explain-index")
}

/// Relative path under [`docs_root`] for a symbol commentary doc.
pub fn commentary_doc_relative_path(node_id: NodeId) -> Option<PathBuf> {
    match node_id {
        NodeId::File(_) => Some(PathBuf::from("files").join(format!("{node_id}.md"))),
        NodeId::Symbol(_) => Some(PathBuf::from("symbols").join(format!("{node_id}.md"))),
        _ => None,
    }
}

/// Repo-relative path for a commentary doc under `.synrepo/`.
pub fn repo_relative_doc_path(node_id: NodeId) -> Option<PathBuf> {
    commentary_doc_relative_path(node_id).map(|relative| {
        PathBuf::from(".synrepo")
            .join("explain-docs")
            .join(relative)
    })
}

/// Upsert one materialized commentary doc. Returns the absolute doc path when
/// the file changed on disk.
pub fn upsert_commentary_doc(
    synrepo_dir: &Path,
    node_id: NodeId,
    entry: &CommentaryEntry,
    freshness: FreshnessState,
    metadata: &CommentaryDocSymbolMetadata,
) -> crate::Result<Option<PathBuf>> {
    let Some(relative_path) = commentary_doc_relative_path(node_id) else {
        return Ok(None);
    };
    let absolute_path = docs_root(synrepo_dir).join(relative_path);
    let rendered = render_commentary_doc(node_id, entry, freshness, metadata)?;

    // Cheap size gate before reading: a size mismatch (or missing file)
    // skips the full read and goes straight to write.
    let size_matches = fs::metadata(&absolute_path)
        .map(|md| md.len() == rendered.len() as u64)
        .unwrap_or(false);
    if size_matches && fs::read(&absolute_path).ok().as_deref() == Some(rendered.as_bytes()) {
        return Ok(None);
    }

    if let Some(parent) = absolute_path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(&absolute_path, rendered.as_bytes())?;
    Ok(Some(absolute_path))
}

/// Delete one materialized commentary doc. Returns the absolute doc path when
/// a file was removed.
pub fn delete_commentary_doc(
    synrepo_dir: &Path,
    node_id: NodeId,
) -> crate::Result<Option<PathBuf>> {
    let Some(relative_path) = commentary_doc_relative_path(node_id) else {
        return Ok(None);
    };
    let absolute_path = docs_root(synrepo_dir).join(relative_path);
    if !absolute_path.exists() {
        return Ok(None);
    }
    fs::remove_file(&absolute_path)?;
    Ok(Some(absolute_path))
}

/// Reconcile the explaind commentary docs against the current overlay and
/// graph state. Returns the absolute doc paths that changed.
pub fn reconcile_commentary_docs(
    synrepo_dir: &Path,
    graph: &dyn GraphReader,
    overlay: Option<&SqliteOverlayStore>,
) -> crate::Result<Vec<PathBuf>> {
    let mut touched = Vec::new();
    let mut expected = BTreeSet::new();

    let entries = match overlay {
        Some(overlay) => crate::overlay::with_overlay_read_snapshot(overlay, |_| {
            overlay.all_commentary_entries()
        })?,
        None => Vec::new(),
    };

    // Bulk-load symbol (id, file_id, qname); cache file lookups across entries
    // so N commentary rows living in F files cost one summary query plus F
    // point lookups, not 2N queries.
    let symbol_lookup: HashMap<NodeId, (FileNodeId, String)> = if entries.is_empty() {
        HashMap::new()
    } else {
        graph
            .all_symbols_summary()?
            .into_iter()
            .map(|(sym_id, file_id, qname, _kind, _body_hash)| {
                (NodeId::Symbol(sym_id), (file_id, qname))
            })
            .collect()
    };
    let mut file_cache: HashMap<FileNodeId, Option<FileNode>> = HashMap::new();

    for entry in entries {
        let resolved = match entry.node_id {
            NodeId::File(file_id) => {
                let file = match file_cache.entry(file_id) {
                    std::collections::hash_map::Entry::Occupied(slot) => slot.get().clone(),
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(graph.get_file(file_id)?).clone()
                    }
                };
                file.map(|file| (file, String::new()))
            }
            NodeId::Symbol(_) => match symbol_lookup.get(&entry.node_id).cloned() {
                Some((file_id, qualified_name)) => {
                    let file = match file_cache.entry(file_id) {
                        std::collections::hash_map::Entry::Occupied(slot) => slot.get().clone(),
                        std::collections::hash_map::Entry::Vacant(slot) => {
                            slot.insert(graph.get_file(file_id)?).clone()
                        }
                    };
                    file.map(|file| (file, qualified_name))
                }
                None => None,
            },
            NodeId::Concept(_) => None,
        };
        let Some((file, qualified_name)) = resolved else {
            if let Some(path) = delete_commentary_doc(synrepo_dir, entry.node_id)? {
                touched.push(path);
            }
            continue;
        };

        let metadata = CommentaryDocSymbolMetadata {
            qualified_name,
            source_path: file.path.clone(),
        };
        let freshness = derive_freshness(&entry, &file.content_hash);
        if let Some(path) =
            upsert_commentary_doc(synrepo_dir, entry.node_id, &entry, freshness, &metadata)?
        {
            touched.push(path.clone());
            expected.insert(path);
        } else if let Some(path) = commentary_doc_relative_path(entry.node_id)
            .map(|relative| docs_root(synrepo_dir).join(relative))
        {
            expected.insert(path);
        }
    }

    remove_orphaned_docs(synrepo_dir, &expected, &mut touched)?;

    Ok(touched)
}

/// Parse only the metadata header of a materialized commentary doc.
pub fn parse_commentary_doc_header(
    absolute_path: &Path,
) -> crate::Result<Option<CommentaryDocHeader>> {
    parse_commentary_doc(absolute_path).map(|doc| doc.map(|(header, _)| header))
}

/// Parse a materialized commentary doc into its header and editable body.
pub fn parse_commentary_doc(
    absolute_path: &Path,
) -> crate::Result<Option<(CommentaryDocHeader, String)>> {
    let text = match fs::read_to_string(absolute_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    let mut node_id = None;
    let mut node_kind = None;
    let mut qualified_name = None;
    let mut source_path = None;
    let mut source_content_hash = None;
    let mut commentary_state = None;
    let mut generated_at = None;
    let mut model_identity = None;
    let mut body_start = None;

    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed == "---" {
            body_start = Some(offset + line.len());
            break;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            offset += line.len();
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "node_id" => node_id = Some(value),
            "node_kind" => node_kind = Some(value),
            "qualified_name" => qualified_name = Some(value),
            "source_path" => source_path = Some(value),
            "source_content_hash" => source_content_hash = Some(value),
            "commentary_state" => commentary_state = Some(value),
            "generated_at" => generated_at = Some(value),
            "model_identity" => model_identity = Some(value),
            _ => {}
        }
        offset += line.len();
    }

    let Some(node_id) = node_id else {
        return Ok(None);
    };
    let node_kind = node_kind.unwrap_or_else(|| {
        NodeId::from_str(&node_id)
            .map(|id| node_kind_label(id).to_string())
            .unwrap_or_default()
    });
    let body = body_start
        .map(|start| text[start..].trim_start_matches('\n').to_string())
        .unwrap_or_default();
    Ok(Some((
        CommentaryDocHeader {
            node_id,
            node_kind,
            qualified_name: qualified_name.unwrap_or_default(),
            source_path: source_path.unwrap_or_default(),
            source_content_hash: source_content_hash.unwrap_or_default(),
            commentary_state: commentary_state
                .unwrap_or_else(|| FreshnessState::Missing.as_str().to_string()),
            generated_at: generated_at.unwrap_or_default(),
            model_identity: model_identity.unwrap_or_default(),
        },
        body,
    )))
}

fn render_commentary_doc(
    node_id: NodeId,
    entry: &CommentaryEntry,
    freshness: FreshnessState,
    metadata: &CommentaryDocSymbolMetadata,
) -> crate::Result<String> {
    let generated_at = entry
        .provenance
        .generated_at
        .format(&Rfc3339)
        .map_err(|err| crate::Error::Other(anyhow::anyhow!("invalid timestamp: {err}")))?;
    Ok(format!(
        "# Advisory Commentary\n\n\
         node_id: {node_id}\n\
         node_kind: {node_kind}\n\
         qualified_name: {qualified_name}\n\
         source_path: {source_path}\n\
         source_content_hash: {source_content_hash}\n\
         commentary_state: {commentary_state}\n\
         generated_at: {generated_at}\n\
         model_identity: {model_identity}\n\
         source_store: overlay\n\n\
         ---\n\n\
         {body}\n",
        node_kind = node_kind_label(node_id),
        qualified_name = metadata.qualified_name,
        source_path = metadata.source_path,
        source_content_hash = entry.provenance.source_content_hash,
        commentary_state = freshness.as_str(),
        model_identity = entry.provenance.model_identity,
        body = entry.text.trim_end(),
    ))
}

fn node_kind_label(node_id: NodeId) -> &'static str {
    match node_id {
        NodeId::File(_) => "file",
        NodeId::Symbol(_) => "symbol",
        NodeId::Concept(_) => "concept",
    }
}

fn remove_orphaned_docs(
    synrepo_dir: &Path,
    expected: &BTreeSet<PathBuf>,
    touched: &mut Vec<PathBuf>,
) -> crate::Result<()> {
    for dirname in ["files", "symbols"] {
        let dir = docs_root(synrepo_dir).join(dirname);
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            if expected.contains(&path) {
                continue;
            }
            fs::remove_file(&path)?;
            touched.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
