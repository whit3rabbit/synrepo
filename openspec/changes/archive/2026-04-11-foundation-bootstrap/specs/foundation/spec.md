## ADDED Requirements

### Requirement: Use OpenSpec as the planning layer
synrepo SHALL use OpenSpec artifacts to capture enduring product behavior and active changes without treating them as runtime truth or the primary end-user surface.

#### Scenario: Create a new roadmap-aligned change
- **WHEN** a contributor proposes a future synrepo feature
- **THEN** the proposal is created under `openspec/changes/<change-name>/`
- **AND** the lasting behavior it changes is represented in `openspec/specs/<capability>/spec.md`

### Requirement: Define repository artifact boundaries
synrepo SHALL define what belongs in `openspec/specs/`, `openspec/changes/`, `docs/`, and `.synrepo/` so contributors can place planning, reference, and runtime artifacts consistently.

#### Scenario: Decide where to record new information
- **WHEN** a contributor needs to add a contract, proposal, reference doc, or runtime state
- **THEN** the repository boundary rules identify the correct home for that information
- **AND** the change does not have to re-argue the storage model
