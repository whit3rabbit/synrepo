## Why

The repo already models Git-derived facts in types, comments, and config, but Git intelligence is still only an implied future layer. `git_observed` exists in the graph model, card types already reserve fields like `last_change` and `co_changes`, and config already includes git history depth, yet there is no active change that turns those ideas into an implementable contract.

This change is opened early to sharpen the contract boundary while the relevant code and docs are still fresh. It does not change the roadmap's execution order, which still places Git intelligence implementation after Milestone 2 structural-graph and watch/reconcile work.

## What Changes

- Define the first real Git-intelligence behavior for ownership, hotspots, co-change, last meaningful change, and churn-aware ranking.
- Lock how Git-derived evidence enters synrepo as `git_observed` data without overriding parser-observed structure.
- Define how Git-history signals are surfaced in cards and routing-oriented responses.
- Define degraded-history behavior for shallow clones, detached HEADs, missing history, and other partial repository states.
- Add implementation tasks and validation expectations for history mining and Git-derived card enrichment.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `git-intelligence`: sharpen Git-derived routing, ranking, degraded-history handling, and card-facing outputs into implementable behavior

## Impact

- Affects Git mining and ranking work in the structural pipeline and graph layer
- Affects card enrichment behavior, especially change-risk and file-oriented surfaces
- Depends on repository history behavior and current config fields such as `git_commit_depth`
- Does not change the graph versus overlay trust boundary or introduce machine-authored history analysis
