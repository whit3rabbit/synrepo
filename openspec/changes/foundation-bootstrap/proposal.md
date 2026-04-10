## Why

`ROADMAP.md` already defines synrepo's OpenSpec intent, but the repo does not yet have the durable spec spine or the first active change that future work can build on. That gap creates avoidable argument about where behavior belongs, how changes should be named, and whether OpenSpec is planning infrastructure or part of the runtime product.

This change establishes the minimum foundation first. It promotes the existing foundation docs into canonical OpenSpec specs, locks the repository conventions for specs versus changes versus runtime state, and creates one bootstrap change that future roadmap items can follow.

## What Changes

- Create the initial `openspec/specs/` capability tree described in `ROADMAP.md`.
- Promote the product and architecture rules from `docs/FOUNDATION-SPEC.md` and `docs/FOUNDATION.md` into durable capability specs.
- Add durable capability boundaries for git intelligence, storage compatibility, and exports/views where the runtime and docs were already sharper than the initial spec spine.
- Tighten `openspec/config.yaml` so future artifacts stay roadmap-aligned and foundation-first.
- Define the `foundation-bootstrap` change as the canonical starting point for future synrepo planning work.
- Record the contributor workflow and next planned change sequence without opening speculative feature changes yet.

## Capabilities

### New Capabilities
- `foundation`: product mission, trust boundaries, operating modes, and OpenSpec planning-role rules
- `substrate`: lexical indexing, discovery, and file-handling contract
- `graph`: canonical observed-facts model, provenance, and identity stability contract
- `cards`: card types, budget tiers, and source-labeling contract
- `mcp-surface`: task-first tool and response contract
- `bootstrap`: first-run initialization, generated shims, and health-check contract
- `patterns-and-rationale`: optional human-guidance and DecisionCard contract
- `repair-loop`: targeted drift detection and selective repair contract
- `watch-and-ops`: watcher, reconcile, locking, and diagnostics contract
- `overlay`: machine-authored commentary and proposed-link contract
- `evaluation`: success metrics, anti-metrics, and benchmark contract
- `git-intelligence`: git-derived routing and impact enrichment contract
- `storage-and-compatibility`: `.synrepo/` store lifecycle, migration, and rebuild contract
- `exports-and-views`: generated export and runtime view contract

### Modified Capabilities
- None.

## Impact

- Affects the OpenSpec planning layer only.
- Introduces the enduring capability layout that later roadmap changes will target.
- Gives future changes stable naming and placement rules.
- Does not implement runtime product features or mutate `.synrepo/` behavior.
