# Proposal: Setup Wizard for synrepo Client Integration

## Summary

Add a `synrepo setup` command that provides a wizard-driven interface to install or uninstall synrepo integration assets for supported AI clients.

The setup wizard manages:
- target-specific skills / commands installation
- target-specific context files
- optional synrepo MCP registration where a reliable target-specific strategy exists
- gitignore suggestions
- status and uninstall flows

## Scope

**In Scope:**
- `synrepo setup` wizard and direct subcommands
- explicit per-target asset layouts
- first-class support for Claude Code, OpenCode, Codex CLI, Cursor Agent, Windsurf, and GitHub Copilot
- second-wave support for Gemini CLI, Goose, Kiro CLI, Qwen Code, Junie, Roo Code, Tabnine CLI, and Trae
- safe install/uninstall/status flows per target
- manual next-step output when automatic MCP registration is not supported

**Out of Scope:**
- guessing unknown config locations for unsupported targets
- treating all integrations as `.mcp.json` edits
- auto-enabling watch mode

## Problem Statement

Currently, setting up synrepo with different AI clients requires manual steps:
1. Running `synrepo init`
2. Creating/updating `.mcp.json` with the synrepo MCP server
3. Writing client-specific instructions
4. Potentially enabling watch mode

Users with existing MCP configurations risk breaking their setup if they manually edit `.mcp.json`. There's no unified interface to manage synrepo's integration with different AI clients.

## Proposed Solution

A wizard-driven `synrepo setup` command that:
- Provides a menu to select the target AI client
- Runs `synrepo init` if needed
- Installs target-specific assets (skills, commands, workflows) to the correct directories
- Injects synrepo MCP server config where safe (preserving other servers)
- Never auto-enables watch mode
- Prints only the next manual step if approval is needed

### Target Asset Model

Each target is defined with:
- stable target key (e.g., "claude", "cursor", "windsurf")
- command or skills destination directory
- file format and extension
- optional context file location
- whether the target requires its own CLI
- whether synrepo can auto-register MCP for that target

### Auto MCP Registration

Only targets with a reliable, tested registration strategy receive automatic MCP configuration:
- Claude Code: `.mcp.json` (project or `~/.claude/`)
- OpenCode: `opencode.json` or equivalent
- Codex CLI: `.codex/config.toml`

All other targets receive manual next-step guidance instead of guessing.

## Impact

- New command: `synrepo setup` with sub-commands for each client
- New file: `openspec/specs/setup-wizard/spec.md` for command surface
- Modified: CLI command dispatcher to route `setup` subcommand
- Extended: existing `synrepo agent-setup` to support new targets

## Risks

- Target-specific config conventions may differ from assumed patterns
- Some targets may have undocumented or evolving configuration formats

## Timeline

- Design artifact: ~2 hours
- Implementation: ~8-10 hours (split across sessions)
- Testing: ~4 hours (manual verification with each client)