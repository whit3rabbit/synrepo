## Context

The current repo already has most of the Phase 1 graph vocabulary in place: stable file and symbol IDs, graph epistemics, provenance records, edge kinds, and CLI placeholders for graph inspection. What it does not have is a working graph store, concept-node modeling, or a direct path from observed facts to persisted graph queries. That gap matters because every later structural feature, including rename handling, markdown concept links, drift scoring, and Git-derived evidence, assumes a real canonical graph exists first.

The storage-compatibility work also already treats `graph` as a canonical runtime store under `.synrepo/graph/`. The first implementation should fit that contract instead of inventing a parallel layout. `git-intelligence-v1` remains planning-ready and separate: this change may preserve `git_observed` as a valid epistemic label and edge kind, but it does not mine or rank history yet.

## Goals / Non-Goals

**Goals:**
- Implement the first sqlite-backed canonical graph store under `.synrepo/graph/`.
- Complete the graph data model for file, symbol, concept, and edge persistence with provenance intact.
- Expose a small direct-inspection surface for Phase 1, enough for node lookup, simple graph queries, and graph statistics.
- Add tests that lock the observed-only trust boundary, concept-node admission rules, and persisted query behavior.

**Non-Goals:**
- Implement the full structural compile pipeline end to end.
- Implement Git-history mining, ownership heuristics, or co-change population.
- Introduce cards, MCP graph serving, or overlay integration.
- Add non-sqlite graph backends or an in-memory query mirror.

## Decisions

1. Use one sqlite file inside the canonical graph store directory for the first slice.
   The first implementation should materialize `.synrepo/graph/nodes.db`, matching the existing compatibility and bootstrap expectations. A single sqlite file is enough for nodes, edges, and metadata now, and later migrations can split physical layout if measurement ever justifies it.
   Alternative considered: introduce multiple graph database files immediately. Rejected because it adds migration and bootstrap complexity before the graph is even functional.

2. Model concept nodes explicitly in the structure layer now.
   `ConceptNodeId` already exists in core IDs, and both the architecture docs and config already define concept directories. The graph layer should add a concrete `ConceptNode` type plus store operations now, so the type boundary is enforced before later markdown and curated-mode work arrives.
   Alternative considered: defer concept nodes until prose parsing lands. Rejected because it leaves the Phase 1 graph type system incomplete and pushes trust-boundary risk into later work.

3. Keep graph queries intentionally small and deterministic.
   The first user-facing surface should support direct node lookup, graph statistics, and simple edge-filtered traversals over stored nodes. That is enough to prove persistence and inspection without inventing a general query language too early.
   Alternative considered: design a richer ad hoc query DSL now. Rejected because the repo does not need it yet, and it would produce more contract surface than implementation value.

4. Treat Git-derived rows as storable but not producer-backed in this change.
   The graph schema and query code should accept `GitObserved` rows because the enduring graph contract already includes them, but this change will not create them. That keeps `git-intelligence-v1` planning-ready without forcing placeholder Git mining into the store implementation.
   Alternative considered: remove Git-related variants until mining is implemented. Rejected because it would fight the enduring graph contract and create needless churn.

5. Prefer test-local graph construction over pipeline coupling.
   The first graph tests should create nodes and edges directly against the store and CLI helpers. That keeps failures narrow and avoids pretending the structural pipeline is done.
   Alternative considered: wire graph persistence only through `synrepo init`. Rejected because the pipeline is still skeletal, and coupling the store milestone to unfinished parse stages would blur failures.

## Risks / Trade-offs

- Placeholder query behavior could harden into the wrong public contract, mitigation: keep Phase 1 inspection limited to node lookup, stats, and minimal traversal semantics.
- Reusing `.synrepo/graph/nodes.db` is a slightly awkward name if the store later holds more than nodes, mitigation: treat the filename as a compatibility detail that can migrate later rather than blocking the first implementation.
- Adding `ConceptNode` before markdown parsing is complete creates a temporarily underpopulated node type, mitigation: enforce strict admission rules in code and tests so later producers plug into a stable store contract.
- Persisting provenance everywhere adds schema and test weight, mitigation: keep the first schema normalized enough to round-trip existing structs without premature optimization.

## Migration Plan

1. Add the graph data model and sqlite store implementation behind the existing structure/store modules.
2. Wire the Phase 1 CLI inspection commands to the new store without changing bootstrap or substrate behavior.
3. Materialize `.synrepo/graph/nodes.db` only when graph data is actually written, so existing runtimes without a graph store continue to behave under the current compatibility rules.
4. If follow-on changes need richer schema or multiple databases, ship them as explicit graph-store migrations under the storage-compatibility contract.

## Open Questions

- Whether `synrepo init` should eagerly create an empty graph database in this change or wait until the first structural compile writes facts. The smaller choice is lazy creation unless a later task proves eager materialization is needed.
- Whether graph stats should report only persisted counts or also include schema/version metadata. For this first slice, persisted counts are enough.
