//! Native discovery artifacts for materialized explain commentary docs.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use serde::Serialize;

use crate::core::ids::NodeId;
use crate::structure::graph::GraphReader;
use crate::util::atomic_write;

use super::{docs_root, list_commentary_docs};

const SOURCE_STORE_OVERLAY: &str = "overlay";
const INDEX_MD: &str = "index.md";
const CATALOGUE_JSON: &str = "catalogue.json";
const LLMS_TXT: &str = "llms.txt";

/// Summary of discovery artifact maintenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryArtifactsSummary {
    /// Support artifacts currently written by export.
    pub total_artifacts: usize,
    /// Support artifacts whose bytes changed on disk.
    pub changed_artifacts: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CommentaryDocCatalogue {
    source_store: &'static str,
    advisory: bool,
    total_docs: usize,
    freshness: BTreeMap<String, usize>,
    docs: Vec<CommentaryDocCatalogueEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CommentaryDocCatalogueEntry {
    node_id: String,
    node_kind: String,
    source_path: String,
    source_reference: SourceReference,
    doc_path: String,
    qualified_name: String,
    commentary_state: String,
    generated_at: String,
    model_identity: String,
    source_store: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct SourceReference {
    path: String,
    line_start: Option<u32>,
    line_end: Option<u32>,
}

/// Write `index.md`, `catalogue.json`, and `llms.txt` under
/// `.synrepo/explain-docs/`.
pub fn write_discovery_artifacts(
    synrepo_dir: &Path,
    graph: &dyn GraphReader,
) -> crate::Result<DiscoveryArtifactsSummary> {
    let root = docs_root(synrepo_dir);
    fs::create_dir_all(&root)?;

    let catalogue = build_catalogue(synrepo_dir, graph)?;
    let artifacts = [
        (root.join(INDEX_MD), render_index(&catalogue)),
        (
            root.join(CATALOGUE_JSON),
            serde_json::to_string_pretty(&catalogue)
                .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?
                + "\n",
        ),
        (root.join(LLMS_TXT), render_llms_txt(&catalogue)),
    ];

    let mut changed = 0usize;
    for (path, content) in artifacts {
        if write_if_changed(&path, &content)? {
            changed += 1;
        }
    }

    Ok(DiscoveryArtifactsSummary {
        total_artifacts: 3,
        changed_artifacts: changed,
    })
}

fn build_catalogue(
    synrepo_dir: &Path,
    graph: &dyn GraphReader,
) -> crate::Result<CommentaryDocCatalogue> {
    let mut docs = list_commentary_docs(synrepo_dir)?
        .into_iter()
        .filter_map(|doc| {
            let node_id = NodeId::from_str(&doc.node_id).ok()?;
            let source_reference = source_reference(synrepo_dir, graph, node_id, &doc.source_path);
            let doc_path = doc
                .path
                .strip_prefix(docs_root(synrepo_dir))
                .unwrap_or(&doc.path)
                .to_string_lossy()
                .replace('\\', "/");
            Some(CommentaryDocCatalogueEntry {
                node_id: doc.node_id,
                node_kind: doc.node_kind,
                source_path: doc.source_path,
                source_reference,
                doc_path,
                qualified_name: doc.qualified_name,
                commentary_state: doc.commentary_state,
                generated_at: doc.generated_at,
                model_identity: doc.model_identity,
                source_store: SOURCE_STORE_OVERLAY,
            })
        })
        .collect::<Vec<_>>();
    docs.sort_by(|a, b| a.doc_path.cmp(&b.doc_path));

    let mut freshness = BTreeMap::new();
    for doc in &docs {
        *freshness.entry(doc.commentary_state.clone()).or_insert(0) += 1;
    }

    Ok(CommentaryDocCatalogue {
        source_store: SOURCE_STORE_OVERLAY,
        advisory: true,
        total_docs: docs.len(),
        freshness,
        docs,
    })
}

fn source_reference(
    synrepo_dir: &Path,
    graph: &dyn GraphReader,
    node_id: NodeId,
    fallback_path: &str,
) -> SourceReference {
    match node_id {
        NodeId::File(file_id) => {
            let path = graph
                .get_file(file_id)
                .ok()
                .flatten()
                .map(|file| file.path)
                .unwrap_or_else(|| fallback_path.to_string());
            SourceReference {
                path,
                line_start: None,
                line_end: None,
            }
        }
        NodeId::Symbol(symbol_id) => {
            let Some(symbol) = graph.get_symbol(symbol_id).ok().flatten() else {
                return source_reference_path_only(fallback_path);
            };
            let Some(file) = graph.get_file(symbol.file_id).ok().flatten() else {
                return source_reference_path_only(fallback_path);
            };
            let range = repo_root(synrepo_dir)
                .and_then(|root| fs::read_to_string(root.join(&file.path)).ok())
                .map(|source| byte_range_to_line_range(source.as_bytes(), symbol.body_byte_range));
            SourceReference {
                path: file.path,
                line_start: range.map(|(start, _end)| start),
                line_end: range.map(|(_start, end)| end),
            }
        }
        NodeId::Concept(_) => source_reference_path_only(fallback_path),
    }
}

fn source_reference_path_only(path: &str) -> SourceReference {
    SourceReference {
        path: path.to_string(),
        line_start: None,
        line_end: None,
    }
}

fn repo_root(synrepo_dir: &Path) -> Option<&Path> {
    synrepo_dir.parent()
}

fn byte_range_to_line_range(bytes: &[u8], range: (u32, u32)) -> (u32, u32) {
    let len = bytes.len();
    let start = (range.0 as usize).min(len);
    let end = (range.1 as usize).min(len).max(start);
    let end_for_line = if end > start { end - 1 } else { end };
    (
        byte_to_line(bytes, start),
        byte_to_line(bytes, end_for_line),
    )
}

fn byte_to_line(bytes: &[u8], byte: usize) -> u32 {
    bytes[..byte.min(bytes.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count() as u32
        + 1
}

fn render_index(catalogue: &CommentaryDocCatalogue) -> String {
    let mut out = String::from(
        "# Advisory Explain Docs\n\n\
         source_store: overlay\n\n\
         These docs are materialized advisory commentary from the synrepo \
         overlay. Graph/source facts remain canonical.\n\n\
         ## Summary\n\n\
         | Metric | Value |\n\
         | --- | --- |\n",
    );
    out.push_str(&format!(
        "| Total commentary docs | {} |\n",
        catalogue.total_docs
    ));
    out.push_str("| Source store | overlay |\n\n");
    out.push_str("## Freshness\n\n| State | Count |\n| --- | ---: |\n");
    for (state, count) in &catalogue.freshness {
        out.push_str(&format!("| {state} | {count} |\n"));
    }
    if catalogue.freshness.is_empty() {
        out.push_str("| none | 0 |\n");
    }

    out.push_str("\n## Commentary Docs\n\n");
    out.push_str("| Kind | Node ID | Target | Source | State | Model | Generated | Doc |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for doc in &catalogue.docs {
        let target = if doc.qualified_name.is_empty() {
            doc.node_id.as_str()
        } else {
            doc.qualified_name.as_str()
        };
        out.push_str(&format!(
            "| {} | `{}` | `{}` | `{}` | {} | `{}` | `{}` | [{}]({}) |\n",
            doc.node_kind,
            doc.node_id,
            target,
            source_ref_label(&doc.source_reference),
            doc.commentary_state,
            doc.model_identity,
            doc.generated_at,
            doc.doc_path,
            doc.doc_path,
        ));
    }
    if catalogue.docs.is_empty() {
        out.push_str("| none | none | none | none | none | none | none | none |\n");
    }
    out
}

fn render_llms_txt(catalogue: &CommentaryDocCatalogue) -> String {
    let mut out = String::from(
        "# synrepo Explain Docs\n\n\
         > Advisory overlay commentary materialized from synrepo explain. \
         Canonical source truth remains the graph and repository files.\n\n\
         ## Discovery\n\n\
         - [Index](./index.md): human-readable commentary catalogue\n\
         - [Catalogue](./catalogue.json): machine-readable manifest\n\
         - Use `synrepo docs search <query>` for CLI search\n\
         - Use MCP `synrepo_docs_search` for agent search\n\n",
    );
    out.push_str("## Summary\n\n");
    out.push_str(&format!(
        "- Total commentary docs: {}\n",
        catalogue.total_docs
    ));
    out.push_str("- Source store: overlay\n");
    for (state, count) in &catalogue.freshness {
        out.push_str(&format!("- {state}: {count}\n"));
    }
    out.push_str("\n## Commentary\n\n");
    for doc in catalogue.docs.iter().take(50) {
        let target = if doc.qualified_name.is_empty() {
            doc.node_id.as_str()
        } else {
            doc.qualified_name.as_str()
        };
        out.push_str(&format!(
            "- [{}](./{}) (node: `{}`, kind: {}, state: {}, source: {}, model: `{}`, generated: `{}`)\n",
            target,
            doc.doc_path,
            doc.node_id,
            doc.node_kind,
            doc.commentary_state,
            source_ref_label(&doc.source_reference),
            doc.model_identity,
            doc.generated_at,
        ));
    }
    if catalogue.docs.len() > 50 {
        out.push_str("- Additional docs are listed in `catalogue.json`.\n");
    }
    out
}

fn source_ref_label(reference: &SourceReference) -> String {
    match (reference.line_start, reference.line_end) {
        (Some(start), Some(end)) if start != end => format!("{}:{start}-{end}", reference.path),
        (Some(start), _) => format!("{}:{start}", reference.path),
        _ => reference.path.clone(),
    }
}

fn write_if_changed(path: &Path, content: &str) -> crate::Result<bool> {
    if fs::read_to_string(path)
        .ok()
        .is_some_and(|existing| existing == content)
    {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, content.as_bytes())?;
    Ok(true)
}

#[cfg(test)]
mod tests;
