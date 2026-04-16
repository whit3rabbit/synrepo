## Why

Files get split and merged in real repos. When a developer breaks `api.rs` into `api_handlers.rs` and `api_models.rs`, synrepo currently orphans the old `FileNode` and creates two fresh nodes with no relationship to the original. All symbol edges pointing at the old file become dangling references. Similarly, when an ADR references a struct that has since been refactored, synrepo has no signal that the documentation is stale. Both problems undermine the trust value of the graph: nodes appear and disappear with no continuity, and prose rot goes undetected.

The scaffolds for both capabilities already exist (`src/structure/identity.rs`, `src/structure/drift.rs`, `EdgeKind::SplitFrom`/`MergedFrom`) but return empty results and zero scores. This change wires them end-to-end.

## What Changes

- **Stage 6 identity cascade**: implement AST-based split and merge detection so that `resolve_identities` in `src/structure/identity.rs` produces `Split` and `Merge` resolutions, emitting `SplitFrom` and `MergedFrom` edges with `Epistemic::ParserObserved` provenance. The existing content-hash rename detection is preserved; the cascade now runs the full 5-step resolution: rename, split, merge, git rename, breakage.
- **Stage 7 drift scoring**: implement `compute_drift_score` in `src/structure/drift.rs` to produce `[0.0, 1.0]` scores for graph edges by comparing the structural fingerprint of source artifacts at edge-creation time versus current state. Edges where one artifact has been deleted score 1.0 and become candidates for cleanup.
- **Repair-loop integration**: `synrepo check` reports high-drift edges as a new drift class. `synrepo sync` can prune edges at drift 1.0 (deleted artifact) and flag edges above a configurable threshold for review.
- **No breaking changes** to existing node IDs, edge kinds, or card payloads. `SplitFrom`/`MergedFrom` edges are new productions of existing edge kinds.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `graph`: adds concrete scenarios for split resolution, merge resolution, and drift scoring on edges. The existing "identity instability handling" requirement gains implementation-level acceptance criteria.
- `repair-loop`: adds a drift-edge repair surface so that `synrepo check` reports high-drift edges and `synrepo sync` can prune or flag them.

## Impact

- **Source**: `src/structure/identity.rs` (full implementation), `src/structure/drift.rs` (full implementation), `src/pipeline/structural/stages.rs` or `mod.rs` (wiring stages 6 and 7 into the compile cycle), `src/store/sqlite/` (drift score persistence on edges or a separate index), `src/pipeline/repair/` (drift-edge repair surface).
- **Schema**: `EdgeKind::SplitFrom` and `MergedFrom` are already defined. Drift scores require either a new column on the edges table or a lightweight sidecar table.
- **Dependencies**: no new crate dependencies. AST symbol matching reuses existing tree-sitter extraction results.
- **Cards**: drift score does not appear on cards in this change (future `ChangeRiskCard` will consume it). Split/merge edges are traversable via `synrepo graph query`.
