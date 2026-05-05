## ADDED Requirements

### Requirement: Score cross-link candidates through explicit rank features
Cross-link generation SHALL score candidates through an explicit `RankFeatures` module. The initial scorer SHALL preserve the current score and tier boundaries. It SHALL NOT depend on `TriageSource` until that source is persisted and revalidation can reproduce it.

#### Scenario: Revalidation matches generation scoring
- **WHEN** a candidate is revalidated with the same spans and graph distance
- **THEN** the scorer computes the same score and tier as generation
- **AND** semantic-versus-deterministic triage source is not required to reproduce the score
