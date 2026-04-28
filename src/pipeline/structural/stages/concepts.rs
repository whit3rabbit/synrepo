//! Stage 1-3 for markdown concept files: extract concepts and emit Governs
//! edges to governed files.

use std::collections::BTreeSet;

use super::super::{ids::derive_edge_id, provenance::make_provenance};
use super::StageState;
use crate::{
    config::Config,
    core::ids::NodeId,
    structure::{
        graph::{concept_source_path_allowed, Edge, EdgeKind, Epistemic, GraphStore},
        prose,
    },
    substrate::{DiscoveredFile, FileClass},
};

pub(super) fn delete_missing_concepts(
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

pub(super) fn process_markdown_concepts(
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
                if let Some(&file_id) = state
                    .file_map
                    .get(&(file.root_discriminant.clone(), governs_path.clone()))
                {
                    let from = NodeId::Concept(concept_id);
                    let to = NodeId::File(file_id);
                    graph.insert_edge(Edge {
                        id: derive_edge_id(from, to, EdgeKind::Governs),
                        from,
                        to,
                        kind: EdgeKind::Governs,
                        owner_file_id: None,
                        last_observed_rev: None,
                        retired_at_rev: None,
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
