## ADDED Requirements

### Requirement: Describe context-pack as current batched context delivery
The MCP surface SHALL describe `synrepo_context_pack` as the current read-only batching tool for known files, symbols, directories, tests, call paths, searches, and minimum-context artifacts. This description SHALL NOT rename the tool, change its target schema, or introduce `synrepo_ask`.

#### Scenario: Agent reads MCP docs
- **WHEN** an agent reads the MCP guide
- **THEN** `synrepo_context_pack` is described as the current batched card/context delivery surface
- **AND** the docs do not imply that a new high-level ask tool already exists

#### Scenario: MCP tools are listed
- **WHEN** an MCP client retrieves available tools
- **THEN** the tool set remains unchanged by this framing change
- **AND** existing request and response contracts remain unchanged
