//! Bulk node description helpers for commentary context.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId},
    structure::graph::{ConceptNode, FileNode, GraphReader, SymbolNode},
};

#[derive(Default)]
pub(super) struct NodeDescriptions {
    files: BTreeMap<FileNodeId, FileNode>,
    symbols: BTreeMap<SymbolNodeId, SymbolNode>,
    concepts: BTreeMap<ConceptNodeId, ConceptNode>,
}

impl NodeDescriptions {
    pub(super) fn load<I>(graph: &dyn GraphReader, nodes: I) -> Self
    where
        I: IntoIterator<Item = NodeId>,
    {
        let mut file_ids = BTreeSet::new();
        let mut symbol_ids = BTreeSet::new();
        let mut concept_ids = BTreeSet::new();
        for node in nodes {
            match node {
                NodeId::File(id) => {
                    file_ids.insert(id);
                }
                NodeId::Symbol(id) => {
                    symbol_ids.insert(id);
                }
                NodeId::Concept(id) => {
                    concept_ids.insert(id);
                }
            }
        }

        let symbol_ids = symbol_ids.into_iter().collect::<Vec<_>>();
        let symbols = graph.get_symbols(&symbol_ids).unwrap_or_default();
        for symbol in &symbols {
            file_ids.insert(symbol.file_id);
        }

        let file_ids = file_ids.into_iter().collect::<Vec<_>>();
        let concept_ids = concept_ids.into_iter().collect::<Vec<_>>();
        Self {
            files: graph
                .get_files(&file_ids)
                .unwrap_or_default()
                .into_iter()
                .map(|file| (file.id, file))
                .collect(),
            symbols: symbols
                .into_iter()
                .map(|symbol| (symbol.id, symbol))
                .collect(),
            concepts: graph
                .get_concepts(&concept_ids)
                .unwrap_or_default()
                .into_iter()
                .map(|concept| (concept.id, concept))
                .collect(),
        }
    }

    pub(super) fn describe(&self, node: NodeId) -> String {
        match node {
            NodeId::File(file_id) => self
                .files
                .get(&file_id)
                .map(|file| format!("file {} ({})", file.path, file_id))
                .unwrap_or_else(|| format!("file {file_id}")),
            NodeId::Symbol(symbol_id) => self
                .symbols
                .get(&symbol_id)
                .map(|symbol| {
                    let path = self
                        .files
                        .get(&symbol.file_id)
                        .map(|file| file.path.as_str())
                        .unwrap_or("unknown");
                    format!(
                        "symbol {} kind={} visibility={} at {}:{} ({})",
                        symbol.qualified_name,
                        symbol.kind.as_str(),
                        symbol.visibility.as_str(),
                        path,
                        symbol.body_byte_range.0,
                        symbol_id
                    )
                })
                .unwrap_or_else(|| format!("symbol {symbol_id}")),
            NodeId::Concept(concept_id) => self
                .concepts
                .get(&concept_id)
                .map(|concept| format!("concept {} at {}", concept.title, concept.path))
                .unwrap_or_else(|| format!("concept {concept_id}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::{
        core::{
            ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId},
            provenance::{Provenance, SourceRef},
        },
        structure::graph::{
            ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphReader, SymbolKind, SymbolNode,
            Visibility,
        },
    };

    use super::NodeDescriptions;

    struct BulkOnlyReader {
        file_bulk_calls: AtomicUsize,
        symbol_bulk_calls: AtomicUsize,
    }

    impl BulkOnlyReader {
        fn new() -> Self {
            Self {
                file_bulk_calls: AtomicUsize::new(0),
                symbol_bulk_calls: AtomicUsize::new(0),
            }
        }
    }

    impl GraphReader for BulkOnlyReader {
        fn get_file(&self, _id: FileNodeId) -> crate::Result<Option<FileNode>> {
            panic!("description loading should use get_files")
        }

        fn get_files(&self, ids: &[FileNodeId]) -> crate::Result<Vec<FileNode>> {
            self.file_bulk_calls.fetch_add(1, Ordering::SeqCst);
            Ok(ids.iter().map(|id| file_node(*id)).collect())
        }

        fn get_symbol(&self, _id: SymbolNodeId) -> crate::Result<Option<SymbolNode>> {
            panic!("description loading should use get_symbols")
        }

        fn get_symbols(&self, ids: &[SymbolNodeId]) -> crate::Result<Vec<SymbolNode>> {
            self.symbol_bulk_calls.fetch_add(1, Ordering::SeqCst);
            Ok(ids.iter().map(|id| symbol_node(*id)).collect())
        }

        fn get_concept(&self, _id: ConceptNodeId) -> crate::Result<Option<ConceptNode>> {
            Ok(None)
        }

        fn get_concepts(&self, _ids: &[ConceptNodeId]) -> crate::Result<Vec<ConceptNode>> {
            Ok(Vec::new())
        }

        fn file_by_path(&self, _path: &str) -> crate::Result<Option<FileNode>> {
            Ok(None)
        }

        fn outbound(&self, _from: NodeId, _kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
            Ok(Vec::new())
        }

        fn inbound(&self, _to: NodeId, _kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
            Ok(Vec::new())
        }

        fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>> {
            Ok(Vec::new())
        }

        fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>> {
            Ok(Vec::new())
        }

        fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn descriptions_use_bulk_symbol_and_file_reads() {
        let reader = BulkOnlyReader::new();
        let descriptions = NodeDescriptions::load(&reader, [NodeId::Symbol(SymbolNodeId(2))]);

        let rendered = descriptions.describe(NodeId::Symbol(SymbolNodeId(2)));

        assert_eq!(reader.symbol_bulk_calls.load(Ordering::SeqCst), 1);
        assert_eq!(reader.file_bulk_calls.load(Ordering::SeqCst), 1);
        assert!(rendered.contains("symbol crate::thing"));
        assert!(rendered.contains("src/lib.rs:7"));
    }

    fn file_node(id: FileNodeId) -> FileNode {
        FileNode {
            id,
            root_id: "primary".to_string(),
            path: "src/lib.rs".to_string(),
            path_history: Vec::new(),
            content_hash: "hash".to_string(),
            content_sample_hashes: Vec::new(),
            size_bytes: 0,
            language: Some("rust".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: provenance(id),
        }
    }

    fn symbol_node(id: SymbolNodeId) -> SymbolNode {
        SymbolNode {
            id,
            file_id: FileNodeId(1),
            qualified_name: "crate::thing".to_string(),
            display_name: "thing".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            body_byte_range: (7, 11),
            body_hash: "body".to_string(),
            signature: None,
            doc_comment: None,
            first_seen_rev: None,
            last_modified_rev: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: provenance(FileNodeId(1)),
        }
    }

    fn provenance(file_id: FileNodeId) -> Provenance {
        Provenance::structural(
            "test",
            "rev",
            vec![SourceRef {
                file_id: Some(file_id),
                path: "src/lib.rs".to_string(),
                content_hash: "hash".to_string(),
            }],
        )
    }
}
