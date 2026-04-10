## Purpose
Define the optional human-guidance layer for patterns, ADRs, inline rationale, and DecisionCards without making prose mandatory for value.

## Requirements

### Requirement: Define optional human-guidance inputs
synrepo SHALL define optional patterns, ADRs, and inline rationale markers as a human-guidance layer that enriches routing and explanation without becoming mandatory for value.

#### Scenario: Use synrepo on a code-only repository
- **WHEN** a repository has no pattern documents or ADRs
- **THEN** the product remains useful through structural cards alone
- **AND** the absence of rationale artifacts does not block core workflows

### Requirement: Define promotion rules for rationale material
synrepo SHALL define how rationale sources are ingested, linked, and promoted in curated workflows, including the conditions under which they become graph-backed declarations.

#### Scenario: Promote a rationale source
- **WHEN** a curated workflow turns a rationale source into a stronger declaration
- **THEN** the rules require human-authored source material as the promotion basis
- **AND** machine-authored overlay content is not promoted directly into the graph

### Requirement: Define DecisionCard behavior
synrepo SHALL define DecisionCards as optional outputs backed by human-authored rationale sources and linked to structural cards without overriding descriptive truth.

#### Scenario: Ask why a subsystem exists
- **WHEN** an agent requests rationale for a target with linked human-authored decision material
- **THEN** the system can return a DecisionCard
- **AND** the response distinguishes rationale from current code behavior
