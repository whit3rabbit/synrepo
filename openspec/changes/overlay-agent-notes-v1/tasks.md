## 1. Design Approval

- [x] 1.1 Review and approve the note schema, lifecycle states, and trust boundaries.
- [x] 1.2 Decide final MCP and CLI naming for note operations.
- [x] 1.3 Decide retention and pruning policy for forgotten notes and lifecycle transitions.

## 2. Storage And Schema

- [x] 2.1 Add overlay-store tables or typed records for agent notes and lifecycle transitions.
- [x] 2.2 Enforce required provenance fields and invalid-state classification.
- [x] 2.3 Add drift anchors using source hashes, graph revisions, and evidence references.

## 3. Lifecycle Operations

- [x] 3.1 Implement add, link, supersede, forget, verify, and list/query operations.
- [x] 3.2 Preserve audit history for lifecycle transitions.
- [x] 3.3 Keep forgotten notes hidden from normal retrieval while available to audit queries.

## 4. Drift, Status, And UX

- [x] 4.1 Mark notes stale when cited source hashes, graph revisions, or evidence references drift.
- [x] 4.2 Surface active, stale, unverified, superseded, forgotten, and invalid counts in status/dashboard snapshots.
- [x] 4.3 Add repair/check recommendations for stale or invalid notes.

## 5. MCP, CLI, And Cards

- [x] 5.1 Add explicit MCP note operations without removing existing tools.
- [x] 5.2 Add CLI note commands with JSON output.
- [x] 5.3 Add optional, bounded advisory note fields to selected card responses.

## 6. Verification

- [x] 6.1 Add unit tests for provenance validation, lifecycle transitions, and drift invalidation.
- [x] 6.2 Add MCP and CLI contract tests for advisory labels and bounded note retrieval.
- [x] 6.3 Add regression tests proving notes never feed graph-backed structural card truth.
- [x] 6.4 Add OpenSpec status validation before implementation begins.
