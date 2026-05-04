## Requirements

### Requirement: Provide a hidden nudge hook CLI
synrepo SHALL provide a hidden CLI entrypoint `synrepo agent-hook nudge --client <client> --event <event>` for Codex and Claude hook integrations.

#### Scenario: Hook reads JSON from stdin
- **WHEN** the command is invoked with valid hook JSON on stdin
- **THEN** it evaluates the event without persisting prompt, tool, or caller content
- **AND** it emits client-valid JSON when a nudge is appropriate

#### Scenario: Hook receives irrelevant or malformed input
- **WHEN** the command receives unsupported, irrelevant, or malformed input
- **THEN** it exits successfully without blocking the caller
- **AND** it does not run reconcile, refresh commentary, call MCP tools, or mutate repository state

### Requirement: Nudge on codebase and tool-bypass workflows
synrepo SHALL emit short, non-blocking guidance when supported hook events indicate a codebase question, file review, broad search, trace, impact, or pre-edit workflow that is likely to benefit from synrepo context first.

#### Scenario: User prompt asks for codebase review
- **WHEN** a supported `UserPromptSubmit` event contains a codebase review request
- **THEN** the hook returns a short context nudge to orient with synrepo and use compact search or cards before cold source reads

#### Scenario: Tool call bypasses synrepo routing
- **WHEN** a supported `PreToolUse` event targets direct read, search, edit, shell, patch, or MCP flows
- **THEN** the hook returns a short nudge for matching cold-context workflows
- **AND** the nudge is advisory only, never a blocker

#### Scenario: Shell command is RTK-prefixed
- **WHEN** a shell command begins with `rtk`
- **THEN** command classification strips the prefix before deciding whether the command is a search/read/edit workflow

### Requirement: Install local hook configs explicitly
synrepo SHALL install Codex and Claude hook configs only when the operator explicitly requests agent hooks during setup.

#### Scenario: Install Claude nudge hooks
- **WHEN** setup is run for Claude with agent hooks enabled
- **THEN** synrepo writes or merges project-local `.claude/settings.local.json`
- **AND** it preserves unrelated user settings and existing hook entries

#### Scenario: Install Codex nudge hooks
- **WHEN** setup is run for Codex with agent hooks enabled
- **THEN** synrepo writes or merges project-local `.codex/hooks.json`
- **AND** it preserves unrelated hook entries and existing MCP config

#### Scenario: Codex hooks feature is disabled
- **WHEN** Codex hook installation is requested and Codex hooks are not enabled
- **THEN** setup prints the exact `[features] codex_hooks = true` requirement
- **AND** it does not silently rely on an unavailable runtime feature
