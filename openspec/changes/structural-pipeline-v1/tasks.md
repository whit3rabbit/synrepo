## 1. Implement the initial structural producers

- [x] 1.1 Implement the first supported code and concept-markdown producers that emit file nodes, symbol nodes, `defines` edges, and concept nodes from repository inputs
- [x] 1.2 Implement deterministic structural compile orchestration that refreshes the producer-owned graph slice and writes persisted graph facts automatically
- [x] 1.3 Add focused tests for structural compile output, idempotent reruns, and stale-fact replacement

## 2. Integrate automatic graph population into bootstrap

- [x] 2.1 Wire bootstrap and init refresh flows to run the structural compile after substrate rebuild
- [x] 2.2 Update bootstrap reporting so graph population status and next-step guidance are visible after init
- [x] 2.3 Add bootstrap-level tests that confirm fresh init and rerun flows materialize and refresh the graph store automatically

## 3. Tighten contracts and validation

- [x] 3.1 Align structural pipeline, parse, and bootstrap comments with the new automatic graph-population contract
- [x] 3.2 Confirm the change stays scoped away from watcher orchestration, Git-history mining, and full identity-cascade work
- [x] 3.3 Validate the change with `openspec validate structural-pipeline-v1 --strict --type change`
