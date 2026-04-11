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
//! Stage 4 (cross-file edge resolution) is now wired: after stages 1–3
//! commit, a name-resolution pass emits `Calls` and `Imports` edges.
//! Stages 5–8 (git mining, identity cascade, drift scoring, ArcSwap commit)
//! remain TODO stubs.
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
mod stage4;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
    time::Instant,
};

use ids::{derive_edge_id, derive_file_id, derive_symbol_id};
use provenance::make_provenance;

use crate::{
    config::Config,
    core::ids::NodeId,
    pipeline::git::GitRepositorySnapshot,
    structure::{
        graph::{
            concept_source_path_allowed, Edge, EdgeKind, Epistemic, FileNode, GraphStore,
            SymbolNode,
        },
        parse, prose,
    },
    substrate::{self, DiscoveredFile, FileClass},
};

use stage4::CrossFilePending;

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
    // On any error, rollback ensures the next reconcile starts with a clean slate.
    graph.begin()?;
    let txn = match stages_1_to_3(repo_root, config, graph, &discovered, &discovered_paths) {
        Ok(v) => v,
        Err(e) => {
            let _ = graph.rollback();
            return Err(e);
        }
    };
    graph.commit()?;

    // Stage 4: cross-file edge resolution (Calls, Imports).
    // Runs in its own transaction after stages 1–3 are committed so the
    // name index query sees all nodes emitted this cycle.
    let stage4_edges =
        stage4::run_cross_file_resolution(graph, &txn.cross_file_pending, &txn.revision)?;

    Ok(CompileSummary {
        files_discovered,
        files_parsed: txn.files_parsed,
        symbols_extracted: txn.symbols_extracted,
        edges_added: txn.edges_added + stage4_edges,
        concept_nodes_emitted: txn.concept_nodes_emitted,
        identities_resolved: txn.identities_resolved,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

// Output of the stages 1-3 transaction body.
struct StagesTxnResult {
    files_parsed: usize,
    symbols_extracted: usize,
    edges_added: usize,
    concept_nodes_emitted: usize,
    identities_resolved: usize,
    cross_file_pending: Vec<CrossFilePending>,
    revision: String,
}

// Transaction wrapper: begin, run body, commit on Ok or rollback on Err.
// Used by this module and accessible to submodule stage4 via `super::`.
fn with_transaction<T>(
    graph: &mut dyn GraphStore,
    body: impl FnOnce(&mut dyn GraphStore) -> crate::Result<T>,
) -> crate::Result<T> {
    graph.begin()?;
    match body(graph) {
        Ok(v) => {
            graph.commit()?;
            Ok(v)
        }
        Err(e) => {
            let _ = graph.rollback();
            Err(e)
        }
    }
}

/// Stages 1–3 transaction body: stale detection, symbol extraction, concept
/// extraction. Runs inside an already-open transaction; does not call
/// begin/commit itself.
fn stages_1_to_3(
    repo_root: &Path,
    config: &Config,
    graph: &mut dyn GraphStore,
    discovered: &[DiscoveredFile],
    discovered_paths: &BTreeSet<String>,
) -> crate::Result<StagesTxnResult> {
    // Load all current file paths once; used for stale detection and rename matching.
    let existing_file_paths = graph.all_file_paths()?;

    // For each path that has disappeared, load the full node so we can match its
    // content hash against new files that may be renames.
    let mut disappeared_by_hash: HashMap<String, FileNode> = HashMap::new();
    for (path, _) in &existing_file_paths {
        if !discovered_paths.contains(path) {
            if let Some(node) = graph.file_by_path(path)? {
                // Keep first match per hash; duplicate content hashes are
                // astronomically unlikely with blake3 but handled gracefully.
                disappeared_by_hash
                    .entry(node.content_hash.clone())
                    .or_insert(node);
            }
        }
    }

    // Track which disappeared paths were matched as renames so we skip deleting them
    // (the upsert in the parse loop moves the row to the new path in-place).
    let mut rename_matched_old_paths: HashSet<String> = HashSet::new();

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

    let git = GitRepositorySnapshot::inspect(repo_root);
    let revision = git.source_revision().to_string();
    let mut files_parsed = 0usize;
    let mut symbols_extracted = 0usize;
    let mut edges_added = 0usize;
    let mut concept_nodes_emitted = 0usize;
    let mut identities_resolved = 0usize;
    // Stage 4: collect cross-file refs from newly parsed files.
    let mut cross_file_pending: Vec<CrossFilePending> = Vec::new();

    for file in discovered {
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

            let file_id = if let Some(ref file_node) = existing {
                // Same path, different content: reuse the stable ID (content change, not rename).
                file_node.id
            } else if let Some(old_node) = disappeared_by_hash.get(&content_hash) {
                // Same content, different path: this is a rename. Preserve the old ID
                // and record the old path so we skip deleting it at the end.
                rename_matched_old_paths.insert(old_node.path.clone());
                identities_resolved += 1;
                old_node.id
            } else {
                // Genuinely new file.
                derive_file_id(&content_hash)
            };

            let language = match file.class {
                FileClass::SupportedCode { language } => Some(language.to_string()),
                _ => None,
            };

            graph.upsert_file(FileNode {
                id: file_id,
                path: file.relative_path.clone(),
                path_history: disappeared_by_hash
                    .get(&content_hash)
                    .map(|old| {
                        let mut h = old.path_history.clone();
                        h.insert(0, old.path.clone());
                        h
                    })
                    .unwrap_or_default(),
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

            if let Some(parsed) = parse::parse_file(file.absolute_path.as_path(), &content)? {
                if !parsed.symbols.is_empty() {
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
                // Collect cross-file refs for stage 4, regardless of symbol count.
                if !parsed.call_refs.is_empty() || !parsed.import_refs.is_empty() {
                    cross_file_pending.push(CrossFilePending {
                        file_id,
                        file_path: file.relative_path.clone(),
                        call_refs: parsed.call_refs,
                        import_refs: parsed.import_refs,
                    });
                }
            }
        }

        if matches!(file.class, FileClass::Markdown)
            && concept_source_path_allowed(&file.relative_path, &config.concept_directories)
        {
            let content = match std::fs::read(&file.absolute_path) {
                Ok(content) => content,
                Err(_) => continue,
            };

            if let Some(concept) =
                prose::extract_concept(&file.relative_path, &content, &revision)?
            {
                graph.upsert_concept(concept)?;
                concept_nodes_emitted += 1;
            }
        }
    }

    // Delete file nodes that disappeared and were not matched as renames.
    for (path, file_id) in &existing_file_paths {
        if !discovered_paths.contains(path) && !rename_matched_old_paths.contains(path) {
            graph.delete_node(NodeId::File(*file_id))?;
        }
    }

    Ok(StagesTxnResult {
        files_parsed,
        symbols_extracted,
        edges_added,
        concept_nodes_emitted,
        identities_resolved,
        cross_file_pending,
        revision,
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
