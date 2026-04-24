## ADDED Requirements

### Requirement: Use canonical doctrine for shim freshness checks
Generated shim freshness SHALL be evaluated against the canonical agent doctrine and current target-specific template content.

#### Scenario: Doctrine block changes
- **WHEN** the canonical doctrine block changes after a shim was generated
- **THEN** setup or agent-setup can classify the existing shim as stale
- **AND** the report points to `--regen` or the existing regeneration flow rather than embedding divergent doctrine text

#### Scenario: Shim is current
- **WHEN** an existing generated shim matches the current canonical doctrine and target template
- **THEN** setup reports the shim as current
- **AND** no write is performed for that shim
