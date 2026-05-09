use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::ids::NodeId;
use crate::core::provenance::Provenance;
use crate::structure::graph::{ConceptNode, EdgeKind, Epistemic, FileNode, SymbolNode};

#[test]
fn graph_payload_serializes_nodes_in_bounded_batches() {
    let graph = TrackingGraphReader::new(GRAPH_EXPORT_BATCH_SIZE * 2 + 3);
    let context = GraphExportContext::load(&graph, Budget::Normal).unwrap();
    let mut out = Vec::new();

    context.write_compact_json(&mut out).unwrap();

    assert_eq!(
        graph.max_file_batch.load(Ordering::Relaxed),
        GRAPH_EXPORT_BATCH_SIZE
    );
    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        parsed["nodes"].as_array().unwrap().len(),
        GRAPH_EXPORT_BATCH_SIZE * 2 + 3
    );
}

struct TrackingGraphReader {
    file_count: usize,
    max_file_batch: AtomicUsize,
}

impl TrackingGraphReader {
    fn new(file_count: usize) -> Self {
        Self {
            file_count,
            max_file_batch: AtomicUsize::new(0),
        }
    }

    fn file_node(id: FileNodeId) -> FileNode {
        FileNode {
            id,
            root_id: "primary".to_string(),
            path: format!("src/{:04}.rs", id.0),
            path_history: Vec::new(),
            content_hash: "hash".to_string(),
            content_sample_hashes: Vec::new(),
            size_bytes: 1,
            language: Some("rust".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: Some(1),
            epistemic: Epistemic::ParserObserved,
            provenance: Provenance::structural("test", "rev", Vec::new()),
        }
    }

    fn record_file_batch(&self, len: usize) {
        let mut current = self.max_file_batch.load(Ordering::Relaxed);
        while len > current {
            match self.max_file_batch.compare_exchange(
                current,
                len,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(next) => current = next,
            }
        }
    }
}

impl GraphReader for TrackingGraphReader {
    fn get_file(&self, id: FileNodeId) -> crate::Result<Option<FileNode>> {
        Ok(Some(Self::file_node(id)))
    }

    fn get_files(&self, ids: &[FileNodeId]) -> crate::Result<Vec<FileNode>> {
        self.record_file_batch(ids.len());
        Ok(ids.iter().copied().map(Self::file_node).collect())
    }

    fn get_symbol(&self, _id: SymbolNodeId) -> crate::Result<Option<SymbolNode>> {
        Ok(None)
    }

    fn get_concept(&self, _id: ConceptNodeId) -> crate::Result<Option<ConceptNode>> {
        Ok(None)
    }

    fn file_by_path(&self, _path: &str) -> crate::Result<Option<FileNode>> {
        Ok(None)
    }

    fn outbound(
        &self,
        _from: NodeId,
        _kind: Option<EdgeKind>,
    ) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        Ok(Vec::new())
    }

    fn inbound(
        &self,
        _to: NodeId,
        _kind: Option<EdgeKind>,
    ) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        Ok(Vec::new())
    }

    fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>> {
        Ok((0..self.file_count)
            .map(|index| {
                let id = FileNodeId(index as u128 + 1);
                (format!("src/{:04}.rs", id.0), id)
            })
            .collect())
    }

    fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>> {
        Ok(Vec::new())
    }

    fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
        Ok(Vec::new())
    }

    fn all_edges(&self) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        Ok(Vec::new())
    }
}
