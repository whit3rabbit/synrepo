## MODIFIED Requirements

### Requirement: Define agent-setup target expansion
synrepo SHALL support `cursor`, `codex`, and `windsurf` as named targets for `synrepo agent-setup`, in addition to the existing `claude`, `copilot`, and `generic` targets. A `--regen` flag SHALL update an existing shim file in place when its content differs from the current template. The `synrepo setup` and `synrepo agent-setup` commands SHALL accept `--only <tool,tool>` and `--skip <tool,tool>` for multi-client invocation, and SHALL reject the combination of both flags as a usage error.

#### Scenario: Generate a cursor shim
- **WHEN** a user runs `synrepo agent-setup cursor`
- **THEN** synrepo writes a shim to `.cursor/skills/synrepo/SKILL.md` describing the available MCP tools and their usage
- **AND** the shim begins with YAML frontmatter containing `name: synrepo` and a `description` so Cursor auto-discovers it as a skill
- **AND** the shim content reflects the current shipped MCP surface

#### Scenario: Regenerate an existing shim
- **WHEN** a user runs `synrepo agent-setup claude --regen` and the existing shim differs from the current template
- **THEN** synrepo overwrites the shim and prints a summary of what changed
- **AND** if the shim is already current, the command exits zero with no changes

#### Scenario: Generate a codex shim
- **WHEN** a user runs `synrepo agent-setup codex`
- **THEN** synrepo writes a shim to `.codex/skills/synrepo/SKILL.md` describing the MCP server and tool list
- **AND** the shim begins with YAML frontmatter containing `name: synrepo` and a `description` so Codex CLI auto-discovers it as a skill
- **AND** the shim notes how to configure the MCP server for codex usage

#### Scenario: Multi-client setup with --only
- **WHEN** a user runs `synrepo setup --only claude,cursor`
- **THEN** synrepo configures both clients in sequence and prints a per-tool outcome summary
- **AND** a single-tool positional invocation (`synrepo setup claude`) continues to work unchanged

#### Scenario: Multi-client setup with --skip
- **WHEN** a user runs `synrepo agent-setup --skip copilot,generic`
- **THEN** synrepo configures every other supported tool for which the host has detection signals
- **AND** the per-tool summary names every skipped tool

#### Scenario: Conflicting flags rejected
- **WHEN** a user runs `synrepo setup --only claude --skip claude`
- **THEN** synrepo rejects the invocation with a usage error naming the conflict
- **AND** no shim or MCP registration is written

#### Scenario: Unknown tool rejected
- **WHEN** a user runs `synrepo setup --only claude,nonesuch`
- **THEN** synrepo rejects the invocation with an error naming `nonesuch` and listing supported tools
- **AND** no partial configuration is left on disk
