## Why

The repo already defines the shape of the canonical graph in types and docs, but Phase 1 behavior is still mostly aspirational: there is no graph store implementation, concept nodes are not modeled in the structure layer, and the CLI cannot yet query persisted graph facts. This change turns the graph from architectural scaffolding into the first real observed-facts runtime layer, while keeping Git intelligence as a separate later implementation change.

## What Changes

- Implement the first sqlite-backed canonical graph store for file, symbol, concept, and edge facts with persisted provenance and epistemic status.
- Complete the Phase 1 graph data model so concept nodes and observed-only graph boundaries are enforced in code.
- Define the first structural-graph query behavior for direct node lookup and simple graph traversal, without introducing cards or overlay behavior.
- Add focused tests for graph identity, provenance persistence, and graph-store query behavior.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `graph`: sharpen the Phase 1 canonical graph contract into implementable behavior for persisted nodes, edges, provenance, and direct graph queries

## Impact

- Affects `src/structure/graph/`, `src/store/`, and related core identity/provenance types
- Affects the Phase 1 CLI paths for `synrepo graph query` and `synrepo node`
- Adds or updates tests around graph persistence, concept-node handling, and observed-only invariants
- Explicitly does not implement Git-history mining or Git-derived ranking behavior from `git-intelligence-v1`
