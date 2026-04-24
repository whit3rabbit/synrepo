//! Shared fixture helpers for export tests.

use std::fs;

use time::OffsetDateTime;

use crate::core::ids::FileNodeId;
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::store::sqlite::SqliteGraphStore;
use crate::structure::graph::{Epistemic, FileNode, GraphStore};

pub(super) fn init_empty_graph(synrepo_dir: &std::path::Path) -> crate::Result<()> {
    let graph_dir = synrepo_dir.join("graph");
    fs::create_dir_all(&graph_dir)?;
    // Open the store to trigger schema creation.
    let _ = crate::store::sqlite::SqliteGraphStore::open(&graph_dir)?;
    Ok(())
}

pub(super) fn seed_files(synrepo_dir: &std::path::Path, count: usize) {
    let graph_dir = synrepo_dir.join("graph");
    fs::create_dir_all(&graph_dir).unwrap();
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    graph.begin().unwrap();
    for i in 0..count {
        let path = format!("src/gen_{i:04}.rs");
        let hash = format!("hash-{i}");
        graph
            .upsert_file(FileNode {
                id: FileNodeId((i as u128) + 1),
                path: path.clone(),
                path_history: Vec::new(),
                content_hash: hash.clone(),
                size_bytes: 128,
                language: Some("rust".to_string()),
                inline_decisions: Vec::new(),
                last_observed_rev: None,
                epistemic: Epistemic::ParserObserved,
                provenance: Provenance {
                    created_at: OffsetDateTime::UNIX_EPOCH,
                    source_revision: "rev".to_string(),
                    created_by: CreatedBy::StructuralPipeline,
                    pass: "parse".to_string(),
                    source_artifacts: vec![SourceRef {
                        file_id: None,
                        path,
                        content_hash: hash,
                    }],
                },
            })
            .unwrap();
    }
    graph.commit().unwrap();
}
