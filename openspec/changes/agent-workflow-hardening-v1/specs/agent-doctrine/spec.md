## ADDED Requirements

### Requirement: Require bounded-context workflow guidance
The canonical agent doctrine SHALL state the preferred workflow: orient first, find bounded cards, inspect impact or risks before edits, validate tests, and check changed context before claiming completion.

#### Scenario: Generated shim includes workflow guidance
- **WHEN** synrepo generates or regenerates an agent shim
- **THEN** the shim includes the bounded-context workflow guidance
- **AND** the guidance tells agents to use full-file reads only after cards identify the relevant target or when bounded cards are insufficient

#### Scenario: Doctrine remains source-truth safe
- **WHEN** the workflow guidance mentions overlay notes, commentary, or advisory content
- **THEN** it states that graph-backed structural facts remain authoritative
- **AND** it does not imply overlay content can define source truth
