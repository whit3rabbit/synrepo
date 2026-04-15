## Context

The pipeline already computes file-pair co-change counts in `GitHistoryInsights.co_changes` (`src/pipeline/git_intelligence/analysis.rs`). The `EdgeKind::CoChangesWith` variant is defined and serialized. `FileCard.co_changes` is a `Vec<FileRef>` that is always empty. The gap is a single missing step: converting computed co-change data into persisted graph edges.

The structural compile (stages 1-4) runs as one transaction emitting parser-observed facts. Git mining runs separately, feeding the card compiler's `GitCache`. Co-change edge emission belongs in the git mining pass, not the structural compile, to maintain the epistemic boundary (parser-observed vs git-observed facts are separate concerns).

Current data flow:
```
git context → GitHistoryIndex → GitHistoryInsights.co_changes → card cache → FileCard (but co_changes field ignored)
```

Target data flow:
```
git context → GitHistoryIndex → GitHistoryInsights.co_changes → graph CoChangesWith edges → FileCard.co_changes
                                                           ↘ card cache → FileCard.git_intelligence (unchanged)
```

## Goals / Non-Goals

**Goals:**
- Persist co-change relationships as `CoChangesWith` edges with `Epistemic::GitObserved` authority.
- Populate `FileCard.co_changes` from graph edges, filtering to pairs without existing `Imports` edges (hidden coupling signal).
- Re-emit edges on each reconcile pass with deterministic IDs for idempotent upsert.
- Apply a minimum co-change count threshold to suppress noise.

**Non-Goals:**
- Symbol-level co-change (still file-scoped, matching current git-intelligence granularity).
- `CoChangesWith` edges between non-file nodes (only `FileNode` to `FileNode`).
- Changes to `FileCard.git_intelligence` (that field already surfaces co-change data from the cache; `co_changes` is the graph-backed view).
- Configurable threshold via `Config` (hardcoded default; can be made configurable later if needed).

## Decisions

### 1. Edge emission happens in the reconcile pass, not structural compile

The structural compile (stages 1-4) owns parser-observed facts. Git-observed facts belong in a separate transaction. This keeps the epistemic boundary clean and allows git mining to fail without rolling back structural work.

**Alternative considered:** Add a stage 5 to `run_structural_compile`. Rejected because it would require git context inside the structural compile transaction, breaking the current separation where structural compile is git-free.

### 2. Idempotent edge IDs via derive_edge_id

`derive_edge_id(NodeId::File(left), NodeId::File(right), EdgeKind::CoChangesWith)` produces a deterministic ID. Upsert semantics on reconcile: existing edges with the same ID are overwritten. This avoids accumulation of stale edges.

### 3. Full re-emit on each reconcile

On each reconcile, delete all existing `CoChangesWith` edges, then re-emit from current `GitHistoryInsights`. This is simpler than incremental diffing and correct because the co-change set changes when history is re-sampled.

**Alternative considered:** Incremental upsert with staleness detection. Rejected as premature; the edge count is bounded by O(N^2) file pairs but in practice sparse because of the threshold.

### 4. Card surface filters hidden coupling only

`FileCard.co_changes` shows only co-change partners without an existing `Imports` edge. This is the semantic described in the field comment ("hidden coupling"). The raw co-change set remains available in `FileCard.git_intelligence` for consumers that want the full picture.

### 5. Minimum co-change threshold of 2

A single co-occurrence is noise (two files touched in the same commit by coincidence). Require at least 2 sampled commits touching both files. The threshold is a compile-time constant, not a config field.

## Risks / Trade-offs

- **Edge count on large repos**: A monorepo with many files could produce O(N^2) edges below threshold. Mitigation: threshold of 2 plus the existing `max_results` cap in git-intelligence analysis (currently 10 co-change partners per file). Edge count is bounded at 10 * N_files.
- **Reconcile cost**: Full re-emit means deleting and reinserting all co-change edges each pass. Mitigation: edge count is bounded, and SQLite batch operations are fast for typical repo sizes.
- **Path staleness**: Co-change edges use file paths at emission time. A file renamed between reconciles will produce edges against the old path until the next reconcile. Mitigation: `FileNodeId` stability handles renames; edges reference node IDs, not paths.
- **Duplicate signal**: `co_changes` (graph-backed) and `git_intelligence.co_changes` (cache-backed) surface overlapping data. Mitigation: `co_changes` is the filtered hidden-coupling view; `git_intelligence` is the full git-derived picture. Different semantics, same source data.
