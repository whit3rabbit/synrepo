## ADDED Requirements

### Requirement: Describe layered context artifacts
The foundation SHALL describe synrepo as a local code-context compiler that turns repository files into canonical graph facts, compiles those facts into code artifacts, bundles artifacts into task contexts, and serves those contexts through cards and MCP. This framing SHALL preserve cards as the current primary delivery packet while clarifying that the graph is infrastructure and artifacts and contexts are the product abstraction.

#### Scenario: Reader learns synrepo's product model
- **WHEN** a contributor reads the foundation document or foundation spec
- **THEN** they can identify the layers `repo files`, `graph facts`, `code artifacts`, `task contexts`, and `cards/MCP`
- **AND** they can tell which layers are canonical, compiled, bundled, or delivered

#### Scenario: Runtime behavior remains unchanged
- **WHEN** the framing language is updated
- **THEN** no foundation requirement implies a new MCP tool, new storage surface, new background job, or changed trust boundary
