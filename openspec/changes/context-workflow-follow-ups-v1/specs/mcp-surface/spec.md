## MODIFIED Requirements

### Requirement: Expose workflow aliases
synrepo SHALL expose MCP workflow aliases for orienting, finding, explaining, impact inspection, risk shorthand, test discovery, and changed-context review. The `synrepo_risks` alias SHALL return the same bounded context as `synrepo_impact` so agents who follow the CLI doctrine find a matching MCP tool.

#### Scenario: Agent follows the workflow aliases
- **WHEN** an agent calls `synrepo_orient`, `synrepo_find`, `synrepo_explain`, `synrepo_impact`, `synrepo_risks`, `synrepo_tests`, or `synrepo_changed`
- **THEN** each alias returns bounded graph-backed or explicitly labeled overlay-backed context
- **AND** existing MCP tools remain available unchanged

#### Scenario: Agent calls synrepo_risks and synrepo_impact with the same target
- **WHEN** an agent invokes `synrepo_risks` and `synrepo_impact` with identical `target` and `budget` values on a stable repository state
- **THEN** both tools return byte-identical content
- **AND** both tools share the same accounting metadata
