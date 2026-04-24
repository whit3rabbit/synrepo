## ADDED Requirements

### Requirement: Render capability readiness states
The dashboard SHALL render capability readiness rows from the shared matrix rather than running independent subsystem checks.

#### Scenario: Readiness matrix contains degraded rows
- **WHEN** the dashboard receives readiness rows for parser failures, stale index, missing git, disabled embeddings, disabled watch, or unavailable overlay
- **THEN** it displays each row with state, severity, and next action
- **AND** the dashboard preserves the distinction between disabled, unavailable, degraded, stale, and blocked
