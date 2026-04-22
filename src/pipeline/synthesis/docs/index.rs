//! Dedicated syntext index maintenance for synthesized commentary docs.

use std::fs;
use std::path::{Path, PathBuf};

use syntext::index::{ExternalFileRecord, Index};
use syntext::Config as SyntextConfig;
use syntext::SearchOptions;
use walkdir::WalkDir;

use super::corpus::{docs_root, index_dir};
use crate::substrate::incremental::should_rebuild;

/// How the commentary-doc index was maintained on this call.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentaryIndexSyncMode {
    NoChange,
    Updated,
    Rebuilt,
}

/// Small summary for callers that want to surface index maintenance progress.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommentaryIndexSyncSummary {
    pub mode: CommentaryIndexSyncMode,
    pub touched_paths: usize,
}

/// Incrementally sync the commentary-doc index, rebuilding when necessary.
pub fn sync_commentary_index(
    synrepo_dir: &Path,
    touched_paths: &[PathBuf],
) -> crate::Result<CommentaryIndexSyncSummary> {
    let docs_root = docs_root(synrepo_dir);
    fs::create_dir_all(&docs_root)?;
    let index_dir = index_dir(synrepo_dir);
    let config = syntext_config(&docs_root, &index_dir);

    if !manifest_path(&index_dir).exists() {
        rebuild_commentary_index(&docs_root, &index_dir)?;
        return Ok(CommentaryIndexSyncSummary {
            mode: CommentaryIndexSyncMode::Rebuilt,
            touched_paths: touched_paths.len(),
        });
    }

    if touched_paths.is_empty() {
        return Ok(CommentaryIndexSyncSummary {
            mode: CommentaryIndexSyncMode::NoChange,
            touched_paths: 0,
        });
    }

    let index = match Index::open(config.clone()) {
        Ok(index) => index,
        Err(err) if should_rebuild(&err) => {
            rebuild_commentary_index(&docs_root, &index_dir)?;
            return Ok(CommentaryIndexSyncSummary {
                mode: CommentaryIndexSyncMode::Rebuilt,
                touched_paths: touched_paths.len(),
            });
        }
        Err(err) => return Err(map_index_error(err)),
    };

    for path in touched_paths {
        let exists_as_file = matches!(fs::metadata(path), Ok(md) if md.is_file());
        if exists_as_file {
            index.notify_change(path).map_err(map_index_error)?;
        } else {
            index.notify_delete(path).map_err(map_index_error)?;
        }
    }

    match index.commit_batch() {
        Ok(()) => {
            if let Err(err) = index.maybe_compact() {
                tracing::warn!(error = %err, "commentary-doc index compaction skipped");
            }
            Ok(CommentaryIndexSyncSummary {
                mode: CommentaryIndexSyncMode::Updated,
                touched_paths: touched_paths.len(),
            })
        }
        Err(err)
            if should_rebuild(&err) || matches!(err, syntext::IndexError::OverlayFull { .. }) =>
        {
            rebuild_commentary_index(&docs_root, &index_dir)?;
            Ok(CommentaryIndexSyncSummary {
                mode: CommentaryIndexSyncMode::Rebuilt,
                touched_paths: touched_paths.len(),
            })
        }
        Err(err) => Err(map_index_error(err)),
    }
}

/// Search the commentary-doc index, building it on first use if needed.
pub fn search_commentary_index(
    synrepo_dir: &Path,
    query: &str,
    max_results: usize,
) -> crate::Result<Vec<syntext::SearchMatch>> {
    let docs_root = docs_root(synrepo_dir);
    fs::create_dir_all(&docs_root)?;
    let index_dir = index_dir(synrepo_dir);
    if !manifest_path(&index_dir).exists() {
        rebuild_commentary_index(&docs_root, &index_dir)?;
    }

    let index = Index::open(syntext_config(&docs_root, &index_dir)).map_err(map_index_error)?;
    let options = SearchOptions {
        max_results: Some(max_results),
        ..SearchOptions::default()
    };
    index.search(query, &options).map_err(map_index_error)
}

fn rebuild_commentary_index(docs_root: &Path, index_dir: &Path) -> crate::Result<()> {
    fs::create_dir_all(docs_root)?;
    fs::create_dir_all(index_dir)?;
    let mut records = Vec::new();
    for entry in WalkDir::new(docs_root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let absolute_path = entry.path().to_path_buf();
        let relative_path = match absolute_path.strip_prefix(docs_root) {
            Ok(path) => PathBuf::from(path.to_string_lossy().replace('\\', "/")),
            Err(_) => continue,
        };
        let size_bytes = entry
            .metadata()
            .map_err(|err| {
                crate::Error::Other(anyhow::anyhow!(
                    "unable to read commentary-doc metadata for {}: {err}",
                    absolute_path.display()
                ))
            })?
            .len();
        records.push(ExternalFileRecord {
            absolute_path,
            relative_path,
            size_bytes,
        });
    }
    Index::build_from_file_records(syntext_config(docs_root, index_dir), records)
        .map_err(map_index_error)?;
    Ok(())
}

fn manifest_path(index_dir: &Path) -> PathBuf {
    index_dir.join("manifest.json")
}

fn syntext_config(docs_root: &Path, index_dir: &Path) -> SyntextConfig {
    SyntextConfig {
        repo_root: docs_root.to_path_buf(),
        index_dir: index_dir.to_path_buf(),
        ..SyntextConfig::default()
    }
}

fn map_index_error(error: syntext::IndexError) -> crate::Error {
    crate::Error::Other(anyhow::anyhow!(
        "unable to maintain commentary-doc index: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::synthesis::docs::{upsert_commentary_doc, CommentaryDocSymbolMetadata};
    use crate::{
        core::ids::{NodeId, SymbolNodeId},
        overlay::{CommentaryEntry, CommentaryProvenance, FreshnessState},
    };
    use time::OffsetDateTime;

    #[test]
    fn commentary_index_searches_materialized_docs() {
        let dir = tempfile::tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let entry = CommentaryEntry {
            node_id: NodeId::Symbol(SymbolNodeId(1)),
            text: "needle commentary".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: "h1".to_string(),
                pass_id: "test".to_string(),
                model_identity: "fixture".to_string(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        };
        upsert_commentary_doc(
            &synrepo_dir,
            entry.node_id,
            &entry,
            FreshnessState::Fresh,
            &CommentaryDocSymbolMetadata {
                qualified_name: "crate::demo::run".to_string(),
                source_path: "src/lib.rs".to_string(),
            },
        )
        .unwrap();

        sync_commentary_index(
            &synrepo_dir,
            &[docs_root(&synrepo_dir)
                .join("symbols")
                .join(format!("{}.md", entry.node_id))],
        )
        .unwrap();
        let hits = search_commentary_index(&synrepo_dir, "needle commentary", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }
}
