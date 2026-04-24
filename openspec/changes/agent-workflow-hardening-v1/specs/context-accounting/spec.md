## ADDED Requirements

### Requirement: Track observable workflow usage counters
Context accounting SHALL track observable workflow tool usage separately from estimated context-savings counters.

#### Scenario: Workflow tools are used
- **WHEN** an agent invokes orient, find, explain, impact, risks, tests, changed, or minimum-context through synrepo
- **THEN** context metrics can report per-tool usage counts
- **AND** those counts are labeled as observed synrepo calls

#### Scenario: Cold-read avoidance is estimated
- **WHEN** synrepo reports full-file-read avoidance or estimated raw tokens avoided
- **THEN** the metric is labeled as estimated from card accounting data
- **AND** it is not presented as proof that an external agent did not read files outside synrepo
