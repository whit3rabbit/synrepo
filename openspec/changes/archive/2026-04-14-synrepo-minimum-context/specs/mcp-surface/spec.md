## ADDED Requirements

### Requirement: Expose synrepo_minimum_context as a task-first MCP tool
synrepo SHALL expose `synrepo_minimum_context` as a task-first MCP tool that returns a budget-bounded 1-hop neighborhood around a focal symbol or file. The tool SHALL accept parameters: `target` (node ID or qualified path, required), `budget` (`tiny`, `normal`, `deep`, default `normal`). The response SHALL follow the minimum-context spec contract: focal card, structural neighbor summaries or full cards depending on budget, governing decisions, and co-change partners.

#### Scenario: Tool registration appears in MCP capabilities
- **WHEN** an MCP client connects and enumerates available tools
- **THEN** `synrepo_minimum_context` appears in the tool list alongside the existing task-first tools
- **AND** the tool description indicates it returns a budget-bounded neighborhood for a focal node

#### Scenario: Default budget is normal
- **WHEN** an agent invokes `synrepo_minimum_context` without specifying a budget
- **THEN** the tool uses `normal` budget as the default
- **AND** the response includes structural neighbor summaries and top-3 co-change partners
