//! Editable commentary-doc import and listing helpers.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use time::OffsetDateTime;
use walkdir::WalkDir;

use crate::core::ids::NodeId;
use crate::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use crate::pipeline::repair::resolve_commentary_node;
use crate::structure::graph::GraphReader;

use super::corpus::{docs_root, parse_commentary_doc, parse_commentary_doc_header};

/// One materialized commentary doc found under `.synrepo/explain-docs/`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentaryDocListItem {
    /// Absolute filesystem path to the Markdown doc.
    pub path: PathBuf,
    /// Commentary target node ID.
    pub node_id: String,
    /// Target kind label: `file` or `symbol`.
    pub node_kind: String,
    /// Repo-relative source path the commentary describes.
    pub source_path: String,
    /// Qualified symbol name, empty for file commentary.
    pub qualified_name: String,
    /// Freshness label recorded in the doc header.
    pub commentary_state: String,
    /// Model or actor identity recorded in the doc header.
    pub model_identity: String,
}

/// Outcome for importing one edited commentary doc.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentaryDocImportOutcome {
    /// Absolute filesystem path that was considered.
    pub path: PathBuf,
    /// Parsed commentary target node ID, when the header was valid enough.
    pub node_id: Option<String>,
    /// Import result status.
    pub status: CommentaryDocImportStatus,
    /// Human-readable skip reason, absent for successful imports.
    pub reason: Option<String>,
}

/// Stable import result status for a materialized commentary doc.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentaryDocImportStatus {
    /// The editable body was persisted to the overlay store.
    Imported,
    /// The doc was ignored and the reason field explains why.
    Skipped,
}

impl CommentaryDocImportStatus {
    /// Stable lowercase label for command output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::Skipped => "skipped",
        }
    }
}

/// List parseable commentary docs currently materialized on disk.
pub fn list_commentary_docs(synrepo_dir: &Path) -> crate::Result<Vec<CommentaryDocListItem>> {
    let mut docs = Vec::new();
    for path in commentary_doc_paths(synrepo_dir)? {
        let Some(header) = parse_commentary_doc_header(&path)? else {
            continue;
        };
        docs.push(CommentaryDocListItem {
            path,
            node_id: header.node_id,
            node_kind: header.node_kind,
            source_path: header.source_path,
            qualified_name: header.qualified_name,
            commentary_state: header.commentary_state,
            model_identity: header.model_identity,
        });
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

/// Return all materialized commentary Markdown files under the docs root.
pub fn commentary_doc_paths(synrepo_dir: &Path) -> crate::Result<Vec<PathBuf>> {
    let root = docs_root(synrepo_dir);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            paths.push(path.to_path_buf());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Import the editable body of one materialized commentary doc into the overlay.
pub fn import_commentary_doc(
    graph: &dyn GraphReader,
    overlay: &mut dyn OverlayStore,
    path: &Path,
) -> crate::Result<CommentaryDocImportOutcome> {
    let Some((header, body)) = parse_commentary_doc(path)? else {
        return Ok(skipped(path, None, "missing or invalid doc header"));
    };
    let node_id = match NodeId::from_str(&header.node_id) {
        Ok(node_id) => node_id,
        Err(err) => {
            return Ok(skipped(
                path,
                Some(header.node_id),
                format!("invalid node_id: {err}"),
            ));
        }
    };
    let Some(snapshot) = resolve_commentary_node(graph, node_id)? else {
        return Ok(skipped(path, Some(header.node_id), "node no longer exists"));
    };
    if header.source_content_hash != snapshot.content_hash {
        return Ok(skipped(
            path,
            Some(header.node_id),
            "source_content_hash does not match current graph",
        ));
    }
    let text = body.trim_end().to_string();
    if text.trim().is_empty() {
        return Ok(skipped(
            path,
            Some(header.node_id),
            "editable body is empty",
        ));
    }
    overlay.insert_commentary(CommentaryEntry {
        node_id,
        text,
        provenance: CommentaryProvenance {
            source_content_hash: snapshot.content_hash,
            pass_id: "user-edit".to_string(),
            model_identity: "user-edited".to_string(),
            generated_at: OffsetDateTime::now_utc(),
        },
    })?;
    Ok(CommentaryDocImportOutcome {
        path: path.to_path_buf(),
        node_id: Some(header.node_id),
        status: CommentaryDocImportStatus::Imported,
        reason: None,
    })
}

fn skipped(
    path: &Path,
    node_id: Option<String>,
    reason: impl Into<String>,
) -> CommentaryDocImportOutcome {
    CommentaryDocImportOutcome {
        path: path.to_path_buf(),
        node_id,
        status: CommentaryDocImportStatus::Skipped,
        reason: Some(reason.into()),
    }
}

#[cfg(test)]
mod tests;
