## Why

The repo now has a canonical graph store and direct inspection surface, but graph data still has to be inserted manually because the structural compile pipeline remains a stub. That is the next real Milestone 2 gap: without automatic graph population from repository state, the observed-facts core is not actually alive.

## What Changes

- Define and implement the first structural compile stages that discover supported source files, parse code and markdown inputs, and write observed graph facts into the canonical graph store.
- Define how `synrepo init` and refresh flows trigger structural graph population so the graph inspection surface reflects repository state automatically.
- Lock the first deterministic producer set for files, symbols, `defines` edges, markdown-backed concept nodes, and directly-declared governance or mention-style prose links where feasible.
- Add focused tests for compile behavior, graph materialization, and idempotent refresh from repository state.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `graph`: add the structural compile behavior that produces persisted graph facts automatically from repository state
- `bootstrap`: define bootstrap-time structural graph population and refresh behavior alongside existing initialization

## Impact

- Affects `src/pipeline/structural.rs`, `src/structure/parse.rs`, and related structure modules that currently stop at scaffolding
- Affects bootstrap and init-triggered refresh behavior in `src/bootstrap/` and CLI dispatch paths
- Adds tests around structural compile output, persisted graph materialization, and re-run behavior
- Does not implement watch/daemon orchestration or Git-history mining, which remain separate follow-on work
