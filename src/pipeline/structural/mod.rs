//! The structural compile pipeline.
//!
//! Runs synchronously, LLM-free, on every `synrepo init` and on-demand
//! refresh. This initial producer set covers stages 1–3 of the full
//! eight-stage pipeline:
//!
//! 1. **Discover** — walk the repo via `substrate::discover`, reusing its
//!    `.gitignore` / `.synignore` / redaction rules.
//! 2. **Parse code** — tree-sitter for each `SupportedCode` file, extract
//!    symbols and within-file `defines` edges.
//! 3. **Parse prose** — markdown link parser for files in configured concept
//!    directories, extract concept nodes.
//!
//! Stages 4–8 (cross-file edge resolution, git mining, identity cascade,
//! drift scoring, ArcSwap commit) are NOT part of this change and remain
//! TODO stubs.
//!
//! ## Relationship to watch and reconcile
//!
//! The watcher (`pipeline::watch`) drives this function as a trigger-and-
//! coalesce layer rather than as an independent graph producer. Each
//! reconcile pass calls `run_structural_compile` under the writer lock
//! (`pipeline::writer`). This function should remain stateless and
//! re-entrant so the reconcile path can call it safely on any event burst.
//!
//! ## Replacement contract
//!
//! Each compile run replaces stale facts for the producer-owned slice:
//! - File nodes with changed content are deleted (cascading to their symbols
//!   and edges) and re-inserted, keeping the original stable ID.
//! - File nodes whose paths have disappeared from the discovered set are
//!   deleted.
//! - Concept nodes whose paths have disappeared are deleted.
//! - The run is idempotent, unchanged files are skipped entirely.

mod ids;
mod provenance;

#[cfg(test)]
mod tests;

use std::{collections::BTreeSet, path::Path, time::Instant};

use ids::{derive_edge_id, derive_file_id, derive_symbol_id};
use provenance::{current_git_revision, make_provenance};

use crate::{
    config::Config,
    core::ids::NodeId,
    structure::{
        graph::{
            concept_source_path_allowed, Edge, EdgeKind, Epistemic, FileNode, GraphStore,
            SymbolNode,
        },
        parse, prose,
    },
    substrate::{self, FileClass},
};

/// Run one structural compile cycle.
///
/// Re-entrant and idempotent, calling twice with the same repository state
/// produces the same graph contents both times.
pub fn run_structural_compile(
    repo_root: &Path,
    config: &Config,
    graph: &mut dyn GraphStore,
) -> crate::Result<CompileSummary> {
    let start = Instant::now();

    let discovered = substrate::discover(repo_root, config)?;
    let files_discovered = discovered.len();

    let discovered_paths: BTreeSet<String> =
        discovered.iter().map(|f| f.relative_path.clone()).collect();

    // Wrap all graph reads and writes in a single transaction so that each
    // compile cycle is atomic and inserts are batched rather than auto-committed.
    graph.begin()?;

    for (path, file_id) in &graph.all_file_paths()? {
        if !discovered_paths.contains(path) {
            graph.delete_node(NodeId::File(*file_id))?;
        }
    }

    let discovered_concept_paths: BTreeSet<String> = discovered
        .iter()
        .filter(|f| {
            matches!(f.class, FileClass::Markdown)
                && concept_source_path_allowed(&f.relative_path, &config.concept_directories)
        })
        .map(|f| f.relative_path.clone())
        .collect();

    for (path, concept_id) in &graph.all_concept_paths()? {
        if !discovered_concept_paths.contains(path) {
            graph.delete_node(NodeId::Concept(*concept_id))?;
        }
    }

    let revision = current_git_revision(repo_root);
    let mut files_parsed = 0usize;
    let mut symbols_extracted = 0usize;
    let mut edges_added = 0usize;
    let mut concept_nodes_emitted = 0usize;

    for file in &discovered {
        if matches!(file.class, FileClass::SupportedCode { .. }) {
            let content = match std::fs::read(&file.absolute_path) {
                Ok(content) => content,
                Err(_) => continue,
            };

            let content_hash = hex::encode(blake3::hash(&content).as_bytes());
            let existing = graph.file_by_path(&file.relative_path)?;

            if let Some(ref file_node) = existing {
                if file_node.content_hash == content_hash {
                    continue;
                }
                graph.delete_node(NodeId::File(file_node.id))?;
            }

            let file_id = existing
                .map(|file_node| file_node.id)
                .unwrap_or_else(|| derive_file_id(&content_hash));

            let language = match file.class {
                FileClass::SupportedCode { language } => Some(language.to_string()),
                _ => None,
            };

            graph.upsert_file(FileNode {
                id: file_id,
                path: file.relative_path.clone(),
                path_history: vec![],
                content_hash: content_hash.clone(),
                size_bytes: file.size_bytes,
                language,
                epistemic: Epistemic::ParserObserved,
                provenance: make_provenance(
                    "discover",
                    &revision,
                    &file.relative_path,
                    &content_hash,
                ),
            })?;

            match parse::parse_file(file.absolute_path.as_path(), &content)? {
                Some(parsed) if !parsed.symbols.is_empty() => {
                    files_parsed += 1;
                    for symbol in &parsed.symbols {
                        let symbol_id = derive_symbol_id(
                            file_id,
                            &symbol.qualified_name,
                            symbol.kind,
                            &symbol.body_hash,
                        );
                        let provenance = make_provenance(
                            "parse_code",
                            &revision,
                            &file.relative_path,
                            &content_hash,
                        );

                        graph.upsert_symbol(SymbolNode {
                            id: symbol_id,
                            file_id,
                            qualified_name: symbol.qualified_name.clone(),
                            display_name: symbol.display_name.clone(),
                            kind: symbol.kind,
                            body_byte_range: symbol.body_byte_range,
                            body_hash: symbol.body_hash.clone(),
                            signature: symbol.signature.clone(),
                            doc_comment: symbol.doc_comment.clone(),
                            epistemic: Epistemic::ParserObserved,
                            provenance: provenance.clone(),
                        })?;

                        graph.insert_edge(Edge {
                            id: derive_edge_id(
                                NodeId::File(file_id),
                                NodeId::Symbol(symbol_id),
                                EdgeKind::Defines,
                            ),
                            from: NodeId::File(file_id),
                            to: NodeId::Symbol(symbol_id),
                            kind: EdgeKind::Defines,
                            epistemic: Epistemic::ParserObserved,
                            drift_score: 0.0,
                            provenance,
                        })?;

                        symbols_extracted += 1;
                        edges_added += 1;
                    }
                }
                _ => {}
            }
        }

        if matches!(file.class, FileClass::Markdown)
            && concept_source_path_allowed(&file.relative_path, &config.concept_directories)
        {
            let content = match std::fs::read(&file.absolute_path) {
                Ok(content) => content,
                Err(_) => continue,
            };

            if let Some(concept) = prose::extract_concept(&file.relative_path, &content, &revision)?
            {
                graph.upsert_concept(concept)?;
                concept_nodes_emitted += 1;
            }
        }
    }

    graph.commit()?;

    Ok(CompileSummary {
        files_discovered,
        files_parsed,
        symbols_extracted,
        edges_added,
        concept_nodes_emitted,
        identities_resolved: 0,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// Summary of what one compile cycle produced.
#[derive(Clone, Debug, Default)]
pub struct CompileSummary {
    /// Files discovered and classified.
    pub files_discovered: usize,
    /// Files parsed for code symbols.
    pub files_parsed: usize,
    /// Symbols extracted across all parsed files.
    pub symbols_extracted: usize,
    /// `defines` edges added this cycle.
    pub edges_added: usize,
    /// Concept nodes emitted from markdown files.
    pub concept_nodes_emitted: usize,
    /// Identity resolutions performed (phase-1+).
    pub identities_resolved: usize,
    /// Wall-clock time in milliseconds.
    pub elapsed_ms: u64,
}
