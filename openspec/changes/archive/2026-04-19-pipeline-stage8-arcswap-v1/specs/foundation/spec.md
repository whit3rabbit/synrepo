## ADDED Requirements

### Requirement: Publish an immutable graph snapshot after structural compile
synrepo SHALL rebuild and atomically publish an immutable in-memory graph snapshot after each successful structural compile commit. The snapshot SHALL be derived from committed SQLite graph state, and readers SHALL observe either the previously published snapshot or the newly published snapshot, never a partial compile.

#### Scenario: Publish a new snapshot after a successful compile
- **WHEN** stages 1 through 7 of the structural pipeline commit successfully
- **THEN** stage 8 rebuilds a full in-memory graph snapshot from the committed graph state and atomically publishes it
- **AND** read-path consumers can observe a consistent snapshot epoch for the duration of a request

#### Scenario: Retain the previous snapshot when compile fails
- **WHEN** a structural compile fails before the SQLite commit completes
- **THEN** synrepo does not publish a replacement snapshot
- **AND** readers continue to observe the last successfully published snapshot

### Requirement: Keep SQLite authoritative when snapshots are enabled
synrepo SHALL treat the in-memory graph snapshot as a derived optimization, not as the authoritative store. Mutations SHALL continue to write SQLite first, and the snapshot SHALL remain rebuildable from SQLite alone after process restart.

#### Scenario: Rebuild snapshot state after process restart
- **WHEN** a new synrepo process starts with no in-memory snapshot populated yet
- **THEN** it can rebuild and republish the snapshot from the persisted SQLite graph
- **AND** no canonical graph fact depends on persisting the in-memory snapshot itself

### Requirement: Bound snapshot memory with operator controls
synrepo SHALL expose an operator control for the maximum in-memory snapshot size and SHALL warn when a published snapshot exceeds that advisory ceiling. Setting the ceiling to `0` SHALL disable snapshot publication and keep read paths on SQLite.

#### Scenario: Snapshot exceeds the advisory ceiling
- **WHEN** stage 8 builds a snapshot larger than the configured `max_graph_snapshot_bytes`
- **THEN** synrepo emits a warning naming the snapshot size and graph counts
- **AND** it still publishes the snapshot unless the configured ceiling is `0`

#### Scenario: Operator disables snapshot publication
- **WHEN** `max_graph_snapshot_bytes` is set to `0`
- **THEN** synrepo skips publishing the in-memory snapshot
- **AND** read-path consumers fall back to SQLite-backed graph reads
