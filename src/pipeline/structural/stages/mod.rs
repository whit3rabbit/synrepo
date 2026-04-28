use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use super::stage4::CrossFilePending;
use crate::{
    config::Config,
    core::ids::{FileNodeId, NodeId},
    pipeline::git::GitRepositorySnapshot,
    structure::graph::{FileNode, GraphStore},
    substrate::DiscoveredFile,
};

mod code;
mod concepts;
mod identity_cascade;

use code::process_supported_code_files;
use concepts::{delete_missing_concepts, process_markdown_concepts};
use identity_cascade::run_identity_cascade;

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
    discovered_paths: &BTreeSet<(String, String)>,
    active_root_ids: Option<&BTreeSet<String>>,
    compile_rev: Option<u64>,
) -> crate::Result<StagesTxnResult> {
    let existing_file_paths = graph.all_file_paths()?;
    let disappeared_by_hash = load_disappeared_by_hash(
        graph,
        &existing_file_paths,
        discovered_paths,
        active_root_ids,
    )?;
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
        file_map: load_existing_file_map(graph, &existing_file_paths)?,
    };

    process_supported_code_files(
        graph,
        discovered,
        &revision,
        &disappeared_by_hash,
        &mut rename_matched_old_paths,
        &mut state,
        compile_rev,
    )?;
    process_markdown_concepts(graph, config, discovered, &revision, &mut state)?;

    // Stage 6: identity cascade. Before deleting disappeared files, attempt
    // symbol-set matching and git rename detection for splits and merges.
    let identity_edges = run_identity_cascade(
        graph,
        discovered_paths,
        &existing_file_paths,
        active_root_ids,
        &mut rename_matched_old_paths,
        discovered,
        &revision,
        &mut state.identities_resolved,
        repo_root,
    )?;
    state.edges_added += identity_edges;

    delete_missing_files(
        graph,
        discovered_paths,
        &existing_file_paths,
        active_root_ids,
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

pub(super) struct StageState {
    pub(super) files_parsed: usize,
    pub(super) symbols_extracted: usize,
    pub(super) edges_added: usize,
    pub(super) concept_nodes_emitted: usize,
    pub(super) identities_resolved: usize,
    pub(super) cross_file_pending: Vec<CrossFilePending>,
    pub(super) file_map: HashMap<(String, String), FileNodeId>,
}

fn load_disappeared_by_hash(
    graph: &mut dyn GraphStore,
    existing_file_paths: &[(String, FileNodeId)],
    discovered_paths: &BTreeSet<(String, String)>,
    active_root_ids: Option<&BTreeSet<String>>,
) -> crate::Result<HashMap<(String, String), FileNode>> {
    let mut disappeared_by_hash = HashMap::new();
    for (path, file_id) in existing_file_paths {
        if let Some(node) = graph.get_file(*file_id)? {
            if active_root_ids.is_some_and(|roots| !roots.contains(&node.root_id)) {
                continue;
            }
            if !discovered_paths.contains(&(node.root_id.clone(), path.clone())) {
                disappeared_by_hash
                    .entry((node.root_id.clone(), node.content_hash.clone()))
                    .or_insert(node);
            }
        }
    }
    Ok(disappeared_by_hash)
}

fn load_existing_file_map(
    graph: &mut dyn GraphStore,
    existing_file_paths: &[(String, FileNodeId)],
) -> crate::Result<HashMap<(String, String), FileNodeId>> {
    let mut file_map = HashMap::new();
    for (_, file_id) in existing_file_paths {
        if let Some(node) = graph.get_file(*file_id)? {
            file_map.insert((node.root_id, node.path), *file_id);
        }
    }
    Ok(file_map)
}

fn delete_missing_files(
    graph: &mut dyn GraphStore,
    discovered_paths: &BTreeSet<(String, String)>,
    existing_file_paths: &[(String, FileNodeId)],
    active_root_ids: Option<&BTreeSet<String>>,
    rename_matched_old_paths: &HashSet<String>,
) -> crate::Result<()> {
    for (path, file_id) in existing_file_paths {
        if let Some(node) = graph.get_file(*file_id)? {
            if active_root_ids.is_some_and(|roots| !roots.contains(&node.root_id)) {
                continue;
            }
            let key = (node.root_id, path.clone());
            if !discovered_paths.contains(&key) && !rename_matched_old_paths.contains(path) {
                graph.delete_node(NodeId::File(*file_id))?;
            }
        }
    }
    Ok(())
}
