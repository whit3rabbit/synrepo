## ADDED Requirements

### Requirement: Register projects during setup
`synrepo setup <tool>` and `synrepo setup <tool> --global` SHALL ensure the current repository is recorded in the user-level project registry after initialization succeeds. Setup SHALL preserve existing registry metadata and SHALL NOT record a project whose initialization or readiness step fails.

#### Scenario: Scripted setup registers project
- **WHEN** the user runs `synrepo setup claude` in a repository that initializes successfully
- **THEN** synrepo records the repository in `~/.synrepo/projects.toml`
- **AND** repeated setup does not create duplicate project entries

#### Scenario: Setup readiness fails
- **WHEN** setup cannot initialize or prepare the repository for normal operation
- **THEN** synrepo reports the setup failure
- **AND** it does not add a new managed project entry for the failed repository

### Requirement: Configure global MCP entries without a repo flag
When setup performs global MCP registration for a supported agent target, it SHALL write a user-scoped agent configuration that launches `synrepo mcp` without `--repo .`. Project-scoped setup SHALL continue to write a repository-scoped entry that launches `synrepo mcp --repo .`.

#### Scenario: Configure supported global MCP target
- **WHEN** the user runs `synrepo setup claude --global`
- **THEN** synrepo writes or updates the user-scoped Claude MCP config
- **AND** the `synrepo` server command launches `synrepo mcp`
- **AND** the current repository is registered as a managed project

#### Scenario: Configure project-scoped MCP target
- **WHEN** the user runs `synrepo setup claude` without `--global`
- **THEN** synrepo writes or updates the project-scoped MCP config
- **AND** the `synrepo` server command launches `synrepo mcp --repo .`

### Requirement: Report unsupported global targets clearly
If an agent target does not have a supported user-scoped MCP configuration writer, `synrepo setup <tool> --global` SHALL report that global MCP registration is unsupported for that target. It SHALL NOT silently write a project-scoped MCP entry while claiming global setup.

#### Scenario: Global setup target lacks writer
- **WHEN** the user runs `synrepo setup <tool> --global` for a target without a supported global MCP writer
- **THEN** synrepo reports that global MCP registration is unsupported for that target
- **AND** no project-scoped MCP config is written as a substitute

#### Scenario: Multi-client global setup has mixed support
- **WHEN** the user runs `synrepo setup --only claude,codex --global` and one target lacks global support
- **THEN** synrepo reports per-client outcomes
- **AND** supported targets are configured globally while unsupported targets are reported as unsupported or failed
