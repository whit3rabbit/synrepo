## ADDED Requirements

### Requirement: Card-returning MCP tool descriptions name the escalation default
synrepo SHALL include a single, consistent escalation-default sentence in the `description` field of every card-returning MCP tool (`synrepo_card`, `synrepo_module_card`, `synrepo_public_api`, `synrepo_minimum_context`, `synrepo_entrypoints`, `synrepo_where_to_edit`, `synrepo_change_impact`). The sentence SHALL be sourced from a shared compile-time constant tied to the canonical agent-doctrine block so wording cannot drift per tool.

#### Scenario: Agent enumerates tools
- **WHEN** an MCP client connects and retrieves the tool list
- **THEN** every card-returning tool's description ends with the same escalation-default sentence
- **AND** non-card tools (`synrepo_search`, `synrepo_findings`, `synrepo_recent_activity`, `synrepo_overview`) do not include the escalation-default sentence because their default-budget semantics differ

#### Scenario: Shared constant prevents drift
- **WHEN** a contributor edits the escalation sentence in one tool description directly
- **THEN** the compiled tool descriptions diverge from the shared constant
- **AND** the shims test or a dedicated MCP description test fails, blocking the change
