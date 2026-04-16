## Context

The structural pipeline runs 8 stages. Stages 1-5 are wired end-to-end. Stages 6 (identity cascade) and 7 (drift scoring) have scaffold files with TODO stubs:

- `src/structure/identity.rs` defines `IdentityResolution` (Rename, Split, Merge, GitRename, Breakage) and a `resolve_identities()` function that returns an empty vec.
- `src/structure/drift.rs` defines `compute_drift_score()` that returns 0.0.
- `EdgeKind` already has `SplitFrom` and `MergedFrom` variants but they are never produced.
- Content-hash rename detection is already wired in the pipeline (stage 5 area). This handles the simple "same content, new path" case.

The gap: when symbols from one deleted file are distributed across multiple new files (split), or multiple deleted files consolidate into one new file (merge), the current pipeline creates fresh nodes and orphans the old ones. No drift signal exists to flag edges whose source artifacts have diverged.

## Goals / Non-Goals

**Goals:**
- Implement AST symbol-set matching for split and merge detection in stage 6.
- Produce `SplitFrom` and `MergedFrom` edges with `Epistemic::ParserObserved`.
- Implement structural drift scoring in stage 7 for all graph edges.
- Persist drift scores so they survive across compile cycles.
- Surface high-drift edges through the repair loop (`synrepo check` / `sync`).

**Non-Goals:**
- Symbol-level identity resolution (this change is file-level only).
- Drift-driven card payload changes (future `ChangeRiskCard` will consume drift scores).
- `CoChangesWith` edge production (separate change, different data source).
- ArcSwap commit (stage 8, separate change).
- Semantic drift detection (meaning changes without structural changes).

## Decisions

### D1: Symbol-set overlap threshold for split/merge detection

**Decision**: Use Jaccard similarity on qualified symbol names between disappeared and new files. Threshold of 0.4 for split detection, 0.5 for merge detection.

**Rationale**: Tree-sitter extraction already produces `ExtractedSymbol` with qualified names. Jaccard on name sets is deterministic, fast, and language-agnostic. The split threshold is lower because a large file split into two parts naturally distributes symbols unevenly.

**Alternatives considered**:
- AST subtree hash matching: more precise but requires per-language AST comparison logic and is fragile across formatting changes.
- Line-level similarity: noisy for refactored code (renamed variables, reordered imports).
- Pure content hash: already used for rename detection; cannot detect splits since parts never match the whole.

### D2: Drift score storage: sidecar table, not edge column

**Decision**: Store drift scores in a separate `edge_drift` table keyed by `(edge_id, revision)`, not as a column on the edges table.

**Rationale**: Drift scores change every compile cycle. Keeping them separate avoids write amplification on the main edges table and makes it cheap to truncate and recompute. The edges table is read-heavy during card compilation; adding a frequently-updated column would contend with readers.

**Schema**:
```sql
CREATE TABLE IF NOT EXISTS edge_drift (
    edge_id BLOB NOT NULL,
    revision TEXT NOT NULL,
    drift_score REAL NOT NULL,
    PRIMARY KEY (edge_id, revision)
) WITHOUT ROWID;
```

**Alternatives considered**:
- Column on edges table: simpler schema but increases write contention.
- In-memory only: lost between runs, cannot surface through repair loop or future MCP tools.
- File-based: no query advantage, adds I/O overhead.

### D3: Drift score computation uses structural fingerprint, not full AST diff

**Decision**: Define a "structural fingerprint" per artifact as the sorted set of (qualified_name, signature_hash) pairs for symbols in that file. Drift between two fingerprints is computed as 1 - Jaccard(signature_pairs_before, signature_pairs_after).

**Rationale**: The signature hash captures the declaration shape (name, parameters, return type) without body content. This aligns with the scoring bands already documented in `drift.rs`: cosmetic changes score 0.0-0.1 (signatures unchanged), signature-only changes score 0.1-0.3, etc. Full body hashing would over-weight formatting changes.

**Alternatives considered**:
- Full body hash: too sensitive to whitespace and comment changes.
- Token count comparison: too coarse, misses signature-level changes.
- AST edit distance: correct but expensive to compute per edge per cycle.

### D4: Cascade ordering in stage 6

**Decision**: Run the identity cascade in this order: (1) content-hash rename, (2) symbol-set split, (3) symbol-set merge, (4) git rename fallback, (5) breakage. Each disappeared file is consumed by the first matching rule and is not reconsidered.

**Rationale**: Content-hash rename is the most precise (exact match). Split and merge are next because they have multi-file evidence. Git rename is a fallback when AST evidence is inconclusive. Breakage is the default when nothing matches.

### D5: Drift-edge repair surface is report-only for scores below 1.0

**Decision**: `synrepo sync` only auto-prunes edges at drift 1.0 (artifact deleted). Edges with drift in (0.0, 1.0) are reported by `synrepo check` but not automatically modified.

**Rationale**: A high drift score means the artifacts diverged, not that the edge is wrong. The edge may still be valid (the ADR's target struct was extended, not removed). Auto-pruning anything below 1.0 risks deleting valid graph relationships. The report gives agents and users the information to decide.

## Risks / Trade-offs

- **False positives in split/merge detection**: Low-overlap splits (one symbol moved to a new file) may fall below the 0.4 threshold and be classified as breakage instead of split. Mitigation: the breakage classification still logs the reason; a future change can lower thresholds based on telemetry.
- **Drift score churn**: Every compile cycle recomputes all drift scores. For large graphs this is O(edges) fingerprint comparisons. Mitigation: fingerprint computation is cheap (symbol name + signature hash, already extracted in stage 2). Batch write to `edge_drift` table.
- **Sidecar table growth**: `edge_drift` accumulates one row per edge per revision. Mitigation: truncate old revisions at the start of each cycle. Only the latest revision's scores are needed for repair-loop reporting.
- **Split/merge not detected on first run**: The identity cascade compares disappeared files to new files. On the very first `synrepo init`, there are no disappeared files, so no splits or merges. This is correct: identity resolution only matters across compile cycles.

## Open Questions

- Should drift scores be surfaced on `SymbolCard` or `FileCard` payloads in this change, or deferred to the `ChangeRiskCard` change? Current proposal: deferred.
- Should `synrepo check` report per-edge drift or aggregate (e.g., "12 edges above 0.7 drift")? Current proposal: aggregate in check, per-edge via `synrepo graph query`.
