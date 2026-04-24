## Why

`context-accounting-and-workflow-v1` deliberately deferred generic overlay note CRUD. Agent-authored notes need their own trust model: schema, provenance, decay, drift invalidation, retrieval labels, and operator UX. Without that boundary, notes risk becoming informal source truth or accidental session memory.

synrepo already separates deterministic graph facts from advisory overlay content. This change designs the next overlay surface for agent observations while preserving that boundary.

## What Changes

- Add an `overlay-agent-notes` capability for advisory, agent-authored notes attached to files, symbols, concepts, paths, tests, or card targets.
- Define note lifecycle actions: add, link, supersede, forget, verify, and list/query.
- Require note provenance: author/tool identity, timestamp, target, claim, evidence references, source hashes or graph revision anchors, confidence, lifecycle status, and drift behavior.
- Define source-drift invalidation and soft decay for notes only. Structural graph facts do not decay.
- Define trust and retrieval behavior so notes are always labeled as overlay/advisory and never merged into graph-backed card truth.
- Implement CLI/MCP surfaces for explicit note operations.
- Keep existing commentary and cross-link overlay behavior intact.

## Capabilities

### New Capabilities
- `overlay-agent-notes`: Agent-authored advisory notes with provenance, drift invalidation, lifecycle state, trust labels, and explicit retrieval boundaries.

### Modified Capabilities
- Extend `overlay`, `mcp-surface`, `cards`, `dashboard`, and `repair-loop` surfaces with bounded advisory note access.

## Impact

- Overlay store schema under `.synrepo/overlay/`, separate from graph truth.
- MCP and CLI surfaces for explicit note operations.
- Card response rendering for optional advisory notes.
- Repair/status/dashboard visibility for stale, superseded, forgotten, and unverified notes.
- No source graph changes, no generic session memory, and no LLM consolidation of source facts in this change.
