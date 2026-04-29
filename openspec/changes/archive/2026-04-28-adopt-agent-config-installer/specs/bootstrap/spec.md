## MODIFIED Requirements

### Requirement: Configure global MCP entries without a repo flag
When setup performs MCP registration for a supported agent target, it SHALL default to a user-scoped (global) agent configuration that launches `synrepo mcp` without `--repo .` whenever the target supports `Scope::Global` (as reported by the underlying installer). Project-scoped setup SHALL remain available via an explicit `--project` opt-in and SHALL write a repository-scoped entry that launches `synrepo mcp --repo .`. The setup flow SHALL persist the chosen scope so re-running setup is idempotent in either mode.

#### Scenario: Default global setup for a supported target
- **WHEN** the user runs `synrepo setup claude`
- **THEN** synrepo writes or updates the user-scoped Claude MCP config
- **AND** the `synrepo` server command launches `synrepo mcp`
- **AND** the current repository is registered as a managed project

#### Scenario: Explicit project-scoped setup
- **WHEN** the user runs `synrepo setup claude --project`
- **THEN** synrepo writes or updates the project-scoped MCP config
- **AND** the `synrepo` server command launches `synrepo mcp --repo .`

#### Scenario: Re-run yields no diff when scope is unchanged
- **WHEN** the user re-runs `synrepo setup claude` with the same scope as a prior install
- **THEN** synrepo reports the install as already current
- **AND** no file content changes on disk

### Requirement: Report unsupported global targets clearly
If an agent target is not supported by the installer at `Scope::Global`, `synrepo setup <tool>` SHALL detect this before any write and either fall back to project-scoped registration with an explicit notice, or refuse with a clear message that names the unsupported target. It SHALL NOT silently write a project-scoped MCP entry while claiming global setup.

#### Scenario: Global setup target lacks writer
- **WHEN** the user runs `synrepo setup <tool>` for a target without supported global MCP registration
- **THEN** synrepo reports that global MCP registration is unsupported for that target
- **AND** the operator is shown how to opt into project-scoped setup with `--project`
- **AND** no project-scoped MCP config is written without that explicit opt-in

#### Scenario: Multi-client default-global setup has mixed support
- **WHEN** the user runs `synrepo setup --only claude,codex` (default global)
- **THEN** synrepo reports per-client outcomes
- **AND** targets with global support are configured globally while targets without it are reported as unsupported or project-scoped per the operator's explicit choice

## ADDED Requirements

### Requirement: Delegate agent integration writes to the agent-config installer
synrepo SHALL delegate MCP server registration, agent skill placement, and agent instruction placement to the `agent-config` crate's installer surface (`McpSpec`/`SkillSpec`/`InstructionSpec` plus the `mcp_by_id`/`skill_by_id`/`instruction_by_id` registries). The installer SHALL be invoked with `owner = "synrepo"` so subsequent `synrepo remove` operations are scoped by ownership tag and cannot disturb other consumers' entries. The installer's atomicity guarantees (write-to-temp-and-rename, first-touch `.bak` backup, idempotent re-install, ownership ledger) SHALL be preserved at the synrepo boundary; synrepo SHALL NOT bypass the installer with hand-rolled JSON or TOML edits for the same surfaces.

#### Scenario: Setup writes through the installer
- **WHEN** the user runs `synrepo setup` for any supported target
- **THEN** the resulting MCP, skill, or instruction file changes are produced by the agent-config installer
- **AND** any pre-existing target file the installer modified has a single `<path>.bak` sibling created on first touch

#### Scenario: Removal is owner-scoped
- **WHEN** the user runs `synrepo remove <tool>` for a target previously installed by `synrepo setup`
- **THEN** synrepo invokes the installer's uninstall path keyed on `(name = "synrepo", owner = "synrepo")`
- **AND** unrelated entries belonging to other owners or other server names are preserved

#### Scenario: Re-install after a change to spec content updates the file
- **WHEN** the user re-runs setup after the doctrine or shim content changes
- **THEN** the installer reports the file as patched (not already-installed)
- **AND** the on-disk file matches the new spec content byte-for-byte

### Requirement: Surface installer-reported file paths in setup output
`synrepo setup` SHALL surface the absolute path of every file created or patched during the run, sourced from the installer's report rather than from a synrepo-side hard-coded table. Output SHALL distinguish created from patched targets and SHALL identify each by display name.

#### Scenario: Setup prints created and patched paths
- **WHEN** `synrepo setup claude` results in a created MCP entry and a patched skill file
- **THEN** the output names both files by their absolute path
- **AND** the labels distinguish "created" vs "patched"

### Requirement: Migrate pre-existing installs without ownership markers
`synrepo upgrade --apply` SHALL detect MCP, skill, or instruction targets that were written by an earlier synrepo version (no `_agent_config_tag` marker, no ownership ledger entry) and offer to adopt them by replaying the install through the agent-config installer with `owner = "synrepo"`. Adoption SHALL be idempotent and SHALL refuse to clobber a target whose content does not match the current synrepo spec without an explicit confirmation step.

#### Scenario: Legacy install adopted on upgrade
- **WHEN** the user runs `synrepo upgrade --apply` against a repository whose `.mcp.json` contains a `synrepo` entry written before this change
- **THEN** the upgrade replays the install through the installer, adding the `_agent_config_tag` marker and the ownership ledger entry
- **AND** subsequent `synrepo remove` succeeds without manual editing

#### Scenario: Legacy content differs from current spec
- **WHEN** the legacy entry's content does not match what the current synrepo would write
- **THEN** the upgrade reports the divergence and exits non-zero unless the operator passes a confirmation flag
- **AND** no file is mutated without the confirmation
