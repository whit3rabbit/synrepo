use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use super::{
    ids::{derive_edge_id, derive_file_id, derive_symbol_id},
    provenance::make_provenance,
    stage4::CrossFilePending,
};
use crate::{
    config::Config,
    core::ids::{FileNodeId, NodeId},
    pipeline::git::GitRepositorySnapshot,
    structure::{
        graph::{
            concept_source_path_allowed, Edge, EdgeKind, Epistemic, FileNode, GraphStore,
            SymbolNode,
        },
        parse, prose, rationale,
    },
    substrate::{DiscoveredFile, FileClass},
};

pub(super) struct StagesTxnResult {
    pub(super) files_parsed: usize,
    pub(super) symbols_extracted: usize,
    pub(super) edges_added: usize,
    pub(super) concept_nodes_emitted: usize,
    pub(super) identities_resolved: usize,
    pub(super) cross_file_pending: Vec<CrossFilePending>,
    pub(super) revision: String,
}

pub(super) fn stages_1_to_3(
    repo_root: &Path,
    config: &Config,
    graph: &mut dyn GraphStore,
    discovered: &[DiscoveredFile],
    discovered_paths: &BTreeSet<String>,
) -> crate::Result<StagesTxnResult> {
    let existing_file_paths = graph.all_file_paths()?;
    let disappeared_by_hash =
        load_disappeared_by_hash(graph, &existing_file_paths, discovered_paths)?;
    let mut rename_matched_old_paths = HashSet::new();

    delete_missing_concepts(graph, config, discovered)?;

    let git = GitRepositorySnapshot::inspect(repo_root);
    let revision = git.source_revision().to_string();
    let mut state = StageState {
        files_parsed: 0,
        symbols_extracted: 0,
        edges_added: 0,
        concept_nodes_emitted: 0,
        identities_resolved: 0,
        cross_file_pending: Vec::new(),
        file_map: existing_file_paths.iter().cloned().collect(),
    };

    process_supported_code_files(
        graph,
        discovered,
        &revision,
        &disappeared_by_hash,
        &mut rename_matched_old_paths,
        &mut state,
    )?;
    process_markdown_concepts(graph, config, discovered, &revision, &mut state)?;
    delete_missing_files(
        graph,
        discovered_paths,
        &existing_file_paths,
        &rename_matched_old_paths,
    )?;

    Ok(StagesTxnResult {
        files_parsed: state.files_parsed,
        symbols_extracted: state.symbols_extracted,
        edges_added: state.edges_added,
        concept_nodes_emitted: state.concept_nodes_emitted,
        identities_resolved: state.identities_resolved,
        cross_file_pending: state.cross_file_pending,
        revision,
    })
}

struct StageState {
    files_parsed: usize,
    symbols_extracted: usize,
    edges_added: usize,
    concept_nodes_emitted: usize,
    identities_resolved: usize,
    cross_file_pending: Vec<CrossFilePending>,
    file_map: HashMap<String, FileNodeId>,
}

fn load_disappeared_by_hash(
    graph: &mut dyn GraphStore,
    existing_file_paths: &[(String, FileNodeId)],
    discovered_paths: &BTreeSet<String>,
) -> crate::Result<HashMap<String, FileNode>> {
    let mut disappeared_by_hash = HashMap::new();
    for (path, _) in existing_file_paths {
        if !discovered_paths.contains(path) {
            if let Some(node) = graph.file_by_path(path)? {
                disappeared_by_hash
                    .entry(node.content_hash.clone())
                    .or_insert(node);
            }
        }
    }
    Ok(disappeared_by_hash)
}

fn delete_missing_concepts(
    graph: &mut dyn GraphStore,
    config: &Config,
    discovered: &[DiscoveredFile],
) -> crate::Result<()> {
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
    Ok(())
}

fn process_supported_code_files(
    graph: &mut dyn GraphStore,
    discovered: &[DiscoveredFile],
    revision: &str,
    disappeared_by_hash: &HashMap<String, FileNode>,
    rename_matched_old_paths: &mut HashSet<String>,
    state: &mut StageState,
) -> crate::Result<()> {
    for file in discovered {
        if !matches!(file.class, FileClass::SupportedCode { .. }) {
            continue;
        }

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

        let file_id = resolve_file_id(
            existing.as_ref(),
            &content_hash,
            disappeared_by_hash,
            rename_matched_old_paths,
            &mut state.identities_resolved,
        );

        let language = match file.class {
            FileClass::SupportedCode { language } => Some(language.to_string()),
            _ => None,
        };
        let inline_decisions = rationale::extract_inline_decisions(&content, &file.class);

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
            inline_decisions,
            epistemic: Epistemic::ParserObserved,
            provenance: make_provenance("discover", revision, &file.relative_path, &content_hash),
        })?;
        state.file_map.insert(file.relative_path.clone(), file_id);

        if let Some(parsed) = parse::parse_file(file.absolute_path.as_path(), &content)? {
            if !parsed.symbols.is_empty() {
                state.files_parsed += 1;
                for symbol in &parsed.symbols {
                    let symbol_id = derive_symbol_id(
                        file_id,
                        &symbol.qualified_name,
                        symbol.kind,
                        &symbol.body_hash,
                    );
                    let provenance =
                        make_provenance("parse_code", revision, &file.relative_path, &content_hash);

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
                        first_seen_rev: None,
                        last_modified_rev: None,
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

                    state.symbols_extracted += 1;
                    state.edges_added += 1;
                }
            }
            if !parsed.call_refs.is_empty() || !parsed.import_refs.is_empty() {
                state.cross_file_pending.push(CrossFilePending {
                    file_id,
                    file_path: file.relative_path.clone(),
                    call_refs: parsed.call_refs,
                    import_refs: parsed.import_refs,
                });
            }
        }
    }
    Ok(())
}

fn resolve_file_id(
    existing: Option<&FileNode>,
    content_hash: &str,
    disappeared_by_hash: &HashMap<String, FileNode>,
    rename_matched_old_paths: &mut HashSet<String>,
    identities_resolved: &mut usize,
) -> FileNodeId {
    if let Some(file_node) = existing {
        return file_node.id;
    }
    if let Some(old_node) = disappeared_by_hash.get(content_hash) {
        rename_matched_old_paths.insert(old_node.path.clone());
        *identities_resolved += 1;
        return old_node.id;
    }
    derive_file_id(content_hash)
}

fn process_markdown_concepts(
    graph: &mut dyn GraphStore,
    config: &Config,
    discovered: &[DiscoveredFile],
    revision: &str,
    state: &mut StageState,
) -> crate::Result<()> {
    for file in discovered {
        if !matches!(file.class, FileClass::Markdown)
            || !concept_source_path_allowed(&file.relative_path, &config.concept_directories)
        {
            continue;
        }

        let content = match std::fs::read(&file.absolute_path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        if let Some((concept, governs_paths)) =
            prose::extract_concept(&file.relative_path, &content, revision)?
        {
            let concept_id = concept.id;
            let content_hash = concept.provenance.source_artifacts[0].content_hash.clone();
            graph.upsert_concept(concept)?;
            state.concept_nodes_emitted += 1;

            for governs_path in &governs_paths {
                if let Some(&file_id) = state.file_map.get(governs_path.as_str()) {
                    let from = NodeId::Concept(concept_id);
                    let to = NodeId::File(file_id);
                    graph.insert_edge(Edge {
                        id: derive_edge_id(from, to, EdgeKind::Governs),
                        from,
                        to,
                        kind: EdgeKind::Governs,
                        epistemic: Epistemic::HumanDeclared,
                        drift_score: 0.0,
                        provenance: make_provenance(
                            "parse_prose",
                            revision,
                            &file.relative_path,
                            &content_hash,
                        ),
                    })?;
                    state.edges_added += 1;
                } else {
                    tracing::debug!(
                        concept = %file.relative_path,
                        governs_path = %governs_path,
                        "governs path has no matching file node; edge not emitted"
                    );
                }
            }
        }
    }
    Ok(())
}

fn delete_missing_files(
    graph: &mut dyn GraphStore,
    discovered_paths: &BTreeSet<String>,
    existing_file_paths: &[(String, FileNodeId)],
    rename_matched_old_paths: &HashSet<String>,
) -> crate::Result<()> {
    for (path, file_id) in existing_file_paths {
        if !discovered_paths.contains(path) && !rename_matched_old_paths.contains(path) {
            graph.delete_node(NodeId::File(*file_id))?;
        }
    }
    Ok(())
}
