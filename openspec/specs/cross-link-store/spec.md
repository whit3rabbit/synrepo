# cross-link-store Specification

## Purpose
TBD - created by archiving change cross-link-overlay-v1. Update Purpose after archive.
## Requirements
### Requirement: Persist cross-link candidates in the overlay store
synrepo SHALL persist cross-link candidates in a `cross_links` table inside `.synrepo/overlay/overlay.db`, physically separate from the canonical graph store and schema-isolated from the `commentary` table. Each candidate SHALL carry both endpoint node IDs, an `OverlayEdgeKind`, cited source and target evidence spans, a numeric confidence score, a surfaced confidence tier (`high`, `review_queue`, `below_threshold`), both endpoints' content hashes at generation time, and a full `Provenance` record (pass identifier, model identity, generation timestamp).

#### Scenario: Store a new cross-link candidate
- **WHEN** the cross-link generator produces a candidate for a prose → code relationship
- **THEN** the overlay store persists it in the `cross_links` table with all required fields
- **AND** the entry is written to `.synrepo/overlay/overlay.db`, never to `.synrepo/graph/nodes.db`

#### Scenario: Retrieve candidates for a node
- **WHEN** the card compiler queries the overlay store for candidates involving a node ID
- **THEN** the store returns every candidate where the node appears as source or target, with full provenance and current freshness state
- **AND** the returned candidates carry their surfaced confidence tier

#### Scenario: Reject a candidate with missing evidence
- **WHEN** a cross-link candidate is presented for storage without any cited source or target spans
- **THEN** the overlay store rejects the candidate with an error
- **AND** no partial entry is written

### Requirement: Derive cross-link freshness from both endpoints
synrepo SHALL derive the freshness state of a cross-link candidate by comparing the stored `from_content_hash` and `to_content_hash` against the current `FileNode.content_hash` for each endpoint's file. Both endpoints matching yields `fresh`; any mismatch yields `stale`; either endpoint missing from the graph yields `source_deleted`; missing required provenance fields yields `invalid`; absence of any candidate for a queried pair yields `missing`.

#### Scenario: Candidate is fresh
- **WHEN** both stored content hashes match their corresponding current `FileNode.content_hash` values
- **THEN** the overlay store derives freshness as `fresh`

#### Scenario: Candidate becomes stale after a code edit
- **WHEN** the `to_content_hash` no longer matches the code endpoint's current hash and the prose endpoint is unchanged
- **THEN** the overlay store derives freshness as `stale`
- **AND** the stale candidate is returned to callers with an explicit staleness label, not silently withheld

#### Scenario: Endpoint node is deleted
- **WHEN** either endpoint node ID no longer exists in the graph
- **THEN** the overlay store derives freshness as `source_deleted`
- **AND** the candidate remains stored for audit until `prune_orphans` runs

### Requirement: Prune orphaned cross-links during reconcile
synrepo SHALL provide a `prune_orphans` operation on the overlay store that deletes `cross_links` rows whose `from_node` or `to_node` is no longer in the graph. The operation SHALL be invoked during `synrepo reconcile` after the structural compile completes. Pruned candidates SHALL leave an immutable record in `cross_link_audit` documenting the prune event, endpoint IDs, and reason.

#### Scenario: Prune candidate whose source node was deleted
- **WHEN** a cross-link candidate's `from_node` is removed from the graph and `synrepo reconcile` runs
- **THEN** the candidate is removed from `cross_links`
- **AND** an audit row is appended recording the endpoint IDs, reason (`source_deleted`), and timestamp
- **AND** no orphaned entries accumulate across reconcile cycles

### Requirement: Maintain immutable cross-link audit trail
synrepo SHALL persist every cross-link lifecycle event in a `cross_link_audit` table. Recorded events SHALL include candidate creation, confidence score changes, review decisions, promotion to graph, rejection, and prune. Each audit row SHALL carry candidate identity (by endpoint pair and kind), event kind, reviewer identity when applicable, timestamp, and a snapshot of the candidate's provenance at event time. Audit rows SHALL NOT be deleted or mutated after write; deletion of a candidate from `cross_links` SHALL NOT remove its audit rows.

#### Scenario: Audit a promoted candidate
- **WHEN** an operator queries the audit trail for a candidate that was promoted to the graph
- **THEN** the overlay store returns the original generation event, any intervening score changes, the review decision, and the promotion event
- **AND** the audit record is returned even if the candidate row itself was later pruned

#### Scenario: Record a rejection with reviewer identity
- **WHEN** a reviewer rejects a candidate through `synrepo links reject`
- **THEN** an audit row is appended with event kind `rejected`, the reviewer identity provided by the CLI, and the current timestamp
- **AND** the candidate row is updated to `rejected` state but not deleted

### Requirement: Enforce write isolation between overlay tables
synrepo SHALL ensure no code path writes `cross_links` data into the graph store or into the `commentary` table, and no code path writes commentary or graph data into `cross_links` or `cross_link_audit`. The overlay store SHALL use non-overlapping schema namespaces and distinct open constructors or table-scoped accessors per content type.

#### Scenario: Graph store open path does not create cross-link tables
- **WHEN** the graph store is opened at `.synrepo/graph/nodes.db`
- **THEN** no `cross_links` or `cross_link_audit` table exists in that database

#### Scenario: Commentary writer cannot write to cross-link tables
- **WHEN** the commentary writer is invoked with a cross-link-shaped payload (e.g. due to a programming error)
- **THEN** the operation fails at the type system level or at the database-accessor boundary
- **AND** no cross-link data is written to the `commentary` table

