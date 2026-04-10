## 1. Define Git-history mining behavior

- [ ] 1.1 Implement the first deterministic Git-mining pass for co-change, ownership hints, hotspots, and last meaningful change summaries
- [ ] 1.2 Add tests for normal history-mining behavior on representative repositories and commit histories
- [ ] 1.3 Add tests for degraded cases such as shallow history, detached HEAD, and missing blame coverage

## 2. Integrate Git-derived evidence into synrepo surfaces

- [ ] 2.1 Wire Git-derived evidence into the graph or related structural outputs with explicit `git_observed` authority
- [ ] 2.2 Expose Git-derived enrichment through existing card fields and routing-oriented summaries without overriding parser-observed structure
- [ ] 2.3 Confirm the `git_commit_depth` and related config behavior match the Git-intelligence contract

## 3. Validate and document the change

- [ ] 3.1 Align code comments and implementation notes with the new Git-intelligence contract
- [ ] 3.2 Add or update benchmarks or fixture expectations for history-derived ranking quality where feasible
- [ ] 3.3 Validate the change with `openspec validate git-intelligence-v1 --strict --type change`
