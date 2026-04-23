## ADDED Requirements

### Requirement: Expose workflow aliases
synrepo SHALL expose MCP workflow aliases for orienting, finding, explaining, impact inspection, test discovery, and changed-context review.

#### Scenario: Agent follows the workflow aliases
- **WHEN** an agent calls `synrepo_orient`, `synrepo_find`, `synrepo_explain`, `synrepo_impact`, `synrepo_tests`, or `synrepo_changed`
- **THEN** each alias returns bounded graph-backed or explicitly labeled overlay-backed context
- **AND** existing MCP tools remain available unchanged

### Requirement: Accept optional numeric caps on card aliases
MCP card and workflow aliases SHALL accept `budget_tokens` where the response contains a set of cards.

#### Scenario: Agent supplies budget_tokens
- **WHEN** an agent invokes a card-set MCP alias with `budget_tokens`
- **THEN** synrepo treats that value as a hard response ceiling where the tool can safely truncate ranked results
- **AND** the returned accounting metadata reports whether truncation occurred
