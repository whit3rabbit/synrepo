## ADDED Requirements

### Requirement: Resolve MCP repository state explicitly
The MCP server SHALL resolve repository state from an optional default repository and an optional per-tool `repo_root` parameter. When `repo_root` is provided, the server SHALL canonicalize it, require that it is either the default repository or a registered managed project, prepare state for that repository, and return errors without falling back to another repository.

#### Scenario: Repo-bound MCP call omits repo_root
- **WHEN** the MCP server was started with a usable default repository and a repo-addressable tool omits `repo_root`
- **THEN** the tool uses the default repository state

#### Scenario: Global MCP call supplies registered repo_root
- **WHEN** a repo-addressable tool is called with `repo_root = "/work/app"` and `/work/app` is registered
- **THEN** the tool resolves and uses `/work/app` repository state

#### Scenario: Global MCP call omits repo_root with no default
- **WHEN** the MCP server has no usable default repository and a repo-addressable tool omits `repo_root`
- **THEN** the tool returns an explicit error that `repo_root` is required
- **AND** no other repository state is used

#### Scenario: Tool supplies unregistered repo_root
- **WHEN** a repo-addressable tool is called with a path that is not the default repository and is not registered
- **THEN** the tool returns an error explaining that the repository is not managed by synrepo
- **AND** the error names `synrepo project add <path>` as the remedy

#### Scenario: Requested repository cannot be prepared
- **WHEN** a requested registered repository is uninitialized, partial, or store-incompatible
- **THEN** the tool returns the preparation error for that repository
- **AND** the server does not fall back to the default repository

### Requirement: Allow MCP startup without a default repository
`synrepo mcp` SHALL be able to start from a non-repository working directory when it is intended to serve registered projects by explicit `repo_root`. Startup without a default repository SHALL NOT make any repository-addressable tool succeed unless the tool call supplies a resolvable `repo_root`.

#### Scenario: Global agent launches MCP from home directory
- **WHEN** an agent launches `synrepo mcp` from a directory that is not initialized with synrepo
- **THEN** the MCP server starts in defaultless mode
- **AND** repository data is served only after a tool call supplies a registered `repo_root`

#### Scenario: Explicit repo override is invalid
- **WHEN** the user launches `synrepo mcp --repo /work/app` and `/work/app` cannot be prepared
- **THEN** startup fails with the repository preparation error
- **AND** defaultless mode is not used to hide the explicit invalid override

### Requirement: Accept repo_root on repo-addressable MCP tools
Every MCP tool that reads or mutates repository-scoped synrepo state SHALL accept an optional `repo_root` parameter unless it is explicitly documented as server-default-only. Repo-addressable tools include card lookup, search, docs search, context pack, graph primitives, where-to-edit, impact/risk, entrypoints, notes, module/public API cards, workflow aliases, findings, recent activity, and edit-capable tools.

#### Scenario: Graph primitive routes by repo_root
- **WHEN** an agent calls `synrepo_edges` with a valid node ID and `repo_root = "/work/app"`
- **THEN** the edge traversal runs against `/work/app`

#### Scenario: Workflow alias routes by repo_root
- **WHEN** an agent calls a workflow alias such as `synrepo_find` with `repo_root = "/work/app"`
- **THEN** the workflow result and any per-repo metrics are associated with `/work/app`

#### Scenario: Tool lacks repo_root support
- **WHEN** a repository-scoped MCP tool cannot accept `repo_root`
- **THEN** the tool description SHALL state that it only uses the server default repository
- **AND** it SHALL return a clear error when no default repository exists
