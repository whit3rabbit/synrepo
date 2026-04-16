# Setup Wizard

> Spec: `setup-wizard`

## Overview

The setup wizard provides an interactive CLI interface for installing and uninstalling synrepo integration with various AI clients (Claude Code, OpenCode, Cursor/Codex). It uses the `ratatui` library to provide a rich terminal-based UI with menus, prompts, and status views. It safely injects MCP configuration into existing `.mcp.json` files, writes client-specific instruction files, and never auto-enables watch mode.

## ADDED Requirements

### Requirement: TUI framework integration
The setup wizard SHALL use the `ratatui` library for terminal-based UI rendering.

#### Scenario: TUI rendering
- **WHEN** the wizard runs in a terminal
- **THEN** it renders a curses-style menu with keyboard navigation
- **AND** arrow keys navigate options, Enter selects, Escape cancels

### Requirement: Setup command surface
synrepo SHALL expose a `synrepo setup` command that launches an interactive wizard when run without arguments and supports direct invocation with `--install` or `--uninstall` flags.

#### Scenario: User runs `synrepo setup` with no arguments
- **WHEN** the user runs `synrepo setup` with no arguments
- **THEN** an interactive wizard menu is displayed with options: Claude Code, OpenCode, Cursor/Codex, Uninstall, Status, Quit

#### Scenario: User runs direct install command
- **WHEN** the user runs `synrepo setup claude --install`
- **THEN** the install flow runs directly without the interactive menu

### Requirement: Install flow
When installing synrepo for a client, the wizard SHALL verify the binary, run init if needed, inject MCP config, write instruction files, check gitignore, and prompt about watch mode.

#### Scenario: Install with no prior initialization
- **WHEN** installing synrepo and `.synrepo/` doesn't exist
- **THEN** the wizard runs `synrepo init` before proceeding with MCP injection

#### Scenario: Install completes successfully
- **WHEN** install flow completes
- **THEN** the wizard prints a summary and any remaining manual steps (e.g., restart IDE)

### Requirement: MCP configuration injection
The wizard SHALL locate `.mcp.json`, inject synrepo entry while preserving existing servers, create backup before modification, and refuse to modify invalid JSON.

#### Scenario: MCP file has other servers
- **WHEN** the user's `.mcp.json` contains other MCP server entries
- **THEN** all other MCP servers are preserved; synrepo is added without overwriting

#### Scenario: MCP file is invalid JSON
- **WHEN** the user's `.mcp.json` contains invalid JSON
- **THEN** an error is displayed asking the user to fix the file manually, no modifications are made

#### Scenario: MCP file doesn't exist
- **WHEN** no `.mcp.json` file exists
- **THEN** a new `.mcp.json` is created with synrepo MCP server entry only

### Requirement: Uninstall flow
When uninstalling synrepo for a client, the wizard SHALL confirm first, remove MCP entry, delete instruction files, and print summary.

#### Scenario: User confirms uninstall
- **WHEN** the user confirms the uninstall
- **THEN** the synrepo entry is removed from `.mcp.json` and client instruction files are deleted

### Requirement: Client instruction files
The wizard SHALL write instruction files for each client at expected locations: `.claude.md` for Claude Code, `.opencode.md` for OpenCode, `.cursorrules` for Cursor/Codex.

#### Scenario: Writing Claude Code instructions
- **WHEN** installing for Claude Code
- **THEN** a `.claude.md` file is written to the project root

### Requirement: Status view
The wizard SHALL display current integration status showing for each client whether MCP server and instruction file are installed.

#### Scenario: User views status
- **WHEN** the user selects "View current status" from the menu
- **THEN** a status table is displayed showing integration state for all clients

### Requirement: Watch mode opt-in
The wizard SHALL never auto-enable watch mode and shall ask the user explicitly if they want to enable it.

#### Scenario: Watch mode prompt
- **WHEN** install flow completes
- **THEN** the wizard asks "Would you like to enable watch mode? (y/N)" and prints manual command if confirmed
