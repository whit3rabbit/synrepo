## ADDED Requirements

### Requirement: Guide agents to pass repo_root for global MCP use
Generated agent guidance SHALL explain that a global synrepo MCP integration serves registered projects by absolute repository path. In global or defaultless contexts, agents SHALL pass the current workspace's absolute path as `repo_root` to repo-addressable tools.

#### Scenario: Agent reads global integration guidance
- **WHEN** an agent reads generated synrepo doctrine or a generated shim after global integration support exists
- **THEN** the guidance tells the agent to pass the current workspace root as `repo_root` when using a global MCP server
- **AND** the guidance preserves the existing orient, find, impact, edit, tests, changed workflow

#### Scenario: Repository is not registered
- **WHEN** a global MCP tool reports that a repository is not managed by synrepo
- **THEN** the guidance tells the agent to ask the user to run `synrepo project add <path>`
- **AND** the guidance does not imply the agent should bypass registry gating

### Requirement: Preserve repo-bound default behavior in doctrine
Generated agent guidance SHALL state that repo-bound MCP configurations may omit `repo_root` because the server has a default repository, but passing the absolute repository root remains valid and preferred when an agent can identify it reliably.

#### Scenario: Agent uses project-scoped MCP config
- **WHEN** an agent is operating through a project-scoped MCP config that launches `synrepo mcp --repo .`
- **THEN** the guidance permits omitting `repo_root`
- **AND** it does not contradict the global guidance to pass `repo_root` when using a global server
