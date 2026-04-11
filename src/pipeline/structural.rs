//! The structural compile pipeline.
//!
//! Runs synchronously, LLM-free, on every `synrepo init` and on-demand
//! refresh. This initial producer set covers stages 1–3 of the full
//! eight-stage pipeline:
//!
//! 1. **Discover** — walk the repo via `substrate::discover`, reusing its
//!    `.gitignore` / `.synignore` / redaction rules.
//! 2. **Parse code** — tree-sitter for each `SupportedCode` file; extract
//!    symbols and within-file `defines` edges.
//! 3. **Parse prose** — markdown link parser for files in configured concept
//!    directories; extract concept nodes.
//!
//! Stages 4–8 (cross-file edge resolution, git mining, identity cascade,
//! drift scoring, ArcSwap commit) are NOT part of this change and remain
//! TODO stubs. The watcher / reconcile loop is also out of scope here.
//!
//! ## Replacement contract
//!
//! Each compile run replaces stale facts for the producer-owned slice:
//! - File nodes with changed content are deleted (cascading to their symbols
//!   and edges) and re-inserted, keeping the original stable ID.
//! - File nodes whose paths have disappeared from the discovered set are
//!   deleted.
//! - Concept nodes whose paths have disappeared are deleted.
//! - The run is idempotent: unchanged files are skipped entirely.

use std::{
    collections::BTreeSet,
    path::Path,
    time::Instant,
};

use crate::{
    config::Config,
    core::{
        ids::{EdgeId, FileNodeId, NodeId, SymbolNodeId},
        provenance::{Provenance, SourceRef},
    },
    structure::{
        graph::{concept_source_path_allowed, Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolNode, SymbolKind},
        parse,
        prose,
    },
    substrate::{self, FileClass},
};

/// Run one structural compile cycle.
///
/// Re-entrant and idempotent: calling twice with the same repository state
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

    // Stale detection: remove file nodes whose paths are gone from the corpus.
    let existing_file_paths = graph.all_file_paths()?;
    for (path, file_id) in &existing_file_paths {
        if !discovered_paths.contains(path) {
            graph.delete_node(NodeId::File(*file_id))?;
        }
    }

    // Stale detection: remove concept nodes whose paths are gone.
    let discovered_concept_paths: BTreeSet<String> = discovered
        .iter()
        .filter(|f| {
            matches!(f.class, FileClass::Markdown)
                && concept_source_path_allowed(&f.relative_path, &config.concept_directories)
        })
        .map(|f| f.relative_path.clone())
        .collect();

    let existing_concept_paths = graph.all_concept_paths()?;
    for (path, concept_id) in &existing_concept_paths {
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
        // SupportedCode files get FileNodes, SymbolNodes, and Defines edges.
        // TextCode, Jupyter, and other classes are indexed by the substrate but
        // receive no graph artifacts at this pipeline stage.
        if matches!(file.class, FileClass::SupportedCode { .. }) {
            let content = match std::fs::read(&file.absolute_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let content_hash = hex::encode(blake3::hash(&content).as_bytes());

            // Per-file idempotency: skip files whose content is unchanged.
            let existing = graph.file_by_path(&file.relative_path)?;
            if let Some(ref e) = existing {
                if e.content_hash == content_hash {
                    // Content unchanged; file node and symbols are already current.
                    continue;
                }
                // Content changed: delete the old node (cascades to symbols
                // and edges) before re-inserting with the same stable ID.
                graph.delete_node(NodeId::File(e.id))?;
            }
            let file_id = match existing {
                Some(e) => e.id, // Keep the original stable ID.
                None => derive_file_id(&content_hash), // First-seen: derive from content.
            };

            let language_str = match file.class {
                FileClass::SupportedCode { language } => Some(language.to_string()),
                _ => None,
            };

            let file_prov = make_provenance(
                "discover",
                &revision,
                &file.relative_path,
                &content_hash,
            );
            graph.upsert_file(FileNode {
                id: file_id,
                path: file.relative_path.clone(),
                path_history: vec![],
                content_hash: content_hash.clone(),
                size_bytes: file.size_bytes,
                language: language_str,
                epistemic: Epistemic::ParserObserved,
                provenance: file_prov,
            })?;

            match parse::parse_file(file.absolute_path.as_path(), &content)? {
                Some(parsed) if !parsed.symbols.is_empty() => {
                    files_parsed += 1;
                    for sym in &parsed.symbols {
                        let sym_id = derive_symbol_id(
                            file_id,
                            &sym.qualified_name,
                            sym.kind,
                            &sym.body_hash,
                        );
                        let sym_prov = make_provenance(
                            "parse_code",
                            &revision,
                            &file.relative_path,
                            &content_hash,
                        );
                        graph.upsert_symbol(SymbolNode {
                            id: sym_id,
                            file_id,
                            qualified_name: sym.qualified_name.clone(),
                            display_name: sym.display_name.clone(),
                            kind: sym.kind,
                            body_byte_range: sym.body_byte_range,
                            body_hash: sym.body_hash.clone(),
                            signature: sym.signature.clone(),
                            doc_comment: sym.doc_comment.clone(),
                            epistemic: Epistemic::ParserObserved,
                            provenance: sym_prov,
                        })?;

                        let edge_id = derive_edge_id(
                            NodeId::File(file_id),
                            NodeId::Symbol(sym_id),
                            EdgeKind::Defines,
                        );
                        let edge_prov = make_provenance(
                            "parse_code",
                            &revision,
                            &file.relative_path,
                            &content_hash,
                        );
                        graph.insert_edge(Edge {
                            id: edge_id,
                            from: NodeId::File(file_id),
                            to: NodeId::Symbol(sym_id),
                            kind: EdgeKind::Defines,
                            epistemic: Epistemic::ParserObserved,
                            drift_score: 0.0,
                            provenance: edge_prov,
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
                Ok(c) => c,
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

    graph.commit()?;

    Ok(CompileSummary {
        files_discovered,
        files_parsed,
        symbols_extracted,
        edges_added,
        concept_nodes_emitted,
        identities_resolved: 0, // TODO(phase-1): implement identity cascade
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// Derive a stable `FileNodeId` from the content hash of the first-seen version.
///
/// Uses the first 8 bytes of a secondary blake3 hash of the hex hash string.
/// This indirection preserves the "first-seen hash" invariant: for new files
/// the ID is derived from the current content; for existing files the caller
/// uses the stored ID from the graph.
fn derive_file_id(content_hash: &str) -> FileNodeId {
    FileNodeId(hash_to_u64(blake3::hash(content_hash.as_bytes())))
}

/// Derive a stable `SymbolNodeId` from `(file_id, qualified_name, kind, body_hash)`.
fn derive_symbol_id(
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
fn derive_edge_id(from: NodeId, to: NodeId, kind: EdgeKind) -> EdgeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(from.to_string().as_bytes());
    hasher.update(to.to_string().as_bytes());
    hasher.update(kind.as_str().as_bytes());
    EdgeId(hash_to_u64(hasher.finalize()))
}

/// Take the first 8 bytes of a blake3 hash as a little-endian u64.
fn hash_to_u64(hash: blake3::Hash) -> u64 {
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 output is 32 bytes"))
}

/// Build a `Provenance` record for a structural-pipeline row.
fn make_provenance(pass: &str, revision: &str, path: &str, content_hash: &str) -> Provenance {
    Provenance::structural(
        pass,
        revision,
        vec![SourceRef {
            file_id: None, // not yet committed when provenance is constructed
            path: path.to_string(),
            content_hash: content_hash.to_string(),
        }],
    )
}

/// Read the current git HEAD SHA for provenance records.
///
/// Returns "unknown" if the repository has no git history or the HEAD
/// file cannot be resolved (e.g. freshly initialised temp repos in tests).
fn current_git_revision(repo_root: &Path) -> String {
    let head_path = repo_root.join(".git/HEAD");
    let Ok(head) = std::fs::read_to_string(&head_path) else {
        return "unknown".to_string();
    };
    let head = head.trim();

    if let Some(ref_path) = head.strip_prefix("ref: ") {
        let ref_file = repo_root.join(".git").join(ref_path);
        if let Ok(sha) = std::fs::read_to_string(&ref_file) {
            let sha = sha.trim().to_string();
            if !sha.is_empty() {
                return sha;
            }
        }
        return "unknown".to_string();
    }

    // Detached HEAD — already a SHA.
    if head.len() >= 7 {
        return head.to_string();
    }

    "unknown".to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, store::sqlite::SqliteGraphStore, structure::graph::GraphStore};
    use std::fs;
    use tempfile::tempdir;

    fn open_graph(repo: &tempfile::TempDir) -> SqliteGraphStore {
        let graph_dir = repo.path().join(".synrepo/graph");
        SqliteGraphStore::open(&graph_dir).unwrap()
    }

    #[test]
    fn structural_compile_populates_file_nodes_and_symbols() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn hello() -> &'static str { \"hi\" }\n",
        )
        .unwrap();

        let config = Config::default();
        let mut graph = open_graph(&repo);

        let summary =
            run_structural_compile(repo.path(), &config, &mut graph).unwrap();

        assert_eq!(summary.files_discovered, 1);
        assert_eq!(summary.files_parsed, 1);
        assert!(summary.symbols_extracted >= 1);
        assert_eq!(summary.edges_added, summary.symbols_extracted);

        let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
        assert_eq!(file.language.as_deref(), Some("rust"));
    }

    #[test]
    fn structural_compile_is_idempotent() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn stable() {}\n").unwrap();

        let config = Config::default();
        let mut graph = open_graph(&repo);
        let s1 = run_structural_compile(repo.path(), &config, &mut graph).unwrap();
        let s2 = run_structural_compile(repo.path(), &config, &mut graph).unwrap();

        // First run produced symbols; second run skips unchanged files entirely.
        assert!(s1.symbols_extracted >= 1, "first run must extract at least one symbol");
        assert_eq!(s2.files_parsed, 0, "second run should skip unchanged files");
        assert_eq!(s2.symbols_extracted, 0, "second run should emit no new symbols");
    }

    #[test]
    fn structural_compile_replaces_stale_symbols_on_content_change() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn old_fn() {}\n",
        )
        .unwrap();

        let config = Config::default();
        let mut graph = open_graph(&repo);
        run_structural_compile(repo.path(), &config, &mut graph).unwrap();

        // Rewrite the file: old symbol gone, new symbol present.
        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn new_fn() {}\n",
        )
        .unwrap();
        run_structural_compile(repo.path(), &config, &mut graph).unwrap();

        let (paths, _ids): (Vec<_>, Vec<_>) = graph.all_file_paths().unwrap().into_iter().unzip();
        assert!(paths.contains(&"src/lib.rs".to_string()));

        // Verify old symbol gone, new symbol present via outbound edges.
        let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
        let edges = graph
            .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
            .unwrap();
        assert_eq!(edges.len(), 1);

        let sym = graph.get_symbol(match edges[0].to {
            NodeId::Symbol(id) => id,
            _ => panic!("expected symbol node"),
        }).unwrap().unwrap();
        assert_eq!(sym.display_name, "new_fn");
    }

    #[test]
    fn structural_compile_removes_deleted_files_from_graph() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/to_delete.rs"), "pub fn x() {}\n").unwrap();
        fs::write(repo.path().join("src/keep.rs"), "pub fn y() {}\n").unwrap();

        let config = Config::default();
        let mut graph = open_graph(&repo);
        run_structural_compile(repo.path(), &config, &mut graph).unwrap();

        // Remove one file and recompile.
        fs::remove_file(repo.path().join("src/to_delete.rs")).unwrap();
        run_structural_compile(repo.path(), &config, &mut graph).unwrap();

        let paths: Vec<_> = graph
            .all_file_paths()
            .unwrap()
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert!(!paths.contains(&"src/to_delete.rs".to_string()));
        assert!(paths.contains(&"src/keep.rs".to_string()));
    }

    #[test]
    fn structural_compile_emits_concept_nodes_from_configured_dirs() {
        let repo = tempdir().unwrap();
        let adr_dir = repo.path().join("docs/adr");
        fs::create_dir_all(&adr_dir).unwrap();
        fs::write(adr_dir.join("0001-arch.md"), "# Architecture\n\nWhy we built it this way.\n")
            .unwrap();

        let config = Config {
            concept_directories: vec!["docs/adr".to_string()],
            ..Config::default()
        };
        let mut graph = open_graph(&repo);
        let summary = run_structural_compile(repo.path(), &config, &mut graph).unwrap();

        assert_eq!(summary.concept_nodes_emitted, 1);

        let concept_paths: Vec<_> = graph
            .all_concept_paths()
            .unwrap()
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert!(concept_paths.contains(&"docs/adr/0001-arch.md".to_string()));
    }
}
