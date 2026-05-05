## ADDED Requirements

### Requirement: Surface cheap test-risk ranking signals
`TestEntry` SHALL include optional `risk_score` and `risk_reasons` fields. The score SHALL be derived from cheap graph and path signals at card compile time. Direct-call coverage SHALL rank above path-only association when both are available.

#### Scenario: Deep test surface includes direct-call risk reason
- **WHEN** a deep test-surface card finds a test with outbound `Calls` edges into the scoped source file
- **THEN** that entry includes a higher `risk_score`
- **AND** `risk_reasons` explains the direct-call signal

### Requirement: Surface estimated commentary freshness cheaply
Default status SHALL be allowed to show estimated commentary freshness fields without running the full commentary freshness scan. Exact `fresh` SHALL remain populated only when the full scan runs.

#### Scenario: Default status has an estimate but no exact freshness
- **WHEN** status runs without `--full` and commentary rows exist
- **THEN** `fresh` remains absent
- **AND** estimated freshness fields may be populated with a confidence label
