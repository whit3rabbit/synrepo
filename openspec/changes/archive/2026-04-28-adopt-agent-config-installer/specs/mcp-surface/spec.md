## ADDED Requirements

### Requirement: MCP server registration is performed via the agent-config installer
The synrepo CLI SHALL register the `synrepo` MCP server in agent harness configurations exclusively through the `agent-config` installer (`McpSpec` + `mcp_by_id(<id>).install_mcp(<scope>, <spec>)`). The installed entry SHALL run the `synrepo` binary directly (no node, npx, uv, or wrapper indirection). For global scope the spec SHALL pass no `--repo` argument; for project scope the spec SHALL pass `--repo .` so the server resolves to the configured repository. The owner tag for every MCP install written by synrepo SHALL be the literal string `"synrepo"`.

#### Scenario: Global MCP install for Claude
- **WHEN** synrepo registers the MCP server globally for Claude
- **THEN** the installer writes an entry with `command = "synrepo"` and `args = ["mcp"]`
- **AND** the install is owned by tag `"synrepo"`

#### Scenario: Project-scoped MCP install for Codex
- **WHEN** synrepo registers the MCP server project-scoped for Codex
- **THEN** the installer writes an entry with `command = "synrepo"` and `args = ["mcp", "--repo", "."]` under `mcp_servers.synrepo`
- **AND** the install is owned by tag `"synrepo"`

#### Scenario: Repeated registration is idempotent
- **WHEN** synrepo registers the MCP server twice with the same scope and content
- **THEN** the second call reports `already_installed = true`
- **AND** no file content changes on disk

### Requirement: MCP install scope coverage tracks installer support
The set of harnesses for which `synrepo setup` automates MCP registration SHALL be derived at runtime from `agent_config::mcp_by_id(<id>).is_some()` and that integration's `supported_scopes()`. synrepo SHALL NOT maintain a parallel hand-coded list of "automated" vs "shim-only" tiers for MCP registration. Harnesses that the installer does not support for a given scope SHALL be reported to the operator with the recommended fallback (project-scoped install, manual configuration, or unsupported).

#### Scenario: New installer-supported harness becomes automated
- **WHEN** a new agent harness gains MCP support in the agent-config crate
- **THEN** updating the synrepo dependency surfaces that harness for `synrepo setup`
- **AND** no per-harness MCP writer is added to synrepo

#### Scenario: Installer reports an unsupported scope
- **WHEN** `synrepo setup` is invoked for a harness that supports only one scope
- **THEN** synrepo selects the supported scope or reports the limitation before writing anything
- **AND** the operator is shown how to override the default scope

### Requirement: Inline-secret refusal is surfaced as a setup error
If a future synrepo MCP spec ever supplies an environment value to the installer in a way the installer would refuse (for example `InlineSecretInLocalScope`), `synrepo setup` SHALL surface the refusal as a setup error with the offending key name and SHALL NOT bypass the installer's secret policy. synrepo's own MCP server takes no secrets today, so the default path SHALL pass no inline secrets; this requirement governs future extensions.

#### Scenario: Refused inline secret aborts setup
- **WHEN** an MCP install would write an inline secret refused by the installer
- **THEN** `synrepo setup` aborts with the integration ID and env-key name
- **AND** no partial config is left on disk
