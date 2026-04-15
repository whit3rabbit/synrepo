## ADDED Requirements

### Requirement: Define edge-drift as a repair surface
synrepo SHALL define an edge-drift repair surface that reports graph edges with high structural drift scores and supports pruning edges whose source artifacts have been deleted.

#### Scenario: Check reports edges above drift threshold
- **WHEN** `synrepo check` runs after a structural compile that computed drift scores
- **THEN** edges with drift score >= 0.7 are reported as a drift class with the edge ID, drift score, and connected artifact paths
- **AND** edges with drift 1.0 are reported as requiring pruning (deleted artifact)

#### Scenario: Sync prunes edges at drift 1.0
- **WHEN** `synrepo sync` runs and edges exist with drift score 1.0 (one artifact deleted)
- **THEN** sync SHALL remove those edges from the graph store
- **AND** the repair log SHALL record each pruned edge with its drift score and deleted artifact reference

#### Scenario: Sync does not modify edges below drift 1.0
- **WHEN** `synrepo sync` runs and edges exist with drift score in (0.0, 1.0)
- **THEN** sync SHALL NOT remove or modify those edges
- **AND** those edges remain available for reporting via `synrepo check`

#### Scenario: Edge-drift surface reports absent when no drift scores exist
- **WHEN** `synrepo check` runs before any structural compile has computed drift scores
- **THEN** the edge-drift surface is reported as absent, not stale
