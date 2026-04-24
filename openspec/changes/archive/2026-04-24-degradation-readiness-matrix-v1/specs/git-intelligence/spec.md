## ADDED Requirements

### Requirement: Map git intelligence availability to readiness
Git intelligence SHALL report readiness states that distinguish ready history, degraded history, unavailable git context, and intentionally absent git usage.

#### Scenario: Repository has no git context
- **WHEN** synrepo cannot open git context for the repository
- **THEN** the readiness matrix marks git intelligence as unavailable
- **AND** graph-backed parser facts remain usable without git-derived ownership or co-change claims

#### Scenario: History is shallow or degraded
- **WHEN** git context exists but history is shallow, detached, or missing sampled touches
- **THEN** the readiness matrix marks git intelligence as degraded
- **AND** cards that depend on git labels expose the same degraded state rather than inventing ownership or co-change facts
