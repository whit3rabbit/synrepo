## Context

synrepo ships five card types today: `SymbolCard`, `FileCard`, `DecisionCard`, `EntryPointCard`, and `ModuleCard`. Each is compiled by `GraphCardCompiler` from graph-observed facts with no LLM involvement. The compiler lives in `src/surface/card/compiler/`, types in `src/surface/card/types.rs`, and MCP tool handlers in `crates/synrepo-mcp/src/main.rs`.

The existing `EntryPointCard` and `ModuleCard` establish the pattern for specialist cards: a graph-only compile method, a dedicated spec in `openspec/specs/<name>/spec.md`, budget-tier truncation, and an MCP tool wired through `synrepo_module` / `synrepo_entrypoints`.

This change adds two more specialist cards following the same pattern.

## Goals / Non-Goals

**Goals:**
- Compile `CallPathCard` by tracing `Calls` edges backward from a target symbol to entry points (or forward from an entry point to a target).
- Compile `TestSurfaceCard` by discovering test functions/blocks related to a file or symbol scope.
- Wire both to MCP tools following the existing `synrepo_entrypoints` pattern.
- Keep both cards graph-derived only (no overlay, no LLM).

**Non-Goals:**
- `ChangeRiskCard` or `PublicAPICard` (separate changes, different infrastructure deps).
- Graph-level `CoChangesWith` edges (still future work).
- Symbol-granularity test coverage (file and module scope only for now).
- Path-weighting or probabilistic ranking of call paths.

## Decisions

### D1: CallPathCard uses backward BFS from target

**Decision:** Trace `Calls` edges in reverse from the target symbol toward entry points, collecting all paths up to a bounded depth.

**Rationale:** Forward traversal from all entry points is wasteful when the caller knows the target symbol. Backward BFS from one symbol is O(edges-in-neighborhood) and naturally terminates at entry points (symbols with no callers). The graph already stores `Calls` edges with source/target `SymbolNodeId` pairs from stage 4.

**Alternative considered:** Forward traversal from entry points. Rejected because it requires enumerating all entry points first and explores far more of the graph for a single-target query.

### D2: CallPathCard bounds depth at 8 hops by default

**Decision:** Default max depth of 8 hops, configurable at `deep` budget (up to 12). Paths exceeding the depth budget are truncated with a `truncated: true` marker on the final edge.

**Rationale:** 8 hops covers nearly all meaningful call chains in mid-size repos without risking unbounded traversal. The truncation marker preserves honesty about incompleteness.

### D3: TestSurfaceCard uses symbol-name and file-path heuristics

**Decision:** Discover tests by matching `SymbolKind::Test` markers (tree-sitter test annotations) combined with file-path heuristics: files under `tests/`, `*_test.rs`, `*_test.go`, `test_*.py`, `*.test.ts`, `*.spec.ts`. Relate tests to source files by path convention (`src/foo.rs` <-> `tests/foo.rs`, `src/foo.rs` <-> `src/foo_test.rs`).

**Rationale:** The graph already indexes test functions with `SymbolKind::Test` and file paths. Path-based association is deterministic and requires no new infrastructure. Cross-file `Calls` edges from test bodies to production symbols can strengthen the association at `deep` budget.

**Alternative considered:** Requiring `Calls` edges from test to production code. Rejected because not all test frameworks produce direct call edges (e.g., macros, reflection-based runners).

### D4: Both cards follow the existing specialist-card spec pattern

**Decision:** Each card gets its own spec at `openspec/specs/<name>/spec.md` with Purpose, Requirements, and Scenarios sections, matching `entry-point-card/spec.md` and `module-card/spec.md`.

**Rationale:** Consistency with the two existing specialist card specs. Each spec is self-contained and independently archivable.

### D5: File-scoped test surface only in v1

**Decision:** `TestSurfaceCard` accepts a file path or directory scope. Symbol-scoped queries are supported by filtering test entries that call the target symbol, but the discovery unit remains the file.

**Rationale:** Symbol-scoped coverage requires `Calls` edges from test bodies to production symbols, which may be sparse for some languages. File scope works reliably across all supported languages today.

## Risks / Trade-offs

- **[Risk] Call path BFS on large call graphs may be slow** -> Bounded depth (8 default) and a configurable max-results cap prevent unbounded traversal. The graph is in-memory via SQLite WAL reads, so per-hop cost is low.
- **[Risk] Path conventions miss non-standard test layouts** -> The heuristic covers Rust, Python, TypeScript/TSX, and Go conventions. Repos using non-standard layouts get fewer test associations. The card returns an empty `tests` list rather than guessing, consistent with the "no result over spurious result" principle from `EntryPointCard`.
- **[Risk] Backward BFS may find many paths to the same entry point** -> Deduplicate by (entry_point, target) pair, returning at most 3 distinct paths per pair. Additional paths are counted in a `paths_omitted` field.
- **[Trade-off] Test-to-source association is path-based, not semantic** -> Faster and deterministic, but misses tests that exercise source code via indirect imports. Acceptable for v1; `Calls` edge strengthening at `deep` budget partially mitigates.
