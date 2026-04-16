# Proposal: Setup Wizard for synrepo Client Integration

## Summary

Add a `synrepo setup` command that provides a wizard-driven interface to install/uninstall synrepo for various AI clients (Claude Code, OpenCode, Codex). The command manages MCP server configuration injection into existing `.mcp.json` files without breaking other MCP servers, handles skill installation, and offers gitignore suggestions.

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
- Injects synrepo MCP server config into existing `.mcp.json` (preserving other servers)
- Writes client-specific instruction files
- Never auto-enables watch mode
- Prints only the next manual step if approval is needed

### Scope

**In Scope:**
- `synrepo setup` command with interactive wizard
- Support for Claude Code, OpenCode, and Codex targets
- Safe `.mcp.json` injection/rollback
- Gitignore recommendation
- Uninstall capability via the same wizard

**Out of Scope:**
- Claude plugin marketplace integration
- Separate MCP daemon (keep stdio launch as default)
- Auto-enabling watch mode (always opt-in)

## Impact

- New command: `synrepo setup` with sub-commands for each client
- New file: `openspec/specs/setup-wizard/spec.md` for command surface
- Modified: CLI command dispatcher to route `setup` subcommand
- Modified: Agent shim generator (extend existing `synrepo agent-setup`)

## Risks

- `.mcp.json` format variations across users could cause injection issues
- OpenCode configuration conventions may differ from assumed patterns

## Timeline

- Design artifact: ~2 hours
- Implementation: ~4-6 hours (split across sessions)
- Testing: ~2 hours (manual verification with each client)
