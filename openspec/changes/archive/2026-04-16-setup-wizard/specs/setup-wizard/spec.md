# Setup Wizard

> Spec: `setup-wizard`

## Overview

The setup wizard provides a CLI interface for installing and uninstalling synrepo integration with various AI clients. It supports first-wave targets (Claude Code, OpenCode, Codex CLI, Cursor, Windsurf, GitHub Copilot) with automatic MCP registration, and second-wave targets (Gemini CLI, Goose, Kiro CLI, Qwen Code, Junie, Roo Code, Tabnine CLI, Trae) with manual MCP configuration instructions.

## ADDED Requirements

### Requirement: Setup command surface
synrepo SHALL expose a `synrepo setup <target>` command that installs synrepo integration for a specific client target.

#### Scenario: Setup claude
- **WHEN** the user runs `synrepo setup claude`
- **THEN** synrepo initializes `.synrepo/` if needed, writes the Claude Code shim, and registers MCP in `.mcp.json`

#### Scenario: Setup with unsupported MCP target
- **WHEN** the user runs `synrepo setup <target>` where target doesn't support auto MCP registration
- **THEN** synrepo writes the shim file and prints the manual next step for MCP configuration

### Requirement: Client Asset Locations
The setup wizard SHALL model each supported integration as a target-specific asset layout rather than a single generic instruction file.

Each target definition MUST include:
- a stable target key
- a command or skills destination directory
- a file format and extension
- an optional context file location
- whether the target requires its own CLI
- whether synrepo can auto-register MCP for that target

#### Scenario: Installing Claude Code
- **WHEN** installing for Claude Code
- **THEN** synrepo writes `.claude/synrepo-context.md`
- **AND** prints instruction to add `@.claude/synrepo-context.md` to CLAUDE.md

#### Scenario: Installing OpenCode
- **WHEN** installing for OpenCode
- **THEN** synrepo writes `AGENTS.md`
- **AND** prints instruction that OpenCode loads it automatically

#### Scenario: Installing Codex CLI
- **WHEN** installing for Codex CLI
- **THEN** synrepo writes `.codex/instructions.md`
- **AND** registers MCP in `.codex/config.toml` if it exists

#### Scenario: Installing Cursor Agent
- **WHEN** installing for Cursor Agent
- **THEN** synrepo writes `.cursor/synrepo.mdc`
- **AND** prints manual step to add synrepo to Cursor MCP settings

#### Scenario: Installing Windsurf
- **WHEN** installing for Windsurf
- **THEN** synrepo writes `.windsurf/rules/synrepo.md`
- **AND** prints manual step to add synrepo to Windsurf MCP settings

#### Scenario: Installing GitHub Copilot
- **WHEN** installing for GitHub Copilot
- **THEN** synrepo writes `synrepo-copilot-instructions.md`
- **AND** prints manual step to configure in Copilot settings

#### Scenario: Installing Second-Wave Targets
- **WHEN** installing for Gemini, Goose, Kiro, Qwen, Junie, Roo, Tabnine, or Trae
- **THEN** synrepo writes the shim to the target-specific location
- **AND** prints manual next step for MCP configuration

### Requirement: Supported Targets and Asset Locations

| Target | Requires CLI | Output Path | Format | Auto MCP |
|---|---:|---|---|:---:|
| Claude Code | yes | `.claude/synrepo-context.md` | markdown | yes |
| OpenCode | yes | `AGENTS.md` | markdown | yes |
| Codex CLI | yes | `.codex/instructions.md` | markdown | yes |
| Cursor Agent | no | `.cursor/synrepo.mdc` | markdown | no |
| Windsurf | no | `.windsurf/rules/synrepo.md` | markdown | no |
| GitHub Copilot | no | `synrepo-copilot-instructions.md` | markdown | no |
| Gemini CLI | yes | `.gemini/commands/synrepo.toml` | toml | no |
| Goose | yes | `.goose/recipes/synrepo.yaml` | yaml | no |
| Kiro CLI | yes | `.kiro/prompts/synrepo.md` | markdown | no |
| Qwen Code | yes | `.qwen/commands/synrepo.md` | markdown | no |
| Junie | yes | `.junie/commands/synrepo.md` | markdown | no |
| Roo Code | no | `.roo/commands/synrepo.md` | markdown | no |
| Tabnine CLI | yes | `.tabnine/agent/commands/synrepo.toml` | toml | no |
| Trae | no | `.trae/skills/synrepo/SKILL.md` | markdown skill | no |

### Requirement: Automatic MCP Registration
Only targets with a reliable, tested registration strategy SHALL receive automatic MCP configuration.

#### Scenario: Auto MCP targets
- **WHEN** the target is Claude, OpenCode, or Codex
- **THEN** synrepo modifies the target's MCP configuration file to add the synrepo server

#### Scenario: Manual MCP targets
- **WHEN** the target does not have a supported automatic MCP registration strategy
- **THEN** synrepo prints a manual next-step instruction instead of guessing

### Requirement: Install flow
When installing synrepo for a client, the wizard SHALL verify the binary, run init if needed, write shim files, optionally inject MCP config, and print the next manual step.

#### Scenario: Install with no prior initialization
- **WHEN** installing synrepo and `.synrepo/` doesn't exist
- **THEN** the wizard runs `synrepo init` before proceeding

#### Scenario: Install completes successfully
- **WHEN** install flow completes
- **AND** the target supports auto MCP registration
- **THEN** the wizard prints that MCP server was configured automatically

- **AND** the target does NOT support auto MCP registration
- **THEN** the wizard prints the manual next step for MCP configuration

### Requirement: Uninstall flow
The setup wizard SHALL provide a separate `synrepo setup uninstall <target>` command that removes shim files and MCP configuration.

#### Scenario: Uninstall Claude
- **WHEN** the user runs `synrepo setup uninstall claude`
- **THEN** synrepo removes `.claude/synrepo-context.md` and synrepo entry from `.mcp.json`

### Requirement: Watch mode opt-in
The wizard SHALL never auto-enable watch mode.

#### Scenario: Watch mode
- **WHEN** install flow completes
- **THEN** the wizard prints "Configure your agent to use `synrepo mcp --repo .` as a stdio MCP server"
- **AND** does NOT automatically start or enable watch mode