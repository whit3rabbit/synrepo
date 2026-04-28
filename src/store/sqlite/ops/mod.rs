//! GraphStore operations for SqliteGraphStore.

mod drift;
mod edges;
mod helpers;
mod lists;
mod nodes;
mod transactions;

use crate::core::ids::NodeId;
use crate::structure::graph::{EdgeKind, GraphReader, GraphStore};

use super::SqliteGraphStore;

impl GraphReader for SqliteGraphStore {
    fn get_file(
        &self,
        id: crate::core::ids::FileNodeId,
    ) -> crate::Result<Option<crate::structure::graph::FileNode>> {
        nodes::get_file(self, id)
    }

    fn get_symbol(
        &self,
        id: crate::core::ids::SymbolNodeId,
    ) -> crate::Result<Option<crate::structure::graph::SymbolNode>> {
        nodes::get_symbol(self, id)
    }

    fn get_concept(
        &self,
        id: crate::core::ids::ConceptNodeId,
    ) -> crate::Result<Option<crate::structure::graph::ConceptNode>> {
        nodes::get_concept(self, id)
    }

    fn file_by_path(&self, path: &str) -> crate::Result<Option<crate::structure::graph::FileNode>> {
        nodes::file_by_path(self, path)
    }

    fn file_by_root_path(
        &self,
        root_id: &str,
        path: &str,
    ) -> crate::Result<Option<crate::structure::graph::FileNode>> {
        nodes::file_by_root_path(self, root_id, path)
    }

    fn outbound(
        &self,
        from: NodeId,
        kind: Option<EdgeKind>,
    ) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        edges::outbound(self, from, kind)
    }

    fn inbound(
        &self,
        to: NodeId,
        kind: Option<EdgeKind>,
    ) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        edges::inbound(self, to, kind)
    }

    fn all_edges(&self) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        edges::all_edges(self)
    }

    fn all_file_paths(&self) -> crate::Result<Vec<(String, crate::core::ids::FileNodeId)>> {
        lists::all_file_paths(self)
    }

    fn all_concept_paths(&self) -> crate::Result<Vec<(String, crate::core::ids::ConceptNodeId)>> {
        lists::all_concept_paths(self)
    }

    fn all_symbol_names(
        &self,
    ) -> crate::Result<
        Vec<(
            crate::core::ids::SymbolNodeId,
            crate::core::ids::FileNodeId,
            String,
        )>,
    > {
        lists::all_symbol_names(self)
    }

    fn all_symbols_summary(
        &self,
    ) -> crate::Result<
        Vec<(
            crate::core::ids::SymbolNodeId,
            crate::core::ids::FileNodeId,
            String,
            String,
            String,
        )>,
    > {
        lists::all_symbols_summary(self)
    }

    fn all_symbols_for_resolution(
        &self,
    ) -> crate::Result<
        Vec<(
            crate::core::ids::SymbolNodeId,
            crate::core::ids::FileNodeId,
            String,
            crate::structure::graph::SymbolKind,
            crate::structure::graph::Visibility,
        )>,
    > {
        lists::all_symbols_for_resolution(self)
    }

    fn symbols_for_file(
        &self,
        file_id: crate::core::ids::FileNodeId,
    ) -> crate::Result<Vec<crate::structure::graph::SymbolNode>> {
        self.symbols_for_file_impl(file_id)
    }

    fn edges_owned_by(
        &self,
        file_id: crate::core::ids::FileNodeId,
    ) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        self.edges_owned_by_impl(file_id)
    }

    fn active_edges(&self) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        self.active_edges_impl()
    }
}

impl GraphStore for SqliteGraphStore {
    fn upsert_file(&mut self, node: crate::structure::graph::FileNode) -> crate::Result<()> {
        nodes::upsert_file(self, node)
    }

    fn upsert_symbol(&mut self, node: crate::structure::graph::SymbolNode) -> crate::Result<()> {
        nodes::upsert_symbol(self, node)
    }

    fn upsert_concept(&mut self, node: crate::structure::graph::ConceptNode) -> crate::Result<()> {
        nodes::upsert_concept(self, node)
    }

    fn insert_edge(&mut self, edge: crate::structure::graph::Edge) -> crate::Result<()> {
        edges::insert_edge(self, edge)
    }

    fn delete_edge(&mut self, edge_id: crate::core::ids::EdgeId) -> crate::Result<()> {
        edges::delete_edge(self, edge_id)
    }

    fn delete_edges_by_kind(&mut self, kind: EdgeKind) -> crate::Result<usize> {
        edges::delete_edges_by_kind(self, kind)
    }

    fn delete_node(&mut self, id: NodeId) -> crate::Result<()> {
        nodes::delete_node(self, id)
    }

    fn begin(&mut self) -> crate::Result<()> {
        transactions::begin(self)
    }

    fn commit(&mut self) -> crate::Result<()> {
        transactions::commit(self)
    }

    fn rollback(&mut self) -> crate::Result<()> {
        transactions::rollback(self)
    }

    fn begin_read_snapshot(&self) -> crate::Result<()> {
        transactions::begin_read_snapshot(self)
    }

    fn end_read_snapshot(&self) -> crate::Result<()> {
        transactions::end_read_snapshot(self)
    }

    fn latest_drift_revision(&self) -> crate::Result<Option<String>> {
        drift::latest_drift_revision(self)
    }

    fn write_drift_scores(
        &mut self,
        scores: &[(crate::core::ids::EdgeId, f32)],
        revision: &str,
    ) -> crate::Result<()> {
        drift::write_drift_scores(self, scores, revision)
    }

    fn read_drift_scores(
        &self,
        revision: &str,
    ) -> crate::Result<Vec<(crate::core::ids::EdgeId, f32)>> {
        drift::read_drift_scores(self, revision)
    }

    fn truncate_drift_scores(&self, older_than_revision: &str) -> crate::Result<usize> {
        drift::truncate_drift_scores(self, older_than_revision)
    }

    fn has_any_drift_scores(&self) -> crate::Result<bool> {
        drift::has_any_drift_scores(self)
    }

    fn latest_fingerprint_revision(&self) -> crate::Result<Option<String>> {
        drift::latest_fingerprint_revision(self)
    }

    fn write_fingerprints(
        &mut self,
        fingerprints: &[(
            crate::core::ids::FileNodeId,
            crate::structure::drift::StructuralFingerprint,
        )],
        revision: &str,
    ) -> crate::Result<()> {
        drift::write_fingerprints(self, fingerprints, revision)
    }

    fn read_fingerprints(
        &self,
        revision: &str,
    ) -> crate::Result<
        std::collections::HashMap<
            crate::core::ids::FileNodeId,
            crate::structure::drift::StructuralFingerprint,
        >,
    > {
        drift::read_fingerprints(self, revision)
    }

    fn truncate_fingerprints(&self, older_than_revision: &str) -> crate::Result<usize> {
        drift::truncate_fingerprints(self, older_than_revision)
    }

    fn next_compile_revision(&mut self) -> crate::Result<u64> {
        self.next_compile_revision_impl()
    }

    fn retire_symbol(
        &mut self,
        id: crate::core::ids::SymbolNodeId,
        revision: u64,
    ) -> crate::Result<()> {
        self.retire_symbol_impl(id, revision)
    }

    fn retire_edge(&mut self, id: crate::core::ids::EdgeId, revision: u64) -> crate::Result<()> {
        self.retire_edge_impl(id, revision)
    }

    fn unretire_symbol(
        &mut self,
        id: crate::core::ids::SymbolNodeId,
        revision: u64,
    ) -> crate::Result<()> {
        self.unretire_symbol_impl(id, revision)
    }

    fn unretire_edge(&mut self, id: crate::core::ids::EdgeId, revision: u64) -> crate::Result<()> {
        self.unretire_edge_impl(id, revision)
    }

    fn compact_retired(
        &mut self,
        older_than_rev: u64,
    ) -> crate::Result<crate::structure::graph::CompactionSummary> {
        self.compact_retired_impl(older_than_rev)
    }
}
