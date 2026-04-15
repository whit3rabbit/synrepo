## ADDED Requirements

### Requirement: Expose synrepo_recent_activity as a task-first MCP tool
synrepo SHALL expose `synrepo_recent_activity` as a task-first MCP tool per the `recent-activity` spec contract. The tool SHALL appear in the MCP capabilities list alongside the other operational tools. The full behavioral contract is defined in `openspec/specs/recent-activity/spec.md`.

#### Scenario: Tool registration appears in MCP capabilities
- **WHEN** an MCP client connects and enumerates available tools
- **THEN** `synrepo_recent_activity` appears in the tool list
- **AND** the tool description indicates it returns a bounded lane of synrepo operational events

#### Scenario: Tool is not a session-memory or agent-history surface
- **WHEN** `synrepo_recent_activity` is invoked
- **THEN** the response contains only synrepo's own operational events (reconcile, repair, cross-link, overlay, hotspot)
- **AND** no agent identity, prompt content, or inter-session interaction data appears in any response field
