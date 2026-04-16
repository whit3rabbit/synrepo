## ADDED Requirements

### Requirement: Define compact-specific maintenance actions
synrepo SHALL define compact-specific maintenance actions that extend the existing compatibility-driven maintenance model. These actions target overlay commentary retention, cross-link audit summarization, repair-log rotation, lexical index rebuild, and SQLite WAL checkpointing.

#### Scenario: Plan a compact pass
- **WHEN** a user runs `synrepo compact` without `--apply`
- **THEN** the maintenance planner evaluates each store against the selected policy and produces a plan listing compactable commentary entries, summarizable cross-link audit rows, rotatable repair-log entries, and whether a WAL checkpoint or index rebuild is warranted
- **AND** no stores are mutated
- **AND** the plan includes estimated row counts for each action

#### Scenario: Execute a compact pass
- **WHEN** a user runs `synrepo compact --apply`
- **THEN** each planned action is executed in order: overlay commentary compaction, cross-link audit summarization, repair-log rotation, index rebuild (if warranted), and WAL checkpoint on both databases
- **AND** the command exits zero if all actions succeed
- **AND** a summary of rows affected per action is printed

### Requirement: Define compact policy presets
synrepo SHALL define three compact policy presets (`default`, `aggressive`, `audit-heavy`) with hard-coded retention thresholds. Policy selection SHALL be explicit via `--policy` flag, defaulting to `default`.

#### Scenario: Apply default policy
- **WHEN** a compact pass runs with `default` policy
- **THEN** stale commentary entries older than 30 days are dropped, active commentary is retained, cross-link audit rows for promoted or rejected candidates older than 90 days are summarized into counts, repair-log entries older than 30 days are summarized into a header line, and a WAL checkpoint runs on both databases

#### Scenario: Apply aggressive policy
- **WHEN** a compact pass runs with `aggressive` policy
- **THEN** stale commentary entries older than 7 days are dropped, cross-link audit rows older than 30 days are summarized, and repair-log entries older than 7 days are summarized

#### Scenario: Apply audit-heavy policy
- **WHEN** a compact pass runs with `audit-heavy` policy
- **THEN** stale commentary entries older than 90 days are dropped, cross-link audit rows are retained in full for 180 days before summarization, and repair-log entries older than 90 days are summarized

### Requirement: Compaction SHALL NOT touch canonical graph rows
synrepo SHALL enforce that the compact command never modifies or deletes rows in the canonical graph store (`nodes.db`). This includes nodes, edges, and provenance records.

#### Scenario: Verify graph integrity after compaction
- **WHEN** a compact pass completes
- **THEN** all canonical graph rows (FileNode, SymbolNode, ConceptNode, Edge, Provenance) are unchanged
- **AND** a test exists that asserts row counts and content are identical before and after compaction

### Requirement: Define overlay store compaction trait methods
synrepo SHALL extend the `OverlayStore` trait with methods for querying compactable row counts and executing commentary and cross-link compaction according to the supplied policy.

#### Scenario: Query compactable commentary stats
- **WHEN** the maintenance planner needs compactable commentary statistics
- **THEN** the overlay store returns counts of stale entries within and beyond the policy's retention window, grouped by staleness age

#### Scenario: Execute commentary compaction
- **WHEN** a compact pass executes commentary compaction
- **THEN** the overlay store drops stale commentary entries older than the policy threshold and retains all active entries
- **AND** the returned summary includes the count of entries dropped

#### Scenario: Execute cross-link audit summarization
- **WHEN** a compact pass executes cross-link audit compaction
- **THEN** the overlay store summarizes promoted and rejected audit rows older than the policy threshold into aggregate counts and drops the individual rows
- **AND** active candidates are never summarized or dropped by compaction
- **AND** the returned summary includes the count of rows summarized and dropped
