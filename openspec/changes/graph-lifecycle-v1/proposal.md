## Context

The structural pipeline treats any content-hash change as "destroy then rebuild": `stages.rs` calls `graph.delete_node(NodeId::File(...))`, cascading through all owned symbols and incident edges, then re-inserts the file and re-parses. This lifecycle conflicts with drift scoring (shipped in `structural-resilience-v2`), which assumes stable `FileNodeId` across revisions and persisted prior fingerprints. It also prevents repair, audit, and future card surfaces from reasoning about what changed versus what disappeared.

Three distinct states are currently conflated:
- **deleted**: file/symbol removed from the repo
- **retired**: previously observed, not re-emitted this revision
- **changed**: persists with advanced version fields (body_hash, signature, content_hash)

The destructive rebuild model forces "changed" into the "deleted then recreated" bucket, losing cross-revision continuity that the rest of the system increasingly depends on.

## Goals

- Stabilize `FileNodeId` across in-place content edits, not just renames.
- Introduce observation ownership so recompiling a file retires its stale observations without cascade-deleting the graph.
- Add soft-retirement semantics: parser observations not re-emitted at the current revision are marked retired, not physically deleted.
- Reserve physical deletion for genuinely missing files and deferred compaction.
- Preserve drift scoring correctness by feeding it observation windows rather than post-deletion graph survivors.
- Add a compaction maintenance pass for retired observations older than a configurable retention window.

## Non-Goals

- Overlay lifecycle changes (overlay has its own retirement semantics).
- ArcSwap commit (stage 8, separate change).
- `ConceptNodeId` derivation changes (path-derived, invariant 4 unchanged).
- Synthesis pipeline, card surface, or MCP tool changes beyond repair output.
- Symbol-level identity resolution beyond the existing `(file_id, qualified_name, kind, body_hash)` key.
