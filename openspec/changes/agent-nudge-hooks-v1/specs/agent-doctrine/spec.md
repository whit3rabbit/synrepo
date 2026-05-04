## MODIFIED Requirements

### Requirement: Require synrepo-first context for agent workflows
Agents using synrepo SHALL prefer synrepo MCP tools or CLI fallback before cold source reads for orientation, codebase questions, file reviews, broad search, change impact, and pre-edit context when a `.synrepo/` directory is present.

#### Scenario: Agent receives a codebase question
- **WHEN** an agent is asked to answer a question about repository code
- **THEN** the documented default path starts with synrepo orientation, search or find, and bounded cards before full source reads

#### Scenario: Agent reviews files
- **WHEN** an agent is asked to review one or more files
- **THEN** the documented default path uses synrepo cards, minimum context, risks, and test discovery before forming conclusions

#### Scenario: Agent uses direct shell or file tools
- **WHEN** direct file, search, shell, or review tools are used first
- **THEN** hook guidance MAY remind the agent to use synrepo first
- **AND** the guidance remains client-side and advisory, not MCP server interception
