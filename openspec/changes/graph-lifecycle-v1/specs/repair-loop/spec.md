## Delta: graph-lifecycle-v1

### Requirement: Define retired observations as a repair surface
synrepo SHALL define `retired_observations` as a named repair surface for graph facts that have been soft-retired but not yet compacted. When the count of retired symbols or edges exceeds a reporting threshold, `synrepo check` SHALL report the surface with recommended action `compact_retired`. When `synrepo sync` processes this action, it SHALL run the compaction pass using the configured retention window.

#### Scenario: Check reports retired observation accumulation
- **WHEN** `synrepo check` runs and the graph contains retired symbols or edges
- **THEN** a drift finding is produced with surface `retired_observations`, drift class `current`, and the count of retired facts
- **AND** when the count exceeds the reporting threshold, the recommended action is `compact_retired`

#### Scenario: Sync compacts retired observations
- **WHEN** `synrepo sync` processes a `compact_retired` action
- **THEN** synrepo runs the compaction pass for retired facts older than `retain_retired_revisions`
- **AND** the resolution log records the number of symbols, edges, and sidecar rows removed
- **AND** the graph store is modified only by removing retired facts, not active observations

#### Scenario: Compaction does not affect other repair surfaces
- **WHEN** `synrepo sync` processes a `compact_retired` action
- **THEN** the overlay store is not modified
- **AND** other repair surfaces (exports, commentary, proposed links) are reported independently
