//! High-level maintenance operations for materialized commentary docs.

use std::fs;
use std::path::{Path, PathBuf};

use crate::store::overlay::SqliteOverlayStore;
use crate::structure::graph::GraphReader;

use super::{
    docs_root, index_dir, list_commentary_docs, reconcile_commentary_docs, sync_commentary_index,
    write_discovery_artifacts, CommentaryIndexSyncMode,
};

/// Options for exporting materialized commentary docs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommentaryDocsExportOptions {
    /// Rebuild `.synrepo/explain-docs/` and `.synrepo/explain-index/` before
    /// materializing from overlay commentary.
    pub force: bool,
}

/// Summary of a commentary-doc export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentaryDocsExportSummary {
    /// Total materialized docs after export.
    pub total_docs: usize,
    /// Files rewritten, created, or removed during export.
    pub changed_paths: usize,
    /// Materialized docs directory.
    pub docs_dir: PathBuf,
    /// Index sync mode.
    pub index_mode: CommentaryIndexSyncMode,
    /// Paths touched by index maintenance.
    pub index_touched_paths: usize,
    /// Discovery support artifacts currently written under `.synrepo/explain-docs/`.
    pub discovery_artifacts: usize,
    /// Discovery support artifacts whose bytes changed on disk.
    pub discovery_changed_paths: usize,
}

/// Summary of a commentary-doc clean operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentaryDocsCleanSummary {
    /// Whether the command actually removed files.
    pub applied: bool,
    /// Files under `.synrepo/explain-docs/` before cleaning.
    pub doc_files: usize,
    /// Files under `.synrepo/explain-index/` before cleaning.
    pub index_files: usize,
    /// Materialized docs directory.
    pub docs_dir: PathBuf,
    /// Dedicated docs index directory.
    pub index_dir: PathBuf,
}

/// Materialize editable commentary docs from overlay commentary.
pub fn export_commentary_docs(
    synrepo_dir: &Path,
    graph: &dyn GraphReader,
    overlay: Option<&SqliteOverlayStore>,
    options: CommentaryDocsExportOptions,
) -> crate::Result<CommentaryDocsExportSummary> {
    let docs_dir = docs_root(synrepo_dir);
    let index_path = index_dir(synrepo_dir);
    if options.force {
        remove_dir_if_exists(&docs_dir)?;
        remove_dir_if_exists(&index_path)?;
    }

    let touched = reconcile_commentary_docs(synrepo_dir, graph, overlay)?;
    let discovery = write_discovery_artifacts(synrepo_dir, graph)?;
    let index = sync_commentary_index(synrepo_dir, &touched)?;
    let total_docs = list_commentary_docs(synrepo_dir)?.len();

    Ok(CommentaryDocsExportSummary {
        total_docs,
        changed_paths: touched.len(),
        docs_dir,
        index_mode: index.mode,
        index_touched_paths: index.touched_paths,
        discovery_artifacts: discovery.total_artifacts,
        discovery_changed_paths: discovery.changed_artifacts,
    })
}

/// Remove materialized commentary docs and their search index. Overlay
/// commentary remains untouched.
pub fn clean_commentary_docs(
    synrepo_dir: &Path,
    apply: bool,
) -> crate::Result<CommentaryDocsCleanSummary> {
    let docs_dir = docs_root(synrepo_dir);
    let index_path = index_dir(synrepo_dir);
    let doc_files = count_files(&docs_dir)?;
    let index_files = count_files(&index_path)?;

    if apply {
        remove_dir_if_exists(&docs_dir)?;
        remove_dir_if_exists(&index_path)?;
    }

    Ok(CommentaryDocsCleanSummary {
        applied: apply,
        doc_files,
        index_files,
        docs_dir,
        index_dir: index_path,
    })
}

fn count_files(path: &Path) -> crate::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let mut count = 0usize;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            count += count_files(&path)?;
        } else if path.is_file() {
            count += 1;
        }
    }
    Ok(count)
}

fn remove_dir_if_exists(path: &Path) -> crate::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}
