## ADDED Requirements

### Requirement: Report degraded capabilities after bootstrap
Bootstrap success and repair output SHALL report degraded optional and core capabilities using the same readiness labels as runtime probe.

#### Scenario: Bootstrap completes with degraded optional capability
- **WHEN** bootstrap succeeds but git history or embeddings are unavailable
- **THEN** the success output reports the capability as unavailable or disabled with the relevant next action
- **AND** core graph readiness remains successful when source-derived graph operation is usable

#### Scenario: Bootstrap completes with partial core capability
- **WHEN** bootstrap completes but parser failures or stale index state limit graph coverage
- **THEN** the output reports the degraded capability and next action
- **AND** it does not claim full readiness for the affected surface
