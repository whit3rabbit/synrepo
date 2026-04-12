## ADDED Requirements

### Requirement: Persist commentary entries in a physically separate SQLite store
synrepo SHALL persist commentary entries in `.synrepo/overlay/overlay.db`, separate from the canonical graph store at `.synrepo/graph/nodes.db`. The overlay store SHALL be created on first use. Each commentary entry SHALL carry the annotated node ID, commentary text, and a full `CommentaryProvenance` record including: source content hash at generation time, producing pass identifier, model identity, and generation timestamp.

#### Scenario: Store a new commentary entry
- **WHEN** the commentary generator produces an entry for a symbol node
- **THEN** the overlay store persists it with all required provenance fields
- **AND** the entry is written to `.synrepo/overlay/overlay.db`, never to `.synrepo/graph/nodes.db`

#### Scenario: Retrieve a commentary entry for a known node
- **WHEN** the card compiler queries the overlay store for a symbol node ID at `Deep` budget
- **THEN** the store returns the commentary entry if one exists, or `None` if absent
- **AND** the returned entry carries its full provenance record

#### Scenario: Reject storage of an entry with missing provenance
- **WHEN** a commentary entry is presented for storage without one or more required provenance fields
- **THEN** the overlay store rejects the entry with an error
- **AND** no partial entry is written

### Requirement: Derive freshness by content-hash comparison
synrepo SHALL derive the freshness state of a commentary entry by comparing the `source_content_hash` stored at generation time against the current `FileNode.content_hash` for the annotated node's file. A match yields `fresh`; a mismatch yields `stale`. An entry with missing required provenance fields yields `invalid`. A node kind with no commentary pipeline yields `unsupported`. Absence of any entry yields `missing`.

#### Scenario: Commentary entry is fresh
- **WHEN** the stored `source_content_hash` matches the current `FileNode.content_hash`
- **THEN** the overlay store derives freshness as `fresh`

#### Scenario: Commentary entry is stale
- **WHEN** the stored `source_content_hash` does not match the current `FileNode.content_hash`
- **THEN** the overlay store derives freshness as `stale`
- **AND** the stale entry is returned to callers with an explicit staleness label; it is not silently withheld

#### Scenario: Commentary entry has missing provenance
- **WHEN** a stored commentary entry is missing one or more required provenance fields
- **THEN** the overlay store derives freshness as `invalid`
- **AND** the entry is withheld from normal user responses and flagged in audit queries

### Requirement: Prune orphaned commentary entries during reconcile
synrepo SHALL provide a `prune_orphans` operation on the overlay store that deletes commentary entries whose annotated node ID no longer exists in the graph. This operation SHALL be called during `synrepo reconcile` after the structural compile completes.

#### Scenario: Prune entry for a deleted symbol
- **WHEN** a symbol node is removed from the graph and `synrepo reconcile` runs
- **THEN** the commentary entry for that node is removed from the overlay store
- **AND** no orphaned entries accumulate across reconcile cycles

### Requirement: Enforce write isolation between graph and overlay stores
synrepo SHALL ensure that no code path writes commentary data to the graph store or graph data to the overlay store. The stores are opened via distinct `open` constructors that target their respective file paths and use non-overlapping schema namespaces.

#### Scenario: Graph store open path does not create overlay tables
- **WHEN** the graph store is opened at `.synrepo/graph/nodes.db`
- **THEN** no `commentary` table exists in that database

#### Scenario: Overlay store open path does not create graph tables
- **WHEN** the overlay store is opened at `.synrepo/overlay/overlay.db`
- **THEN** no `files`, `symbols`, `concepts`, or `edges` tables exist in that database
