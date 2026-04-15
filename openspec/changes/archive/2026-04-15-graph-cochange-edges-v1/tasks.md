## 1. Graph store: add delete_edges_by_kind

- [x] 1.1 Add `fn delete_edges_by_kind(&mut self, kind: EdgeKind) -> crate::Result<usize>` to `GraphStore` trait in `src/structure/graph/store.rs` (default no-op returning 0)
- [x] 1.2 Implement `delete_edges_by_kind` in `src/store/sqlite/ops.rs` with `DELETE FROM edges WHERE kind = ?1`
- [x] 1.3 Add unit test: insert CoChangesWith edges, call delete_edges_by_kind, verify all removed and other edges untouched

## 2. Git-intelligence: co-change edge emission

- [x] 2.1 Add `fn emit_cochange_edges(graph: &mut dyn GraphStore, insights: &GitHistoryInsights, file_index: &HashMap<String, FileNodeId>, revision: &str) -> crate::Result<usize>` in `src/pipeline/git_intelligence/` (new file or extend analysis.rs)
- [x] 2.2 Function iterates `insights.co_changes`, filters pairs below threshold (count < 2), resolves both paths to `FileNodeId` via `file_index`, derives edge ID with `derive_edge_id(NodeId::File(left), NodeId::File(right), EdgeKind::CoChangesWith)`, creates `Edge` with `Epistemic::GitObserved` and provenance `stage5_cochange`
- [x] 2.3 Add unit test for `emit_cochange_edges` using a mock graph store: verify correct edges emitted, threshold filtering, missing-path skipping

## 3. Pipeline: wire co-change emission into reconcile

- [x] 3.1 In the reconcile pass (or structural compile orchestrator), after git-intelligence analysis completes, call `delete_edges_by_kind(EdgeKind::CoChangesWith)` then `emit_cochange_edges` with the current `GitHistoryInsights` and file path index
- [x] 3.2 Wrap the delete+emit in a graph transaction (`begin`/`commit` with rollback on error)
- [x] 3.3 Integration test: run full reconcile on a temp repo with multi-file commits, verify CoChangesWith edges appear in the graph store

## 4. Card surface: populate FileCard.co_changes from graph edges

- [x] 4.1 In `src/surface/card/compiler/file.rs`, replace `co_changes: vec![]` with graph-backed population: query `outbound(file_id, Some(EdgeKind::CoChangesWith))` + `inbound(file_id, Some(EdgeKind::CoChangesWith))`, collect partner `FileNodeId`s
- [x] 4.2 Filter out partners that already have an `Imports` edge to the focal file (hidden-coupling-only semantic per the field comment)
- [x] 4.3 Resolve each partner `FileNodeId` to a `FileRef` (path + kind) via `get_file`
- [x] 4.4 Remove the "co_changes is empty until..." comment in `src/surface/card/compiler/mod.rs`
- [x] 4.5 Update snapshot tests: co_changes will now be populated in snapshots that have git context

## 5. Verify and clean up

- [x] 5.1 Run `make check` (fmt, clippy, tests) and verify all pass
- [x] 5.2 Run `cargo run -- init` on the synrepo repo itself and verify CoChangesWith edges appear in the graph via `cargo run -- graph stats`
- [x] 5.3 Run `cargo run -- export --deep` and verify FileCard.co_changes is populated in the JSON output
