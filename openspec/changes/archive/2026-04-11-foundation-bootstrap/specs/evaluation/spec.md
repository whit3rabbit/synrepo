## ADDED Requirements

### Requirement: Foundation changes must be structurally validated
synrepo SHALL treat OpenSpec workspace validation as a required check for planning-foundation changes so the spec spine is usable before feature work begins.

#### Scenario: Finish the foundation bootstrap pass
- **WHEN** the initial planning foundation is created
- **THEN** the workspace is validated with OpenSpec CLI checks for specs and the active change
- **AND** future contributors inherit a structurally sound planning surface
