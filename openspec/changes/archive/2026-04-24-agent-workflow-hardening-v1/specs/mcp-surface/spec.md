## ADDED Requirements

### Requirement: Expose workflow guidance in MCP descriptions
The MCP server SHALL expose concise workflow guidance in server info and relevant task-first tool descriptions.

#### Scenario: MCP client lists tools
- **WHEN** an MCP client enumerates synrepo tools
- **THEN** task-first tools such as orient, find, explain, impact, risks, tests, changed, and minimum-context include concise guidance about bounded context and escalation
- **AND** descriptions remain short enough to avoid bloating tool-list responses

#### Scenario: MCP server info is requested
- **WHEN** a client requests synrepo server info or instructions
- **THEN** the response names the orient, find, impact or risks, edit, tests, changed workflow
- **AND** it tells agents to read full files only after card routing or explicit insufficiency
