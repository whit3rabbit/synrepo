## MODIFIED Requirements

### Requirement: Provide task-first MCP tools
synrepo SHALL define an MCP surface centered on task-first tools for orientation, card lookup, where-to-edit, change impact, entrypoints, call paths, test surface, minimum context, and findings. The tool surface includes: `synrepo_symbol`, `synrepo_file`, `synrepo_entrypoints`, `synrepo_module`, `synrepo_call_path`, `synrepo_test_surface`, `synrepo_minimum_context`, and `synrepo_findings`.

#### Scenario: Route an edit from task language
- **WHEN** an agent asks where to edit for a task description
- **THEN** the MCP surface defines a task-first tool that returns bounded card-based results
- **AND** the tool contract does not require raw graph traversal knowledge from the caller
