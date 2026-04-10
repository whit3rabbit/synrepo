## Purpose
Define the task-first MCP tools and response contracts that expose card-based synrepo behavior to coding agents.

## Requirements

### Requirement: Provide task-first MCP tools
synrepo SHALL define an MCP surface centered on task-first tools for orientation, card lookup, where-to-edit, change impact, entrypoints, call paths, test surface, minimum context, and findings.

#### Scenario: Route an edit from task language
- **WHEN** an agent asks where to edit for a task description
- **THEN** the MCP surface defines a task-first tool that returns bounded card-based results
- **AND** the tool contract does not require raw graph traversal knowledge from the caller

### Requirement: Require provenance and freshness in responses
synrepo SHALL define MCP response contracts that expose provenance, source-store labeling, and freshness behavior for all graph-backed and overlay-backed content.

#### Scenario: Consume mixed-source context
- **WHEN** an MCP response contains both structural facts and optional overlay data
- **THEN** the caller can distinguish the source and freshness of each returned field
- **AND** the contract prevents silent trust escalation

### Requirement: Default to minimal truthful context
synrepo SHALL define minimal-context behavior as the default for MCP responses, with budget-controlled escalation for deeper inspection.

#### Scenario: First call on an unfamiliar codebase
- **WHEN** an agent requests project orientation without specifying a deep read
- **THEN** the MCP surface returns the smallest useful context first
- **AND** it provides a defined path to request more detail when needed
