## ADDED Requirements

### Requirement: Provide a doctor aggregation command
synrepo SHALL provide a `synrepo doctor` command that consumes the shared status snapshot and reports only components whose severity is not `Healthy`. The command SHALL exit zero when every component is healthy and non-zero when any component is stale or blocked.

#### Scenario: Healthy repo exits zero
- **WHEN** an operator runs `synrepo doctor` on a repository whose status snapshot reports every component as `Healthy`
- **THEN** the command prints a short confirmation line
- **AND** the command exits with status 0

#### Scenario: Degraded repo exits non-zero
- **WHEN** an operator runs `synrepo doctor` on a repository whose status snapshot reports at least one stale or blocked component
- **THEN** the command prints a compact list naming each degraded component, its severity, and the recommended action
- **AND** the command exits with a non-zero status

#### Scenario: JSON output for CI
- **WHEN** an operator runs `synrepo doctor --json`
- **THEN** the command prints a structured JSON report of degraded components
- **AND** on a healthy repository the JSON report contains an empty degraded list and the command still exits zero

#### Scenario: Single source of truth with status
- **WHEN** a new field is added to the shared status snapshot
- **THEN** `synrepo doctor` surfaces it via the same severity-based filter used by the dashboard without duplicating status logic
