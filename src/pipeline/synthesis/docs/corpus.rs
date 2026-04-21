//! Commentary-doc materialization helpers.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommentaryDocHeader {
    pub node_id: String,
    pub qualified_name: String,
    pub source_path: String,
    pub commentary_state: String,
    pub generated_at: String,
    pub model_identity: String,
}

/// Root directory for materialized synthesized docs.
pub fn docs_root(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("synthesis-docs")
}

/// Dedicated syntext index directory for synthesized docs.
pub fn index_dir(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("synthesis-index")
}

/// Relative path under [`docs_root`] for a symbol commentary doc.
pub fn commentary_doc_relative_path(node_id: NodeId) -> Option<PathBuf> {
    match node_id {
        NodeId::Symbol(_) => Some(PathBuf::from("symbols").join(format!("{node_id}.md"))),
        _ => None,
    }
}

/// Repo-relative path for a commentary doc under `.synrepo/`.
pub fn repo_relative_doc_path(node_id: NodeId) -> Option<PathBuf> {
    commentary_doc_relative_path(node_id).map(|relative| {
        PathBuf::from(".synrepo")
            .join("synthesis-docs")
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

/// Reconcile the synthesized commentary docs against the current overlay and
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
        let Some((file_id, qualified_name)) = symbol_lookup.get(&entry.node_id).cloned() else {
            if let Some(path) = delete_commentary_doc(synrepo_dir, entry.node_id)? {
                touched.push(path);
            }
            continue;
        };
        let file = match file_cache.entry(file_id) {
            std::collections::hash_map::Entry::Occupied(slot) => slot.get().clone(),
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(graph.get_file(file_id)?).clone()
            }
        };
        let Some(file) = file else {
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

    let symbols_dir = docs_root(synrepo_dir).join("symbols");
    if symbols_dir.exists() {
        for entry in fs::read_dir(&symbols_dir)? {
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

    Ok(touched)
}

pub(crate) fn parse_commentary_doc_header(
    absolute_path: &Path,
) -> crate::Result<Option<CommentaryDocHeader>> {
    let text = match fs::read_to_string(absolute_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    let mut node_id = None;
    let mut qualified_name = None;
    let mut source_path = None;
    let mut commentary_state = None;
    let mut generated_at = None;
    let mut model_identity = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "node_id" => node_id = Some(value),
            "qualified_name" => qualified_name = Some(value),
            "source_path" => source_path = Some(value),
            "commentary_state" => commentary_state = Some(value),
            "generated_at" => generated_at = Some(value),
            "model_identity" => model_identity = Some(value),
            _ => {}
        }
    }

    let Some(node_id) = node_id else {
        return Ok(None);
    };
    Ok(Some(CommentaryDocHeader {
        node_id,
        qualified_name: qualified_name.unwrap_or_default(),
        source_path: source_path.unwrap_or_default(),
        commentary_state: commentary_state
            .unwrap_or_else(|| FreshnessState::Missing.as_str().to_string()),
        generated_at: generated_at.unwrap_or_default(),
        model_identity: model_identity.unwrap_or_default(),
    }))
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
         qualified_name: {qualified_name}\n\
         source_path: {source_path}\n\
         commentary_state: {commentary_state}\n\
         generated_at: {generated_at}\n\
         model_identity: {model_identity}\n\
         source_store: overlay\n\n\
         ---\n\n\
         {body}\n",
        qualified_name = metadata.qualified_name,
        source_path = metadata.source_path,
        commentary_state = freshness.as_str(),
        model_identity = entry.provenance.model_identity,
        body = entry.text.trim_end(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    use crate::overlay::CommentaryProvenance;
    use crate::surface::card::compiler::tests::fixtures::{
        fresh_symbol_fixture, make_overlay_store,
    };
    use time::OffsetDateTime;

    #[test]
    fn upsert_and_parse_commentary_doc_round_trip() {
        let repo = tempfile::tempdir().unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let entry = CommentaryEntry {
            node_id: NodeId::from_str("sym_00000000000000000000000000000001").unwrap(),
            text: "Fresh prose.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: "h1".to_string(),
                pass_id: "test".to_string(),
                model_identity: "fixture".to_string(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        };
        let metadata = CommentaryDocSymbolMetadata {
            qualified_name: "crate::demo::run".to_string(),
            source_path: "src/lib.rs".to_string(),
        };

        let path = upsert_commentary_doc(
            &synrepo_dir,
            entry.node_id,
            &entry,
            FreshnessState::Fresh,
            &metadata,
        )
        .unwrap()
        .unwrap();
        let header = parse_commentary_doc_header(&path).unwrap().unwrap();
        assert_eq!(header.node_id, entry.node_id.to_string());
        assert_eq!(header.qualified_name, "crate::demo::run");
        assert_eq!(header.source_path, "src/lib.rs");
        assert_eq!(header.commentary_state, "fresh");
        assert_eq!(header.model_identity, "fixture");
    }

    #[test]
    fn reconcile_removes_orphaned_commentary_docs() {
        let (repo, graph, sym_id) = fresh_symbol_fixture();
        let overlay = make_overlay_store(&repo);
        let entry = CommentaryEntry {
            node_id: NodeId::Symbol(sym_id),
            text: "Fresh prose.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: graph
                    .file_by_path("src/lib.rs")
                    .unwrap()
                    .unwrap()
                    .content_hash,
                pass_id: "test".to_string(),
                model_identity: "fixture".to_string(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        };
        overlay.lock().insert_commentary(entry.clone()).unwrap();

        let synrepo_dir = repo.path().join(".synrepo");
        let overlay_store =
            SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
        let first = reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();
        assert_eq!(first.len(), 1);

        overlay.lock().prune_orphans(&[]).unwrap();
        let overlay_store =
            SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
        let second = reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();
        assert_eq!(second.len(), 1);
        assert!(!docs_root(&synrepo_dir)
            .join("symbols")
            .join(format!("{}.md", NodeId::Symbol(sym_id)))
            .exists());
    }

    #[test]
    fn reconcile_rewrites_commentary_state_when_overlay_entry_is_stale() {
        let (repo, graph, sym_id) = fresh_symbol_fixture();
        let overlay = make_overlay_store(&repo);
        overlay
            .lock()
            .insert_commentary(CommentaryEntry {
                node_id: NodeId::Symbol(sym_id),
                text: "Stale prose.".to_string(),
                provenance: CommentaryProvenance {
                    source_content_hash: "outdated-hash".to_string(),
                    pass_id: "test".to_string(),
                    model_identity: "fixture".to_string(),
                    generated_at: OffsetDateTime::UNIX_EPOCH,
                },
            })
            .unwrap();

        let synrepo_dir = repo.path().join(".synrepo");
        let overlay_store =
            SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
        reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();

        let doc_path = docs_root(&synrepo_dir)
            .join("symbols")
            .join(format!("{}.md", NodeId::Symbol(sym_id)));
        let header = parse_commentary_doc_header(&doc_path).unwrap().unwrap();
        assert_eq!(header.commentary_state, "stale");
    }
}
