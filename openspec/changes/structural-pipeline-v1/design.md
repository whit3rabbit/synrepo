## Context

The repo now has three important pieces in place: deterministic discovery and lexical indexing, a canonical sqlite-backed graph store, and a direct graph inspection CLI. What is still missing is the bridge between them. `src/pipeline/structural.rs` is still a stub, `src/structure/parse.rs` does not yet emit real symbols, and bootstrap only rebuilds the lexical substrate. That leaves the graph store real but mostly empty unless tests or manual code write into it.

This change is the smallest follow-on that makes the observed-facts core start behaving like a product layer. It should populate the graph automatically from repository state, but it should not absorb watch/reconcile behavior, Git-history mining, or the full identity cascade. Those remain separate slices so Milestone 2 does not turn into a junk drawer.

## Goals / Non-Goals

**Goals:**
- Implement the first structural compile stages that discover eligible files, parse supported code, parse concept-backed markdown, and write canonical graph facts automatically.
- Define when bootstrap or refresh runs the structural compile so a user gets persisted graph data without manual seeding.
- Lock a deterministic first producer set for file nodes, symbol nodes, `defines` edges, concept nodes, and directly-declared prose relationships that are cheap and trustworthy.
- Add tests for compile idempotence, graph materialization, and refresh behavior from repository state.

**Non-Goals:**
- Implement file watching, daemonization, reconcile locking, or background operations.
- Implement Git-history mining, ownership, hotspots, or co-change production.
- Complete the full rename, split, merge, and drift pipeline in the same change.
- Expand the graph query surface or cards beyond what already exists.

## Decisions

1. The first automatic compile is producer-first, not phase-complete.
   This change should land the producers that make the graph useful immediately: discovered file nodes, parsed symbol nodes, `defines` edges, concept nodes from configured markdown directories, and directly-observed prose edges where the parser can prove them cheaply. It should leave Git, drift, and complex cross-file resolution for later.
   Alternative considered: implement the entire eight-stage structural pipeline at once. Rejected because the risk surface is too large and would mix unrelated failure modes.

2. Bootstrap should trigger graph population after substrate rebuild.
   `synrepo init` already creates runtime layout, writes config, and rebuilds the lexical substrate. The smallest durable extension is to run the structural compile immediately after the substrate step and report graph-oriented status in the bootstrap summary.
   Alternative considered: add a separate manual `synrepo graph rebuild` command first. Rejected because it would postpone automatic graph population again and weaken the “first-run value” story.

3. Producer inputs should reuse the existing discovery contract.
   The structural compile should consume the same discovered file set and repository rules already established by the substrate change, instead of inventing a second walker with subtly different admission behavior.
   Alternative considered: let the structural pipeline rediscover files independently. Rejected because it would create drift between indexed and structural corpora.

4. Graph writes should be deterministic and replace stale structural state for the produced slice.
   The initial compile path should clear and repopulate the node and edge subsets it owns for each run, so refreshes converge on current repository truth instead of appending duplicates.
   Alternative considered: incremental merge-only writes from day one. Rejected because the current graph layer does not yet have enough identity and drift machinery to do that safely.

5. Markdown concept production should stay trust-bounded.
   Concept nodes should only come from configured concept directories, and prose-derived edges should only be emitted from direct, human-authored declarations the parser can verify structurally.
   Alternative considered: broaden markdown parsing to all docs immediately. Rejected because it would blur the concept-node boundary the project is explicitly trying to protect.

## Risks / Trade-offs

- Clearing and repopulating the producer-owned slice is simpler but gives up some future incremental efficiency, mitigation: keep this contract scoped to the initial compile path and revisit once identity handling is implemented.
- Tree-sitter producer coverage can be uneven across languages, mitigation: start with the currently supported languages and a minimal symbol/`defines` extraction set that is easy to test.
- Extending bootstrap to run structural compile increases first-run work, mitigation: keep the initial producer set narrow and deterministic so init remains cheap.
- If the change tries to include watch/reconcile or Git mining, it will sprawl, mitigation: keep those concerns explicitly out of scope in both tasks and code comments.

## Migration Plan

1. Implement the initial structural producers and compile orchestration behind `src/pipeline/structural.rs`.
2. Wire bootstrap to invoke the structural compile after substrate rebuild and surface graph-oriented status.
3. Add or update tests so fresh init and rerun flows materialize the graph store automatically and converge on current repository truth.
4. Leave watcher/reconcile and Git-derived stages for their own follow-on changes.

## Open Questions

- Whether the first prose slice should include only concept-node creation or also direct `mentions` and `governs` edge extraction. The safe answer is to include only what the markdown parser can prove cheaply in this change.
- Whether graph refresh should delete all producer-owned facts before reinsert or keep a narrower per-file replacement model. The change should start with the simpler whole-slice replacement unless implementation shows it is too blunt.
