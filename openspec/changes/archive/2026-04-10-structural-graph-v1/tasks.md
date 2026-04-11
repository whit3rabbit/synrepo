## 1. Complete the canonical graph model and persistence layer

- [x] 1.1 Add the missing graph data-model pieces for concept nodes and graph-oriented ID parsing/helpers
- [x] 1.2 Implement the first sqlite-backed `GraphStore` for files, symbols, concepts, and edges under `.synrepo/graph/`
- [x] 1.3 Add round-trip tests for graph persistence, provenance retention, and concept-node admission boundaries

## 2. Expose direct graph inspection

- [x] 2.1 Implement persisted node lookup and graph statistics for the Phase 1 CLI surface
- [x] 2.2 Implement a small deterministic graph query path for edge-filtered inbound and outbound traversal
- [x] 2.3 Add CLI-level tests for node lookup, graph stats, and simple traversal queries

## 3. Tighten contracts and validation

- [x] 3.1 Align graph/store comments and public exports with the Phase 1 canonical-graph contract
- [x] 3.2 Confirm graph materialization behavior remains compatible with the current storage-compatibility rules
- [x] 3.3 Validate the change with `openspec validate structural-graph-v1 --strict --type change`
