## Context

The card compiler can already produce individual cards (`SymbolCard`, `FileCard`, `DecisionCard`, `ModuleCard`, `EntryPointCard`) and the MCP surface exposes them through task-first tools. The graph store supports `outbound(from, kind)` and `inbound(to, kind)` queries, and `find_governing_concepts(node_id)` resolves incoming `Governs` edges. Git-intelligence analysis (co-change, ownership, hotspots) is computed per-file and cached in the card compiler session.

What does not exist: a single tool that assembles a bounded neighborhood around a focal node, combining structural edges with git co-change signals into one response scoped by budget.

## Goals / Non-Goals

**Goals:**

- Ship `synrepo_minimum_context` MCP tool that returns a focal card plus a budget-bounded 1-hop neighborhood.
- Neighborhood includes: outbound structural edges (`Calls`, `Imports`), incoming `Governs` (as DecisionCard summaries), and top-N co-change partners from git intelligence.
- Budget tiers control neighborhood breadth: `tiny` returns focal card with edge counts, `normal` adds structural neighbors and top-3 co-change, `deep` adds full neighbor cards and top-5 co-change.
- Neighborhood resolution runs under a single graph read snapshot to ensure a consistent epoch.

**Non-Goals:**

- Graph-level `CoChangesWith` edges. Co-change data comes from the existing git-intelligence per-file cache, not from persisted graph edges. That upgrade (ROADMAP Phase 1 §11.2) is a separate change.
- Overlay content in neighborhood responses. The tool reads only from the graph store and git-intelligence cache. No commentary, no proposed links.
- Multi-hop traversal. Strictly 1-hop. Callers who need deeper context issue additional calls.
- New card types. The response reuses existing card structs for neighbors.

## Decisions

### 1. Neighborhood resolution in the Surface layer, not a new pipeline stage

The resolution logic lives in `src/surface/card/` (a new `neighborhood.rs` module or an extension of the compiler). It calls existing `GraphStore` methods (`outbound`, `inbound`, `find_governing_concepts`) and the git-intelligence cache. No pipeline stage changes, no graph writes, no schema changes.

**Alternative considered**: A new pipeline stage that pre-computes and persists neighborhoods. Rejected because neighborhoods are budget-dependent and read-only; persisting them would add write-path complexity for a query-time concern.

### 2. Co-change partners from git-intelligence cache, not graph edges

`CoChangesWith` is defined in `EdgeKind` but never produced. Rather than emitting graph edges (which requires pipeline changes, write-path work, and storage), the neighborhood resolver reads co-change data directly from the git-intelligence cache that `git-data-surfacing-v1` already wired into the card compiler. The response labels co-change entries with `source: "git_intelligence"` and `granularity: "file"` so callers know the precision boundary.

**Alternative considered**: Emit `CoChangesWith` edges during stage 5 and query them through `outbound`. This is the correct long-term approach but is out of scope for this change because it touches the write path, schema, and repair loop.

### 3. Dedicated MCP tool, not an extension of `synrepo_card`

`synrepo_minimum_context` gets its own tool definition with distinct parameters (`target`, `budget`) and a different response shape (focal card + neighbor summaries + co-change partners). Extending `synrepo_card` would overload its response contract and confuse callers who expect a single card.

### 4. Neighbor summaries at `normal`, full cards at `deep`

At `normal` budget, neighbors appear as lightweight summaries: node ID, qualified name, kind, and edge type. At `deep`, the resolver produces full cards for each neighbor. This matches the progressive-disclosure pattern and keeps `normal` responses affordable in tokens.

## Risks / Trade-offs

- **Co-change data is file-granularity, not symbol-granularity.** When the focal target is a symbol, co-change partners are derived from the containing file's history. The response labels this with `granularity: "file"`. This is the same precision boundary as `SymbolCard.last_change` and is documented in the spec.
- **Neighborhood size at `deep` budget can be large.** A highly connected symbol with many callers and callees could produce a large response. The spec caps structural neighbors at 10 per edge kind and co-change partners at 5. Callers who need more issue targeted `synrepo_card` calls.
- **No overlay content means no cross-link candidates.** If a caller needs proposed links for a neighbor, they must call `synrepo_card` at `deep` budget separately. This is intentional: the minimum-context tool prioritizes speed and consistency over exhaustive coverage.
